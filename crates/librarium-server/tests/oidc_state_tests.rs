use actix_web::{cookie::Cookie, http::StatusCode, test, web, App};
use librarium::config::AppConfig;
use librarium::db::Database;
use librarium::middleware::AuthMiddleware;
use librarium::routes::{oidc, AppState};
use librarium::services::{MarkdownParser, SearchIndex};
use librarium::watcher::FileWatcher;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

fn test_app_state(db: Database) -> web::Data<AppState> {
    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    web::Data::new(AppState {
        db,
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
    })
}

fn test_config() -> web::Data<AppConfig> {
    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "oidc-state-test-secret".to_string();
    config.auth.oidc_issuer_url = Some("https://example.invalid".to_string());
    config.auth.oidc_client_id = Some("test-client".to_string());
    config.auth.oidc_client_secret = Some("test-secret".to_string());
    config.auth.oidc_redirect_uri =
        Some("http://localhost:8080/api/auth/oidc/callback".to_string());
    web::Data::new(config)
}

#[actix_web::test]
async fn oidc_callback_rejects_missing_state_cookie() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("oidc-state.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(test_app_state(db))
            .app_data(test_config())
            .wrap(AuthMiddleware)
            .configure(oidc::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/auth/oidc/callback?code=test-code&state=expected-state")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body["message"],
        "Authentication required: Missing OIDC state cookie"
    );
}

#[actix_web::test]
async fn oidc_callback_rejects_mismatched_state_cookie() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("oidc-state-mismatch.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(test_app_state(db))
            .app_data(test_config())
            .wrap(AuthMiddleware)
            .configure(oidc::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/auth/oidc/callback?code=test-code&state=returned-state")
        .cookie(Cookie::new("librarium_oidc_state", "expected-state"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body["message"],
        "Authentication required: Invalid OIDC state"
    );
}
