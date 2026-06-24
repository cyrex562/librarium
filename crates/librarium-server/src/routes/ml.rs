use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::models::{
    AnalyzeNoteRequest, ApplyChange, ApplyOrganizationSuggestionRequest,
    ApplyOrganizationSuggestionResponse, GenerateOrganizationSuggestionsRequest,
    GenerateOutlineRequest, MlUndoReceipt, OrganizationSuggestion, OrganizationSuggestionKind,
    RenameSuggestionRequest, RenameSuggestionResponse, ReverseAction, UndoMlActionResponse,
};
use crate::routes::vaults::AppState;
use crate::services::{
    embedding_service, frontmatter_service, rewrite_wiki_links, FileService, MlService,
    RenameStrategy,
};
use actix_web::{post, web, HttpResponse};
use chrono::Utc;
use serde_json::{Map, Value};
use std::path::{Component, Path};
use uuid::Uuid;

fn default_max_sections() -> usize {
    24
}

fn default_max_suggestions() -> usize {
    8
}

#[post("/api/vaults/{vault_id}/ml/outline")]
async fn generate_outline(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    body: web::Json<GenerateOutlineRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let req = body.into_inner();

    let vault = state.db.get_vault(&vault_id).await?;

    let content = match req.content {
        Some(content) => content,
        None => FileService::read_file(&vault.path, &req.file_path)?.content,
    };

    let max_sections = req.max_sections.unwrap_or_else(default_max_sections);
    let outline = MlService::generate_outline(&req.file_path, &content, max_sections);

    Ok(HttpResponse::Ok().json(outline))
}

#[post("/api/vaults/{vault_id}/ml/analyze")]
async fn analyze_note(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    vault_id: web::Path<String>,
    body: web::Json<AnalyzeNoteRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let req = body.into_inner();

    let vault = state.db.get_vault(&vault_id).await?;

    let (frontmatter, content) = match req.content {
        Some(raw_content) => frontmatter_service::parse_frontmatter(&raw_content)?,
        None => {
            let file = FileService::read_file(&vault.path, &req.file_path)?;
            (file.frontmatter, file.content)
        }
    };

    let tier = config.ml.tier.as_str();
    let analysis = MlService::analyze(&req.file_path, &content, frontmatter.as_ref(), tier);

    Ok(HttpResponse::Ok().json(analysis))
}

#[post("/api/vaults/{vault_id}/ml/suggestions")]
async fn generate_suggestions(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    vault_id: web::Path<String>,
    body: web::Json<GenerateOrganizationSuggestionsRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let req = body.into_inner();

    let vault = state.db.get_vault(&vault_id).await?;

    let (frontmatter, content) = match req.content {
        Some(raw_content) => {
            let (frontmatter, content) = frontmatter_service::parse_frontmatter(&raw_content)?;
            (frontmatter, content)
        }
        None => {
            let file = FileService::read_file(&vault.path, &req.file_path)?;
            (file.frontmatter, file.content)
        }
    };

    let max_suggestions = req.max_suggestions.unwrap_or_else(default_max_suggestions);

    // Keyphrase tags are gated on the active tier (empty under `heuristic`).
    let tier = config.ml.tier.as_str();
    let keyphrases = MlService::keyphrases_for_tier(&content, tier, max_suggestions.max(8));

    let mut suggestions = MlService::suggest_organization(
        &req.file_path,
        &content,
        frontmatter.as_ref(),
        &keyphrases,
        max_suggestions,
    );

    // LIB-061: layer controlled-vocabulary semantic tags on top of Tier 1 when
    // the embeddings tier is active. Exclude tags already present or suggested.
    let mut covered: std::collections::HashSet<String> = suggestions
        .existing_tags
        .iter()
        .map(|t| t.trim().trim_start_matches('#').to_lowercase())
        .collect();
    for s in &suggestions.suggestions {
        if let Some(tag) = &s.tag {
            covered.insert(tag.trim().trim_start_matches('#').to_lowercase());
        }
    }

    let semantic = embedding_service::suggest_semantic_tags(
        &state.db,
        &config.ml,
        &vault_id,
        &req.file_path,
        &content,
        &covered,
        max_suggestions,
    )
    .await?;

    for (tag, score) in semantic {
        suggestions.suggestions.push(OrganizationSuggestion {
            id: format!("tag:{}", tag),
            kind: OrganizationSuggestionKind::Tag,
            confidence: score.clamp(0.0, 1.0),
            rationale: "Semantically similar to other notes you've given this tag.".to_string(),
            tag: Some(tag),
            category: None,
            target_folder: None,
            new_name: None,
            source: Some("semantic".to_string()),
        });
    }

    // LIB-062: semantic/TF-IDF folder placement. When it yields a target, it
    // replaces the heuristic `move_to_folder` suggestion from Tier 1.
    if let Some(folder_suggestion) =
        suggest_folder_placement(&state.db, &config.ml, &vault_id, &vault.path, &req.file_path, &content)
            .await?
    {
        suggestions
            .suggestions
            .retain(|s| !matches!(s.kind, OrganizationSuggestionKind::MoveToFolder));
        suggestions.suggestions.push(folder_suggestion);
    }

    // Re-sort the merged list by confidence and re-apply the cap.
    suggestions.suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions.suggestions.truncate(max_suggestions.clamp(1, 25));

    Ok(HttpResponse::Ok().json(suggestions))
}

#[post("/api/vaults/{vault_id}/ml/rename-suggestion")]
async fn rename_suggestion(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    vault_id: web::Path<String>,
    body: web::Json<RenameSuggestionRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let req = body.into_inner();

    let vault = state.db.get_vault(&vault_id).await?;

    let (frontmatter, content) = match req.content {
        Some(raw_content) => frontmatter_service::parse_frontmatter(&raw_content)?,
        None => {
            let file = FileService::read_file(&vault.path, &req.file_path)?;
            (file.frontmatter, file.content)
        }
    };

    let scheme = req
        .naming_scheme
        .clone()
        .unwrap_or_else(|| config.ml.naming_scheme.clone());

    let tier = config.ml.tier.as_str();
    let keyphrases = MlService::keyphrases_for_tier(&content, tier, 6);

    let current_name = Path::new(&req.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&req.file_path)
        .to_string();

    let proposed_name = MlService::suggest_rename(
        &req.file_path,
        &content,
        frontmatter.as_ref(),
        &keyphrases,
        &scheme,
    );

    let (proposed_path, rationale, suggestion) = match &proposed_name {
        Some(name) => {
            let dir = parent_dir(&req.file_path);
            let path = if dir.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", dir, name)
            };
            let suggestion = OrganizationSuggestion {
                id: format!("rename:{}", name),
                kind: OrganizationSuggestionKind::Rename,
                confidence: 0.72,
                rationale: format!("Canonical name under the '{}' scheme.", scheme),
                tag: None,
                category: None,
                target_folder: None,
                new_name: Some(name.clone()),
                source: Some("rule".to_string()),
            };
            (
                Some(path),
                format!("Suggested '{}' under the '{}' naming scheme.", name, scheme),
                Some(suggestion),
            )
        }
        None => (
            None,
            "Note name already follows the configured naming scheme.".to_string(),
            None,
        ),
    };

    let response = RenameSuggestionResponse {
        file_path: req.file_path,
        current_name,
        proposed_name,
        proposed_path,
        naming_scheme: scheme,
        rationale,
        suggestion,
        generated_at: Utc::now(),
    };

    Ok(HttpResponse::Ok().json(response))
}

#[post("/api/vaults/{vault_id}/ml/apply-suggestion")]
async fn apply_suggestion(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    body: web::Json<ApplyOrganizationSuggestionRequest>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let req = body.into_inner();

    if req.file_path.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "file_path is required for suggestion application".to_string(),
        ));
    }

    let vault = state.db.get_vault(&vault_id).await?;
    let mut changes: Vec<ApplyChange> = Vec::new();
    let mut updated_file_path: Option<String> = None;
    let mut receipt_id_out: Option<String> = None;
    let mut updated_links: Option<usize> = None;

    match req.suggestion.kind {
        OrganizationSuggestionKind::Tag => {
            let tag = req.suggestion.tag.as_ref().ok_or(AppError::InvalidInput(
                "tag suggestion requires 'tag'".to_string(),
            ))?;

            let file = FileService::read_file(&vault.path, &req.file_path)?;
            let mut frontmatter = ensure_frontmatter_object(file.frontmatter);
            let normalized_tag = tag.trim().trim_start_matches('#').to_string();
            let changed = add_tag_to_frontmatter(&mut frontmatter, tag);

            if changed {
                changes.push(ApplyChange {
                    kind: "frontmatter_tag".to_string(),
                    description: format!("Add tag '#{}'", normalized_tag),
                });
            } else {
                changes.push(ApplyChange {
                    kind: "noop".to_string(),
                    description: format!("Tag '#{}' already present", normalized_tag),
                });
            }

            if changed && !req.dry_run {
                let updated = FileService::write_file(
                    &vault.path,
                    &req.file_path,
                    &file.content,
                    Some(file.modified),
                    Some(&frontmatter),
                )?;
                let _ = state
                    .search_index
                    .update_file(&vault_id, &req.file_path, updated.content);

                let rid = Uuid::new_v4().to_string();
                let receipt = MlUndoReceipt {
                    receipt_id: rid.clone(),
                    vault_id: vault_id.clone(),
                    file_path: req.file_path.clone(),
                    description: format!("Add tag '#{}'", normalized_tag),
                    reverse_action: ReverseAction::RemoveTag {
                        tag: normalized_tag,
                    },
                    applied_at: Utc::now(),
                    group_id: None,
                };
                state.db.save_ml_undo_receipt(&receipt).await?;
                receipt_id_out = Some(rid);
            }
        }
        OrganizationSuggestionKind::Category => {
            let category = req
                .suggestion
                .category
                .as_ref()
                .ok_or(AppError::InvalidInput(
                    "category suggestion requires 'category'".to_string(),
                ))?;

            let file = FileService::read_file(&vault.path, &req.file_path)?;
            let mut frontmatter = ensure_frontmatter_object(file.frontmatter);
            let previous_category = if let Value::Object(ref obj) = frontmatter {
                obj.get("category")
                    .and_then(Value::as_str)
                    .map(String::from)
            } else {
                None
            };
            let changed = set_category_frontmatter(&mut frontmatter, category);

            if changed {
                changes.push(ApplyChange {
                    kind: "frontmatter_category".to_string(),
                    description: format!("Set category to '{}'", category.trim()),
                });
            } else {
                changes.push(ApplyChange {
                    kind: "noop".to_string(),
                    description: format!("Category already set to '{}'", category.trim()),
                });
            }

            if changed && !req.dry_run {
                let updated = FileService::write_file(
                    &vault.path,
                    &req.file_path,
                    &file.content,
                    Some(file.modified),
                    Some(&frontmatter),
                )?;
                let _ = state
                    .search_index
                    .update_file(&vault_id, &req.file_path, updated.content);

                let rid = Uuid::new_v4().to_string();
                let receipt = MlUndoReceipt {
                    receipt_id: rid.clone(),
                    vault_id: vault_id.clone(),
                    file_path: req.file_path.clone(),
                    description: format!("Set category to '{}'", category.trim()),
                    reverse_action: ReverseAction::RestoreCategory {
                        previous_value: previous_category,
                    },
                    applied_at: Utc::now(),
                    group_id: None,
                };
                state.db.save_ml_undo_receipt(&receipt).await?;
                receipt_id_out = Some(rid);
            }
        }
        OrganizationSuggestionKind::MoveToFolder => {
            let target_folder =
                req.suggestion
                    .target_folder
                    .as_ref()
                    .ok_or(AppError::InvalidInput(
                        "move_to_folder suggestion requires 'target_folder'".to_string(),
                    ))?;

            let normalized_folder = normalize_target_folder(target_folder)?;
            let filename = Path::new(&req.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(AppError::InvalidInput(
                    "file_path must include a valid filename".to_string(),
                ))?;

            let proposed_path = if normalized_folder.is_empty() {
                filename.to_string()
            } else {
                format!("{}/{}", normalized_folder, filename)
            };

            if proposed_path == req.file_path {
                changes.push(ApplyChange {
                    kind: "noop".to_string(),
                    description: "File already in suggested folder".to_string(),
                });
            } else {
                changes.push(ApplyChange {
                    kind: "file_move".to_string(),
                    description: format!("Move file to '{}'", normalized_folder),
                });

                if req.dry_run {
                    updated_file_path = Some(proposed_path);
                } else {
                    let original_path = req.file_path.clone();
                    let final_path = FileService::rename(
                        &vault.path,
                        &req.file_path,
                        &proposed_path,
                        RenameStrategy::Fail,
                    )?;
                    updated_file_path = Some(final_path.clone());

                    let _ = state.search_index.remove_file(&vault_id, &original_path);
                    if let Ok(updated_file) = FileService::read_file(&vault.path, &final_path) {
                        let _ = state.search_index.update_file(
                            &vault_id,
                            &final_path,
                            updated_file.content,
                        );
                    }

                    let rid = Uuid::new_v4().to_string();
                    let receipt = MlUndoReceipt {
                        receipt_id: rid.clone(),
                        vault_id: vault_id.clone(),
                        file_path: original_path.clone(),
                        description: format!("Move file to '{}'", normalized_folder),
                        reverse_action: ReverseAction::MoveBack {
                            from_path: original_path,
                            to_path: final_path,
                        },
                        applied_at: Utc::now(),
                        group_id: None,
                    };
                    state.db.save_ml_undo_receipt(&receipt).await?;
                    receipt_id_out = Some(rid);
                }
            }
        }
        OrganizationSuggestionKind::Rename => {
            let new_name = req.suggestion.new_name.as_ref().ok_or(AppError::InvalidInput(
                "rename suggestion requires 'new_name'".to_string(),
            ))?;
            validate_bare_filename(new_name)?;

            let dir = parent_dir(&req.file_path);
            let proposed_path = if dir.is_empty() {
                new_name.clone()
            } else {
                format!("{}/{}", dir, new_name)
            };

            let old_stem = path_stem(&req.file_path).ok_or(AppError::InvalidInput(
                "file_path must include a valid filename".to_string(),
            ))?;
            let new_stem = path_stem(new_name).ok_or(AppError::InvalidInput(
                "new_name must include a valid filename".to_string(),
            ))?;

            if proposed_path == req.file_path {
                changes.push(ApplyChange {
                    kind: "noop".to_string(),
                    description: "File already has the suggested name".to_string(),
                });
            } else if req.dry_run {
                // Preview: count inbound links that would be rewritten.
                let link_changes = compute_rename_link_changes(
                    &vault.path,
                    &req.file_path,
                    &proposed_path,
                    &old_stem,
                    &new_stem,
                    &dir,
                )?;
                updated_links = Some(link_changes.len());
                updated_file_path = Some(proposed_path);
                changes.push(ApplyChange {
                    kind: "file_rename".to_string(),
                    description: format!(
                        "Rename to '{}' ({} note(s) link here)",
                        new_name,
                        link_changes.len()
                    ),
                });
            } else {
                let original_path = req.file_path.clone();

                // Rename the file first (the operation that can fail), then
                // rewrite inbound links to the new stem.
                let final_path = FileService::rename(
                    &vault.path,
                    &original_path,
                    &proposed_path,
                    RenameStrategy::Fail,
                )?;

                let link_changes = compute_rename_link_changes(
                    &vault.path,
                    &original_path,
                    &final_path,
                    &old_stem,
                    &new_stem,
                    &dir,
                )?;

                let mut link_files: Vec<String> = Vec::with_capacity(link_changes.len());
                for (rel, frontmatter, new_body) in &link_changes {
                    FileService::write_file(
                        &vault.path,
                        rel,
                        new_body,
                        None,
                        frontmatter.as_ref(),
                    )?;
                    let _ = state
                        .search_index
                        .update_file(&vault_id, rel, new_body.clone());
                    link_files.push(rel.clone());
                }

                // Update the search index for the renamed file itself.
                let _ = state.search_index.remove_file(&vault_id, &original_path);
                if let Ok(updated_file) = FileService::read_file(&vault.path, &final_path) {
                    let _ = state.search_index.update_file(
                        &vault_id,
                        &final_path,
                        updated_file.content,
                    );
                }

                updated_file_path = Some(final_path.clone());
                updated_links = Some(link_files.len());
                changes.push(ApplyChange {
                    kind: "file_rename".to_string(),
                    description: format!(
                        "Renamed to '{}'; rewrote links in {} note(s)",
                        new_name,
                        link_files.len()
                    ),
                });

                let rid = Uuid::new_v4().to_string();
                let receipt = MlUndoReceipt {
                    receipt_id: rid.clone(),
                    vault_id: vault_id.clone(),
                    file_path: original_path.clone(),
                    description: format!("Rename to '{}'", new_name),
                    reverse_action: ReverseAction::RenameWithLinks {
                        from_path: original_path,
                        to_path: final_path,
                        old_stem,
                        new_stem,
                        link_files,
                    },
                    applied_at: Utc::now(),
                    group_id: None,
                };
                state.db.save_ml_undo_receipt(&receipt).await?;
                receipt_id_out = Some(rid);
            }
        }
    }

    let has_effective_change = changes.iter().any(|c| c.kind != "noop");

    let response = ApplyOrganizationSuggestionResponse {
        file_path: req.file_path,
        applied: !req.dry_run && has_effective_change,
        dry_run: req.dry_run,
        updated_file_path,
        changes,
        applied_at: Utc::now(),
        receipt_id: receipt_id_out,
        updated_links,
    };

    Ok(HttpResponse::Ok().json(response))
}

fn ensure_frontmatter_object(frontmatter: Option<Value>) -> Value {
    match frontmatter {
        Some(Value::Object(map)) => Value::Object(map),
        _ => Value::Object(Map::new()),
    }
}

fn add_tag_to_frontmatter(frontmatter: &mut Value, tag: &str) -> bool {
    let normalized = tag.trim().trim_start_matches('#');
    if normalized.is_empty() {
        return false;
    }

    let obj = match frontmatter {
        Value::Object(obj) => obj,
        _ => return false,
    };

    match obj.get_mut("tags") {
        None => {
            obj.insert(
                "tags".to_string(),
                Value::Array(vec![Value::String(normalized.to_string())]),
            );
            true
        }
        Some(Value::String(existing)) => {
            if existing.eq_ignore_ascii_case(normalized) {
                false
            } else {
                let current = existing.clone();
                obj.insert(
                    "tags".to_string(),
                    Value::Array(vec![
                        Value::String(current),
                        Value::String(normalized.to_string()),
                    ]),
                );
                true
            }
        }
        Some(Value::Array(arr)) => {
            let exists = arr
                .iter()
                .filter_map(Value::as_str)
                .any(|v| v.eq_ignore_ascii_case(normalized));
            if exists {
                false
            } else {
                arr.push(Value::String(normalized.to_string()));
                true
            }
        }
        Some(_) => {
            obj.insert(
                "tags".to_string(),
                Value::Array(vec![Value::String(normalized.to_string())]),
            );
            true
        }
    }
}

fn set_category_frontmatter(frontmatter: &mut Value, category: &str) -> bool {
    let normalized = category.trim();
    if normalized.is_empty() {
        return false;
    }

    let obj = match frontmatter {
        Value::Object(obj) => obj,
        _ => return false,
    };

    let changed = !obj
        .get("category")
        .and_then(Value::as_str)
        .map(|v| v.eq_ignore_ascii_case(normalized))
        .unwrap_or(false);

    if changed {
        obj.insert(
            "category".to_string(),
            Value::String(normalized.to_string()),
        );
    }

    changed
}

fn normalize_target_folder(target_folder: &str) -> AppResult<String> {
    let normalized = target_folder
        .replace('\\', "/")
        .trim()
        .trim_matches('/')
        .to_string();

    if normalized.is_empty() {
        return Ok(String::new());
    }

    let path = Path::new(&normalized);
    for component in path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::InvalidInput(
                    "target_folder contains invalid path traversal components".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(normalized)
}

/// Vault-relative parent directory of a file path (forward slashes, no leading/
/// trailing separators). Empty string for a file at the vault root.
fn parent_dir(file_path: &str) -> String {
    Path::new(file_path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
        .trim_matches('/')
        .to_string()
}

/// Validate a proposed rename target is a bare filename (no directory parts,
/// no traversal). Renames keep the note in its current folder.
fn validate_bare_filename(name: &str) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(AppError::InvalidInput(
            "new_name must be a bare filename without path separators".to_string(),
        ));
    }
    Ok(())
}

/// Compute the inbound wiki-link rewrites a rename would cause. Returns, per
/// affected note, its `(rel_path, frontmatter, rewritten_body)`. The renamed
/// file itself (old and new paths) is skipped.
fn compute_rename_link_changes(
    vault_path: &str,
    old_path: &str,
    new_path: &str,
    old_stem: &str,
    new_stem: &str,
    old_dir: &str,
) -> AppResult<Vec<(String, Option<Value>, String)>> {
    let mut changes = Vec::new();
    for (rel, raw) in FileService::list_markdown_files(vault_path)? {
        let rel = rel.trim_start_matches('/').to_string();
        if rel == old_path || rel == new_path {
            continue;
        }
        let (frontmatter, body) = frontmatter_service::parse_frontmatter(&raw)?;
        let (new_body, n) = rewrite_wiki_links(&body, old_stem, new_stem, old_dir);
        if n > 0 {
            changes.push((rel, frontmatter, new_body));
        }
    }
    Ok(changes)
}

/// File stem (no extension) of a vault-relative path.
fn path_stem(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

/// LIB-062: produce a folder-placement `move_to_folder` suggestion for a note,
/// preferring Tier-2 embedding kNN, then Tier-1 TF-IDF nearest folder. Returns
/// `None` when no confident, non-current folder is found (callers keep the Tier-1
/// heuristic). The returned suggestion carries a `semantic`/`tfidf` source.
async fn suggest_folder_placement(
    db: &crate::db::Database,
    ml: &crate::config::MlConfig,
    vault_id: &str,
    vault_path: &str,
    file_path: &str,
    content: &str,
) -> AppResult<Option<OrganizationSuggestion>> {
    // Tier 2: embedding kNN over the vault's cached note vectors.
    if let Some((folder, confidence)) =
        embedding_service::suggest_folder(db, ml, vault_id, file_path, content).await?
    {
        return Ok(Some(folder_move_suggestion(folder, confidence, "semantic")));
    }

    // Tier 1: TF-IDF nearest folder over the vault's notes (skip under the
    // bare `heuristic` tier, which keeps the string-match fallback).
    if ml.tier == crate::config::MlTier::Heuristic {
        return Ok(None);
    }

    let current = parent_dir(file_path);
    let notes: Vec<(String, String)> = FileService::list_markdown_files(vault_path)?
        .into_iter()
        .filter(|(rel, _)| rel.trim_start_matches('/') != file_path)
        .map(|(rel, raw)| (parent_dir(rel.trim_start_matches('/')), raw))
        .collect();

    if let Some((folder, score)) = MlService::nearest_folder_tfidf(content, &notes, ml.min_confidence) {
        if folder != current {
            return Ok(Some(folder_move_suggestion(folder, score, "tfidf")));
        }
    }
    Ok(None)
}

fn folder_move_suggestion(folder: String, confidence: f32, source: &str) -> OrganizationSuggestion {
    OrganizationSuggestion {
        id: format!("move:{}", folder),
        kind: OrganizationSuggestionKind::MoveToFolder,
        confidence: confidence.clamp(0.0, 1.0),
        rationale: "Similar notes live in this folder.".to_string(),
        tag: None,
        category: None,
        target_folder: Some(folder),
        new_name: None,
        source: Some(source.to_string()),
    }
}

fn remove_tag_from_frontmatter(frontmatter: &mut Value, tag: &str) {
    let normalized = tag.trim().trim_start_matches('#');
    if normalized.is_empty() {
        return;
    }

    let obj = match frontmatter {
        Value::Object(obj) => obj,
        _ => return,
    };

    match obj.get_mut("tags") {
        Some(Value::Array(arr)) => {
            arr.retain(|v| {
                !v.as_str()
                    .map(|s| s.eq_ignore_ascii_case(normalized))
                    .unwrap_or(false)
            });
            if arr.is_empty() {
                obj.remove("tags");
            }
        }
        Some(Value::String(existing)) if existing.eq_ignore_ascii_case(normalized) => {
            obj.remove("tags");
        }
        _ => {}
    }
}

#[post("/api/vaults/{vault_id}/ml/undo")]
async fn undo_ml_action(
    state: web::Data<AppState>,
    vault_id: web::Path<String>,
    body: web::Json<Value>,
) -> AppResult<HttpResponse> {
    let vault_id = vault_id.into_inner();
    let receipt_id = body
        .get("receipt_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput("receipt_id is required".to_string()))?
        .to_string();

    let receipt = state
        .db
        .consume_ml_undo_receipt(&vault_id, &receipt_id)
        .await?;

    let vault = state.db.get_vault(&vault_id).await?;

    match &receipt.reverse_action {
        ReverseAction::RemoveTag { tag } => {
            let file = FileService::read_file(&vault.path, &receipt.file_path)?;
            let mut frontmatter = ensure_frontmatter_object(file.frontmatter);
            remove_tag_from_frontmatter(&mut frontmatter, tag);
            let updated = FileService::write_file(
                &vault.path,
                &receipt.file_path,
                &file.content,
                Some(file.modified),
                Some(&frontmatter),
            )?;
            let _ = state
                .search_index
                .update_file(&vault_id, &receipt.file_path, updated.content);
        }
        ReverseAction::RestoreCategory { previous_value } => {
            let file = FileService::read_file(&vault.path, &receipt.file_path)?;
            let mut frontmatter = ensure_frontmatter_object(file.frontmatter);
            if let Value::Object(ref mut obj) = frontmatter {
                match previous_value {
                    None => {
                        obj.remove("category");
                    }
                    Some(prev) => {
                        obj.insert("category".to_string(), Value::String(prev.clone()));
                    }
                }
            }
            let updated = FileService::write_file(
                &vault.path,
                &receipt.file_path,
                &file.content,
                Some(file.modified),
                Some(&frontmatter),
            )?;
            let _ = state
                .search_index
                .update_file(&vault_id, &receipt.file_path, updated.content);
        }
        ReverseAction::MoveBack { from_path, to_path } => {
            let final_path =
                FileService::rename(&vault.path, to_path, from_path, RenameStrategy::Fail)?;
            let _ = state.search_index.remove_file(&vault_id, to_path);
            if let Ok(f) = FileService::read_file(&vault.path, &final_path) {
                let _ = state
                    .search_index
                    .update_file(&vault_id, &final_path, f.content);
            }
        }
        ReverseAction::RenameWithLinks {
            from_path,
            to_path,
            old_stem,
            new_stem,
            link_files,
        } => {
            // Move the note back to its original name.
            let final_path =
                FileService::rename(&vault.path, to_path, from_path, RenameStrategy::Fail)?;
            let _ = state.search_index.remove_file(&vault_id, to_path);
            if let Ok(f) = FileService::read_file(&vault.path, &final_path) {
                let _ = state
                    .search_index
                    .update_file(&vault_id, &final_path, f.content);
            }

            // Restore the rewritten inbound links (new_stem -> old_stem) in the
            // exact files the apply touched. The directory is unchanged by a
            // rename, so it is the same for both stems.
            let dir = parent_dir(from_path);
            for rel in link_files {
                let file = match FileService::read_file(&vault.path, rel) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let (restored_body, n) =
                    rewrite_wiki_links(&file.content, new_stem, old_stem, &dir);
                if n == 0 {
                    continue;
                }
                FileService::write_file(
                    &vault.path,
                    rel,
                    &restored_body,
                    None,
                    file.frontmatter.as_ref(),
                )?;
                let _ = state
                    .search_index
                    .update_file(&vault_id, rel, restored_body);
            }
        }
    }

    let response = UndoMlActionResponse {
        receipt_id,
        undone: true,
        description: format!("Undone: {}", receipt.description),
        file_path: receipt.file_path,
        undone_count: 1,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(generate_outline)
        .service(analyze_note)
        .service(generate_suggestions)
        .service(rename_suggestion)
        .service(apply_suggestion)
        .service(undo_ml_action);
}
