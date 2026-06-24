use actix_web::{test, web, App};
use librarium::db::Database;
use librarium::routes::{ml, AppState};
use librarium::services::{MarkdownParser, SearchIndex};
use librarium::watcher::FileWatcher;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

#[actix_web::test]
async fn apply_tag_and_undo_restores_file_and_receipt_is_single_use() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("ml-tag-undo.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir_all(&vault_path).unwrap();

    let note_rel = "note.md";
    let note_abs = vault_path.join(note_rel);
    std::fs::write(&note_abs, "# Demo\n\nhello\n").unwrap();

    let vault = db
        .create_vault(
            "ML Test Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let app = test::init_service(App::new().app_data(state.clone()).configure(ml::configure)).await;

    let apply_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/apply-suggestion", vault.id))
        .set_json(json!({
            "file_path": note_rel,
            "dry_run": false,
            "suggestion": {
                "id": "s-tag-1",
                "kind": "tag",
                "confidence": 0.95,
                "rationale": "Looks like project work",
                "tag": "project"
            }
        }))
        .to_request();

    let apply_resp = test::call_service(&app, apply_req).await;
    assert!(apply_resp.status().is_success());
    let apply_body: serde_json::Value = test::read_body_json(apply_resp).await;
    assert_eq!(apply_body["applied"], true);
    let receipt_id = apply_body["receipt_id"].as_str().unwrap().to_string();

    let content_after_apply = std::fs::read_to_string(&note_abs).unwrap();
    assert!(content_after_apply.contains("project"));

    let undo_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "receipt_id": receipt_id }))
        .to_request();

    let undo_resp = test::call_service(&app, undo_req).await;
    assert!(undo_resp.status().is_success());
    let undo_body: serde_json::Value = test::read_body_json(undo_resp).await;
    assert_eq!(undo_body["undone"], true);

    let content_after_undo = std::fs::read_to_string(&note_abs).unwrap();
    assert!(!content_after_undo.contains("project"));

    let undo_again_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "receipt_id": undo_body["receipt_id"] }))
        .to_request();

    let undo_again_resp = test::call_service(&app, undo_again_req).await;
    assert_eq!(undo_again_resp.status().as_u16(), 404);
}

#[actix_web::test]
async fn apply_move_and_undo_restores_original_path() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("ml-move-undo.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let vault_path = temp_dir.path().join("vault");
    let inbox_dir = vault_path.join("inbox");
    let archive_dir = vault_path.join("archive");
    std::fs::create_dir_all(&inbox_dir).unwrap();
    std::fs::create_dir_all(&archive_dir).unwrap();

    let from_rel = "inbox/task.md";
    let from_abs = vault_path.join(from_rel);
    let to_rel = "archive/task.md";
    let to_abs = vault_path.join(to_rel);
    std::fs::write(&from_abs, "# Task\n\nmove me\n").unwrap();

    let vault = db
        .create_vault(
            "ML Move Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let app = test::init_service(App::new().app_data(state.clone()).configure(ml::configure)).await;

    let apply_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/apply-suggestion", vault.id))
        .set_json(json!({
            "file_path": from_rel,
            "dry_run": false,
            "suggestion": {
                "id": "s-move-1",
                "kind": "move_to_folder",
                "confidence": 0.91,
                "rationale": "Archive completed notes",
                "target_folder": "archive"
            }
        }))
        .to_request();

    let apply_resp = test::call_service(&app, apply_req).await;
    assert!(apply_resp.status().is_success());
    let apply_body: serde_json::Value = test::read_body_json(apply_resp).await;
    assert_eq!(apply_body["updated_file_path"], to_rel);
    let receipt_id = apply_body["receipt_id"].as_str().unwrap().to_string();

    assert!(!from_abs.exists());
    assert!(to_abs.exists());

    let undo_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "receipt_id": receipt_id }))
        .to_request();

    let undo_resp = test::call_service(&app, undo_req).await;
    assert!(undo_resp.status().is_success());

    assert!(from_abs.exists());
    assert!(!to_abs.exists());
}

#[actix_web::test]
async fn undo_receipt_persists_across_app_reinitialization() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("ml-persisted-receipt.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir_all(&vault_path).unwrap();

    let note_rel = "persist.md";
    let note_abs = vault_path.join(note_rel);
    std::fs::write(&note_abs, "# Persist\n\nhello\n").unwrap();

    let vault = db
        .create_vault(
            "ML Persisted Receipt Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let receipt_id = {
        let search_index = SearchIndex::new();
        let (watcher, _) = FileWatcher::new().unwrap();
        let watcher = Arc::new(Mutex::new(watcher));
        let (event_tx, _) = broadcast::channel(100);

        let state = web::Data::new(AppState {
            db: db.clone(),
            search_index,
            watcher,
            event_broadcaster: event_tx,
            ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
            change_log_retention_days: 7,
            ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
            document_parser: Arc::new(MarkdownParser),
            entity_type_registry: librarium::services::EntityTypeRegistry::new(),
            relation_type_registry: librarium::services::RelationTypeRegistry::new(),
            plugins_dir: std::path::PathBuf::new(),
        });

        let app =
            test::init_service(App::new().app_data(state.clone()).configure(ml::configure)).await;

        let apply_req = test::TestRequest::post()
            .uri(&format!("/api/vaults/{}/ml/apply-suggestion", vault.id))
            .set_json(json!({
                "file_path": note_rel,
                "dry_run": false,
                "suggestion": {
                    "id": "s-tag-persist-1",
                    "kind": "tag",
                    "confidence": 0.9,
                    "rationale": "Persisted receipt check",
                    "tag": "persisted"
                }
            }))
            .to_request();

        let apply_resp = test::call_service(&app, apply_req).await;
        assert!(apply_resp.status().is_success());
        let apply_body: serde_json::Value = test::read_body_json(apply_resp).await;
        apply_body["receipt_id"].as_str().unwrap().to_string()
    };

    let content_after_apply = std::fs::read_to_string(&note_abs).unwrap();
    assert!(content_after_apply.contains("persisted"));

    {
        let search_index = SearchIndex::new();
        let (watcher, _) = FileWatcher::new().unwrap();
        let watcher = Arc::new(Mutex::new(watcher));
        let (event_tx, _) = broadcast::channel(100);

        let state = web::Data::new(AppState {
            db: db.clone(),
            search_index,
            watcher,
            event_broadcaster: event_tx,
            ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
            change_log_retention_days: 7,
            ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
            document_parser: Arc::new(MarkdownParser),
            entity_type_registry: librarium::services::EntityTypeRegistry::new(),
            relation_type_registry: librarium::services::RelationTypeRegistry::new(),
            plugins_dir: std::path::PathBuf::new(),
        });

        let app =
            test::init_service(App::new().app_data(state.clone()).configure(ml::configure)).await;

        let undo_req = test::TestRequest::post()
            .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
            .set_json(json!({ "receipt_id": receipt_id }))
            .to_request();

        let undo_resp = test::call_service(&app, undo_req).await;
        assert!(undo_resp.status().is_success());
    }

    let content_after_undo = std::fs::read_to_string(&note_abs).unwrap();
    assert!(!content_after_undo.contains("persisted"));
}

#[actix_web::test]
async fn organize_vault_then_apply_plan_and_bulk_undo() {
    use librarium::config::AppConfig;

    let temp_dir = TempDir::new().unwrap();
    let db = Database::new(&format!(
        "sqlite://{}",
        temp_dir.path().join("ml-organize.db").display()
    ))
    .await
    .unwrap();

    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir_all(vault_path.join("projects")).unwrap();
    std::fs::create_dir_all(vault_path.join("inbox")).unwrap();
    std::fs::write(
        vault_path.join("projects/alpha.md"),
        "# Alpha\n\nrust project roadmap milestone deliverable\n",
    )
    .unwrap();
    std::fs::write(
        vault_path.join("projects/beta.md"),
        "# Beta\n\nrust project sprint planning milestone\n",
    )
    .unwrap();
    let stray = vault_path.join("inbox/stray.md");
    std::fs::write(&stray, "# Stray\n\nrust project roadmap notes\n").unwrap();

    let vault = db
        .create_vault(
            "Organize Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });
    let config = web::Data::new(AppConfig::default());

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .app_data(config.clone())
            .configure(ml::configure),
    )
    .await;

    // 1. Compute a plan for the whole vault.
    let organize_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/organize-vault", vault.id))
        .set_json(json!({}))
        .to_request();
    let organize_resp = test::call_service(&app, organize_req).await;
    assert!(organize_resp.status().is_success());
    let plan: serde_json::Value = test::read_body_json(organize_resp).await;
    assert_eq!(plan["rows"].as_array().unwrap().len(), 3);

    // 2. Apply a batch: tag two notes and move the stray one into projects.
    let apply_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/apply-plan", vault.id))
        .set_json(json!({
            "dry_run": false,
            "rows": [
                { "file_path": "inbox/stray.md", "apply_tags": ["triaged"], "apply_folder": "projects" },
                { "file_path": "projects/alpha.md", "apply_tags": ["reviewed"] }
            ]
        }))
        .to_request();
    let apply_resp = test::call_service(&app, apply_req).await;
    assert!(apply_resp.status().is_success());
    let apply_body: serde_json::Value = test::read_body_json(apply_resp).await;
    assert_eq!(apply_body["applied"], true);
    let group_id = apply_body["group_id"].as_str().unwrap().to_string();

    // The stray note moved and both notes gained their tags.
    assert!(!stray.exists());
    let moved = vault_path.join("projects/stray.md");
    assert!(moved.exists());
    assert!(std::fs::read_to_string(&moved).unwrap().contains("triaged"));
    assert!(std::fs::read_to_string(vault_path.join("projects/alpha.md"))
        .unwrap()
        .contains("reviewed"));

    // 3. Bulk-undo the whole group.
    let undo_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "group_id": group_id }))
        .to_request();
    let undo_resp = test::call_service(&app, undo_req).await;
    assert!(undo_resp.status().is_success());
    let undo_body: serde_json::Value = test::read_body_json(undo_resp).await;
    assert_eq!(undo_body["undone"], true);
    assert!(undo_body["undone_count"].as_u64().unwrap() >= 3);

    // Everything is back: stray returned to inbox, tags removed.
    assert!(stray.exists());
    assert!(!moved.exists());
    assert!(!std::fs::read_to_string(&stray).unwrap().contains("triaged"));
    assert!(!std::fs::read_to_string(vault_path.join("projects/alpha.md"))
        .unwrap()
        .contains("reviewed"));

    // The group is single-use.
    let undo_again = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "group_id": group_id }))
        .to_request();
    let undo_again_resp = test::call_service(&app, undo_again).await;
    assert_eq!(undo_again_resp.status().as_u16(), 404);
}

#[actix_web::test]
async fn apply_rename_rewrites_inbound_links_and_undo_restores() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("ml-rename-undo.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir_all(&vault_path).unwrap();

    // The note to rename, plus two notes linking to it (bare + path-qualified).
    let target_rel = "target.md";
    let target_abs = vault_path.join(target_rel);
    std::fs::write(&target_abs, "# Target\n\nbody\n").unwrap();

    let linker_abs = vault_path.join("linker.md");
    std::fs::write(
        &linker_abs,
        "See [[target]] and [[target#Heading|alias]] here.\n",
    )
    .unwrap();

    std::fs::create_dir_all(vault_path.join("sub")).unwrap();
    let other_abs = vault_path.join("sub/other.md");
    std::fs::write(&other_abs, "Refers to [[target]] from a subfolder.\n").unwrap();

    let vault = db
        .create_vault(
            "ML Rename Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let app = test::init_service(App::new().app_data(state.clone()).configure(ml::configure)).await;

    let rename_suggestion = json!({
        "id": "s-rename-1",
        "kind": "rename",
        "confidence": 0.72,
        "rationale": "Canonical name",
        "new_name": "renamed.md"
    });

    // Dry run: reports the inbound-link count without mutating anything.
    let dry_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/apply-suggestion", vault.id))
        .set_json(json!({
            "file_path": target_rel,
            "dry_run": true,
            "suggestion": rename_suggestion,
        }))
        .to_request();
    let dry_resp = test::call_service(&app, dry_req).await;
    assert!(dry_resp.status().is_success());
    let dry_body: serde_json::Value = test::read_body_json(dry_resp).await;
    assert_eq!(dry_body["applied"], false);
    assert_eq!(dry_body["updated_links"], 2);
    assert!(target_abs.exists());

    // Apply for real.
    let apply_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/apply-suggestion", vault.id))
        .set_json(json!({
            "file_path": target_rel,
            "dry_run": false,
            "suggestion": rename_suggestion,
        }))
        .to_request();
    let apply_resp = test::call_service(&app, apply_req).await;
    assert!(apply_resp.status().is_success());
    let apply_body: serde_json::Value = test::read_body_json(apply_resp).await;
    assert_eq!(apply_body["applied"], true);
    assert_eq!(apply_body["updated_file_path"], "renamed.md");
    assert_eq!(apply_body["updated_links"], 2);
    let receipt_id = apply_body["receipt_id"].as_str().unwrap().to_string();

    assert!(!target_abs.exists());
    assert!(vault_path.join("renamed.md").exists());
    let linker_after = std::fs::read_to_string(&linker_abs).unwrap();
    assert!(linker_after.contains("[[renamed]]"));
    assert!(linker_after.contains("[[renamed#Heading|alias]]"));
    assert!(!linker_after.contains("[[target"));
    let other_after = std::fs::read_to_string(&other_abs).unwrap();
    assert!(other_after.contains("[[renamed]]"));

    // Undo: file moves back and links are restored.
    let undo_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/undo", vault.id))
        .set_json(json!({ "receipt_id": receipt_id }))
        .to_request();
    let undo_resp = test::call_service(&app, undo_req).await;
    assert!(undo_resp.status().is_success());

    assert!(target_abs.exists());
    assert!(!vault_path.join("renamed.md").exists());
    let linker_restored = std::fs::read_to_string(&linker_abs).unwrap();
    assert!(linker_restored.contains("[[target]]"));
    assert!(linker_restored.contains("[[target#Heading|alias]]"));
    assert!(!linker_restored.contains("renamed"));
    let other_restored = std::fs::read_to_string(&other_abs).unwrap();
    assert!(other_restored.contains("[[target]]"));
}
