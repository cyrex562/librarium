use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::routes::vaults::AppState;
use crate::services::oidc_provider;
use actix_web::{cookie::Cookie, get, web, HttpRequest, HttpResponse};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::Rng;
use uuid::Uuid;

/// Generate a random state token for CSRF protection.
fn generate_state_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rng.random();
    hex::encode(bytes)
}

/// Step 1: Redirect the user to the OIDC provider's authorization page.
/// Returns the URL the client should redirect to.
#[get("/api/auth/oidc/authorize")]
async fn oidc_authorize(config: web::Data<AppConfig>) -> AppResult<HttpResponse> {
    let issuer = config
        .auth
        .oidc_issuer_url
        .as_deref()
        .ok_or_else(|| AppError::InternalError("OIDC is not configured".to_string()))?;

    let discovery = oidc_provider::fetch_discovery(issuer).await?;
    let state_token = generate_state_token();
    let authorize_url = oidc_provider::build_authorize_url(&discovery, &config.auth, &state_token)?;

    let cookie = Cookie::build("librarium_oidc_state", state_token.clone())
        .path("/")
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Lax)
        .finish();

    Ok(HttpResponse::Ok()
        .cookie(cookie)
        .json(oidc_provider::OidcAuthorizeResponse {
            authorize_url,
            state: state_token,
        }))
}

/// Step 2: OIDC callback after the user authenticates with the provider.
/// Exchanges the code for tokens, fetches user info, creates/finds local user,
/// and issues our own JWT tokens.
#[get("/api/auth/oidc/callback")]
async fn oidc_callback(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    req: HttpRequest,
    query: web::Query<OidcCallbackQuery>,
) -> AppResult<HttpResponse> {
    let issuer = config
        .auth
        .oidc_issuer_url
        .as_deref()
        .ok_or_else(|| AppError::InternalError("OIDC is not configured".to_string()))?;

    if let Some(ref error) = query.error {
        return Err(AppError::Unauthorized(format!(
            "OIDC provider returned error: {error}"
        )));
    }

    let code = query
        .code
        .as_deref()
        .ok_or_else(|| AppError::InvalidInput("Missing authorization code".to_string()))?;
    let returned_state = query
        .state
        .as_deref()
        .ok_or_else(|| AppError::InvalidInput("Missing OIDC state".to_string()))?;
    let expected_state = req
        .cookie("librarium_oidc_state")
        .map(|cookie| cookie.value().to_string())
        .ok_or_else(|| AppError::Unauthorized("Missing OIDC state cookie".to_string()))?;
    if returned_state != expected_state {
        return Err(AppError::Unauthorized("Invalid OIDC state".to_string()));
    }

    // Exchange code for tokens.
    let discovery = oidc_provider::fetch_discovery(issuer).await?;
    let tokens = oidc_provider::exchange_code(&discovery, &config.auth, code).await?;

    // Fetch user info.
    let userinfo = oidc_provider::fetch_userinfo(&discovery, &tokens.access_token).await?;
    let username = oidc_provider::derive_username(&userinfo);

    // Find or create local user.
    //
    // LIB-008: Match returning OIDC users by their stable `sub` identifier first.
    // Matching by username alone is unsafe — a local account named identically to
    // an OIDC user's derived username would be silently taken over. We store `sub`
    // on first login and use it as the canonical OIDC identity on subsequent logins.
    let user_id = if let Some((id, _)) = state.db.get_user_by_oidc_sub(&userinfo.sub).await? {
        // Known OIDC user — verify the derived username hasn't drifted (IdP rename).
        id
    } else {
        // First OIDC login for this sub. Check whether a local account already
        // exists with the derived username to prevent silent account takeover.
        let existing = state.db.get_user_auth_by_username(&username).await?;
        if existing.is_some() {
            // A local account with this username already exists and was NOT created
            // via OIDC (no oidc_sub binding). Refuse to merge automatically.
            return Err(AppError::Conflict(format!(
                "A local account named '{username}' already exists. \
                 Contact an administrator to link your OIDC identity."
            )));
        }

        // Auto-provision a new local user bound to this OIDC sub.
        let placeholder = format!("oidc-managed-{}", Uuid::new_v4());
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(placeholder.as_bytes(), &salt)
            .map_err(|e| AppError::InternalError(format!("Hash failed: {e}")))?
            .to_string();

        let (id, _, _, _) = state
            .db
            .create_user_with_options(&username, &hash, false, false)
            .await
            .map_err(|e| match e {
                // Concurrent provisioning race: re-check by sub before failing.
                AppError::Conflict(_) => AppError::InternalError(
                    "Account provisioning conflict; please try logging in again.".to_string(),
                ),
                other => other,
            })?;

        state.db.set_user_oidc_sub(&id, &userinfo.sub).await?;

        let _ = state
            .db
            .write_audit_log(
                Some(&id),
                Some(&username),
                "oidc_user_provisioned",
                Some(&format!(
                    "Auto-provisioned from OIDC (sub={})",
                    userinfo.sub
                )),
                None,
                true,
            )
            .await;
        id
    };

    let _ = state
        .db
        .write_audit_log(
            Some(&user_id),
            Some(&username),
            "login_success",
            Some("Authenticated via OIDC"),
            None,
            true,
        )
        .await;

    // Issue our own JWT tokens.
    let (response, refresh_jti, refresh_exp) =
        crate::routes::auth::issue_tokens_public(&user_id, &username, "oidc", &config.auth)?;

    let _ = state
        .db
        .create_session(&refresh_jti, &user_id, refresh_exp)
        .await;

    let clear_cookie = Cookie::build("librarium_oidc_state", "")
        .path("/")
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish();

    Ok(HttpResponse::Ok().cookie(clear_cookie).json(response))
}

#[derive(Debug, serde::Deserialize)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(oidc_authorize).service(oidc_callback);
}
