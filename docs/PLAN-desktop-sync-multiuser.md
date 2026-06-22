# Plan: Multi-User Support, Sharing, and Sync Hardening

## Current State Summary

- **Server**: Actix-web 4.9 + SQLite, full REST API, WebSocket file events, JWT + TOTP + OIDC + LDAP + API-key auth, role-based vault access, audit logging
- **Web Frontend**: Vue 3 + Vuetify, rich editor, split panes, all sidebar panels, admin panel, vault sharing UI, invitation flow
- **Desktop App**: Tauri 2 (`crates/librarium-tauri`) — embeds the Vue frontend in a WebView, spawns `librarium-server` in-process
- **Client Library**: `crates/librarium-client` (Rust reqwest + tokio-tungstenite), full API coverage, token management

All Phase 1–4d items from the original plan are complete. See sections below for the current status of remaining items.

---

## Auth & Multi-User — What Is Implemented

### Authentication methods
- ✅ Username/password with Argon2 hashing
- ✅ TOTP 2FA (enrollment, QR code, verification, backup codes)
- ✅ OIDC (state-validated OAuth2 flow, Google/GitHub)
- ✅ LDAP/Active Directory
- ✅ API keys (generate, revoke, middleware acceptance alongside JWT)

### Session management
- ✅ Sessions table populated on every login and token refresh (password, OIDC, refresh paths)
- ✅ Explicit token revocation on logout
- ✅ `GET /api/auth/sessions` — list active sessions for current user
- ✅ `POST /api/auth/logout` revokes all sessions when no refresh token provided

### User management
- ✅ User creation, deactivation, deletion with vault/group cascade
- ✅ Failed login tracking and account lockout
- ✅ Password policy enforcement (configurable in `config.toml`)
- ✅ Audit log (`audit_log` table) for auth and admin events
- ✅ Bulk user import (CSV/JSON)
- ✅ Invitation system with role + expiration + vault scoping

### Group and vault sharing
- ✅ Group creation (any authenticated user), member management (group creator only)
- ✅ Vault sharing with users and groups (owner only, enforced by auth middleware)
- ✅ Ownership transfer endpoint
- ✅ Public/private vault visibility (auth middleware checks `visibility` column for unauthenticated reads)
- ✅ Role enforcement: `RequiredVaultRole::Manage` on all share/revoke endpoints

---

## Security Findings Fixed in LIB-033

### Invitation vault ownership check (CRITICAL — now fixed)

**Problem**: `POST /api/invitations` accepted any `vault_id` without verifying the inviting user
is the vault owner. An authenticated user could create invitations granting access to vaults
they don't own or are only a viewer/editor of.

**Fix** (`crates/librarium-server/src/routes/invitations.rs`):
Before writing the invitation record, `create_invitation` now calls
`db.get_vault_role_for_user(vault_id, user_id)` and rejects with `403 Forbidden` unless
the result is `Some(VaultRole::Owner)`. Admin-only invitations (no vault scoping) are unaffected.

---

## Remaining Gaps

### Sync

- ⏳ **Rename event enrichment**: The file watcher fires delete+create instead of a single rename event with `old_path`. The `FileChangeEvent` type has room for this, but the watcher-level merge hasn't been implemented yet.
- ⏳ **WebSocket heartbeat**: `SyncPing`/`SyncPong` messages are defined in `WsMessage` but the server doesn't send periodic pings; idle connections may be dropped by reverse proxies.
- ⏳ **Change log catch-up**: `file_change_log` table exists and is populated, but the `GET /api/vaults/{id}/changes?since=` endpoint is not yet exposed, so clients do a full tree reload on reconnect.
- ⏳ **Incremental/delta sync**: Full file content is always sent; diff-based sync is not implemented.

### Deployment hardening

- ⏳ **TLS guidance**: No example reverse-proxy config (nginx/Caddy) shipped yet.
- ⏳ **Metrics endpoint**: No Prometheus `/metrics` scrape endpoint.
- ⏳ **Graceful shutdown**: Server does not drain WebSocket connections on SIGTERM.
- ⏳ **`GET /api/version`**: No version/build-hash endpoint.

### Audit completeness

- ⏳ **Share revocation audit logs**: Revoking user/group vault shares does not write to `audit_log`. The creation path already logs; the revocation path should too.

---

## Priority for Next Sprint

1. **WebSocket heartbeat** — Add a 30-second `SyncPing` broadcast from the server and handle `SyncPong` from clients. Prevents proxy-level idle disconnects with no code changes on clients.
2. **Change log endpoint** — Expose `GET /api/vaults/{id}/changes?since=<unix_ms>` so clients can catch up after reconnects without reloading the full tree.
3. **Share revocation audit logs** — Small addition to `revoke_vault_user_share` and `revoke_vault_group_share` routes.
4. **Rename event enrichment** — Capture `old_path` in the watcher and emit a single `Renamed` event instead of delete+create pairs.
