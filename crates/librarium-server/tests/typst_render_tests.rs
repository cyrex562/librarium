//! `POST /api/vaults/{id}/render-typst` through the real auth middleware:
//! rendering is a read, so a viewer can preview a Typst note, and a user
//! with no access to the vault can't.
#![cfg(feature = "typst")]

use actix_web::{http::header, test, web, App};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use librarium::config::AppConfig;
use librarium::db::Database;
use librarium::middleware::AuthMiddleware;
use librarium::models::CreateVaultRequest;
use librarium::routes::{auth, markdown, vaults, AppState};
use librarium::services::{MarkdownParser, SearchIndex};
use librarium::watcher::FileWatcher;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

fn password_hash(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

#[actix_web::test]
async fn viewers_can_render_typst_and_outsiders_cannot() {
    let temp_dir = TempDir::new().unwrap();
    let db_url = format!("sqlite://{}", temp_dir.path().join("typst.db").display());
    let db = Database::new(&db_url).await.unwrap();
    db.bootstrap_admin_if_empty(Some("admin"), Some("hunter2"))
        .await
        .unwrap();
    db.create_user("viewer", &password_hash("password123"))
        .await
        .unwrap();
    db.create_user("outsider", &password_hash("password123"))
        .await
        .unwrap();

    let (watcher, _) = FileWatcher::new().unwrap();
    let state = web::Data::new(AppState {
        db: db.clone(),
        search_index: SearchIndex::new(),
        watcher: Arc::new(Mutex::new(watcher)),
        event_broadcaster: broadcast::channel(100).0,
        ws_broadcaster: broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(std::collections::HashMap::new())),
        shutdown_tx: broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: librarium::services::EntityTypeRegistry::new(),
        relation_type_registry: librarium::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });
    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "typst-test-secret".to_string();

    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .app_data(web::Data::new(config))
            .wrap(AuthMiddleware)
            .configure(auth::configure)
            .configure(vaults::configure)
            .configure(markdown::configure),
    )
    .await;

    let mut tokens = std::collections::HashMap::new();
    for (user, password) in [
        ("admin", "hunter2"),
        ("viewer", "password123"),
        ("outsider", "password123"),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/login")
                .set_json(json!({ "username": user, "password": password }))
                .to_request(),
        )
        .await;
        assert!(resp.status().is_success(), "login {user}");
        let body: Value = test::read_body_json(resp).await;
        tokens.insert(user, body["access_token"].as_str().unwrap().to_string());
    }
    let bearer = |user: &str| (header::AUTHORIZATION, format!("Bearer {}", tokens[user]));

    let vault_dir = temp_dir.path().join("vault");
    std::fs::create_dir_all(vault_dir.join("papers")).unwrap();
    std::fs::write(
        vault_dir.join("papers/intro.typ"),
        "From an *included* file.",
    )
    .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/vaults")
            .insert_header(bearer("admin"))
            .set_json(&CreateVaultRequest {
                name: "Papers".to_string(),
                path: Some(vault_dir.to_string_lossy().to_string()),
            })
            .to_request(),
    )
    .await;
    assert!(resp.status().is_success());
    let vault: Value = test::read_body_json(resp).await;
    let vault_id = vault["id"].as_str().unwrap().to_string();

    let viewer_id = db
        .get_user_by_username("viewer")
        .await
        .unwrap()
        .map(|(id, _)| id)
        .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{vault_id}/shares/users"))
            .insert_header(bearer("admin"))
            .set_json(json!({ "user_id": viewer_id, "role": "viewer" }))
            .to_request(),
    )
    .await;
    assert!(resp.status().is_success());

    let render = |user: &'static str, content: &'static str| {
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{vault_id}/render-typst"))
            .insert_header(bearer(user))
            .set_json(json!({ "path": "papers/main.typ", "content": content }))
            .to_request()
    };

    // A viewer renders, including a file pulled from the vault.
    let resp =
        test::call_service(&app, render("viewer", "= Main\n\n#include \"intro.typ\"\n")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let html = body["html"].as_str().expect("html");
    assert!(html.contains("<h2>Main</h2>"), "{body}");
    assert!(html.contains("<strong>included</strong>"), "{body}");

    // A compile error is a normal response with diagnostics.
    let resp = test::call_service(&app, render("viewer", "ok\n#nope()\n")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["html"].is_null());
    assert_eq!(body["diagnostics"][0]["severity"], "error");
    assert_eq!(body["diagnostics"][0]["line"], 2);

    // No access to the vault: refused before anything compiles.
    let resp = test::call_service(&app, render("outsider", "= Secret\n")).await;
    assert_eq!(resp.status(), 403);
}
