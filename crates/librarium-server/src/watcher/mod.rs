use crate::error::{AppError, AppResult};
use crate::models::{FileChangeEvent, FileChangeType};
use chrono::Utc;
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub type ChangeReceiver = mpsc::UnboundedReceiver<FileChangeEvent>;
pub type ChangeSender = mpsc::UnboundedSender<FileChangeEvent>;

pub struct FileWatcher {
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    vault_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    change_tx: ChangeSender,
}

impl FileWatcher {
    pub fn new() -> AppResult<(Self, ChangeReceiver)> {
        let (change_tx, change_rx) = mpsc::unbounded_channel();
        let vault_paths = Arc::new(RwLock::new(HashMap::new()));
        let vault_paths_clone = vault_paths.clone();
        let tx_clone = change_tx.clone();

        let debouncer = new_debouncer(
            Duration::from_millis(500),
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    for event in events {
                        if let Err(e) =
                            Self::handle_event(event.event, &vault_paths_clone, &tx_clone)
                        {
                            error!("Error handling file event: {}", e);
                        }
                    }
                }
                Err(errors) => {
                    for error in errors {
                        error!("Watch error: {:?}", error);
                    }
                }
            },
        )
        .map_err(|e| AppError::InternalError(format!("Failed to create watcher: {}", e)))?;

        Ok((
            Self {
                debouncer,
                vault_paths,
                change_tx,
            },
            change_rx,
        ))
    }

    pub fn watch_vault(&mut self, vault_id: String, vault_path: PathBuf) -> AppResult<()> {
        info!("Starting to watch vault: {} at {:?}", vault_id, vault_path);

        self.debouncer
            .watch(&vault_path, RecursiveMode::Recursive)
            .map_err(|e| AppError::InternalError(format!("Failed to watch path: {}", e)))?;

        let mut paths = self
            .vault_paths
            .write()
            .map_err(|_| AppError::InternalError("Failed to acquire write lock".to_string()))?;
        paths.insert(vault_id, vault_path);

        Ok(())
    }

    pub fn unwatch_vault(&mut self, vault_id: &str) -> AppResult<()> {
        let mut paths = self
            .vault_paths
            .write()
            .map_err(|_| AppError::InternalError("Failed to acquire write lock".to_string()))?;

        if let Some(vault_path) = paths.remove(vault_id) {
            info!("Stopping watch for vault: {} at {:?}", vault_id, vault_path);

            self.debouncer
                .unwatch(&vault_path)
                .map_err(|e| AppError::InternalError(format!("Failed to unwatch path: {}", e)))?;
        }

        Ok(())
    }

    fn handle_event(
        event: Event,
        vault_paths: &Arc<RwLock<HashMap<String, PathBuf>>>,
        tx: &ChangeSender,
    ) -> AppResult<()> {
        if event.paths.is_empty() {
            return Ok(());
        }

        let vault_map = vault_paths
            .read()
            .map_err(|_| AppError::InternalError("Failed to acquire read lock".to_string()))?;

        // Rename(Both) carries two paths: [from, to]. Treat this as a single
        // Renamed event rather than two independent Modified events so the
        // search index can cleanly remove the old entry and add the new one.
        // This was the primary bug: all Modify variants were mapped to
        // FileChangeType::Modified and only the first path was processed,
        // leaving the old entry stale and the new path never indexed.
        if let EventKind::Modify(ModifyKind::Name(RenameMode::Both)) = event.kind {
            if event.paths.len() >= 2 {
                let from_abs = &event.paths[0];
                let to_abs = &event.paths[1];

                for (vault_id, vault_path) in vault_map.iter() {
                    if from_abs.starts_with(vault_path) && to_abs.starts_with(vault_path) {
                        if is_ignored(from_abs) && is_ignored(to_abs) {
                            return Ok(());
                        }
                        let from_rel = rel(vault_path, from_abs);
                        let to_rel = rel(vault_path, to_abs);

                        let change_event = FileChangeEvent {
                            vault_id: vault_id.clone(),
                            path: from_rel.clone(),
                            event_type: FileChangeType::Renamed {
                                from: from_rel,
                                to: to_rel,
                            },
                            timestamp: Utc::now(),
                        };
                        if let Err(e) = tx.send(change_event) {
                            error!("Failed to send rename event: {}", e);
                        }
                        return Ok(());
                    }
                }
                // Paths span different vaults (unusual) — fall through to
                // per-path handling below.
            }
        }

        // For all other event kinds, process each path independently.
        for abs_path in &event.paths {
            // Find the vault this path belongs to.
            let Some((vault_id, vault_path)) = vault_map
                .iter()
                .find(|(_, vp)| abs_path.starts_with(vp.as_path()))
            else {
                continue;
            };

            // Skip hidden files/dirs and internal vault directories.
            if is_ignored(abs_path) {
                continue;
            }

            let relative_path = rel(vault_path, abs_path);

            let event_type = match event.kind {
                EventKind::Create(_) => FileChangeType::Created,
                EventKind::Remove(_) => FileChangeType::Deleted,
                // Rename(From): the source side of a rename when the debouncer
                // delivers it as two separate events. Treat as deletion.
                EventKind::Modify(ModifyKind::Name(RenameMode::From)) => FileChangeType::Deleted,
                // Rename(To): the destination side. Treat as creation.
                EventKind::Modify(ModifyKind::Name(RenameMode::To)) => FileChangeType::Created,
                // Rename(Any): platform can't tell direction; use file existence.
                EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => {
                    if abs_path.exists() {
                        FileChangeType::Created
                    } else {
                        FileChangeType::Deleted
                    }
                }
                // Generic modification (content changed).
                EventKind::Modify(_) => FileChangeType::Modified,
                // Access, Other, etc. — not actionable for the index.
                _ => {
                    warn!("Unhandled notify event kind {:?} for {:?}", event.kind, abs_path);
                    continue;
                }
            };

            let change_event = FileChangeEvent {
                vault_id: vault_id.clone(),
                path: relative_path,
                event_type,
                timestamp: Utc::now(),
            };

            if let Err(e) = tx.send(change_event) {
                error!("Failed to send change event: {}", e);
            }
        }

        Ok(())
    }

    pub fn get_sender(&self) -> ChangeSender {
        self.change_tx.clone()
    }
}

/// True if this path should never trigger index updates: hidden files/dirs
/// and the internal `.obsidian/` and `.trash/` directories.
fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    })
}

/// Return the vault-relative string for `abs_path` under `vault_path`,
/// normalized to forward slashes so watcher-derived paths match the API and
/// frontend convention on every OS (Windows otherwise yields backslashes,
/// which break search keys, entity paths and open-tab matching).
fn rel(vault_path: &Path, abs_path: &Path) -> String {
    abs_path
        .strip_prefix(vault_path)
        .unwrap_or(abs_path)
        .to_string_lossy()
        .replace('\\', "/")
}
