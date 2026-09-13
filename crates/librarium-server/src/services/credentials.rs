//! The single choke point for password mutation.
//!
//! Every path that changes a credential — the HTTP routes, the `librarium
//! admin` CLI, and the desktop's local reset — goes through here, so Argon2
//! parameters and the password policy live in exactly one place. Drift in
//! credential hashing is a security bug, not a style issue.
//!
//! This service deliberately takes `&Database` rather than `AppState`: the CLI
//! opens the SQLite file directly with no server running, which is precisely
//! when recovery is needed.

use crate::config::AuthConfig;
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::services::validate_password_policy;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use librarium_types::AdminUser;

/// Hash a password with the project's standard Argon2 parameters.
pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::InternalError(format!("Failed to hash password: {e}")))
        .map(|h| h.to_string())
}

pub struct CredentialService<'a> {
    db: &'a Database,
    policy: &'a AuthConfig,
}

impl<'a> CredentialService<'a> {
    pub fn new(db: &'a Database, policy: &'a AuthConfig) -> Self {
        Self { db, policy }
    }

    /// Set `username`'s password, revoking every session they hold.
    ///
    /// Used by the recovery paths (CLI, desktop reset) and by an admin
    /// resetting someone else's password: it does not require the old one.
    ///
    /// Ordering matters. Policy and user lookup are checked before any write,
    /// so a rejected call leaves the stored hash untouched. Revocation and the
    /// audit entry happen after the row update and are best-effort — failing
    /// them is logged, never rolled back, because leaving the user with an
    /// unknown credential state is worse than a missing audit line.
    pub async fn set_password(&self, username: &str, new_password: &str) -> AppResult<()> {
        validate_password_policy(new_password, self.policy)?;

        let (user_id, resolved_username, _) = self
            .db
            .get_user_auth_by_username(username)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User '{username}' not found")))?;

        let hash = hash_password(new_password)?;
        self.db.set_user_password(&user_id, &hash, false).await?;

        self.revoke_and_audit(&user_id, &resolved_username, "password_reset")
            .await;
        Ok(())
    }

    /// Change a password after verifying the current one.
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: &str,
        new_password: &str,
    ) -> AppResult<()> {
        validate_password_policy(new_password, self.policy)?;

        let (_, username, existing_hash) = self
            .db
            .get_user_auth_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

        let parsed = PasswordHash::new(&existing_hash)
            .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;
        Argon2::default()
            .verify_password(current_password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized("Invalid current password".to_string()))?;

        let hash = hash_password(new_password)?;
        self.db.set_user_password(user_id, &hash, false).await?;

        self.revoke_and_audit(user_id, &username, "password_changed")
            .await;
        Ok(())
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        is_admin: bool,
    ) -> AppResult<String> {
        validate_password_policy(password, self.policy)?;
        let hash = hash_password(password)?;
        let (user_id, _, _, _) = self
            .db
            .create_user_with_options(username, &hash, is_admin, false)
            .await?;
        Ok(user_id)
    }

    pub async fn list_users(&self) -> AppResult<Vec<AdminUser>> {
        self.db.list_users().await
    }

    /// Best-effort session revocation plus audit entry. Never fails the caller.
    ///
    /// Revoking matters: without it, resetting a compromised account's password
    /// leaves the attacker's session alive — and on desktop, with a 10-year
    /// non-rotating refresh token, effectively forever.
    async fn revoke_and_audit(&self, user_id: &str, username: &str, action: &str) {
        match self.db.revoke_all_sessions(user_id).await {
            Ok(n) => tracing::info!("Revoked {n} session(s) for '{username}' after {action}"),
            Err(e) => tracing::warn!("Could not revoke sessions for '{username}': {e}"),
        }
        let _ = self
            .db
            .write_audit_log(Some(user_id), Some(username), action, None, None, true)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> (Database, AuthConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .unwrap();
        let cfg = AuthConfig {
            min_password_length: 8,
            ..AuthConfig::default()
        };
        (db, cfg, dir)
    }

    #[actix_web::test]
    async fn set_password_rejects_a_policy_violation_without_writing() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", false).await.unwrap();

        let before = db
            .get_user_auth_by_username("alice")
            .await
            .unwrap()
            .unwrap()
            .2;
        let err = svc.set_password("alice", "short").await.unwrap_err();
        let after = db
            .get_user_auth_by_username("alice")
            .await
            .unwrap()
            .unwrap()
            .2;

        assert!(matches!(err, AppError::InvalidInput(_)));
        assert_eq!(before, after, "hash must be untouched after a policy failure");
    }

    #[actix_web::test]
    async fn set_password_replaces_the_hash() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", false).await.unwrap();

        let before = db
            .get_user_auth_by_username("alice")
            .await
            .unwrap()
            .unwrap()
            .2;
        svc.set_password("alice", "replacementpw").await.unwrap();
        let after = db
            .get_user_auth_by_username("alice")
            .await
            .unwrap()
            .unwrap()
            .2;

        assert_ne!(before, after);
        let parsed = PasswordHash::new(&after).unwrap();
        assert!(Argon2::default()
            .verify_password(b"replacementpw", &parsed)
            .is_ok());
        assert!(Argon2::default()
            .verify_password(b"originalpw", &parsed)
            .is_err());
    }

    #[actix_web::test]
    async fn set_password_revokes_existing_sessions() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        let user_id = svc.create_user("alice", "originalpw", false).await.unwrap();

        let expires = chrono::Utc::now() + chrono::Duration::days(1);
        db.create_session("session-jti-1", &user_id, expires)
            .await
            .unwrap();
        assert!(!db.is_session_revoked("session-jti-1").await.unwrap());

        svc.set_password("alice", "replacementpw").await.unwrap();

        assert!(
            db.is_session_revoked("session-jti-1").await.unwrap(),
            "a password reset must not leave an existing session alive"
        );
    }

    #[actix_web::test]
    async fn change_password_revokes_existing_sessions() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        let user_id = svc.create_user("alice", "originalpw", false).await.unwrap();

        let expires = chrono::Utc::now() + chrono::Duration::days(1);
        db.create_session("session-jti-2", &user_id, expires)
            .await
            .unwrap();

        svc.change_password(&user_id, "originalpw", "replacementpw")
            .await
            .unwrap();

        assert!(db.is_session_revoked("session-jti-2").await.unwrap());
    }

    #[actix_web::test]
    async fn set_password_errors_on_an_unknown_user() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        assert!(svc.set_password("nobody", "replacementpw").await.is_err());
    }

    #[actix_web::test]
    async fn change_password_rejects_a_wrong_current_password() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        let user_id = svc.create_user("alice", "originalpw", false).await.unwrap();

        let err = svc
            .change_password(&user_id, "wrongpassword", "replacementpw")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    #[actix_web::test]
    async fn create_user_then_list_users_round_trips() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", true).await.unwrap();

        let users = svc.list_users().await.unwrap();
        let alice = users.iter().find(|u| u.username == "alice").unwrap();
        assert!(alice.is_admin);
    }
}
