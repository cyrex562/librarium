//! LIB-066: offline-mode guarantee for the ML/organization feature.
//!
//! Mirrors the air-gap posture asserted for the auth stack (LIB-043): with
//! `tier = "embeddings"` but `allow_model_download = false`, the server performs
//! **no** network I/O for ML. The embedder is never constructed (the default
//! build omits the ONNX backend entirely, and even with the feature it refuses
//! to fetch a model), so every ML request transparently falls back to Tier 1.
//!
//! This test runs in the default build (no `embeddings` feature), where the
//! embedder is always `None` — the strongest possible "no network" guarantee.

use actix_web::{test, web, App};
use librarium::config::{AppConfig, MlTier};
use librarium::db::Database;
use librarium::routes::{ml, AppState};
use librarium::services::{embedding_service, MarkdownParser, SearchIndex};
use librarium::watcher::FileWatcher;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

fn offline_embeddings_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.ml.enabled = true;
    config.ml.tier = MlTier::Embeddings;
    config.ml.allow_model_download = false;
    // Point at a directory that does not exist, so even a feature-built server
    // would have nothing to load and could not phone home.
    config.ml.cache_dir = "/nonexistent/librarium-air-gapped-models".to_string();
    config
}

#[actix_web::test]
async fn embeddings_tier_without_download_falls_back_to_tier1_offline() {
    let config = offline_embeddings_config();

    // Priming the embedder with downloads disabled must NOT construct a model
    // and must NOT touch the network — it returns None and we fall back.
    let primed = embedding_service::embedder(&config.ml);
    assert!(
        primed.is_none(),
        "embedder must be unavailable offline (no model, downloads disabled)"
    );
    assert!(embedding_service::embedder_if_ready().is_none());

    // The ML endpoints must still work, using Tier-1 (classical) only.
    let temp_dir = TempDir::new().unwrap();
    let db = Database::new(&format!(
        "sqlite://{}",
        temp_dir.path().join("offline.db").display()
    ))
    .await
    .unwrap();

    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir_all(&vault_path).unwrap();
    std::fs::write(
        vault_path.join("note.md"),
        "# Sprint Meeting\n\nAgenda: discuss project roadmap and action items / todo list\n",
    )
    .unwrap();

    let vault = db
        .create_vault(
            "Offline Vault".to_string(),
            vault_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index: SearchIndex::new(),
        watcher: Arc::new(Mutex::new(FileWatcher::new().unwrap().0)),
        event_broadcaster: broadcast::channel(100).0,
        ws_broadcaster: tokio::sync::broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });
    let config_data = web::Data::new(config);

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .app_data(config_data.clone())
            .configure(ml::configure),
    )
    .await;

    // Suggestions succeed and contain no `semantic`-sourced entries (Tier 2),
    // proving the fallback without any model.
    let req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/suggestions", vault.id))
        .set_json(json!({ "file_path": "note.md" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    let suggestions = body["suggestions"].as_array().unwrap();
    assert!(!suggestions.is_empty(), "Tier-1 should still produce suggestions");
    for s in suggestions {
        assert_ne!(
            s["source"].as_str(),
            Some("semantic"),
            "no semantic suggestions are possible offline"
        );
    }

    // The vault-wide organize plan also works offline (Tier-1 placement, zero
    // clusters since there are no embeddings).
    let organize = test::TestRequest::post()
        .uri(&format!("/api/vaults/{}/ml/organize-vault", vault.id))
        .set_json(json!({}))
        .to_request();
    let organize_resp = test::call_service(&app, organize).await;
    assert!(organize_resp.status().is_success());
    let plan: serde_json::Value = test::read_body_json(organize_resp).await;
    assert_eq!(plan["cluster_count"], 0);
    assert_eq!(plan["rows"].as_array().unwrap().len(), 1);
}
