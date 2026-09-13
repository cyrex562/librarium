# Password Set, Reset, and Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a forgotten password recoverable offline, stop a single spurious 401 from destroying the desktop's long-lived session, and ensure password changes revoke existing sessions.

**Architecture:** One `CredentialService` in `librarium-server` becomes the only place that mutates a password — validate policy, Argon2 hash, update, revoke sessions, audit. Three deliberately different transports reach it: a CLI that opens SQLite directly (works with the server stopped), a `#[cfg(desktop)]` Tauri command (unreachable from other local processes, unlike a loopback HTTP route), and the existing authenticated HTTP routes. Separately, the frontend stops treating one 401 as fatal.

**Tech Stack:** Rust (Actix Web, sqlx/SQLite, Argon2, clap 4, rpassword), Vue 3 + TypeScript + Vuetify 3, Vitest, Tauri 2.

**Spec:** `docs/superpowers/specs/2026-09-13-password-set-reset-recovery-design.md`

## Global Constraints

- `set_password` ordering is fixed: validate policy → Argon2 hash → update row → **revoke all that user's sessions** → write audit entry.
- Policy failure or unknown username must return **before** any mutation.
- Session-revocation or audit failure after a successful row update is logged, never rolled back — an unknown credential state is worse than a missing audit line.
- Passwords are **never** accepted as an argv argument. `rpassword` prompts, no echo, confirm twice.
- The desktop reset is a Tauri command, **never** an HTTP route, and is `#[cfg(desktop)]`-gated so it does not exist in the Android build.
- Argon2 parameters and password policy live in exactly **one** place after this work.
- Reuse the existing `librarium_types::AdminUser` for user listings — do not add a parallel summary type.
- Reuse the existing `services::validate_password_policy(password, &AuthConfig)`.
- `AppConfig` is read once at startup: enabling or disabling auth needs a server restart, and the UI must say so.
- Existing invocations must keep working: `librarium` with no subcommand still runs the server.
- Run `cargo test -p librarium-server` and `npm --prefix frontend test` for tests; `npm --prefix frontend run build` for the frontend typecheck.

## Correction to the spec (verified during planning)

The spec sketches a new `UserSummary` struct. It is redundant: `Database::list_users()` already returns `Vec<librarium_types::AdminUser>` with exactly the needed fields (`id`, `username`, `is_admin`, `must_change_password`, `is_active`, `created_at`). This plan uses `AdminUser` throughout.

## File Structure

| File | Responsibility |
| --- | --- |
| `frontend/src/stores/auth.ts` | Split voluntary `logout()` from involuntary `clearLocalSession()`. |
| `frontend/src/api/client.ts` | On 401: refresh once, retry once, only then clear the session. |
| `frontend/src/composables/useWebSocket.ts` | Use `clearLocalSession`, not `logout`. |
| `frontend/src/stores/auth.test.ts` | **New.** Session-lifecycle regression tests. |
| `crates/librarium-server/src/services/credentials.rs` | **New.** The single choke point for password mutation. |
| `crates/librarium-server/src/services/mod.rs` | Export it. |
| `crates/librarium-server/src/routes/auth.rs` | `change_password` delegates to the service. |
| `crates/librarium-server/src/routes/admin.rs` | Reset branch delegates to the service; drop the local `hash_password`. |
| `crates/librarium-server/src/cli.rs` | **New.** `admin` subcommands. |
| `crates/librarium-server/src/main.rs` | Wire the optional subcommand. |
| `crates/librarium-tauri/src/lib.rs` | `#[cfg(desktop)]` `auth_reset_local_password`. |
| `frontend/src/pages/LoginPage.vue` | Desktop-only "Forgot password?". |
| `frontend/src/components/settings/SecurityPanel.vue` | **New.** Enable/disable auth, change password. |
| `config.example.toml`, `README.md`, `docs/DESIGN.md`, `AGENTS.md` | Document recovery; demote plaintext bootstrap. |

---

## Phase 1 — Stop a spurious 401 destroying the session

Independent of every other phase. Ships first.

### Task 1: Split voluntary and involuntary logout

**Files:**
- Modify: `frontend/src/stores/auth.ts` (the `logout` function, ~line 175)
- Test: `frontend/src/stores/auth.test.ts` (create)

**Interfaces:**
- Consumes: nothing.
- Produces: `clearLocalSession(): void` on the auth store, alongside the existing `async logout(): Promise<void>`. `clearLocalSession` clears in-memory refs and localStorage only. `logout` keeps its current behaviour (server revoke + durable-token clear) and now calls `clearLocalSession` for the local part.

- [ ] **Step 1: Read the current logout implementation**

Run: `sed -n '170,215p' frontend/src/stores/auth.ts`

Note the exact keys cleared and the calls made, so `clearLocalSession` clears the same local state and nothing more.

- [ ] **Step 2: Write the failing test**

Create `frontend/src/stores/auth.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

const apiLogout = vi.fn().mockResolvedValue(undefined);
const authTokenClear = vi.fn().mockResolvedValue(undefined);
const authTokenSet = vi.fn().mockResolvedValue(undefined);
const authTokenGet = vi.fn().mockResolvedValue(null);

vi.mock('@/api/client', () => ({
    apiLogout: (...a: unknown[]) => apiLogout(...a),
    apiLogin: vi.fn(),
    apiRefreshToken: vi.fn(),
    apiGetProfile: vi.fn(),
}));

vi.mock('@/utils/tauri', () => ({
    isTauri: () => false,
    authTokenClear: () => authTokenClear(),
    authTokenSet: (t: string) => authTokenSet(t),
    authTokenGet: () => authTokenGet(),
}));

describe('auth store session lifecycle', () => {
    beforeEach(() => {
        setActivePinia(createPinia());
        localStorage.clear();
        vi.clearAllMocks();
    });

    it('clearLocalSession wipes local state without revoking the server session', async () => {
        const { useAuthStore } = await import('./auth');
        const auth = useAuthStore();

        auth.clearLocalSession();

        expect(auth.accessToken).toBeNull();
        expect(apiLogout).not.toHaveBeenCalled();
        expect(authTokenClear).not.toHaveBeenCalled();
    });

    it('logout revokes the server session and clears the durable token', async () => {
        const { useAuthStore } = await import('./auth');
        const auth = useAuthStore();

        await auth.logout();

        expect(auth.accessToken).toBeNull();
        expect(apiLogout).toHaveBeenCalled();
    });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `npm --prefix frontend test -- auth.test.ts --run`
Expected: FAIL — `auth.clearLocalSession is not a function`.

- [ ] **Step 4: Implement the split**

In `frontend/src/stores/auth.ts`, add `clearLocalSession` before `logout`, moving the local-state clearing out of `logout` and into it:

```ts
    /**
     * Clear local session state WITHOUT revoking the server-side session or
     * deleting the durable on-disk refresh token.
     *
     * This is the involuntary path — a 401 we could not recover from, or a
     * WebSocket auth failure. Those can be spurious, and the desktop's refresh
     * token is a 10-year credential (LIB-080): destroying it on a transient
     * failure is what caused sessions to die after days or weeks of use. The
     * worst case here is one extra login; the worst case for `logout()` is a
     * permanently destroyed credential.
     */
    function clearLocalSession() {
        accessToken.value = null;
        refreshToken.value = null;
        expiresAt.value = 0;
        pendingTotp.value = false;
        localStorage.removeItem(TOKEN_KEY);
        localStorage.removeItem(REFRESH_TOKEN_KEY);
        localStorage.removeItem(EXPIRES_AT_KEY);
    }
```

Match the exact localStorage keys and refs observed in Step 1 — add any this snippet is missing, and drop any it names that do not exist.

Then make `logout` reuse it, keeping its server-revoke and durable-clear behaviour:

```ts
    async function logout() {
        log.warn('logout called', {
            haveAccessToken: !!accessToken.value,
            haveRefreshToken: !!refreshToken.value,
            stack: new Error().stack?.split('\n').slice(1, 5).join(' | '),
        });
        try {
            if (persistRefreshTokenLocally && refreshToken.value) {
                await apiLogout(refreshToken.value);
            } else {
                await apiLogout();
            }
        } catch (err) {
            log.warn('apiLogout server call failed (localStorage still wiped)', {
                message: (err as Error)?.message ?? String(err),
            });
        }
        clearLocalSession();
        void authTokenClear();
    }
```

Keep whatever else the original `logout` did after clearing (router redirect, durable-token clear) — only the local-state clearing moves.

Export `clearLocalSession` from the store's return object alongside `logout`.

- [ ] **Step 5: Run the test to verify it passes**

Run: `npm --prefix frontend test -- auth.test.ts --run`
Expected: PASS, 2 tests.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/stores/auth.ts frontend/src/stores/auth.test.ts
git commit -m "feat(auth): split involuntary clearLocalSession from voluntary logout

An involuntary logout (spurious 401, WebSocket auth blip) must not revoke
the server session or delete the durable on-disk refresh token — on
desktop that is a 10-year credential."
```

---

### Task 2: Retry once before concluding the session is dead

**Files:**
- Modify: `frontend/src/api/client.ts` (`handleUnauthorized`, ~line 190; its call sites at 273, 622, 659, 672, 700)
- Modify: `frontend/src/composables/useWebSocket.ts:155`
- Test: `frontend/src/api/client.test.ts` (create, or extend if present)

**Interfaces:**
- Consumes: `clearLocalSession()` from Task 1.
- Produces: `handleUnauthorized(url: string): Promise<boolean>` — returns `true` when a forced refresh succeeded and the caller should retry the request once, `false` when the session was cleared.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/api/client.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

const clearLocalSession = vi.fn();
const refresh = vi.fn();

vi.mock('@/stores/auth', () => ({
    useAuthStore: () => ({
        accessToken: 'stale-token',
        refresh,
        clearLocalSession,
        logout: vi.fn(),
        ensureFresh: vi.fn(),
    }),
}));

describe('401 handling', () => {
    beforeEach(() => {
        setActivePinia(createPinia());
        vi.clearAllMocks();
        vi.unstubAllGlobals();
    });

    it('refreshes and retries once before giving up', async () => {
        refresh.mockResolvedValueOnce(undefined);
        const fetchMock = vi
            .fn()
            .mockResolvedValueOnce(new Response('', { status: 401 }))
            .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true }), {
                status: 200,
                headers: { 'Content-Type': 'application/json' },
            }));
        vi.stubGlobal('fetch', fetchMock);

        const { apiGetProfile } = await import('./client');
        const result = await apiGetProfile();

        expect(fetchMock).toHaveBeenCalledTimes(2);
        expect(refresh).toHaveBeenCalledTimes(1);
        expect(clearLocalSession).not.toHaveBeenCalled();
        expect(result).toEqual({ ok: true });
    });

    it('clears the session when the retry also 401s', async () => {
        refresh.mockResolvedValueOnce(undefined);
        const fetchMock = vi
            .fn()
            .mockResolvedValue(new Response('', { status: 401 }));
        vi.stubGlobal('fetch', fetchMock);

        const { apiGetProfile } = await import('./client');
        await apiGetProfile().catch(() => undefined);

        expect(clearLocalSession).toHaveBeenCalled();
    });

    it('clears the session when the forced refresh itself fails', async () => {
        refresh.mockRejectedValueOnce(new Error('refresh failed'));
        const fetchMock = vi
            .fn()
            .mockResolvedValue(new Response('', { status: 401 }));
        vi.stubGlobal('fetch', fetchMock);

        const { apiGetProfile } = await import('./client');
        await apiGetProfile().catch(() => undefined);

        expect(clearLocalSession).toHaveBeenCalled();
    });
});
```

If `apiGetProfile` is not the exported name for `GET /api/auth/me`, substitute the real one — check with `grep -n 'auth/me' frontend/src/api/client.ts`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix frontend test -- client.test.ts --run`
Expected: FAIL — only one fetch call; `clearLocalSession` never called (the old code calls `logout`).

- [ ] **Step 3: Rewrite `handleUnauthorized` to attempt recovery**

Replace the body of `handleUnauthorized` in `frontend/src/api/client.ts`:

```ts
/**
 * Handle a 401.
 *
 * Returns `true` if a forced refresh succeeded and the caller should retry the
 * request once. Returns `false` if the session is genuinely dead and has been
 * cleared locally.
 *
 * A single 401 is NOT proof the session is over: `ensureFreshForRequest`
 * deliberately lets a request through when its refresh failed, so a transient
 * blip (wake-from-sleep, loopback hiccup) surfaces here as a 401 carrying a
 * stale token. Treating that as fatal is what killed long-lived desktop
 * sessions. We give the token one honest chance to be renewed first, and clear
 * only local state — never the server session or the durable token.
 */
async function handleUnauthorized(url: string): Promise<boolean> {
    if (isAuthLifecyclePath(requestPath(url))) return false;

    const log = await (async () => {
        try {
            const { getLogger } = await import('@/utils/logger');
            return getLogger('apiClient');
        } catch {
            return null;
        }
    })();

    try {
        const auth = useAuthStore();
        try {
            await auth.refresh();
            log?.info('401 → refresh succeeded, retrying request once', {
                url: requestPath(url),
            });
            return true;
        } catch (err) {
            log?.warn('401 → refresh failed, clearing local session', {
                url: requestPath(url),
                message: (err as Error)?.message ?? String(err),
            });
            auth.clearLocalSession();
            if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
                window.location.href =
                    `/login?redirect=${encodeURIComponent(window.location.pathname)}`;
            }
            return false;
        }
    } catch {
        return false;
    }
}
```

- [ ] **Step 4: Make the main `request` path retry once**

In `request<T>`, replace the existing `if (response.status === 401) await handleUnauthorized(url);` (~line 273) with a single retry:

```ts
    if (response.status === 401) {
        const shouldRetry = await handleUnauthorized(url);
        if (shouldRetry) {
            let retryAuthHeader: Record<string, string> = {};
            try {
                const auth = useAuthStore();
                if (auth.accessToken) {
                    retryAuthHeader = { Authorization: `Bearer ${auth.accessToken}` };
                }
            } catch { /* Pinia not ready — send without the header */ }

            response = await activeTransport(url, {
                ...options,
                headers: {
                    'Content-Type': 'application/json',
                    ...retryAuthHeader,
                    ...(options.headers ?? {}),
                },
            });
            if (response.status === 401) {
                const auth = useAuthStore();
                auth.clearLocalSession();
            }
        }
    }
```

`response` must be declared with `let`, not `const`, for the reassignment. Adjust the four other call sites (~622, 659, 672, 700) to `await handleUnauthorized(url);` unchanged — they are non-JSON paths where a retry adds complexity for little benefit; the forced refresh still happens, so the *next* request succeeds.

- [ ] **Step 5: Point the WebSocket path at `clearLocalSession`**

In `frontend/src/composables/useWebSocket.ts:155`, replace `await authStore.logout();` with:

```ts
            // A WebSocket auth failure is frequently transient (server restart,
            // wake-from-sleep). Clear local state so the UI re-authenticates,
            // but never revoke the server session or the durable token.
            authStore.clearLocalSession();
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm --prefix frontend test -- client.test.ts --run`
Expected: PASS, 3 tests.

Run: `npm --prefix frontend test --run`
Expected: PASS, all suites.

Run: `npm --prefix frontend run build`
Expected: `vue-tsc` clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/client.ts frontend/src/api/client.test.ts frontend/src/composables/useWebSocket.ts
git commit -m "fix(auth): refresh and retry once before treating a 401 as fatal

ensureFreshForRequest deliberately lets a request through when its
refresh failed, so a transient blip surfaces as a 401 carrying a stale
token. handleUnauthorized treated that single 401 as proof the session
was over and called the destructive logout, revoking the server session
and deleting the durable 10-year refresh token. One unlucky moment in
weeks of hourly refreshes was enough to end the session.

Now: force one refresh, retry once, and clear only local state if that
also fails."
```

---

## Phase 2 — The credential service

Independent of Phase 1. Fixes the session-revocation hole.

### Task 3: `CredentialService`

**Files:**
- Create: `crates/librarium-server/src/services/credentials.rs`
- Modify: `crates/librarium-server/src/services/mod.rs`

**Interfaces:**
- Consumes: `Database::{get_user_auth_by_username, get_user_auth_by_id, set_user_password, revoke_all_sessions, create_user_with_options, list_users, write_audit_log}`; `services::validate_password_policy`; `librarium_types::AdminUser`.
- Produces:
  ```rust
  pub struct CredentialService<'a> { /* db, policy */ }
  impl<'a> CredentialService<'a> {
      pub fn new(db: &'a Database, policy: &'a AuthConfig) -> Self;
      pub async fn set_password(&self, username: &str, new_password: &str) -> AppResult<()>;
      pub async fn change_password(&self, user_id: &str, current: &str, new_password: &str) -> AppResult<()>;
      pub async fn create_user(&self, username: &str, password: &str, is_admin: bool) -> AppResult<String>;
      pub async fn list_users(&self) -> AppResult<Vec<AdminUser>>;
  }
  pub fn hash_password(password: &str) -> AppResult<String>;
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/librarium-server/src/services/credentials.rs` with tests only for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthConfig;
    use crate::db::Database;

    async fn setup() -> (Database, AuthConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .unwrap();
        let mut cfg = AuthConfig::default();
        cfg.min_password_length = 8;
        (db, cfg, dir)
    }

    #[tokio::test]
    async fn set_password_rejects_a_policy_violation_without_writing() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", false).await.unwrap();

        let before = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;
        let err = svc.set_password("alice", "short").await.unwrap_err();
        let after = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;

        assert!(matches!(err, AppError::InvalidInput(_)));
        assert_eq!(before, after, "hash must be untouched after a policy failure");
    }

    #[tokio::test]
    async fn set_password_replaces_the_hash() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", false).await.unwrap();

        let before = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;
        svc.set_password("alice", "replacementpw").await.unwrap();
        let after = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;

        assert_ne!(before, after);
        let parsed = argon2::PasswordHash::new(&after).unwrap();
        assert!(argon2::Argon2::default()
            .verify_password(b"replacementpw", &parsed)
            .is_ok());
        assert!(argon2::Argon2::default()
            .verify_password(b"originalpw", &parsed)
            .is_err());
    }

    #[tokio::test]
    async fn set_password_revokes_existing_sessions() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        let user_id = svc.create_user("alice", "originalpw", false).await.unwrap();

        let expires = chrono::Utc::now() + chrono::Duration::days(1);
        db.create_session("session-jti-1", &user_id, expires).await.unwrap();
        assert!(!db.is_session_revoked("session-jti-1").await.unwrap());

        svc.set_password("alice", "replacementpw").await.unwrap();

        assert!(
            db.is_session_revoked("session-jti-1").await.unwrap(),
            "a password reset must not leave an existing session alive"
        );
    }

    #[tokio::test]
    async fn set_password_errors_on_an_unknown_user() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        assert!(svc.set_password("nobody", "replacementpw").await.is_err());
    }

    #[tokio::test]
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

    #[tokio::test]
    async fn create_user_then_list_users_round_trips() {
        let (db, cfg, _dir) = setup().await;
        let svc = CredentialService::new(&db, &cfg);
        svc.create_user("alice", "originalpw", true).await.unwrap();

        let users = svc.list_users().await.unwrap();
        let alice = users.iter().find(|u| u.username == "alice").unwrap();
        assert!(alice.is_admin);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p librarium-server credentials 2>&1 | tail -20`
Expected: FAIL to compile — `CredentialService` not found.

- [ ] **Step 3: Implement the service**

Prepend to `crates/librarium-server/src/services/credentials.rs`:

```rust
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
    /// Ordering matters: policy and user lookup are checked before any write,
    /// so a rejected call leaves the stored hash untouched. Revocation and the
    /// audit entry happen after the row update and are best-effort — failing
    /// them is logged, never rolled back, because leaving the user with an
    /// unknown credential state is worse than a missing audit line.
    pub async fn set_password(&self, username: &str, new_password: &str) -> AppResult<()> {
        validate_password_policy(new_password, self.policy)?;

        let (user_id, username, _) = self
            .db
            .get_user_auth_by_username(username)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User '{username}' not found")))?;

        let hash = hash_password(new_password)?;
        self.db.set_user_password(&user_id, &hash, false).await?;

        self.revoke_and_audit(&user_id, &username, "password_reset").await;
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

        self.revoke_and_audit(user_id, &username, "password_changed").await;
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
```

If `AppError` has no `NotFound` variant, use whichever variant the codebase uses for a missing record — check with `grep -n 'pub enum AppError' -A 20 crates/librarium-server/src/error.rs`.

- [ ] **Step 4: Export the module**

In `crates/librarium-server/src/services/mod.rs`, add `pub mod credentials;` beside the other `pub mod` lines, and add to the re-export block:

```rust
pub use credentials::{hash_password, CredentialService};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p librarium-server credentials 2>&1 | tail -20`
Expected: PASS, 6 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/librarium-server/src/services/credentials.rs crates/librarium-server/src/services/mod.rs
git commit -m "feat(auth): add CredentialService as the single password choke point

Validate policy, Argon2 hash, update, revoke every session the user
holds, audit. Takes &Database rather than AppState so the CLI can call it
with no server running — which is exactly when recovery is needed."
```

---

### Task 4: Route the existing password paths through the service

**Files:**
- Modify: `crates/librarium-server/src/routes/auth.rs` (`change_password`, ~lines 297-357)
- Modify: `crates/librarium-server/src/routes/admin.rs` (reset branch ~lines 236-252; local `hash_password` ~lines 41-47)

**Interfaces:**
- Consumes: `CredentialService::{new, change_password, set_password}` and `services::hash_password` from Task 3.
- Produces: no new API. Both routes gain session revocation as a side effect.

- [ ] **Step 1: Write the failing integration test**

Add to the existing server test suite (`crates/librarium-server/tests/`; put it in the file covering auth routes, or create `password_revocation_tests.rs`):

```rust
//! A password change must not leave older sessions usable.

#[actix_web::test]
async fn changing_a_password_revokes_existing_sessions() {
    // Build the app the same way the neighbouring tests in this directory do:
    // copy their harness/setup helper rather than inventing a new one.
    let (app, state, _tmp) = crate::common::spawn_test_app_with_auth().await;

    // Log in, capture the refresh token's session, then change the password.
    let login = crate::common::login(&app, "admin", "originalpw").await;
    let old_refresh = login.refresh_token.clone();

    let resp = crate::common::post_authed(
        &app,
        "/api/auth/change-password",
        &login.access_token,
        serde_json::json!({
            "current_password": "originalpw",
            "new_password": "replacementpw",
        }),
    )
    .await;
    assert!(resp.status().is_success());

    // The pre-change refresh token must no longer work.
    let refreshed = crate::common::post_json(
        &app,
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": old_refresh }),
    )
    .await;
    assert_eq!(
        refreshed.status(),
        401,
        "a session issued before the password change must be revoked"
    );
}
```

Adapt the helper names to whatever the existing tests in that directory use — run `ls crates/librarium-server/tests/` and read the closest auth test first. If no auth-route harness exists, assert the same property directly against `CredentialService` plus `db.is_session_revoked`, which Task 3 already demonstrates.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p librarium-server revokes_existing_sessions 2>&1 | tail -20`
Expected: FAIL — the old refresh token still works (200, not 401).

- [ ] **Step 3: Delegate `change_password`**

Replace the body of `change_password` in `routes/auth.rs` (keep the attribute and signature):

```rust
    let user = req
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;

    let current_password = body.current_password.trim();
    let new_password = body.new_password.trim();

    if current_password.is_empty() || new_password.is_empty() {
        return Err(AppError::InvalidInput(
            "Current password and new password are required".to_string(),
        ));
    }

    crate::services::CredentialService::new(&state.db, &config.auth)
        .change_password(&user.user_id, current_password, new_password)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": true })))
```

Remove the now-unused Argon2 imports from `auth.rs` if nothing else there uses them.

- [ ] **Step 4: Delegate the admin reset**

In `routes/admin.rs`, replace the `reset_password` branch:

```rust
    // Reset password.
    if let Some(ref new_password) = body.reset_password {
        let (_, target_username, _) = state
            .db
            .get_user_auth_by_id(&user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User '{user_id}' not found")))?;

        crate::services::CredentialService::new(&state.db, &config.auth)
            .set_password(&target_username, new_password)
            .await?;

        state
            .db
            .write_audit_log(
                Some(&admin.user_id),
                Some(&admin.username),
                "user_password_reset",
                Some(&format!("Admin reset password for user {user_id}")),
                None,
                true,
            )
            .await?;
    }
```

Delete the local `hash_password` function (lines ~41-47) and its now-unused Argon2 imports; if any other code in `admin.rs` needs hashing, import `crate::services::hash_password` instead.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p librarium-server 2>&1 | grep -E '^test result|FAILED'`
Expected: every line `ok`, 0 failed.

- [ ] **Step 6: Commit**

```bash
git add crates/librarium-server/src/routes/auth.rs crates/librarium-server/src/routes/admin.rs crates/librarium-server/tests/
git commit -m "fix(auth): revoke sessions when a password changes

Neither change-password nor the admin reset revoked the user's sessions,
so resetting a compromised account left the attacker logged in — and on
desktop, with a 10-year non-rotating refresh token, effectively forever.
Both now delegate to CredentialService, which revokes as part of the
mutation. Also removes the duplicate Argon2 hashing in admin.rs."
```

---

## Phase 3 — CLI recovery

Depends on Phase 2.

### Task 5: `librarium admin` subcommands

**Files:**
- Create: `crates/librarium-server/src/cli.rs`
- Modify: `crates/librarium-server/src/main.rs`
- Modify: `crates/librarium-server/src/lib.rs` (export the module)
- Modify: `crates/librarium-server/Cargo.toml`

**Interfaces:**
- Consumes: `CredentialService` from Task 3; `AppConfig::load_from_file`; `Database::new`.
- Produces: `pub enum AdminCommand` and `pub async fn run_admin(cmd: AdminCommand, config: &AppConfig) -> anyhow::Result<()>`.

- [ ] **Step 1: Add the dependency**

In `crates/librarium-server/Cargo.toml`, under `[dependencies]`:

```toml
rpassword = "7"
```

Run: `cargo fetch 2>&1 | tail -3`

- [ ] **Step 2: Write the failing test**

Create `crates/librarium-server/tests/cli_admin_tests.rs`:

```rust
//! The CLI must be able to repair credentials with no server running.

use librarium::cli::{run_admin, AdminCommand};
use librarium::config::AppConfig;
use librarium::db::Database;
use librarium::services::CredentialService;

async fn temp_config() -> (AppConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("librarium.db");
    let mut cfg = AppConfig::default();
    cfg.database.path = db_path.to_string_lossy().into_owned();
    cfg.auth.min_password_length = 8;
    (cfg, dir)
}

#[tokio::test]
async fn set_password_updates_the_stored_hash() {
    let (cfg, _dir) = temp_config().await;

    let db = Database::new(&format!("sqlite:{}?mode=rwc", cfg.database.path))
        .await
        .unwrap();
    CredentialService::new(&db, &cfg.auth)
        .create_user("alice", "originalpw", false)
        .await
        .unwrap();
    let before = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;
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

    let db = Database::new(&format!("sqlite:{}?mode=rwc", cfg.database.path))
        .await
        .unwrap();
    let after = db.get_user_auth_by_username("alice").await.unwrap().unwrap().2;
    assert_ne!(before, after);
}

#[tokio::test]
async fn create_user_then_list_users_round_trips() {
    let (cfg, _dir) = temp_config().await;

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

    let db = Database::new(&format!("sqlite:{}?mode=rwc", cfg.database.path))
        .await
        .unwrap();
    let users = db.list_users().await.unwrap();
    let bob = users.iter().find(|u| u.username == "bob").unwrap();
    assert!(bob.is_admin);
}
```

The `password: Some(..)` field exists **only** so tests can bypass the interactive prompt. It is not exposed as a CLI flag — see Step 3.

- [ ] **Step 3: Implement the CLI module**

Create `crates/librarium-server/src/cli.rs`:

```rust
//! `librarium admin …` — offline credential repair.
//!
//! These subcommands open the SQLite database directly rather than calling the
//! HTTP API, because they must work when you cannot authenticate (a forgotten
//! password) or when the server will not start. Recovery tooling must not
//! depend on the thing that is broken.
//!
//! Filesystem access to the database IS the authorization model here, which is
//! the right trust boundary for a self-hosted app: anyone who can read the
//! SQLite file can already read every vault file it indexes.

use crate::config::AppConfig;
use crate::db::Database;
use crate::services::CredentialService;
use anyhow::{anyhow, Context};

#[derive(clap::Subcommand, Debug)]
pub enum AdminCommand {
    /// Set (or reset) a user's password. Prompts; never takes it on the command line.
    SetPassword {
        username: String,
        /// Test-only escape hatch. Not a CLI flag — passwords on the command
        /// line are visible in `ps` output and shell history.
        #[arg(skip)]
        password: Option<String>,
    },
    /// Create a user. Prompts for the password.
    CreateUser {
        username: String,
        #[arg(long)]
        admin: bool,
        #[arg(skip)]
        password: Option<String>,
    },
    /// List every user account.
    ListUsers,
}

/// Prompt twice for a password, with no echo, and require the two to match.
fn prompt_new_password() -> anyhow::Result<String> {
    let first = rpassword::prompt_password("New password: ")
        .context("Failed to read password")?;
    let second = rpassword::prompt_password("Confirm password: ")
        .context("Failed to read password confirmation")?;
    if first != second {
        return Err(anyhow!("Passwords did not match"));
    }
    if first.trim().is_empty() {
        return Err(anyhow!("Password cannot be empty"));
    }
    Ok(first)
}

async fn open_db(config: &AppConfig) -> anyhow::Result<Database> {
    let url = if config.database.path.starts_with("sqlite:") {
        config.database.path.clone()
    } else {
        format!("sqlite:{}?mode=rwc", config.database.path)
    };
    Database::new(&url)
        .await
        .with_context(|| format!("Failed to open the database at {}", config.database.path))
}

pub async fn run_admin(cmd: AdminCommand, config: &AppConfig) -> anyhow::Result<()> {
    let db = open_db(config).await?;
    let svc = CredentialService::new(&db, &config.auth);

    match cmd {
        AdminCommand::SetPassword { username, password } => {
            let password = match password {
                Some(p) => p,
                None => prompt_new_password()?,
            };
            svc.set_password(&username, &password).await?;
            println!("Password updated for '{username}'. All existing sessions were revoked.");
        }
        AdminCommand::CreateUser {
            username,
            admin,
            password,
        } => {
            let password = match password {
                Some(p) => p,
                None => prompt_new_password()?,
            };
            svc.create_user(&username, &password, admin).await?;
            println!(
                "Created {} user '{username}'.",
                if admin { "admin" } else { "standard" }
            );
        }
        AdminCommand::ListUsers => {
            let users = svc.list_users().await?;
            if users.is_empty() {
                println!("No users exist yet.");
                return Ok(());
            }
            println!("{:<28} {:<8} {:<8}", "USERNAME", "ADMIN", "ACTIVE");
            for u in users {
                println!(
                    "{:<28} {:<8} {:<8}",
                    u.username,
                    if u.is_admin { "yes" } else { "no" },
                    if u.is_active { "yes" } else { "no" }
                );
            }
        }
    }
    Ok(())
}
```

Add `pub mod cli;` to `crates/librarium-server/src/lib.rs` beside the other module declarations.

- [ ] **Step 4: Wire the subcommand into `main.rs`**

In `crates/librarium-server/src/main.rs`, add the optional subcommand to `Args`:

```rust
#[derive(Parser, Debug)]
#[command(name = "librarium", about = "Librarium knowledge server", version)]
struct Args {
    /// Path to config.toml.
    /// Precedence: --config flag > LIBRARIUM_CONFIG env var > exe-adjacent config.toml > ./config.toml
    #[arg(
        short,
        long,
        default_value = "./config.toml",
        env = "LIBRARIUM_CONFIG",
        value_name = "PATH"
    )]
    config: std::path::PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Offline account administration (works with the server stopped).
    #[command(subcommand)]
    Admin(librarium::cli::AdminCommand),
}
```

Then, in `main`, dispatch after the config is loaded and **before** `librarium::run(config).await`:

```rust
    if let Some(Command::Admin(admin_cmd)) = args.command {
        return librarium::cli::run_admin(admin_cmd, &config).await;
    }
    librarium::run(config).await
```

`args.config` is moved into `locate_config` before this point; capture `args.command` first if the borrow checker objects.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p librarium-server --test cli_admin_tests 2>&1 | tail -12`
Expected: PASS, 2 tests.

Verify the no-subcommand path still parses:

Run: `cargo run -p librarium-server -- --help 2>&1 | head -20`
Expected: help text listing `admin` as a subcommand, with `--config` still present.

- [ ] **Step 6: Commit**

```bash
git add crates/librarium-server/src/cli.rs crates/librarium-server/src/main.rs crates/librarium-server/src/lib.rs crates/librarium-server/Cargo.toml Cargo.lock crates/librarium-server/tests/cli_admin_tests.rs
git commit -m "feat(cli): add librarium admin set-password/create-user/list-users

Offline credential repair. Opens SQLite directly rather than calling the
HTTP API, because it must work when you cannot authenticate or the server
will not start. Passwords are prompted with no echo, never taken from
argv, which is visible in ps output and shell history."
```

---

## Phase 4 — Desktop reset, Settings, docs

Depends on Phase 2.

### Task 6: Desktop local-password reset command

**Files:**
- Modify: `crates/librarium-tauri/src/lib.rs`
- Modify: `frontend/src/utils/tauri.ts`
- Modify: `frontend/src/pages/LoginPage.vue`

**Interfaces:**
- Consumes: `CredentialService` from Task 3.
- Produces: Tauri command `auth_reset_local_password(username: String, new_password: String) -> Result<(), String>`; frontend wrapper `resetLocalPassword(username: string, newPassword: string): Promise<void>` in `utils/tauri.ts`.

- [ ] **Step 1: Add the Tauri command**

In `crates/librarium-tauri/src/lib.rs`, beside the existing `auth_token_*` commands:

```rust
/// Reset the local admin's password without needing the old one.
///
/// Desktop only, and deliberately a Tauri command rather than an HTTP route:
/// the embedded server listens on loopback, where any local process — and the
/// browser build — could reach an HTTP endpoint. A Tauri command is callable
/// only from this app's own WebView.
///
/// No further gate is required. The server is single-user and loopback-only,
/// and the vault files plus the SQLite database are already readable by this
/// OS account, so the password guards against a casual glance, not against
/// someone holding the unlocked machine.
#[cfg(desktop)]
#[tauri::command]
async fn auth_reset_local_password(
    config: tauri::State<'_, DesktopConfig>,
    username: String,
    new_password: String,
) -> Result<(), String> {
    let cfg = config.0.clone();
    let url = format!("sqlite:{}?mode=rwc", cfg.database.path);
    let db = librarium::db::Database::new(&url)
        .await
        .map_err(|e| format!("Could not open the database: {e}"))?;
    librarium::services::CredentialService::new(&db, &cfg.auth)
        .set_password(&username, &new_password)
        .await
        .map_err(|e| e.to_string())
}
```

Register it in the `invoke_handler` list beside `auth_token_get`/`auth_token_set`/`auth_token_clear`, gated the same way the other desktop-only commands are.

`DesktopConfig` is a newtype holding the loaded `AppConfig`. If `run_setup` already manages the config in Tauri state, reuse that; otherwise add `app.manage(DesktopConfig(config.clone()));` next to the existing `app.manage(SessionStore::new(...))` call and define:

```rust
#[cfg(desktop)]
struct DesktopConfig(librarium::config::AppConfig);
```

- [ ] **Step 2: Grant the command in the Tauri ACL**

Add `allow-auth-reset-local-password` to the desktop capability file in `crates/librarium-tauri/capabilities/`, following the existing `allow-auth-token-*` entries. Do **not** add it to any Android capability set.

- [ ] **Step 3: Add the frontend wrapper**

In `frontend/src/utils/tauri.ts`, beside `authTokenClear`:

```ts
/**
 * Reset the local desktop admin's password without the old one.
 *
 * Throws in a browser context — the command exists only in the desktop shell.
 */
export const resetLocalPassword = async (
  username: string,
  newPassword: string,
): Promise<void> => {
  if (!isTauri()) throw new Error('Password reset is only available in the desktop app');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('auth_reset_local_password', { username, newPassword });
};
```

- [ ] **Step 4: Add the login-screen affordance**

In `frontend/src/pages/LoginPage.vue`, render a "Forgot password?" link **only** when `isTauri()`. It opens a dialog asking for the username (default `admin`) and a new password twice, calls `resetLocalPassword`, then signs in with the new credentials. Show the returned error text verbatim on failure — the policy messages from `validate_password_policy` are already user-facing.

- [ ] **Step 5: Verify the build**

Run: `npm --prefix frontend run build`
Expected: `vue-tsc` clean.

Run: `cargo check -p librarium-tauri --target aarch64-linux-android 2>&1 | tail -5`
Expected: clean — proves the command is properly `#[cfg(desktop)]`-gated and absent from the Android build. (Per AGENTS.md this needs the Rust target but not the NDK.)

- [ ] **Step 6: Commit**

```bash
git add crates/librarium-tauri/src/lib.rs crates/librarium-tauri/capabilities/ frontend/src/utils/tauri.ts frontend/src/pages/LoginPage.vue
git commit -m "feat(desktop): add a local password reset on the login screen

A Tauri command rather than an HTTP route: the embedded server is
loopback, where any local process or the browser build could reach an
endpoint, while a Tauri command is callable only from this app's WebView.
cfg(desktop)-gated so it does not exist on Android."
```

---

### Task 7: Security settings panel

**Files:**
- Create: `frontend/src/components/settings/SecurityPanel.vue`
- Modify: `frontend/src/components/settings/SettingsModal.vue`

**Interfaces:**
- Consumes: `resetLocalPassword` from Task 6; the existing `POST /api/auth/change-password`.
- Produces: no new exports.

- [ ] **Step 1: Read the existing panel pattern**

Run: `sed -n '1,60p' frontend/src/components/settings/AboutPanel.vue` and `grep -n 'Panel' frontend/src/components/settings/SettingsModal.vue`

Follow that structure exactly — props, emits, and how a panel is registered as a tab.

- [ ] **Step 2: Build the panel**

Create `frontend/src/components/settings/SecurityPanel.vue`, desktop-only (render nothing, or a "managed by your server administrator" note, when `!isTauri()`), with:

- **Auth currently enabled:** a change-password form (current, new, confirm) posting to `/api/auth/change-password`, and a "Turn off password protection" action.
- **Auth currently disabled:** a "Turn on password protection" form taking a new password twice.
- A persistent, plainly-worded note: **"Turning password protection on or off takes effect the next time Librarium starts."** `AppConfig` is read once at startup, so the UI must not imply an immediate change.

Surface server error text verbatim; the policy messages are already user-facing.

- [ ] **Step 3: Register the panel**

Add it to `SettingsModal.vue` alongside About and API Keys, following the existing registration pattern.

- [ ] **Step 4: Verify**

Run: `npm --prefix frontend test --run`
Expected: PASS, all suites.

Run: `npm --prefix frontend run build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/settings/SecurityPanel.vue frontend/src/components/settings/SettingsModal.vue
git commit -m "feat(desktop): add a Security settings panel

Enable, disable, or change the desktop password without editing
config.toml. States the restart requirement plainly rather than appearing
to apply a change that has not happened."
```

---

### Task 8: Documentation and version bump

**Files:**
- Modify: `config.example.toml`, `README.md`, `docs/DESIGN.md`, `AGENTS.md`
- Modify: version files via `cargo xtask bump-version`

- [ ] **Step 1: Demote the plaintext bootstrap in `config.example.toml`**

The generated-credentials flow already exists (`lib.rs:300-320`: `bootstrap_admin_generated_if_empty` → `write_first_run_credentials` → forced change at first login). Rewrite the bootstrap comment block so that path is the documented default, and mark `bootstrap_admin_password` as a discouraged alternative that puts a plaintext credential in a file people commit by accident.

- [ ] **Step 2: Document recovery in `README.md`**

Add a short "Forgot your password?" subsection under the auth/quick-start material: desktop users click "Forgot password?" on the login screen; server operators run `librarium admin set-password <username>` on the host.

- [ ] **Step 3: Document the architecture in `docs/DESIGN.md`**

Record: `CredentialService` as the single choke point (and that every password change now revokes sessions); the three transports and **why** each was chosen — CLI direct-SQLite so recovery survives a server that will not start, Tauri command so the desktop reset is not reachable from other local processes, HTTP for authenticated in-app changes; and the 401 retry plus the voluntary/involuntary logout split, with the reason (a spurious 401 must not destroy a 10-year credential).

- [ ] **Step 4: Document the CLI in `AGENTS.md`**

Add the three `librarium admin` subcommands to the operator-facing command list.

- [ ] **Step 5: Bump the version**

```bash
cargo xtask bump-version
git diff
```

Review, confirm every file moved together.

- [ ] **Step 6: Full verification**

```bash
cargo test -p librarium-server 2>&1 | grep -E '^test result|FAILED'
npm --prefix frontend test --run
npm --prefix frontend run build
```

Expected: 0 failures everywhere, clean build.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "docs: document password recovery; bump version"
```

---

## Self-Review

**Spec coverage.** Defect 1 (no recovery) → Tasks 5 and 6. Defect 2 (spurious 401 kills the session) → Tasks 1 and 2. Defect 3 (no session revocation) → Tasks 3 and 4. Settings UI → Task 7. Documentation, including the `config.example.toml` correction → Task 8. Every "Files affected" row in the spec appears in a task.

**Placeholders.** None. Every code step carries runnable code; every test step carries real assertions. Three steps deliberately say "match the existing pattern" (the test harness in Task 4, the panel structure in Task 7, the ACL entry in Task 6) — each names the exact file to read first, because inventing a parallel pattern would be worse than following the established one.

**Type consistency.** `CredentialService::new(&Database, &AuthConfig)` is used identically in Tasks 3-6. `set_password(&self, username, new_password)` takes a **username**; `change_password(&self, user_id, current, new)` takes a **user id** — Task 4's admin branch therefore looks up the username first, which the code shows. `AdminUser` (from `librarium-types`) is the listing type throughout; the spec's `UserSummary` is dropped, as recorded above. `clearLocalSession()` is spelled identically in Tasks 1, 2, and 7. `handleUnauthorized` returns `Promise<boolean>` in Task 2 and every call site is updated there.

**One deviation from the spec**, recorded above: `UserSummary` is replaced by the existing `librarium_types::AdminUser`.
