//! LDAP / Active Directory authentication provider.
//!
//! Flow:
//! 1. Bind with the service account (ldap_bind_dn / ldap_bind_password)
//! 2. Search for the user entry using the configured filter
//! 3. Attempt a bind with the found DN and the user's password
//! 4. If successful, return the username from the LDAP entry

use crate::config::AuthConfig;
use crate::error::{AppError, AppResult};
use ldap3::{LdapConnAsync, Scope, SearchEntry};

/// Authenticate a user against LDAP. Returns the canonical username from the directory.
pub async fn authenticate_ldap(
    auth_cfg: &AuthConfig,
    username: &str,
    password: &str,
) -> AppResult<String> {
    let ldap_url = auth_cfg
        .ldap_url
        .as_deref()
        .ok_or_else(|| AppError::InternalError("ldap_url not configured".to_string()))?;
    let base_dn = auth_cfg
        .ldap_base_dn
        .as_deref()
        .ok_or_else(|| AppError::InternalError("ldap_base_dn not configured".to_string()))?;

    // Step 1: Connect and bind with service account.
    let (conn, mut ldap) = LdapConnAsync::new(ldap_url)
        .await
        .map_err(|e| AppError::InternalError(format!("LDAP connect failed: {e}")))?;

    // Drive the connection in the background.
    ldap3::drive!(conn);

    if let (Some(bind_dn), Some(bind_pw)) = (
        auth_cfg.ldap_bind_dn.as_deref(),
        auth_cfg.ldap_bind_password.as_deref(),
    ) {
        let result = ldap
            .simple_bind(bind_dn, bind_pw)
            .await
            .map_err(|e| AppError::InternalError(format!("LDAP service bind failed: {e}")))?;
        if result.rc != 0 {
            return Err(AppError::InternalError(format!(
                "LDAP service bind rejected (rc={})",
                result.rc
            )));
        }
    }

    // Step 2: Search for the user entry.
    let filter = auth_cfg
        .ldap_search_filter
        .replace("{attr}", &auth_cfg.ldap_user_attr)
        .replace("{username}", username);

    let (search_results, _result) = ldap
        .search(
            base_dn,
            Scope::Subtree,
            &filter,
            vec![&auth_cfg.ldap_user_attr, "dn"],
        )
        .await
        .map_err(|e| AppError::InternalError(format!("LDAP search failed: {e}")))?
        .success()
        .map_err(|e| AppError::InternalError(format!("LDAP search error: {e}")))?;

    if search_results.is_empty() {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let entry = SearchEntry::construct(search_results.into_iter().next().unwrap());
    let user_dn = entry.dn.clone();

    // Extract the canonical username from the LDAP entry.
    let canonical_username = entry
        .attrs
        .get(&auth_cfg.ldap_user_attr)
        .and_then(|vals| vals.first())
        .cloned()
        .unwrap_or_else(|| username.to_string());

    // Unbind the service account.
    let _ = ldap.unbind().await;

    // Step 3: Re-connect and bind as the user to verify their password.
    let (conn2, mut ldap2) = LdapConnAsync::new(ldap_url)
        .await
        .map_err(|e| AppError::InternalError(format!("LDAP reconnect failed: {e}")))?;

    ldap3::drive!(conn2);

    let user_bind = ldap2
        .simple_bind(&user_dn, password)
        .await
        .map_err(|e| AppError::Unauthorized(format!("LDAP user bind failed: {e}")))?;

    let _ = ldap2.unbind().await;

    if user_bind.rc != 0 {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    Ok(canonical_username)
}
