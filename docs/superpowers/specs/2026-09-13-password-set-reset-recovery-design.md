# Password set, reset, and recovery — design

**Date:** 2026-09-13
**Status:** Approved, ready for implementation planning
**Scope:** `librarium-server` (credential service, CLI), `librarium-tauri`
(desktop reset command), `frontend` (session lifecycle, Settings panel), docs

## Problem

Three related defects in credential handling.

### 1. There is no way to recover a forgotten password

`POST /api/auth/change-password` requires an authenticated session. An admin can
reset another user's password (`routes/admin.rs:236`). There is no email flow,
no recovery code, no CLI, and no offline path.

Consequences: a single-user desktop instance whose owner forgets the password is
permanently locked out. A server with one admin account is equally stuck. The
only recovery today is editing the SQLite `users` table by hand.

### 2. A single spurious 401 destroys the desktop session

The desktop is configured to stay signed in indefinitely: a 10-year refresh-token
TTL floor (`librarium-tauri/src/lib.rs:528`, LIB-080/LIB-089), a non-rotating
refresh path for long-lived sessions (`routes/auth.rs:184`), a JWT secret
persisted to `config.toml` on first launch, and a durable on-disk token store
(`{data_dir}/session.json`). Despite this, users are returned to the login screen
after days or weeks of continuous use.

The mechanism:

- `ensureFreshForRequest` (`frontend/src/api/client.ts:182-188`) catches any
  refresh failure and **deliberately lets the request proceed** with a stale
  token, commenting that "the 401 handler below owns logout/redirect".
- `handleUnauthorized` (`client.ts:190`) responds to **one** 401 by calling
  `auth.logout()` immediately — no retry, no second refresh attempt.
- `logout()` (`frontend/src/stores/auth.ts:175`) is destructive: it calls
  `apiLogout(refreshToken)`, which **revokes the session server-side**, and
  `authTokenClear()`, which deletes the durable on-disk token.

So one transient failure permanently destroys the 10-year credential. The access
token refreshes hourly, and `refresh()` already retries three times with backoff
for transient errors — but when those retries are exhausted the request goes out
with a stale token anyway, 401s, and the session is gone. Over weeks of hourly
refreshes only one unlucky moment is needed. The probability compounds with
uptime, which matches the reported "days or weeks of normal use".

**Verification honesty:** this is a well-evidenced fix for a mechanism that
demonstrably destroys sessions on a single spurious 401. It is *not* a confirmed
reproduction of the original report — the failure is intermittent and no desktop
install exists on the development machine. If expiry persists after this change,
`logout()` already logs a caller stack trace to `frontend.log`, which will
identify the real trigger.

### 3. Password changes do not revoke existing sessions

Neither `change-password` (`routes/auth.rs:340`) nor the admin reset
(`routes/admin.rs:236`) revokes the user's sessions. Resetting a compromised
account's password leaves the attacker's session alive — and on desktop, with a
10-year non-rotating refresh token, effectively forever. Found while surveying
for this epic; not previously reported.

### Not a problem (corrected during design)

An earlier framing of this epic included "fix the plaintext bootstrap password".
A secure path already exists: when `auth.enabled` is true and no bootstrap
password is configured, `lib.rs:300-320` calls
`bootstrap_admin_generated_if_empty`, writes the generated credentials to a file
via `write_first_run_credentials`, and forces a password change at first login.
What remains is documentation: `config.example.toml` still presents the plaintext
`bootstrap_admin_password` as the primary route. That is the only work in this
area.

## Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| Recovery mechanism | CLI subcommand + desktop in-app reset | Both work offline. No SMTP, preserving the air-gapped/offline-first stance stated in `config.example.toml`. |
| Rejected | Email reset link | Requires SMTP config and outbound network; conflicts with offline-first. |
| Rejected | One-time recovery code | Relies on the user retaining a code they will not retain. The CLI is a stronger backstop with no user burden. |
| Desktop reset gate | None beyond running the app | On desktop the server binds loopback, is single-user, and the vault files plus SQLite DB are already readable by that OS account. The password guards against a casual glance, not against someone holding the unlocked machine. |
| Desktop reset transport | Tauri command, **not** an HTTP route | A loopback HTTP route is reachable by any local process and by the browser build. A Tauri command is reachable only from the desktop shell's own WebView. This is what makes "no extra gate" defensible. |
| CLI transport | Direct SQLite access, in-process | Recovery tooling must not depend on the thing that is broken. An HTTP-based tool is useless when you are locked out or the server will not start. |
| Credential logic | One shared service, three thin callers | Argon2 parameters and password policy must live in exactly one place. Drift in credential hashing is a security bug, not a style issue. |

## Architecture

### `services/credentials.rs` (new)

The single choke point for every password mutation. Lives in `services/` because
the repo treats that as business logic and `routes/` as thin transport, and
because it must be callable without HTTP.

```rust
pub struct CredentialService<'a> {
    db: &'a Database,
    policy: &'a AuthConfig,
}

impl<'a> CredentialService<'a> {
    pub fn new(db: &'a Database, policy: &'a AuthConfig) -> Self;

    /// Validate policy, hash, update, revoke sessions, audit.
    pub async fn set_password(&self, username: &str, new_password: &str) -> AppResult<()>;

    /// Verify the current password first; then as `set_password`.
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: &str,
        new_password: &str,
    ) -> AppResult<()>;

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        is_admin: bool,
    ) -> AppResult<String>;

    pub async fn list_users(&self) -> AppResult<Vec<UserSummary>>;
}

pub struct UserSummary {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
    pub must_change_password: bool,
}
```

`set_password` ordering: validate against `AuthConfig` policy → Argon2 hash →
update the user row → revoke all that user's sessions → write an audit entry.

The write is atomic in the sense that policy failure or an unknown username
returns before any mutation. Session revocation and audit logging happen after
the row update; failure there is logged but does **not** roll back the password
change, since leaving the user with an unknown credential state is worse than a
missing audit line.

**Refactor in scope:** `routes/auth.rs`'s `change_password` and
`routes/admin.rs`'s reset branch move onto this service. Both gain session
revocation as a side effect, fixing defect 3.

### Callers

| Caller | Transport | Notes |
| --- | --- | --- |
| CLI | Opens SQLite directly via `Database::new` | Works with the server stopped. `Database::new` runs migrations, so a fresh DB is fine. |
| Desktop reset | `#[cfg(desktop)]` Tauri command | Not present in the Android build, where the trust model differs. |
| HTTP routes | Authenticated, as today | Normal in-app operation. |

### CLI

`main.rs` gains an optional `clap` subcommand. With no subcommand it runs the
server exactly as now, so existing invocations are unaffected.

```
librarium admin set-password <username>
librarium admin create-user <username> [--admin]
librarium admin list-users
```

Passwords are read with `rpassword` (prompt, no echo, confirm twice) and are
**never** accepted as an argv argument — argv is visible in `ps` output and
lands in shell history. Config resolution reuses the existing `locate_config`
so the CLI targets the same database the server uses.

### Frontend session lifecycle (defect 2)

Two changes:

1. **Retry before concluding the session is dead.** On a 401,
   `handleUnauthorized` forces one refresh and retries the original request
   once. Only if that also returns 401 is the session treated as invalid. This
   makes the handler live up to its own "owns logout/redirect" comment instead
   of treating a single failure as fatal.

2. **Separate voluntary from involuntary logout.**
   - `logout()` — explicit user action. Unchanged: revokes server-side, clears
     the durable token.
   - `clearLocalSession()` — involuntary. Clears in-memory and localStorage
     state only; leaves the server session and the on-disk token intact.

   The 401 path and the WebSocket path (`composables/useWebSocket.ts:155`) use
   `clearLocalSession`. A transient failure then costs one re-login at worst
   rather than destroying the credential.

### Desktop reset and Settings

`auth_reset_local_password(new_password)` — a `#[cfg(desktop)]` Tauri command in
`librarium-tauri` calling `CredentialService` against the embedded server's
SQLite file. It resets the single local admin, revokes sessions, writes an audit
entry, and returns success; the frontend then logs in with the new password.

The login screen shows "Forgot password?" only when `isTauri()` is true. The
browser build never renders it and the command does not exist on Android.

`SecurityPanel.vue` joins `AboutPanel`/`ApiKeysPanel` in
`components/settings/`, desktop-only, with three states: auth disabled (offer to
enable and set a password), auth enabled (change password, disable), and a
pointer to the CLI for server deployments. Enabling writes `auth.enabled = true`
plus the new user and persists config through the same `write_to_file` path
already used for the JWT secret.

**`AppConfig` is read once at startup, so enabling or disabling auth requires a
server restart to take effect.** The panel must state this plainly rather than
appearing to apply a change that has not happened.

## Testing

**`CredentialService`** (temp SQLite, matching existing server test style):
policy accepted and rejected per config knob; the new hash verifies against the
new password and fails against the old; **sessions are revoked after a reset**
(the regression test for defect 3); unknown username errors cleanly;
`change_password` rejects a wrong current password.

**CLI:** `set-password` against a temp DB then verify the hash; `create-user`
then `list-users` round-trip; a subcommand-free invocation still parses as
"run the server".

**Frontend session lifecycle** — the assertions that would have caught defect 2,
with mocked fetch:
- 401 → refresh succeeds → retry succeeds → **no logout**, request result
  returned.
- 401 → refresh succeeds → retry 401s → session cleared.
- involuntary clear does **not** call `apiLogout` and does **not** call
  `authTokenClear`.
- explicit `logout()` still calls both.

**Not covered by automated tests:** the desktop reset command and the Settings
panel require a running Tauri shell. Verified manually; noted rather than
implied.

## Phasing

One spec, four shippable phases.

| Phase | Contents | Depends on |
| --- | --- | --- |
| 1 | Expiry fix: 401 retry, non-destructive involuntary logout | nothing — ships first |
| 2 | `CredentialService`; refactor `change-password` and admin reset onto it | nothing |
| 3 | CLI `admin` subcommands | phase 2 |
| 4 | Desktop reset command, `SecurityPanel`, docs | phase 2 |

Phase 1 is independent and fixes a live bug, so it can merge while the rest is
in progress.

## Out of scope

- Email/SMTP password reset.
- Multi-user account management UI for servers (the CLI covers operators).
- Changing the desktop's 10-year refresh TTL or the non-rotating refresh policy —
  both are deliberate (LIB-080/LIB-089) and are not the cause of defect 2.
- TOTP recovery. Separate mechanism, separate epic.

## Files affected

| File | Change |
| --- | --- |
| `crates/librarium-server/src/services/credentials.rs` | New. The credential service. |
| `crates/librarium-server/src/services/mod.rs` | Export it. |
| `crates/librarium-server/src/routes/auth.rs` | `change_password` delegates to the service. |
| `crates/librarium-server/src/routes/admin.rs` | Reset branch delegates to the service. |
| `crates/librarium-server/src/main.rs` | Optional `admin` subcommand. |
| `crates/librarium-server/Cargo.toml` | Add `rpassword`. |
| `crates/librarium-tauri/src/lib.rs` | `#[cfg(desktop)]` `auth_reset_local_password`. |
| `frontend/src/api/client.ts` | 401 retry; call `clearLocalSession`. |
| `frontend/src/stores/auth.ts` | Split `logout` / `clearLocalSession`. |
| `frontend/src/composables/useWebSocket.ts` | Use `clearLocalSession`. |
| `frontend/src/pages/LoginPage.vue` | Desktop-only "Forgot password?". |
| `frontend/src/components/settings/SecurityPanel.vue` | New. |
| `config.example.toml`, `README.md`, `docs/DESIGN.md`, `AGENTS.md` | Document recovery; demote plaintext bootstrap. |
