use actix_web::{http::header, test, web, App};
use codex::config::AppConfig;
use codex::db::Database;
use codex::middleware::AuthMiddleware;
use codex::models::CreateVaultRequest;
use codex::routes::{auth, files, vaults, AppState};
use codex::services::{MarkdownParser, SearchIndex};
use codex::watcher::FileWatcher;
use serde_json::json;
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};
use zip::write::SimpleFileOptions;

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (path, content) in entries {
            zip.start_file(path, options).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

fn build_tar_with_symlink(file_path: &str, file_content: &[u8], symlink_path: &str) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut buffer);

        let mut file_header = tar::Header::new_gnu();
        file_header.set_size(file_content.len() as u64);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder
            .append_data(&mut file_header, file_path, Cursor::new(file_content))
            .unwrap();

        let mut symlink_header = tar::Header::new_gnu();
        symlink_header.set_entry_type(tar::EntryType::Symlink);
        symlink_header.set_size(0);
        symlink_header.set_mode(0o777);
        symlink_header.set_link_name(file_path).unwrap();
        symlink_header.set_cksum();
        builder
            .append_data(&mut symlink_header, symlink_path, std::io::empty())
            .unwrap();

        builder.finish().unwrap();
    }
    buffer
}

async fn setup_app() -> (
    TempDir,
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse<
            actix_web::body::EitherBody<actix_web::body::BoxBody>,
        >,
        Error = actix_web::Error,
    >,
    String,
    std::path::PathBuf,
    String,
) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("import-archive.db");
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
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx: tokio::sync::broadcast::channel::<()>(1).0,
        document_parser: Arc::new(MarkdownParser),
        entity_type_registry: codex::services::EntityTypeRegistry::new(),
        relation_type_registry: codex::services::RelationTypeRegistry::new(),
        plugins_dir: std::path::PathBuf::new(),
    });

    let mut config = AppConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = "import-archive-test-secret".to_string();
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

    let login_req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(json!({ "username": "admin", "password": "hunter2" }))
        .to_request();
    let login_resp = test::call_service(&app, login_req).await;
    assert!(login_resp.status().is_success());
    let login_body: serde_json::Value = test::read_body_json(login_resp).await;
    let token = login_body["access_token"].as_str().unwrap().to_string();

    let vault_dir = temp_dir.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    let create_vault_req = test::TestRequest::post()
        .uri("/api/vaults")
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_json(&CreateVaultRequest {
            name: "Archive Vault".to_string(),
            path: Some(vault_dir.to_string_lossy().to_string()),
        })
        .to_request();
    let create_vault_resp = test::call_service(&app, create_vault_req).await;
    assert!(create_vault_resp.status().is_success());
    let vault_body: serde_json::Value = test::read_body_json(create_vault_resp).await;
    let vault_id = vault_body["id"].as_str().unwrap().to_string();

    (temp_dir, app, token, vault_dir, vault_id)
}

#[actix_web::test]
async fn upload_session_accepts_frontend_chunk_size() {
    let (_temp_dir, app, token, _vault_dir, vault_id) = setup_app().await;

    let create_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{vault_id}/upload-sessions"))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_json(json!({
            "filename": "large.md",
            "total_size": 300 * 1024,
            "path": ""
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert!(create_resp.status().is_success());
    let create_body: serde_json::Value = test::read_body_json(create_resp).await;
    let session_id = create_body["session_id"].as_str().unwrap();

    let chunk = vec![b'a'; 300 * 1024];
    let upload_req = test::TestRequest::put()
        .uri(&format!(
            "/api/vaults/{vault_id}/upload-sessions/{session_id}"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(chunk)
        .to_request();
    let upload_resp = test::call_service(&app, upload_req).await;
    let status = upload_resp.status();
    let body: serde_json::Value = test::read_body_json(upload_resp).await;

    assert!(
        status.is_success(),
        "expected upload chunk to be accepted, got {status}: {body}"
    );
    assert_eq!(body["uploaded_bytes"], 300 * 1024);
}

#[actix_web::test]
async fn upload_sessions_accept_many_files() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    let auth = format!("Bearer {token}");

    for i in 0..2000 {
        let filename = format!("file-{i:04}.txt");
        let content = format!("content {i}");
        let create_req = test::TestRequest::post()
            .uri(&format!("/api/vaults/{vault_id}/upload-sessions"))
            .insert_header((header::AUTHORIZATION, auth.clone()))
            .set_json(json!({
                "filename": filename,
                "total_size": content.len(),
                "path": "bulk"
            }))
            .to_request();
        let create_resp = test::call_service(&app, create_req).await;
        assert!(
            create_resp.status().is_success(),
            "failed to create upload session for file {i}"
        );
        let create_body: serde_json::Value = test::read_body_json(create_resp).await;
        let session_id = create_body["session_id"].as_str().unwrap();

        let upload_req = test::TestRequest::put()
            .uri(&format!(
                "/api/vaults/{vault_id}/upload-sessions/{session_id}"
            ))
            .insert_header((header::AUTHORIZATION, auth.clone()))
            .set_payload(content)
            .to_request();
        let upload_resp = test::call_service(&app, upload_req).await;
        assert!(
            upload_resp.status().is_success(),
            "failed to upload chunk for file {i}"
        );

        let finish_req = test::TestRequest::post()
            .uri(&format!(
                "/api/vaults/{vault_id}/upload-sessions/{session_id}/finish"
            ))
            .insert_header((header::AUTHORIZATION, auth.clone()))
            .set_json(json!({
                "filename": filename,
                "path": "bulk",
                "conflict": "fail"
            }))
            .to_request();
        let finish_resp = test::call_service(&app, finish_req).await;
        assert!(
            finish_resp.status().is_success(),
            "failed to finish upload session for file {i}"
        );
    }

    let bulk_dir = vault_dir.join("bulk");
    assert_eq!(std::fs::read_dir(&bulk_dir).unwrap().count(), 2000);
    assert_eq!(
        std::fs::read_to_string(bulk_dir.join("file-0168.txt")).unwrap(),
        "content 168"
    );
    assert_eq!(
        std::fs::read_to_string(bulk_dir.join("file-1999.txt")).unwrap(),
        "content 1999"
    );
}

#[actix_web::test]
async fn upload_session_skip_conflict_reports_skipped_file() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    std::fs::write(vault_dir.join("existing.md"), "old content").unwrap();

    let create_req = test::TestRequest::post()
        .uri(&format!("/api/vaults/{vault_id}/upload-sessions"))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_json(json!({
            "filename": "existing.md",
            "total_size": 11,
            "path": ""
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert!(create_resp.status().is_success());
    let create_body: serde_json::Value = test::read_body_json(create_resp).await;
    let session_id = create_body["session_id"].as_str().unwrap();

    let upload_req = test::TestRequest::put()
        .uri(&format!(
            "/api/vaults/{vault_id}/upload-sessions/{session_id}"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload("new content")
        .to_request();
    let upload_resp = test::call_service(&app, upload_req).await;
    assert!(upload_resp.status().is_success());

    let finish_req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/upload-sessions/{session_id}/finish"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_json(json!({
            "filename": "existing.md",
            "path": "",
            "conflict": "skip"
        }))
        .to_request();
    let finish_resp = test::call_service(&app, finish_req).await;
    assert!(finish_resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(finish_resp).await;

    assert_eq!(body["path"], "existing.md");
    assert_eq!(body["filename"], "existing.md");
    assert_eq!(body["skipped"], true);
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("existing.md")).unwrap(),
        "old content"
    );
    assert!(!vault_dir
        .join(".obsidian/uploads")
        .join(session_id)
        .exists());
}

#[actix_web::test]
async fn import_archive_conflict_skip_reports_skipped_entries() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    std::fs::write(vault_dir.join("existing.md"), "old content").unwrap();

    let archive = build_zip(&[
        ("existing.md", b"new content"),
        ("new.md", b"fresh content"),
    ]);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/import-archive?archive_type=zip&conflict=skip"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(archive)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["extracted"][0], "new.md");
    assert_eq!(body["skipped_count"], 1);
    assert_eq!(body["skipped"][0], "existing.md");
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("existing.md")).unwrap(),
        "old content"
    );
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("new.md")).unwrap(),
        "fresh content"
    );
}

#[actix_web::test]
async fn import_archive_conflict_fail_rejects_existing_file() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    std::fs::write(vault_dir.join("existing.md"), "old content").unwrap();

    let archive = build_zip(&[("existing.md", b"new content")]);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/import-archive?archive_type=zip&conflict=fail"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(archive)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status().as_u16(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body["message"],
        "A conflict occurred: Archive entry already exists: existing.md"
    );
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("existing.md")).unwrap(),
        "old content"
    );
}

#[actix_web::test]
async fn import_archive_conflict_overwrite_replaces_existing_file() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    std::fs::write(vault_dir.join("existing.md"), "old content").unwrap();

    let archive = build_zip(&[("existing.md", b"new content")]);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/import-archive?archive_type=zip&conflict=overwrite"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(archive)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["extracted"][0], "existing.md");
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("existing.md")).unwrap(),
        "new content"
    );
}

#[actix_web::test]
async fn import_archive_conflict_rename_keeps_both_files() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;
    std::fs::write(vault_dir.join("existing.md"), "old content").unwrap();

    let archive = build_zip(&[("existing.md", b"new content")]);
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/import-archive?archive_type=zip&conflict=rename_with_timestamp"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(archive)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    let renamed_path = body["extracted"][0].as_str().unwrap();
    assert_ne!(renamed_path, "existing.md");
    assert!(renamed_path.starts_with("existing_"));
    assert!(renamed_path.ends_with(".md"));
    assert_eq!(
        std::fs::read_to_string(vault_dir.join("existing.md")).unwrap(),
        "old content"
    );
    assert_eq!(
        std::fs::read_to_string(vault_dir.join(renamed_path)).unwrap(),
        "new content"
    );
}

#[actix_web::test]
async fn import_archive_tar_skips_non_file_entries() {
    let (_temp_dir, app, token, vault_dir, vault_id) = setup_app().await;

    let archive = build_tar_with_symlink("regular.md", b"from tar", "linked.md");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/vaults/{vault_id}/import-archive?archive_type=tar&conflict=overwrite"
        ))
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .set_payload(archive)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["extracted"][0], "regular.md");
    assert!(vault_dir.join("regular.md").is_file());
    assert!(!vault_dir.join("linked.md").exists());
}
