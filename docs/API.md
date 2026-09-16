# API

Librarium exposes a REST + WebSocket API under `/api`. This is a map, not an
exhaustive endpoint reference — the previous version of this document tried
to be exhaustive and was wrong about nearly everything by the time anyone
read it again (it predated auth entirely, and several paths it listed never
existed under those names). With 20+ route modules and both attribute-macro
(`#[get("/api/...")]`) and builder-style (`web::resource(...).route(...)`)
registration in use, the source is the only thing that won't drift:
`crates/librarium-server/src/routes/*.rs`, one file per module below,
`cargo xtask ci`'s test suite is what actually exercises them.

## Authentication

Two schemes, either works on any endpoint that isn't explicitly public:

- **Bearer JWT** — `Authorization: Bearer <access_token>` from
  `POST /api/auth/login`, refreshed via `POST /api/auth/refresh`.
- **API key** — `X-API-Key: obh_<key>` from `POST /api/auth/api-keys`
  (requires a bearer token to create).

With `auth.enabled = false` (the default), no scheme is required at all —
see [CONFIGURATION.md](CONFIGURATION.md#authentication).

## Response shape

JSON in, JSON out. Success is 2xx with the resource (or `{"success": true}`
for actions with no natural return value); errors are 4xx/5xx with a JSON
body carrying a `message` (and sometimes a structured `error` code the
frontend switches on, e.g. TOTP-required responses).

## Route modules

| Module | Covers |
| --- | --- |
| `health.rs` | `GET /api/health` — liveness + DB connectivity, unauthenticated |
| `version.rs` | `GET /api/version` — build version info |
| `auth.rs` | Login, refresh, logout, `/me`, change-password, sessions, revoke-all |
| `totp.rs` | TOTP enroll/verify/disable/status, login-time TOTP challenge |
| `oidc.rs` | OIDC authorize/callback |
| `api_keys.rs` | Create/list/revoke API keys |
| `admin.rs` | User management, activate/deactivate, bulk import, audit log — admin-only |
| `invitations.rs` | Create and accept vault invitations |
| `groups.rs` | Groups and group membership, for bulk vault sharing |
| `vaults.rs` | Register/list/delete vaults, sharing (users + groups), visibility, ownership transfer |
| `files.rs` | The bulk of the surface: tree, read/write/delete, rename, directories, upload (including chunked upload-sessions), download (raw/zip/tar), thumbnails, trash, random/daily note, wiki-link resolution |
| `markdown.rs` | `POST /api/render` and the vault-scoped variant — Markdown → HTML |
| `entities.rs` | Structured entities, relations, the knowledge graph, reindexing (builder-style routes, not the `#[get]` macro) |
| `tags.rs` | Tag listing and backlinks |
| `bookmarks.rs`, `favorites.rs` | Per-user, per-vault bookmarks and favorites |
| `preferences.rs` | User preferences (theme, editor mode, …), recent files |
| `search.rs` | Full-text search within a vault |
| `ml.rs` | Document organization / AI Insights: outline, analyze, suggestions, apply/undo, organize-vault |
| `plugins.rs` | Plugin discovery and serving plugin assets |
| `ws.rs` | `GET /api/ws` — WebSocket, authenticated via the same bearer/API-key scheme in the query string |

## WebSocket

`GET /api/ws?access_token=<token>` — pushes file-change, reindex, and
indexing-status events to connected clients. The frontend's single source of
truth for this is `frontend/src/composables/useWebSocket.ts`; the exhaustive
message-type switch there (`WsMessage`) is more current than anything this
document could restate.

## Interactive verification

There's no OpenAPI/Swagger generation. For exploring the live surface, log
in and hit endpoints directly:

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"..."}' | jq -r .access_token)

curl http://localhost:8080/api/vaults -H "Authorization: Bearer $TOKEN"
```
