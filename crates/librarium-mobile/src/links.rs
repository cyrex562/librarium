//! Wiki-link resolution and backlink/outgoing-link scans, backed by
//! `librarium_core::wiki_link_service`.
//!
//! `backlinks` mirrors `GET /api/vaults/{id}/backlinks` — a byte-for-byte
//! port of `routes/tags.rs::list_backlinks`'s scan (same substring patterns,
//! same file-set, same sort). `outgoing_links` has no REST equivalent: the
//! server has never needed it because the frontend already holds the open
//! note's content and extracts `[[links]]` from it client-side. On mobile
//! that extraction has to happen here instead, so it's a new (small,
//! self-contained) piece of logic rather than a port.

use librarium_core::error::AppResult;
use librarium_core::wiki_link_service::WikiLinkResolver;
use serde::Serialize;
use std::path::Path;
use std::sync::LazyLock;
use walkdir::WalkDir;

/// Mirrors `ResolveWikiLinkResponse` in `routes/files.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct ResolveWikiLinkResult {
    pub path: String,
    pub exists: bool,
    pub alternatives: Vec<String>,
    pub ambiguous: bool,
}

fn to_result(resolved: librarium_core::wiki_link_service::ResolvedLink) -> ResolveWikiLinkResult {
    ResolveWikiLinkResult {
        ambiguous: !resolved.alternatives.is_empty(),
        path: resolved.path,
        exists: resolved.exists,
        alternatives: resolved.alternatives,
    }
}

/// Mirrors `POST /api/vaults/{id}/resolve-link`.
pub async fn resolve_wiki_link(
    vault_path: &str,
    link: &str,
    current_file: Option<&str>,
) -> AppResult<ResolveWikiLinkResult> {
    let vault_path = vault_path.to_string();
    let link = link.to_string();
    let current_file = current_file.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let resolved = match current_file.as_deref() {
            Some(current) => WikiLinkResolver::resolve_relative(&vault_path, &link, current)?,
            None => WikiLinkResolver::resolve(&vault_path, &link)?,
        };
        Ok(to_result(resolved))
    })
    .await
    .map_err(|e| {
        librarium_core::error::AppError::InternalError(format!(
            "resolve_wiki_link task join error: {e}"
        ))
    })?
}

/// One entry in a backlinks/outgoing-links result: mirrors the ad-hoc
/// `{"path": ..., "title": ...}` shape `list_backlinks` returns.
#[derive(Debug, Clone, Serialize)]
pub struct LinkedNote {
    pub path: String,
    pub title: String,
}

fn title_of(rel_path: &str) -> String {
    Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path)
        .to_string()
}

/// Mirrors `GET /api/vaults/{id}/backlinks?path=...`: every `.md` file whose
/// raw content contains a wiki-link or markdown-link pointing at
/// `target_path`. Same substring patterns as the server (case-insensitive
/// `[[stem]]`, `(path)`, `(path-without-.md)`) — not a full parse, matching
/// the server's implementation exactly rather than being stricter.
pub async fn backlinks(vault_path: &str, target_path: &str) -> AppResult<Vec<LinkedNote>> {
    let vault_path = vault_path.to_string();
    let target_path = target_path.trim().to_string();
    tokio::task::spawn_blocking(move || {
        // Same rules as the server's /backlinks route (#144).
        let target = librarium_core::note_links::BacklinkTarget::new(&target_path);

        let mut results = Vec::new();
        for entry in WalkDir::new(&vault_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file() && librarium_core::note_links::is_note_file(e.path())
            })
        {
            let rel_path = entry
                .path()
                .strip_prefix(&vault_path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");

            if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                if target.is_linked_from(&rel_path, &raw) {
                    results.push(LinkedNote {
                        title: title_of(&rel_path),
                        path: rel_path,
                    });
                }
            }
        }

        results.sort_by(|a, b| a.path.cmp(&b.path));
        results
    })
    .await
    .map_err(|e| {
        librarium_core::error::AppError::InternalError(format!("backlinks task join error: {e}"))
    })
}

/// `[[Target]]`, `[[Target|alias]]`, `![[Target]]` — same shape as
/// `markdown_service`'s (private) `WIKI_LINK_REGEX`, duplicated here rather
/// than exposed from core since extracting links from raw text is a
/// mobile-only need (the server has never had to do it standalone).
static WIKI_LINK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"!?\[\[([^\]|#^]+)").unwrap());

/// No REST equivalent (see module doc): every `[[wiki link]]` target named in
/// `file_path`'s own content, resolved against the vault. This is what lets a
/// mobile note editor show "links out of this note" without a server to ask.
pub async fn outgoing_links(vault_path: &str, file_path: &str) -> AppResult<Vec<LinkedNote>> {
    let vault_path = vault_path.to_string();
    let file_path = file_path.to_string();
    tokio::task::spawn_blocking(move || {
        let full_path =
            librarium_core::file_service::FileService::resolve_path(&vault_path, &file_path)?;
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();

        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();
        for cap in WIKI_LINK_RE.captures_iter(&content) {
            let target = cap[1].trim();
            if target.is_empty() || !seen.insert(target.to_lowercase()) {
                continue;
            }
            let resolved = WikiLinkResolver::resolve_relative(&vault_path, target, &file_path)
                .unwrap_or(librarium_core::wiki_link_service::ResolvedLink {
                    path: target.to_string(),
                    exists: false,
                    alternatives: vec![],
                });
            results.push(LinkedNote {
                title: title_of(&resolved.path),
                path: resolved.path,
            });
        }
        Ok(results)
    })
    .await
    .map_err(|e| {
        librarium_core::error::AppError::InternalError(format!(
            "outgoing_links task join error: {e}"
        ))
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn resolve_wiki_link_finds_existing_note() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("Target.md"), "# Target").unwrap();

        let result = resolve_wiki_link(vault.path().to_str().unwrap(), "Target", None)
            .await
            .unwrap();
        assert!(result.exists);
        assert_eq!(result.path, "Target.md");
        assert!(!result.ambiguous);
    }

    #[tokio::test]
    async fn resolve_wiki_link_unresolved_reports_not_exists() {
        let vault = TempDir::new().unwrap();
        let result = resolve_wiki_link(vault.path().to_str().unwrap(), "Nope", None)
            .await
            .unwrap();
        assert!(!result.exists);
        assert_eq!(result.path, "Nope.md");
    }

    #[tokio::test]
    async fn backlinks_finds_wiki_link_referencing_target() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("Target.md"), "# Target").unwrap();
        std::fs::write(vault.path().join("Linker.md"), "See [[Target]]").unwrap();
        std::fs::write(vault.path().join("Unrelated.md"), "Nothing here").unwrap();

        let result = backlinks(vault.path().to_str().unwrap(), "Target.md")
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "Linker.md");
    }

    #[tokio::test]
    async fn backlinks_excludes_the_target_itself() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("Target.md"), "See [[Target]]").unwrap();

        let result = backlinks(vault.path().to_str().unwrap(), "Target.md")
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn outgoing_links_extracts_and_resolves_links() {
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("Target.md"), "# Target").unwrap();
        std::fs::write(
            vault.path().join("Source.md"),
            "See [[Target]] and ![[Target|alias]] and [[Missing]]",
        )
        .unwrap();

        let result = outgoing_links(vault.path().to_str().unwrap(), "Source.md")
            .await
            .unwrap();
        assert_eq!(result.len(), 2, "dedups repeated target: {result:?}");
        assert!(result.iter().any(|l| l.path == "Target.md"));
        assert!(result.iter().any(|l| l.path == "Missing.md"));
    }

    #[tokio::test]
    async fn outgoing_links_on_missing_file_is_empty() {
        let vault = TempDir::new().unwrap();
        let result = outgoing_links(vault.path().to_str().unwrap(), "Nope.md")
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
