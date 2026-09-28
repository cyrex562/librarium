//! Typst notes in full-text search (#143), through the file routes: creating,
//! editing and renaming a `.typ` note keeps the index current, and the
//! indexed text is the note's prose, not its markup or code.

use actix_web::{test, web};
use librarium::db::Database;
use librarium::routes::{files, search, AppState};
use librarium::services::{EntityTypeRegistry, MarkdownParser, RelationTypeRegistry, SearchIndex};
use librarium::watcher::FileWatcher;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

mod common;
use common::test_app;

#[actix_web::test]
async fn typst_notes_are_searchable_through_create_edit_and_rename() {
    let temp = TempDir::new().unwrap();
    let db = Database::new(&format!("sqlite://{}", temp.path().join("s.db").display()))
        .await
        .unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    let vault = db
        .create_vault("V".to_string(), vault_dir.to_string_lossy().to_string())
        .await
        .unwrap();

    let search_index = SearchIndex::new();
    search_index
        .index_vault(&vault.id, &vault_dir.to_string_lossy())
        .unwrap();
    let (watcher, _) = FileWatcher::new().unwrap();
    let state = web::Data::new(AppState {
        db,
        search_index,
        watcher: Arc::new(Mutex::new(watcher)),
        event_broadcaster: broadcast::channel(100).0,
        ws_broadcaster: broadcast::channel::<librarium::models::WsMessage>(16).0,
        change_log_retention_days: 7,
        ml_undo_store: Arc::new(Mutex::new(std::collections::HashMap::new())),
        shutdown_tx: broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: EntityTypeRegistry::new(),
        relation_type_registry: RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });
    let app = test::init_service(test_app(state, |cfg| {
        files::configure(cfg);
        search::configure(cfg);
    }))
    .await;
    let id = vault.id.clone();

    let search = |q: &str| {
        test::TestRequest::get()
            .uri(&format!("/api/vaults/{id}/search?q={q}"))
            .to_request()
    };

    // Create.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{id}/files"))
            .set_json(json!({ "path": "paper.typ", "content": "#set page(margin: 2cm)\n\n= Findings\n\nA *walrus* result.\n" }))
            .to_request(),
    )
    .await;
    assert!(resp.status().is_success(), "{}", resp.status());
    let body: Value = test::read_body_json(test::call_service(&app, search("walrus")).await).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{body}");
    assert_eq!(results[0]["path"], "paper.typ");
    assert_eq!(results[0]["matches"][0]["line_number"], 5);
    assert_eq!(results[0]["matches"][0]["line_text"], "A walrus result.");
    let body: Value = test::read_body_json(test::call_service(&app, search("margin")).await).await;
    assert!(
        body["results"].as_array().unwrap().is_empty(),
        "code was indexed: {body}"
    );

    // Edit.
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/vaults/{id}/files/paper.typ"))
            .set_json(json!({ "content": "= Findings\n\nA _narwhal_ result.\n" }))
            .to_request(),
    )
    .await;
    assert!(resp.status().is_success(), "{}", resp.status());
    let body: Value = test::read_body_json(test::call_service(&app, search("narwhal")).await).await;
    assert_eq!(body["results"][0]["path"], "paper.typ", "{body}");
    let body: Value = test::read_body_json(test::call_service(&app, search("walrus")).await).await;
    assert!(
        body["results"].as_array().unwrap().is_empty(),
        "stale content: {body}"
    );

    // Rename.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/vaults/{id}/rename"))
            .set_json(json!({ "from": "paper.typ", "to": "archive/paper-v2.typ" }))
            .to_request(),
    )
    .await;
    assert!(resp.status().is_success(), "{}", resp.status());
    let body: Value = test::read_body_json(test::call_service(&app, search("narwhal")).await).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{body}");
    assert_eq!(results[0]["path"], "archive/paper-v2.typ");
}
