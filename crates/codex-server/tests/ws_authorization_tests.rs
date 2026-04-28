use actix_web::{http::header, web, App};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use awc::ws::Frame;
use codex::config::AppConfig;
use codex::db::Database;
use codex::middleware::AuthMiddleware;
use codex::models::{VaultRole, WsMessage};
use codex::routes::{auth, ws, AppState};
use codex::services::{MarkdownParser, SearchIndex};
use codex::watcher::FileWatcher;
use futures::StreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

fn password_hash(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

async fn login_token(
    client: &awc::Client,
    login_url: &str,
    username: &str,
    password: &str,
) -> String {
    let mut response = client
        .post(login_url)
        .send_json(&json!({ "username": username, "password": password }))
        .await
        .unwrap();
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(
        status.is_success(),
        "login failed for {username}: {status} {body}"
    );
    body["access_token"].as_str().unwrap().to_string()
}

#[actix_web::test]
async fn websocket_reindex_complete_is_filtered_by_vault_access() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("ws-auth.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    db.bootstrap_admin_if_empty(Some("admin"), Some("hunter2"))
        .await
        .unwrap();
    db.create_user("alice", &password_hash("password123"))
        .await
        .unwrap();
    db.create_user("bob", &password_hash("password123"))
        .await
        .unwrap();

    let vault_dir = temp_dir.path().join("shared-vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    let vault = db
        .create_vault(
            "Shared Vault".to_string(),
            vault_dir.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

    let alice_id = db
        .get_user_by_username("alice")
        .await
        .unwrap()
        .map(|(id, _)| id)
        .unwrap();
    db.share_vault_with_user(&vault.id, &alice_id, &VaultRole::Viewer)
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);
    let (ws_tx, _) = tokio::sync::broadcast::channel::<WsMessage>(16);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: ws_tx.clone(),
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: codex::services::EntityTypeRegistry::new(),
        relation_type_registry: codex::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "ws-auth-test-secret".to_string();
    let config = web::Data::new(config);

    let srv = actix_test::start(move || {
        App::new()
            .app_data(state.clone())
            .app_data(config.clone())
            .wrap(AuthMiddleware)
            .configure(auth::configure)
            .configure(ws::configure)
    });

    let client = awc::Client::new();
    let alice_token =
        login_token(&client, &srv.url("/api/auth/login"), "alice", "password123").await;
    let bob_token = login_token(&client, &srv.url("/api/auth/login"), "bob", "password123").await;

    let (_, mut alice_ws) = client
        .ws(srv.url("/api/ws"))
        .set_header(header::AUTHORIZATION, format!("Bearer {alice_token}"))
        .connect()
        .await
        .unwrap();
    let (_, mut bob_ws) = client
        .ws(srv.url("/api/ws"))
        .set_header(header::AUTHORIZATION, format!("Bearer {bob_token}"))
        .connect()
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    ws_tx
        .send(WsMessage::ReindexComplete {
            vault_id: vault.id.clone(),
            file_count: 7,
            duration_ms: 42,
        })
        .unwrap();

    let alice_frame = tokio::time::timeout(Duration::from_secs(2), alice_ws.next())
        .await
        .expect("alice should receive a websocket message")
        .expect("alice stream should yield a frame")
        .expect("alice websocket frame should be ok");

    let alice_text = match alice_frame {
        Frame::Text(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
        other => panic!("expected text frame for alice, got {other:?}"),
    };
    let alice_msg: serde_json::Value = serde_json::from_str(&alice_text).unwrap();
    assert_eq!(alice_msg["type"], "ReindexComplete");
    assert_eq!(alice_msg["vault_id"], vault.id);
    assert_eq!(alice_msg["file_count"], 7);
    assert_eq!(alice_msg["duration_ms"], 42);

    let bob_frame = tokio::time::timeout(Duration::from_millis(500), bob_ws.next()).await;
    assert!(
        bob_frame.is_err(),
        "bob unexpectedly received a websocket frame: {bob_frame:?}"
    );
}
