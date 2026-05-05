use crate::error::{AppError, AppResult};
use crate::models::{
    ApplyChange, ApplyOrganizationSuggestionRequest, ApplyOrganizationSuggestionResponse,
    GenerateOrganizationSuggestionsRequest, GenerateOutlineRequest, MlUndoReceipt,
    OrganizationSuggestionKind, ReverseAction, UndoMlActionResponse,
};
use crate::routes::vaults::AppState;
use crate::services::{frontmatter_service, FileService, MlService, RenameStrategy};
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

#[post("/api/vaults/{vault_id}/ml/suggestions")]
async fn generate_suggestions(
    state: web::Data<AppState>,
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

    let suggestions = MlService::suggest_organization(
        &req.file_path,
        &content,
        frontmatter.as_ref(),
        max_suggestions,
    );

    Ok(HttpResponse::Ok().json(suggestions))
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
                    };
                    state.db.save_ml_undo_receipt(&receipt).await?;
                    receipt_id_out = Some(rid);
                }
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
    }

    let response = UndoMlActionResponse {
        receipt_id,
        undone: true,
        description: format!("Undone: {}", receipt.description),
        file_path: receipt.file_path,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(generate_outline)
        .service(generate_suggestions)
        .service(apply_suggestion)
        .service(undo_ml_action);
}
