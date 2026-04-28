use actix_web::{http::header, test, web, App};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use codex::config::AppConfig;
use codex::db::Database;
use codex::middleware::AuthMiddleware;
use codex::routes::{api_keys, auth, files, totp, vaults, AppState};
use codex::services::{MarkdownParser, SearchIndex};
use codex::watcher::FileWatcher;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};
use totp_rs::{Algorithm, Secret, TOTP};

fn password_hash(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

#[actix_web::test]
async fn verify_api_keys_and_totp() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("api-test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    db.bootstrap_admin_if_empty(Some("admin"), Some("hunter2"))
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
        ws_broadcaster: tokio::sync::broadcast::channel::<codex::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: std::sync::Arc::new(tokio::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: codex::services::EntityTypeRegistry::new(),
        relation_type_registry: codex::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "api-test-secret".to_string();
    let config = web::Data::new(config);

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .app_data(config.clone())
            .wrap(AuthMiddleware)
            .configure(auth::configure)
            .configure(totp::configure)
            .configure(api_keys::configure),
    )
    .await;

    // Login
    let login_req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(json!({ "username": "admin", "password": "hunter2" }))
        .to_request();
    let login_resp = test::call_service(&app, login_req).await;
    let login_body: serde_json::Value = test::read_body_json(login_resp).await;
    let token = login_body["access_token"].as_str().unwrap().to_string();
    let auth_header = format!("Bearer {}", token);

    // 1. Generate API Key
    let create_key_req = test::TestRequest::post()
        .uri("/api/auth/api-keys")
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .set_json(json!({ "name": "Desktop Client Key", "expires_in_days": 30 }))
        .to_request();
    let create_key_resp = test::call_service(&app, create_key_req).await;
    assert!(create_key_resp.status().is_success());
    let create_key_body: serde_json::Value = test::read_body_json(create_key_resp).await;
    let api_key = create_key_body["api_key"].as_str().unwrap().to_string();
    let prefix = create_key_body["prefix"].as_str().unwrap().to_string();
    let id = create_key_body["id"].as_str().unwrap().to_string();

    // 2. Validate API Key by calling /api/auth/me bypassing JWT auth
    let me_req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("X-API-Key", api_key.clone()))
        .to_request();
    let me_resp = test::call_service(&app, me_req).await;
    let me_status = me_resp.status();
    let me_body: serde_json::Value = test::read_body_json(me_resp).await;
    assert!(
        me_status.is_success(),
        "Failed to use API key. Status: {}, Body: {}",
        me_status,
        me_body
    );
    assert_eq!(me_body["username"], "admin");

    // 3. List API Keys
    let list_req = test::TestRequest::get()
        .uri("/api/auth/api-keys")
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());
    let list_body: serde_json::Value = test::read_body_json(list_resp).await;
    let keys_array = list_body.as_array().unwrap();
    assert_eq!(keys_array.len(), 1);
    assert_eq!(keys_array[0]["prefix"].as_str().unwrap(), prefix);

    // 4. Revoke API Key
    let revoke_req = test::TestRequest::delete()
        .uri(&format!("/api/auth/api-keys/{}", id))
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .to_request();
    let revoke_resp = test::call_service(&app, revoke_req).await;
    assert!(revoke_resp.status().is_success());

    // 5. Try using revoked Key (Should fail)
    let bad_me_req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("X-API-Key", api_key))
        .to_request();
    let bad_me_resp = test::call_service(&app, bad_me_req).await;
    assert_eq!(bad_me_resp.status().as_u16(), 401);

    // 6. Enroll TOTP
    let enroll_req = test::TestRequest::post()
        .uri("/api/auth/totp/enroll")
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .to_request();
    let enroll_resp = test::call_service(&app, enroll_req).await;
    assert!(enroll_resp.status().is_success());
    let enroll_body: serde_json::Value = test::read_body_json(enroll_resp).await;
    let secret = enroll_body["secret"].as_str().unwrap().to_string();
    let secret_bytes = Secret::Encoded(secret.clone()).to_bytes().unwrap();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("ObsidianHost".to_string()),
        "admin".to_string(),
    )
    .unwrap();
    let current_code = totp.generate_current().unwrap();

    let verify_enroll_req = test::TestRequest::post()
        .uri("/api/auth/totp/verify")
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .set_json(json!({ "code": current_code }))
        .to_request();
    let verify_enroll_resp = test::call_service(&app, verify_enroll_req).await;
    assert!(verify_enroll_resp.status().is_success());

    // 7. Logout should revoke the refresh session.
    let logout_req = test::TestRequest::post()
        .uri("/api/auth/logout")
        .insert_header((header::AUTHORIZATION, auth_header.clone()))
        .set_json(json!({ "refresh_token": login_body["refresh_token"] }))
        .to_request();
    let logout_resp = test::call_service(&app, logout_req).await;
    assert!(logout_resp.status().is_success());

    let refresh_after_logout_req = test::TestRequest::post()
        .uri("/api/auth/refresh")
        .set_json(json!({ "refresh_token": login_body["refresh_token"] }))
        .to_request();
    let refresh_after_logout_resp = test::call_service(&app, refresh_after_logout_req).await;
    assert_eq!(refresh_after_logout_resp.status().as_u16(), 401);

    // 8. Logging in with TOTP enabled should require second-factor completion.
    let totp_login_req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(json!({ "username": "admin", "password": "hunter2" }))
        .to_request();
    let totp_login_resp = test::call_service(&app, totp_login_req).await;
    assert!(totp_login_resp.status().is_success());
    let totp_login_body: serde_json::Value = test::read_body_json(totp_login_resp).await;
    assert_eq!(totp_login_body["totp_required"].as_bool(), Some(true));
    let pending_access = totp_login_body["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let pending_auth_header = format!("Bearer {pending_access}");

    let me_with_pending_req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header((header::AUTHORIZATION, pending_auth_header.clone()))
        .to_request();
    let me_with_pending_resp = test::call_service(&app, me_with_pending_req).await;
    assert_eq!(me_with_pending_resp.status().as_u16(), 403);

    let current_code = totp.generate_current().unwrap();
    let verify_login_req = test::TestRequest::post()
        .uri("/api/auth/totp/login-verify")
        .insert_header((header::AUTHORIZATION, pending_auth_header))
        .set_json(json!({ "code": current_code }))
        .to_request();
    let verify_login_resp = test::call_service(&app, verify_login_req).await;
    assert!(verify_login_resp.status().is_success());
    let verify_login_body: serde_json::Value = test::read_body_json(verify_login_resp).await;
    let verified_auth_header = format!(
        "Bearer {}",
        verify_login_body["access_token"].as_str().unwrap()
    );

    let me_after_totp_req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header((header::AUTHORIZATION, verified_auth_header))
        .to_request();
    let me_after_totp_resp = test::call_service(&app, me_after_totp_req).await;
    assert!(me_after_totp_resp.status().is_success());
}

#[actix_web::test]
async fn test_public_vault_allows_anonymous_reads() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("pub-vault-test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    let db = Database::new(&db_url).await.unwrap();

    db.bootstrap_admin_if_empty(Some("admin"), Some("secret123"))
        .await
        .unwrap();

    let vault_root = temp_dir.path().join("vaults");
    std::fs::create_dir_all(&vault_root).unwrap();

    // ── Build app ─────────────────────────────────────────────────────────
    let search_index = SearchIndex::new();
    let (watcher, _) = FileWatcher::new().unwrap();
    let watcher = Arc::new(Mutex::new(watcher));
    let (event_tx, _) = broadcast::channel(100);

    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: tokio::sync::broadcast::channel::<codex::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: codex::services::EntityTypeRegistry::new(),
        relation_type_registry: codex::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "pub-test-secret".to_string();
    config.vault.base_dir = vault_root.to_string_lossy().to_string();
    let config = web::Data::new(config);

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .app_data(config.clone())
            .wrap(AuthMiddleware)
            .configure(auth::configure)
            .configure(vaults::configure)
            .configure(files::configure),
    )
    .await;

    // ── Login to get a token ──────────────────────────────────────────────
    let login_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(json!({ "username": "admin", "password": "secret123" }))
            .to_request(),
    )
    .await;
    let login_body: serde_json::Value = test::read_body_json(login_resp).await;
    let token = login_body["access_token"].as_str().unwrap().to_string();
    let auth_header = format!("Bearer {token}");

    // ── Create a vault ────────────────────────────────────────────────────
    let vault_path = vault_root.join("testvault");
    std::fs::create_dir_all(&vault_path).unwrap();

    let create_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/vaults")
            .insert_header((header::AUTHORIZATION, auth_header.clone()))
            .set_json(json!({
                "name": "Test Vault",
                "path": vault_path.to_string_lossy()
            }))
            .to_request(),
    )
    .await;
    assert!(create_resp.status().is_success(), "vault creation failed");
    let vault_body: serde_json::Value = test::read_body_json(create_resp).await;
    let vault_id = vault_body["id"].as_str().unwrap().to_string();

    // ── Unauthenticated GET on a private vault → 401 ─────────────────────
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/vaults/{vault_id}/files"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        401,
        "private vault must reject anonymous reads"
    );

    // ── Mark vault as public ─────────────────────────────────────────────
    let vis_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{vault_id}/visibility"))
            .insert_header((header::AUTHORIZATION, auth_header.clone()))
            .set_json(json!({ "visibility": "public" }))
            .to_request(),
    )
    .await;
    assert!(vis_resp.status().is_success(), "setting visibility failed");

    // ── Unauthenticated GET on a public vault → 200 ──────────────────────
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/vaults/{vault_id}/files"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "public vault must allow anonymous reads"
    );

    // ── Unauthenticated write on a public vault → still 401 ──────────────
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/vaults/{vault_id}/files/note.md"))
            .set_payload("# Hello")
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        401,
        "public vault must reject anonymous writes"
    );

    // ── Revert to private ────────────────────────────────────────────────
    let _ = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{vault_id}/visibility"))
            .insert_header((header::AUTHORIZATION, auth_header.clone()))
            .set_json(json!({ "visibility": "private" }))
            .to_request(),
    )
    .await;

    // ── Unauthenticated GET after reverting to private → 401 again ────────
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/vaults/{vault_id}/files"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        401,
        "reverted private vault must reject anonymous reads"
    );

    let _ = state; // keep state alive
}
