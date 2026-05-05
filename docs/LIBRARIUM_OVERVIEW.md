# Librarium — Comprehensive Application Overview

> **Purpose of this document**: A detailed reference describing the Librarium application — what it is, how it is structured, how its components interact, and how to build, configure, and extend it. Intended to give a reader (human or AI) a complete mental model of the codebase.

---

## 1. What Is Librarium?

Librarium is a **self-hosted web UI for Obsidian-compatible markdown vaults**. It is written primarily in Rust (backend) and Vue 3 / TypeScript (frontend). The goal is to let users read, write, and organise their local markdown notes from any browser, while keeping the vault as a folder of plain `.md` files on disk that remain compatible with the Obsidian desktop app.

Key characteristics:

- **Self-hosted** — runs on the user's own server, NAS, or local machine. No cloud dependency.
- **Standalone binary** — the Vue frontend is embedded into the compiled Rust binary via `rust-embed`; only a single executable and a `config.toml` are needed to run.
- **Multi-vault** — multiple separate vault paths can be registered and switched between.
- **Real-time** — a file watcher notifies connected browsers via WebSocket whenever files change on disk, so the UI stays in sync with external editors (including Obsidian desktop).
- **Plugin-extensible** — a JavaScript/WASM plugin system (modelled loosely on Obsidian's plugin API) allows community or custom extensions.
- **Native desktop client** — a Tauri shell (`librarium-tauri`) embeds the Vue frontend in a WebView and runs the Actix server in-process, producing a single self-contained desktop binary.

---

## 2. Repository Layout

```
librarium/
├── Cargo.toml               # Cargo workspace root
├── config.toml              # Default configuration file
├── Dockerfile               # Multi-stage Docker build
├── docker-compose.yml
├── frontend/                # Vue 3 + TypeScript SPA
│   ├── src/
│   │   ├── api/             # HTTP client wrappers
│   │   ├── components/      # Reusable Vue components
│   │   │   ├── editor/      # Editor integrations (CodeJar, TipTap)
│   │   │   ├── modals/      # Modal dialogs
│   │   │   ├── sidebar/     # File tree sidebar
│   │   │   ├── tabs/        # Tab bar
│   │   │   └── viewers/     # File-type viewers (image, PDF, audio, video)
│   │   ├── composables/     # Vue composable hooks
│   │   ├── editor/          # Low-level editor integrations
│   │   ├── pages/           # Route-level components
│   │   ├── router/          # Vue Router config
│   │   ├── stores/          # Pinia state stores
│   │   └── utils/           # Shared utilities
│   ├── vite.config.ts
│   └── vitest.config.ts
├── crates/
│   ├── librarium-server/        # Main Actix Web backend (default workspace member)
│   │   └── src/
│   │       ├── config/      # AppConfig loading (TOML + env vars)
│   │       ├── db/          # SQLx migrations + repositories
│   │       ├── middleware/  # Auth, logging, rate-limiting, request-id
│   │       ├── models/      # Shared API + DB structs
│   │       ├── routes/      # Actix request handlers
│   │       ├── services/    # Core business logic
│   │       ├── watcher/     # Filesystem event watcher
│   │       ├── assets.rs    # rust-embed asset serving
│   │       └── main.rs      # Entry point
│   ├── librarium-types/         # Shared Rust types (used by server + client + desktop)
│   ├── librarium-client/        # Async HTTP + WebSocket client library
│   └── librarium-tauri/         # Tauri desktop shell (single-binary desktop app)
│       └── src/
│           ├── main.rs      # Platform init, server spawn, WebView navigation
│           └── paths.rs     # Platform data-directory resolution (XDG / macOS / Windows)
├── docs/                    # Extended documentation
├── plugins/                 # Drop-in plugin directory
├── tests/                   # Rust integration tests
└── scripts/                 # Build helper scripts
```

---

## 3. Tech Stack Summary

| Layer | Technology |
|---|---|
| Backend language | Rust (edition 2021) |
| Web framework | Actix Web 4 |
| Database | SQLite via SQLx (async) |
| Migrations | SQLx embedded migrations |
| Full-text search | Custom in-memory inverted index |
| File watching | `notify` crate (cross-platform) |
| Markdown parsing | `pulldown-cmark` |
| Asset embedding | `rust-embed` |
| Logging | `tracing` + `tracing-subscriber` + file rotation |
| Frontend framework | Vue 3 (Composition API) |
| Frontend language | TypeScript (strict mode) |
| Build tool | Vite |
| State management | Pinia |
| Component library | Vuetify |
| Unit tests | Vitest |
| E2E tests | Playwright |
| Desktop shell | Tauri 2 (WebView + OS integration) |
| Client SDK | `librarium-client` (reqwest + tokio-tungstenite) |

---

## 4. Backend Architecture

### 4.1 Entry Point (`main.rs`)

Startup sequence:

1. **Logging** — `tracing_subscriber` is configured with an `EnvFilter` (respects `RUST_LOG`). Logs are written both to stdout and to daily-rotating files in `./logs/`. Set `LOG_FORMAT=json` for structured output.
2. **Configuration** — `AppConfig::load()` reads `config.toml` then overlays environment variables (double-underscore notation: `LIBRARIUM__SERVER__PORT`).
3. **JWT secret guard** — if `auth.jwt_secret` is empty, an ephemeral UUID-based secret is generated with a warning. If it equals the hard-coded dev default, another warning is emitted.
4. **Database** — `Database::new()` connects to SQLite and runs all pending migrations. `bootstrap_admin_if_empty()` creates the first admin user if the users table is empty (credentials come from config).
5. **Search index** — `SearchIndex::new()` creates an empty in-memory index.
6. **File watcher** — `FileWatcher::new()` starts a background OS file-event thread. Returns a watcher handle and an `mpsc` receiver.
7. **Broadcast channel** — a `tokio::broadcast::channel` is created to fan out `FileChangeEvent` messages to an arbitrary number of WebSocket subscribers.
8. **Event loop task** — a `tokio::spawn` task reads from the watcher receiver, updates the search index, and re-broadcasts to WebSocket clients.
9. **Vault loading** — all vaults stored in the database are iterated. Missing paths are cleaned up. Each valid vault is watched and indexed.
10. **HTTP server** — `HttpServer::new()` wires all middleware, route handlers, and the embedded frontend. Listens on the configured host/port.
11. **Graceful shutdown** — `SIGTERM` or `Ctrl+C` broadcasts a shutdown signal to all WebSocket sessions, waits 500 ms for close frames, then drains in-flight HTTP requests.

### 4.2 Application State (`AppState`)

Shared across all Actix handlers via `web::Data<AppState>`:

```rust
pub struct AppState {
    pub db: Database,
    pub search_index: SearchIndex,
    pub storage: Arc<dyn StorageBackend>,
    pub watcher: Arc<Mutex<FileWatcher>>,
    pub event_broadcaster: broadcast::Sender<FileChangeEvent>,
    pub change_log_retention_days: u32,
    pub ml_undo_store: Arc<Mutex<HashMap<String, UndoReceipt>>>,
    pub shutdown_tx: broadcast::Sender<()>,
}
```

### 4.3 Middleware Stack

Applied in order (outermost first):

| Middleware | Purpose |
|---|---|
| `Cors` | Configurable allowed origins; `Access-Control-*` headers |
| `RequestLogging` | Structured request/response log lines |
| `RequestIdMiddleware` | Attaches a UUID `X-Request-Id` to every request/response |
| `RateLimitMiddleware` | Token-bucket per IP; configurable via `RATE_LIMIT_REQUESTS` env var |
| `AuthMiddleware` | JWT Bearer token or `X-API-Key` validation; injects `AuthUser` into request extensions |
| `Compress` | Brotli/gzip response compression |

### 4.4 Route Handlers

| Module | Prefix | Description |
|---|---|---|
| `health` | `/api/health` | Liveness / readiness probe |
| `version` | `/api/version` | Server version string |
| `auth` | `/api/auth/...` | Login, logout, refresh, profile, sessions, TOTP, change-password, OIDC |
| `admin` | `/api/admin/...` | User management, audit log, bulk import (Admin role required) |
| `groups` | `/api/groups/...` | Group CRUD and membership management |
| `vaults` | `/api/vaults/...` | Vault registration, listing, deletion, sharing |
| `files` | `/api/vaults/{id}/files/...` | File tree, CRUD, move, upload, thumbnail |
| `search` | `/api/vaults/{id}/search` | Full-text search |
| `ml` | `/api/vaults/{id}/ml/...` | AI outline generation, organisation suggestions, apply/undo |
| `ws` | `/api/ws` | WebSocket upgrade; streams `FileChangeEvent` JSON |
| `markdown` | `/api/markdown/render` | Server-side markdown → HTML rendering |
| `preferences` | `/api/preferences` | Per-user settings GET/PUT |
| `plugins` | `/api/plugins/...` | Plugin manifest listing and enable/disable |
| `bookmarks` | `/api/bookmarks/...` | Starred files |
| `tags` | `/api/vaults/{id}/tags/...` | Tag listing, backlink resolution |
| `api_keys` | `/api/auth/api-keys` | Programmatic API key management |
| `totp` | `/api/auth/totp/...` | TOTP enroll / verify / disable |
| `invitations` | `/api/invitations/...` | User invitation flow |
| `oidc` | `/api/auth/oidc/...` | OIDC authorize + callback |
| `static` | `/**` | Serves embedded Vue SPA (release) or Vite build dir (debug) |

### 4.5 Services

#### `FileService`

All filesystem I/O goes through this service.

- **Path security**: every user-supplied path is run through `std::fs::canonicalize` and checked to be inside the vault root. Path traversal attempts are rejected.
- **Soft delete**: `DELETE` operations move the file/folder to a `.trash/` subdirectory rather than permanently removing it.
- **Operations**: read content, write content, create (with recursive directory creation), delete (to trash), move/rename, list directory tree, serve raw bytes for images/attachments, generate image thumbnails (resized PNG).

#### `SearchService` / `SearchIndex`

An in-memory full-text search index.

- **Structure**: inverted index mapping lowercase tokens → `(vault_id, file_path, line_number)` tuples.
- **Indexing**: on startup, `index_vault()` walks all `.md` files, tokenises content, and populates the index. Non-markdown files are skipped.
- **Incremental updates**: on `Modified` / `Created` events, `update_file()` replaces the old entries for that file. On `Deleted`, `remove_file()` purges them.
- **Query**: `search()` tokenises the query, intersects result sets for multi-word queries, and returns ranked snippets with surrounding context lines.
- **Exclusions**: directories listed in `vault.index_exclusions` (default: `.git`, `.obsidian`, `.trash`, `node_modules`, `target`) are skipped.

#### `MarkdownService`

Wraps `pulldown-cmark` to convert raw markdown to sanitised HTML. Handles Obsidian-specific syntax:
- `[[wiki-links]]` resolved to vault-relative URLs.
- `![[embed]]` for image and note embeds.
- Frontmatter (`---` YAML blocks) stripped before rendering.
- Code blocks with syntax highlighting classes.

#### `WikiLinkService`

Resolves `[[Note Title]]` links to actual file paths within the vault, supporting:
- Exact filename match.
- Case-insensitive match.
- Short name (no extension) match.
- Alias syntax: `[[Actual Title|Display Text]]`.

#### `FrontmatterService`

Parses and updates YAML frontmatter in markdown files:
- Extract key/value pairs as `serde_json::Value`.
- Merge/replace specific keys without touching file content.
- Validate against expected types.

#### `MlService`

Local (no external API) ML-assisted features:
- **Outline generation**: analyses heading structure and content density to produce a navigable outline.
- **Organisation suggestions**: heuristic-based recommendations for renaming, moving, or tagging notes.
- **Apply / undo**: suggestions can be applied with a receipt ID returned for single-use undo.

#### `PluginService`

Backend component of the plugin system:
- Scans the `plugins/` directory for `manifest.json` files.
- Validates manifests (required fields, semver, capability declarations).
- Serves plugin JS/CSS assets via `/api/plugins/`.
- Tracks enabled/disabled state in the database.

#### `ImageService`

On-demand image resizing via the `image` crate:
- Serves vault images at arbitrary dimensions: `GET /api/vaults/{id}/thumbnail/{path}?width=N&height=N`.
- Returned as PNG bytes with appropriate `Content-Type`.

#### `StorageBackend`

An abstract trait with two implementations:

| Backend | Config | Status |
|---|---|---|
| `LocalStorage` | `storage.backend = "local"` | Fully implemented |
| `S3Storage` | `storage.backend = "s3"` | Scaffolded; targets MinIO / SeaweedFS / AWS S3 |

S3 backend configuration requires `endpoint`, `bucket`, `region`, `access_key`, `secret_key`, and `path_style` (for MinIO-compatible path-style URLs).

### 4.6 File Watcher (`watcher/`)

- Uses the `notify` crate for cross-platform OS-level file events (inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows).
- Runs in a dedicated background thread.
- **Debouncing**: rapid successive events for the same path are coalesced (configurable debounce window) to prevent flooding the broadcast channel.
- Emits typed `FileChangeEvent` values: `Created`, `Modified`, `Deleted`, `Renamed { from, to }`.
- Each event carries `vault_id` and relative `path` within the vault.

### 4.7 WebSocket Handler (`routes/ws.rs`)

- Accepts upgrade to WebSocket at `GET /api/ws`.
- Subscribes to the broadcast channel.
- Serialises each `FileChangeEvent` as JSON and pushes it to the client.
- Listens for a shutdown signal to send a proper WebSocket Close frame before the server exits.
- Multiple concurrent connections are supported; each gets its own broadcast receiver.

### 4.8 Database Schema (`db/`)

SQLx manages schema via embedded migration files. Key tables:

| Table | Purpose |
|---|---|
| `vaults` | Registered vault name + path |
| `users` | User accounts (username, password hash, role) |
| `sessions` | Refresh token records with expiry |
| `api_keys` | Hashed API keys with name and expiry |
| `totp_secrets` | Encrypted TOTP secrets per user |
| `preferences` | Per-user JSON settings blob |
| `recent_files` | Per-user recently opened file history |
| `bookmarks` | Starred `(user_id, vault_id, path)` entries |
| `groups` | User group definitions |
| `group_members` | Group ↔ user membership |
| `vault_shares` | Per-vault access grants to users or groups |
| `file_change_log` | Audit log of file events (retained per config) |
| `audit_log` | Admin security audit events |
| `invitations` | Pending user invitation tokens |
| `plugins` | Plugin enabled/disabled state |

### 4.9 Authentication & Security

Authentication is **optional** (`auth.enabled = false` by default).

When enabled:

- **JWT Bearer tokens** — `POST /api/auth/login` returns `access_token` (short-lived) and `refresh_token` (long-lived, stored in the `sessions` table). Tokens are signed with `auth.jwt_secret`.
- **API Keys** — users generate named keys via `POST /api/auth/api-keys`; presented as `X-API-Key: obh_<key>`.
- **TOTP (2FA)** — optional TOTP enrollment per user; verified on login.
- **Three auth providers**: `password` (built-in), `ldap` (Active Directory / LDAP bind), `oidc` (OAuth2/OpenID Connect via Google, GitHub, etc.).
- **Roles**: `Admin` and regular `User`. Admin endpoints are gated by role check in middleware.
- **Groups and vault sharing**: vaults can be shared with individual users or groups with read/write permissions.
- **Rate limiting**: configurable request-per-minute cap per IP.
- **Password policies**: optional minimum length, complexity requirements, and account lockout after failed attempts.

---

## 5. Frontend Architecture

The frontend is a **Vue 3 Single Page Application** built with Vite and TypeScript. In a release build, all compiled assets in `frontend/dist/` are embedded into the Rust binary by `rust-embed` at compile time and served from memory — no separate static file server is needed.

### 5.1 Module Layout

```
frontend/src/
├── api/
│   ├── client.ts      # Core fetch wrapper (auth headers, error handling)
│   └── types.ts       # TypeScript interfaces mirroring Rust models
├── stores/            # Pinia stores
│   ├── auth.ts        # Login state, token storage, refresh logic
│   ├── vaults.ts      # Vault list, active vault
│   ├── files.ts       # File tree, open/save file content
│   ├── tabs.ts        # Open tab list and active tab
│   ├── editor.ts      # Editor mode and preferences
│   ├── preferences.ts # User preference sync
│   └── ui.ts          # Sidebar width, theme, modal state
├── components/
│   ├── sidebar/       # Collapsible file tree, context menus
│   ├── editor/        # Markdown editor panels (CodeJar raw, TipTap WYSIWYG)
│   ├── viewers/       # Read-only viewers for images, PDF, audio, video, code
│   ├── tabs/          # Tab bar component
│   ├── modals/        # New file, rename, settings, search, quick-switcher dialogs
│   └── TopBar.vue     # Top navigation bar
├── pages/
│   ├── LoginPage.vue
│   ├── AdminUsersPage.vue
│   └── ChangePasswordPage.vue
├── router/            # Vue Router (hash mode)
├── composables/       # Reusable composition functions (useWebSocket, useSearch, …)
└── editor/            # CodeJar and TipTap low-level integration
```

### 5.2 State Management (Pinia)

| Store | Responsibilities |
|---|---|
| `auth` | Access token, refresh token, user profile, auto-refresh loop |
| `vaults` | Vault list, active vault selection, vault registration/removal |
| `files` | File tree nodes, file content cache, create/read/write/delete/move ops |
| `tabs` | Open file tabs, active tab, tab close and reorder |
| `editor` | Current editor mode (raw / side-by-side / preview / WYSIWYG), font size |
| `preferences` | Round-trips user preferences to `/api/preferences` |
| `ui` | Sidebar collapsed state, modal visibility, theme (light/dark) |

### 5.3 Editor Modes

| Mode | Component | Description |
|---|---|---|
| Raw | CodeJar | Plain textarea with syntax highlighting; minimal overhead |
| Formatted raw | CodeJar + highlight.js | Same as raw but with markdown token colouring |
| Side-by-side | CodeJar + rendered preview | Editor on left, live `pulldown-cmark` preview on right |
| Preview | HTML viewer | Read-only rendered markdown |
| WYSIWYG | TipTap | Rich-text editor; stores/exports markdown |

### 5.4 File Type Support

| Type | Extensions | Experience |
|---|---|---|
| Markdown | `.md` | Full editor (all modes) |
| Images | `.png`, `.jpg`, `.jpeg`, `.gif`, `.svg`, `.webp` | Viewer with zoom/pan; embed in notes |
| PDF | `.pdf` | Native browser PDF viewer with search |
| Audio | `.mp3`, `.wav`, `.ogg` | HTML5 audio player |
| Video | `.mp4`, `.webm` | HTML5 video player |
| Code | `.js`, `.ts`, `.py`, `.rs`, `.go`, `.java`, `.c`, `.cpp`, `.css`, `.html`, `.json`, `.yaml`, … | Read-only syntax-highlighted viewer |
| Other | anything else | Download button |

### 5.5 Real-time Sync

The frontend opens a persistent WebSocket connection to `GET /api/ws`. On receiving a `FileChangeEvent`:

- **`Created` / `Modified`**: if the changed file is currently open in a tab, the tab is marked stale and the user is offered a reload prompt. The file tree is refreshed.
- **`Deleted`**: open tab is closed or marked unavailable. File tree node is removed.
- **`Renamed`**: open tab path is updated, file tree node is moved.

If the connection drops, the composable implements exponential back-off reconnection.

### 5.6 Search

- Global search modal opens with `Ctrl+F` or the search icon.
- Sends `GET /api/vaults/{id}/search?q=...` as the user types (debounced).
- Results display file path, line number, and a snippet with the matching term highlighted.
- Clicking a result opens the file and scrolls to the matching line.

### 5.7 Quick Switcher

- Opens with `Ctrl+O` / `Cmd+O`.
- Fuzzy-filters the in-memory file tree by filename.
- No server round-trip needed.

### 5.8 Wiki Link Autocomplete

Typing `[[` in the raw editor triggers an autocomplete dropdown populated from the file tree. Selecting an entry inserts `[[filename]]` (without extension).

---

## 6. Desktop Client (`librarium-tauri`)

A native desktop application built with **Tauri 2**. It is a thin shell that:

1. Resolves platform data directories (XDG on Linux, `~/Library` on macOS, `%APPDATA%` on Windows).
2. Handles first-launch initialisation — creates directories, writes a default `config.toml`, prompts for a vault directory.
3. Spawns the Actix server (`librarium-server::run()`) on a background thread.
4. Polls `GET /api/health` then navigates the embedded WebView to `http://localhost:{port}`.

All application UI is the same Vue frontend used in the browser. No separate desktop UI codebase.

### 6.1 Architecture

```
main.rs     – Platform init, server thread spawn, health polling, WebView navigation
paths.rs    – LibrariumPaths resolution via Tauri path API
```

### 6.2 OS Integration

| Feature | Plugin |
|---|---|
| Native file/directory dialogs | `tauri-plugin-dialog` |
| System tray (open / quit / status) | Built-in Tauri tray API |
| Desktop notifications | `tauri-plugin-notification` |
| `librarium://` deep links + `.md` associations | `tauri-plugin-deep-link` |

### 6.3 Platform Data Directories

| Platform | Config / Data | Cache |
|---|---|---|
| Linux (XDG) | `~/.config/librarium/` / `~/.local/share/librarium/` | `~/.cache/librarium/` |
| macOS | `~/Library/Application Support/librarium/` | `~/Library/Caches/librarium/` |
| Windows | `%APPDATA%\librarium\` | `%LOCALAPPDATA%\librarium\` |

Default vault location (prompted on first launch): `~/Documents/Librarium/`

### 6.4 Startup Sequence

```
main()
  ├─ resolve LibrariumPaths → create dirs → load/write config.toml
  ├─ thread::spawn → actix_web::rt::System::new().block_on(librarium_server::run(config))
  └─ tauri setup hook
       ├─ show "Starting Librarium…" loading screen
       └─ poll GET /api/health every 100 ms (10 s timeout)
             success  → navigate WebView to http://localhost:{port}
             timeout  → show error screen with log path

---

## 7. Client Library (`librarium-client`)

A standalone async Rust library (`reqwest` + `tokio-tungstenite`) that wraps the server HTTP and WebSocket API.

Key types:

- `ObsidianClient` — stateful HTTP client holding base URL and auth tokens. Handles automatic token refresh.
- `ClientError` — unified error enum covering HTTP errors, server error responses, WebSocket errors, and serialisation failures.
- `WsStream` — type alias for the WebSocket stream type, exposed so the desktop can own it.

The library is designed to be usable by any Rust consumer (desktop, CLI tools, test harnesses) without pulling in the full server crate.

---

## 8. Plugin System

### 8.1 Discovery

On startup (and via the `/api/plugins` route), the server scans the `plugins/` directory for subdirectories containing a `manifest.json`.

### 8.2 Manifest Fields

```json
{
  "id": "com.example.myplugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "...",
  "author": "...",
  "main": "main.js",
  "plugin_type": "javascript",
  "styles": ["styles.css"],
  "min_host_version": "0.1.0",
  "capabilities": ["read_files", "modify_ui", "commands"],
  "hooks": ["on_load", "on_file_open", "on_file_save"]
}
```

### 8.3 Plugin Types

- **JavaScript** (default) — `main.js` is served to the frontend and executed in the browser context. Currently runs with full `window` access (no iframe sandboxing yet).
- **WASM** — `plugin.wasm` compiled from Rust, C++, or any WASM target. Better isolation (planned).

### 8.4 Capabilities (Permissions)

Plugins must declare required permissions: `read_files`, `write_files`, `delete_files`, `vault_metadata`, `network`, `storage`, `modify_ui`, `commands`, `editor_access`, `system_exec`.

### 8.5 JavaScript API Surface (Frontend)

```javascript
// File access
const content = await api.readFile(path);
await api.writeFile(path, content);

// UI
api.addRibbonIcon('icon-id', 'Tooltip', callback);
api.addCommand({ id: 'cmd', name: 'My Command', callback, hotkey: 'Ctrl+Shift+M' });
api.showNotice('Hello!');

// Storage
await api.storage.set('key', value);
const value = await api.storage.get('key');
```

### 8.6 Lifecycle Hooks

`on_load`, `on_unload`, `on_startup`, `on_shutdown`, `on_file_open`, `on_file_save`, `on_file_create`, `on_file_delete`, `on_file_rename`, `on_editor_change`, `on_vault_switch`.

---

## 9. Configuration Reference

Configuration is resolved from three sources in priority order (highest first):

1. Environment variables (`LIBRARIUM__<SECTION>__<KEY>`)
2. `config.toml` in the working directory
3. Hard-coded defaults

### Full `config.toml` Reference

```toml
[server]
host = "127.0.0.1"   # "0.0.0.0" required for Docker / remote access
port = 8080

[database]
path = "./librarium.db"

[vault]
base_dir = "./vaults"                   # Default location for new vaults
index_exclusions = [".git", ".obsidian", ".trash", "node_modules", "target"]

[auth]
enabled = true
provider = "password"                   # "password" | "ldap" | "oidc"
jwt_secret = ""                         # Generate: openssl rand -hex 32
access_token_ttl = 3600                 # seconds
refresh_token_ttl = 604800              # seconds
bootstrap_admin_username = "admin"
bootstrap_admin_password = ""

# Password policy (optional)
# min_password_length = 12
# max_failed_logins = 5
# lockout_minutes = 15

# OIDC (when provider = "oidc")
# oidc_issuer_url = "https://accounts.google.com"
# oidc_client_id = ""
# oidc_client_secret = ""
# oidc_redirect_uri = "http://localhost:8080/api/auth/oidc/callback"

# LDAP (when provider = "ldap")
# ldap_url = "ldap://ldap.example.com:389"
# ldap_base_dn = "ou=people,dc=example,dc=com"
# ldap_bind_dn = "cn=admin,dc=example,dc=com"
# ldap_bind_password = ""

[sync]
change_log_retention_days = 7

[cors]
allowed_origins = ["http://localhost:5173", "http://localhost:8080"]

[storage]
backend = "local"   # "local" | "s3"

# [storage.s3]
# endpoint   = "http://minio:9000"
# bucket     = "librarium"
# region     = "us-east-1"
# access_key = ""
# secret_key = ""
# path_style = true
```

### Key Environment Variables

| Variable | Purpose |
|---|---|
| `RUST_LOG` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |
| `LOG_FORMAT` | Set to `json` for structured / cloud logging |
| `RATE_LIMIT_REQUESTS` | Max requests per 60 s per IP (default: 120) |
| `LIBRARIUM__AUTH__JWT_SECRET` | Override JWT secret without editing config file |
| `LIBRARIUM_DISABLE_ML` | Desktop: disable ML features |
| `LIBRARIUM_DISABLE_SYNC` | Desktop: disable WebSocket event sync |
| `LIBRARIUM_DIAGNOSTICS` | Desktop: enable diagnostics panel |

---

## 10. Data Flow Walkthrough

### User saves a file (browser)

```
Browser editor (Vue)
  → PUT /api/vaults/{id}/files/{path}  (JSON body: {content})
    → AuthMiddleware validates JWT
    → files::update_file handler
      → FileService::write_file (canonicalize path, write to disk)
        ← OS confirms write
  → FileWatcher detects Modify event
    → event loop task:
        SearchIndex::update_file (re-tokenise, update index)
        broadcast::Sender::send(FileChangeEvent::Modified)
    → WebSocket handler (for every connected client):
        serialise event → JSON → send over WS
  → Other browser tabs / desktop client receive WS message
    → update file tree, mark stale tab, offer reload
```

### Full-text search

```
Browser (user types in search box)
  → GET /api/vaults/{id}/search?q=word1+word2
    → search::search handler
      → SearchIndex::search(vault_id, "word1 word2")
          tokenise → ["word1", "word2"]
          intersect posting lists for each token
          sort by relevance
          return [(path, line, snippet), ...]
    ← JSON array of SearchResult
  → Frontend renders results with match highlights
  → User clicks result → open file at that line number
```

### New vault registration

```
Browser
  → POST /api/vaults  {name, path}
    → VaultService::register_vault
      → validate path exists on disk
      → db.insert_vault(...)
      → FileWatcher::watch_vault(vault_id, path)
      → SearchIndex::index_vault(vault_id, path)
          walk .md files, tokenise, build inverted index
    ← 201 Created {id, name, path}
  → Frontend adds vault to list, switches to new vault
```

---

## 11. Build Instructions

### Prerequisites

- **Rust** (latest stable via `rustup`)
- **Node.js** (LTS) + npm

### Development

```bash
# Terminal 1: frontend with hot-module replacement
cd frontend
npm install
npm run dev        # Vite dev server on :5173

# Terminal 2: backend (serves API on :8080, proxies SPA to :5173 in debug mode)
RUST_LOG=debug cargo run
```

Open `http://localhost:8080`.

### Release (standalone binary)

```bash
# 1. Build frontend (outputs to frontend/dist/)
cd frontend && npm install && npm run build && cd ..

# 2. Build backend (embeds frontend/dist/ into binary)
cargo build --release

# 3. Distribute
cp target/release/librarium dist/
cp config.toml dist/
cd dist && ./librarium
```

### Docker

```bash
docker compose up -d   # builds multi-stage image, starts on :8080
```

The Dockerfile uses three stages:

1. **Node builder** — compiles the Vue SPA.
2. **Rust builder** — compiles the backend with embedded frontend assets.
3. **Runtime** (`debian:bookworm-slim`) — contains only the binary and minimal runtime libs. Results in a small final image.

Volumes: mount local vault folders to `/data/vaults/` inside the container.

### Desktop App (Tauri)

```bash
# Build the frontend first (embedded into the Tauri binary via librarium-server)
cd frontend && npm run build && cd ..

# Build the Tauri desktop binary
cargo build --release -p librarium-tauri

# Run it
./target/release/librarium-tauri
```

Validate without full compilation: `cargo check -p librarium-tauri`

### Release Profile Optimisations

The workspace `Cargo.toml` applies aggressive optimisations in release mode:

```toml
[profile.release]
opt-level    = "z"   # optimise for binary size
lto          = true  # link-time optimisation
codegen-units = 1    # single codegen unit for better inlining
panic        = "abort"
strip        = true  # remove debug symbols
```

Expect longer compile times (~5-10 min first build) but a lean binary (~5-15 MB).

### Cross-Compilation

```bash
cargo install cross
cross build --target x86_64-unknown-linux-gnu --release     # Linux
cross build --target x86_64-pc-windows-gnu --release        # Windows (requires Docker)
```

---

## 12. Testing

### Backend

```bash
cargo test                          # all unit + integration tests
cargo test --test conflict_tests    # specific integration test file
RUST_LOG=debug cargo test -- --nocapture  # with log output
```

Integration tests live in `tests/`. Unit tests are inline `#[cfg(test)] mod tests` in each source file.

### Frontend

```bash
cd frontend
npm test              # Vitest unit tests
npm run test:watch    # watch mode
npm run test:e2e      # Playwright end-to-end tests
```

### Linting

```bash
cargo fmt && cargo clippy    # Rust
cd frontend && npx tsc       # TypeScript type check
```

---

## 13. Deployment Recommendations

### Reverse Proxy (TLS)

The server is HTTP-only. Use nginx or Caddy for HTTPS:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";  # required for WebSocket
}
```

### Backup

The entire server state is:

1. **SQLite database** (`librarium.db`) — back up with `sqlite3 librarium.db ".backup backup.db"`.
2. **Vault directories** — regular filesystem files; back up with rsync or any file backup tool.

### Health Check

```bash
curl http://localhost:8080/api/health
# {"status":"healthy","database":"connected"}
```

### API Keys (Programmatic Access)

```bash
# Create a key (requires an authenticated session)
curl -X POST http://localhost:8080/api/auth/api-keys \
  -H "Authorization: Bearer <token>" \
  -d '{"name": "ci-script", "expires_in_days": 90}'

# Use the key
curl http://localhost:8080/api/vaults \
  -H "X-API-Key: obh_<key>"
```

---

## 14. Key Design Decisions

| Decision | Rationale |
|---|---|
| Single binary with embedded frontend | Zero-dependency deployment; no separate static hosting or Docker volumes for assets |
| Soft delete (`.trash/`) | Protects against accidental loss; preserves compatibility with Obsidian's own trash convention |
| In-memory search index | No additional database or service dependency; fast enough for personal/small-team vaults; rebuilt on startup |
| File watcher → broadcast channel fan-out | Decouples event source from consumers; WebSocket handlers and search index updates are independent |
| `canonicalize` for all user paths | Prevents path traversal attacks without maintaining an allowlist |
| SQLx compile-time checked queries | Catches SQL errors at compile time; migrations are embedded in the binary |
| Optional auth (default: off) | Low barrier for local single-user use; easily hardened for shared/network deployment |
| `librarium-types` shared crate | Server, client library, and desktop all use the same type definitions — no drift or manual sync |

---

## 15. Known Limitations & Roadmap Items

- **No real-time collaborative editing** — last write wins; CRDTs/OT planned.
- **No ETag-based conflict detection in desktop client** — planned.
- **No delta sync for large files** — full content sent on every save.
- **S3 backend** — scaffolded but not production-ready.
- **Plugin sandboxing** — JS plugins have full `window` access; iframe or WASM sandbox planned.
- **WASM plugin support** — partially designed, not yet implemented.
- **Graph view** — data model designed (nodes: files/tags/virtual; edges: wiki-links/embeds/tags), D3.js rendering planned.
- **Git integration** — per-vault auto-commit and push/pull planned.
- **Mobile responsive design** — breakpoints planned; not fully implemented.

---

*Generated from codebase inspection — reflects the state of the repository as of the document creation date.*
