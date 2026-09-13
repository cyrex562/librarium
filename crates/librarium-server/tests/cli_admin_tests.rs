//! The CLI must be able to repair credentials with no server running.
//!
//! That is the whole point of it: an HTTP-based tool is useless exactly when
//! you need recovery — locked out, or the server will not start.

use librarium::cli::{run_admin, AdminCommand};
use librarium::config::AppConfig;
use librarium::db::Database;
use librarium::services::CredentialService;

fn temp_config() -> (AppConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("librarium.db");
    let mut cfg = AppConfig::default();
    cfg.database.path = db_path.to_string_lossy().into_owned();
    cfg.auth.min_password_length = 8;
    (cfg, dir)
}

async fn open(cfg: &AppConfig) -> Database {
    Database::new(&format!("sqlite:{}?mode=rwc", cfg.database.path))
        .await
        .unwrap()
}

#[actix_web::test]
async fn set_password_updates_the_stored_hash() {
    let (cfg, _dir) = temp_config();

    let db = open(&cfg).await;
    CredentialService::new(&db, &cfg.auth)
        .create_user("alice", "originalpw", false)
        .await
        .unwrap();
    let before = db
        .get_user_auth_by_username("alice")
        .await
        .unwrap()
        .unwrap()
        .2;
    drop(db);

    run_admin(
        AdminCommand::SetPassword {
            username: "alice".into(),
            password: Some("replacementpw".into()),
        },
        &cfg,
    )
    .await
    .unwrap();

    let db = open(&cfg).await;
    let after = db
        .get_user_auth_by_username("alice")
        .await
        .unwrap()
        .unwrap()
        .2;
    assert_ne!(
        before, after,
        "the CLI must actually change the stored hash"
    );
}

#[actix_web::test]
async fn set_password_revokes_existing_sessions() {
    let (cfg, _dir) = temp_config();

    let db = open(&cfg).await;
    let user_id = CredentialService::new(&db, &cfg.auth)
        .create_user("alice", "originalpw", false)
        .await
        .unwrap();
    let expires = chrono::Utc::now() + chrono::Duration::days(1);
    db.create_session("cli-session-jti", &user_id, expires)
        .await
        .unwrap();
    drop(db);

    run_admin(
        AdminCommand::SetPassword {
            username: "alice".into(),
            password: Some("replacementpw".into()),
        },
        &cfg,
    )
    .await
    .unwrap();

    let db = open(&cfg).await;
    assert!(
        db.is_session_revoked("cli-session-jti").await.unwrap(),
        "recovering an account must not leave an old session usable"
    );
}

#[actix_web::test]
async fn create_user_then_list_users_round_trips() {
    let (cfg, _dir) = temp_config();

    run_admin(
        AdminCommand::CreateUser {
            username: "bob".into(),
            admin: true,
            password: Some("originalpw".into()),
        },
        &cfg,
    )
    .await
    .unwrap();

    let db = open(&cfg).await;
    let users = db.list_users().await.unwrap();
    let bob = users.iter().find(|u| u.username == "bob").unwrap();
    assert!(bob.is_admin);

    // ListUsers must not error against a populated database.
    run_admin(AdminCommand::ListUsers, &cfg).await.unwrap();
}

#[actix_web::test]
async fn set_password_enforces_the_configured_policy() {
    let (cfg, _dir) = temp_config();

    let db = open(&cfg).await;
    CredentialService::new(&db, &cfg.auth)
        .create_user("alice", "originalpw", false)
        .await
        .unwrap();
    drop(db);

    let err = run_admin(
        AdminCommand::SetPassword {
            username: "alice".into(),
            password: Some("short".into()),
        },
        &cfg,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("at least"), "got: {err}");
}

#[actix_web::test]
async fn set_password_errors_on_an_unknown_user() {
    let (cfg, _dir) = temp_config();

    let err = run_admin(
        AdminCommand::SetPassword {
            username: "nobody".into(),
            password: Some("replacementpw".into()),
        },
        &cfg,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("nobody"), "got: {err}");
}
