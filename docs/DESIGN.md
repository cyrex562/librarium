# Librarium — Design & Architecture

> **Status:** Canonical design document. This file and the root `README.md` are the
> two documents kept current as the project evolves. When a change alters
> architecture, data flow, public APIs, configuration, or build/run steps, update
> this document in the same change (see [Maintaining this document](#11-maintaining-this-document)).
>
> Historical design notes, feature plans, and superseded specs live in
> [`docs/archive/`](archive/). Treat archived files as background, not as a
> description of the current system.

**Version:** 0.100.0

---

## 1. What Librarium is

Librarium is a self-hosted knowledge base and vault manager for
Obsidian-compatible Markdown vaults. The source of truth is plain Markdown
files on disk; everything else (search index, entity graph, metadata) is derived
state that can be rebuilt from those files.

It ships in two shapes from one codebase:

- **Server** — a Rust web service that exposes a REST + WebSocket API and serves
  the embedded single-page frontend. Multi-user, role-based, suitable for a
  homelab or small team.
- **Desktop** — a Tauri 2 shell that embeds the same server on `127.0.0.1` and
  renders the same frontend in a native WebView. Single local user, sessions
  persist across restarts.

Design priorities, in order: **files stay portable and tool-agnostic**,
**derived state is always rebuildable**, **the frontend and backend share one
contract**, and **the same core runs on server and desktop**.

---

## 2. System overview

```text
                ┌─────────────────────────────────────────────┐
                │                Frontend (SPA)                │
                │   Vue 3 + Vuetify + Pinia, served as static  │
                │   assets embedded in the server binary       │
                └───────────────┬───────────────┬─────────────┘
                       REST/JSON │               │ WebSocket
                                 ▼               ▼
        ┌──────────────────────────────────────────────────────────┐
        │                    librarium-server (Actix Web)            │
        │                                                            │
        │   routes/  ── thin transport adapters (HTTP/WS handlers)   │
        │   services/ ─ business logic (files, search, reindex, …)   │
        │   middleware/ auth (JWT / API key / vault-role)            │
        │   watcher/ ── debounced filesystem event source            │
        │   models/, config/, db/                                    │
        └───────┬───────────────┬───────────────────┬───────────────┘
                │               │                   │
                ▼               ▼                   ▼
        ┌──────────────┐ ┌──────────────┐  ┌────────────────────┐
        │ Vault files  │ │   SQLite     │  │   Tantivy index    │
        │ (Markdown    │ │ (users,      │  │ (full-text search, │
        │  on disk —   │ │  vaults,     │  │  per vault)        │
        │  source of   │ │  shares,     │  └────────────────────┘
        │  truth)      │ │  metadata)   │
        └──────┬───────┘ └──────────────┘
               │ notify (500 ms debounce)
               ▼
        FileWatcher ──► event loop ──► search reindex + entity reindex + WS broadcast
```

The desktop app wraps this same server: `librarium-tauri` starts
`librarium-server` bound to loopback, then points a WebView at it.

---

## 3. Workspace layout

Librarium is a Cargo workspace (`Cargo.toml` at the repo root) plus a Node
frontend.

| Member | Path | Role |
| --- | --- | --- |
| `librarium-server` | `crates/librarium-server` | Main Actix Web backend + binary; **default workspace member**. |
| `librarium-types` | `crates/librarium-types` | Shared Rust DTOs and parser/contract types used across crates. |
| `librarium-client` | `crates/librarium-client` | Reusable HTTP + WebSocket client for the Librarium API. |
| `librarium-tauri` | `crates/librarium-tauri` | Tauri 2 desktop shell embedding the server + frontend. |
| `frontend` | `frontend/` | Vue 3 + TypeScript + Vuetify SPA (built with Vite). |
| `plugins` | `plugins/` | Bundled first-party plugin manifests + scripts. |
| `benches` | `benches/` | Criterion benchmarks (e.g. Markdown parsing). |
| `tests` | `tests/` | Workspace-level Rust integration tests. |
| `scripts` | `scripts/` | Packaging / install helpers (PowerShell, shell, Python). |

The contract that holds the workspace together: **`routes` are thin transport
adapters, `services` hold business logic, and `models` / `librarium-types` are
the shared data contracts** consumed by both the backend and (via mirrored
TypeScript types) the frontend.

---

## 4. Backend (`librarium-server`)

**Stack:** Rust (edition 2021), Actix Web, Tokio, SQLx (SQLite), Tantivy,
`notify` + `notify-debouncer-full`, `pulldown-cmark`, Argon2.

### Module layout (`crates/librarium-server/src/`)

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI entrypoint: resolves config path (`--config` / `LIBRARIUM_CONFIG` / `config.toml`), starts the runtime. |
| `lib.rs` | App init: logging, DB bootstrap, builds services, starts the watcher event loop. |
| `config/` | `AppConfig` loaded from TOML + env overrides (server, database, vault, auth, sync, cors, tls, ml). |
| `db/` | SQLite pool, migrations, query layer. |
| `models/` | API + DB structs (bookmarks, graph, plugin, schema, …). |
| `routes/` | Actix request handlers — one module per resource (see below). |
| `services/` | Core business logic (see below). |
| `middleware/` | Auth (JWT, API key, vault-role enforcement), logging, rate limiting, request IDs. |
| `watcher/` | Filesystem event source with debouncing. |
| `error.rs` | `AppError` / `AppResult`. |

### Routes (transport layer)

`routes/` modules map HTTP/WS endpoints to service calls and do request/response
shaping only. Notable modules: `auth`, `totp`, `oidc`, `api_keys`, `admin`,
`users`/`groups`, `invitations`, `vaults` (CRUD + sharing + roles),
`files` (read/write/delete, upload sessions, archive export), `markdown`
(parse/render/preview), `search`, `tags`, `bookmarks`, `entities`, `preferences`,
`plugins`, `ml`, `version`, `health`, and `ws` (WebSocket).

### Services (business logic)

| Service | Responsibility |
| --- | --- |
| `file_service` | All disk I/O. **Owns path-traversal protection** (canonicalize + containment checks), conflict detection, trash/backup on conflict, move/rename. |
| `search_service` | Tantivy wrapper: per-vault index, incremental updates, query + snippet highlighting. |
| `reindex_service` | Two-pass entity/relation indexer from frontmatter; single source of truth for entity state (distinct from full-text search). |
| `markdown_service` | Markdown parsing/rendering (`pulldown-cmark`), link rewriting. |
| `wiki_link_service` | Obsidian `[[wiki link]]` parsing and rewriting. |
| `frontmatter_service` | YAML frontmatter read/write. |
| `auth_provider` / `ldap_provider` / `oidc_provider` | Pluggable auth: local password (Argon2), LDAP/AD, OIDC. |
| `entity_service` / `relation_service` / `schema_service` | User-defined entity/relation types and graph queries. |
| `label_service` | Tags/labels (seeds core labels at startup). |
| `template_service` | Note templates. |
| `image_service` | Image resize / thumbnails. |
| `plugin_service` / `plugin_api` | Plugin lifecycle + capability-gated host API. |
| `ml_service` / `organize_service` / `embedding_service` / `local_lm_service` | Local, offline organization features (keyphrase extraction, optional embeddings). |

### Persistence model

Three layers, with a clear ownership rule — **the filesystem is authoritative;
SQLite and Tantivy are derived and rebuildable:**

1. **Vault files (disk)** — Markdown + YAML frontmatter. The real content.
2. **SQLite** (`librarium.db` via SQLx) — users, vaults, vault shares, groups,
   labels, API keys, sessions, preferences, recent files, ML undo receipts,
   change log. Metadata *about* content and *about* users; never the content
   itself.
3. **Tantivy** — per-vault full-text index, persisted on disk, rebuilt by
   scanning vaults at startup and updated incrementally thereafter.

### The watcher event loop (core data flow)

This loop is the heart of the "files are the source of truth" design and the
most consistency-sensitive code in the system. Changes here must be covered by
integration tests.

1. **User edit** → frontend sends `PUT /api/files/...`.
2. **Route → `FileService`** writes to disk (path-safety enforced).
3. **OS** confirms the write.
4. **`FileWatcher`** (`notify`, recursive, 500 ms debounce) emits a
   `Created` / `Modified` / `Deleted` / `Renamed` event.
5. **Event loop** (in `lib.rs`) batches events and, per change:
   - updates the **Tantivy** index,
   - runs **entity/relation reindex** for affected files,
   - **broadcasts** the change over the WebSocket channel.
6. **Frontend** receives the WS event and refreshes the file tree / warns about
   externally-changed open files / reloads content as appropriate.

Because external edits (git pull, another editor, sync tools) flow through the
exact same watcher path as API writes, the UI and indexes converge regardless of
how a file changed.

---

## 5. Frontend (`frontend/`)

**Stack:** Vue 3 (Composition API), TypeScript, Vuetify 3, Pinia, Vue Router 4,
Vite 6. Editing uses **Tiptap** (rich/WYSIWYG Markdown) and **CodeJar** (raw
Markdown). Rendering helpers: `highlight.js` (code), `mermaid` (diagrams),
`pdfjs-dist` (PDF preview), `d3-force`/`d3-selection` (graph view), `dompurify`
(sanitization), `yaml` (frontmatter).

### Source layout (`frontend/src/`)

| Directory | Contents |
| --- | --- |
| `api/` | REST client modules + WebSocket wiring; TypeScript types mirroring backend JSON. |
| `stores/` | Pinia stores: `auth`, `vaults`, `files`, `editor`, `tabs`, `preferences`, `graph`, `plugins`, `indexing`, `ui`. |
| `components/` | Feature-grouped components: `editor/`, `sidebar/`, `tabs/`, `graph/`, `modals/`, `viewers/`, structural/layout. |
| `composables/` | Reusable logic (`useWebSocket`, `useUndoRedo`, `useNotifications`, `usePlugins`, …). |
| `pages/` + `layouts/` + `router/` | Routed pages (login, change-password, admin) and the main layout; router guards enforce auth + token freshness. |
| `utils/`, `editor/`, `plugins/`, `vendor/` | Helpers, editor internals, Vue plugin setup, vendored bundles. |

The build (`npm run build`) type-checks with `vue-tsc`, bundles with Vite, and
the output is embedded into the server binary at compile time, so the server
ships as a single self-contained executable.

### Frontend ↔ backend contract

The frontend's `api/` types are hand-mirrored from the backend's JSON shapes
(`models/` + `librarium-types`). **Changing one side's payload without the other
is a breaking change** — keep them in lockstep.

---

## 6. Desktop (`librarium-tauri`)

The desktop app is a thin native shell, not a reimplementation:

1. Resolves platform paths (portable / installed) and loads-or-creates `config.toml`.
2. Enforces a long-lived refresh token so the single local user stays signed in
   across restarts (loopback-only, HttpOnly cookies — see
   `archive/PLAN-desktop-sync-multiuser.md` for the original rationale).
3. Sets up a system tray (starting / running / error states).
4. Registers the `librarium://` deep-link handler.
5. Spawns `librarium-server` bound to `127.0.0.1`.
6. Opens a WebView at the local server URL.

Native capabilities exposed to the frontend (via Tauri commands / optional
`@tauri-apps/*` packages): folder picker dialog and desktop notifications.

---

## 7. Plugins

Plugins are JavaScript modules, discovered and loaded by `plugin_service`, with
access mediated by a **capability-gated host API** (`plugin_api`). Each plugin
declares its capabilities and hooks in `manifest.json`.

```text
plugins/<plugin-id>/
├── manifest.json   # id, name, version, capabilities, hooks, config schema
└── main.js         # ES module entry point
```

- **Capabilities** gate what a plugin may do (read files, vault metadata,
  editor access, modify UI, storage, HTTP).
- **Hooks** include `on_load`, `on_file_open`, `on_file_save`, `on_editor_change`.
- **Config schema** (JSON Schema) auto-generates a settings UI.

Bundled examples: `backlinks`, `daily-notes`, `word-count`, `worldbuilding`,
and an `example-plugin` template. Plugin development is documented in
[`docs/archive/PLUGIN_API.md`](archive/PLUGIN_API.md) and
[`docs/archive/PLUGIN_ARCHITECTURE.md`](archive/PLUGIN_ARCHITECTURE.md).

---

## 8. Authentication & security

- **Auth methods:** local password (Argon2), LDAP/AD, OIDC (OAuth2 Authorization
  Code). Optional **TOTP 2FA** and **API keys** (prefix-indexed, optionally
  expiring, revocable).
- **Tokens:** short-lived JWT access tokens + longer-lived refresh tokens; the
  router guard keeps the access token fresh. Desktop uses a long-lived refresh
  token by design.
- **Authorization:** per-vault roles — **Owner / Editor / Viewer** — plus groups,
  sharing, and invitations, enforced in `middleware/auth.rs`.
- **Filesystem safety:** every path is canonicalized and checked for containment
  in `FileService`; this guard must never be bypassed.
- **Transport:** optional TLS (PEM), configurable CORS, rate limiting, request IDs.
- **First run:** if the DB is empty, an admin is bootstrapped (config-provided or
  auto-generated credentials written next to the DB) with a forced password change.

Security-sensitive areas to review carefully on any change: `routes/auth.rs`,
`middleware/auth.rs`, `routes/totp.rs`, `services/file_service.rs`,
`services/search_service.rs`, `services/reindex_service.rs`.

---

## 9. Configuration

The server reads `config.toml` by default, overridable with `LIBRARIUM_CONFIG`
or `--config`. Environment variables override TOML using nested keys
(`LIBRARIUM__SECTION__KEY`). The committed root `config.toml` is
development-oriented and **not** a production baseline; `config.example.toml` is
the annotated reference.

Configurable sections: server, database, vault paths, auth (JWT/LDAP/OIDC), sync,
CORS, TLS, and ML tiers. Full reference: [`docs/archive/CONFIGURATION.md`](archive/CONFIGURATION.md).

---

## 10. Build & run

Prerequisites: Rust (2021 edition toolchain), Node.js, and PowerShell 7+ for the
helper scripts.

```bash
# Frontend (produces assets embedded into the server binary)
npm --prefix frontend install
npm --prefix frontend run build

# Backend
cargo build --release -p librarium-server   # production binary
cargo run -p librarium-server                # dev run

# Desktop
cargo tauri dev      # from crates/librarium-tauri (auto-reload)
cargo tauri build    # release desktop bundle
```

Common checks:

```bash
cargo check --workspace
cargo test -p librarium-server          # backend tests
cargo test --workspace                  # all Rust tests
npm --prefix frontend test              # Vitest unit tests
npm --prefix frontend run test:e2e      # Playwright E2E
cargo bench --bench markdown_benchmarks # benchmarks
```

Release profiles in the root `Cargo.toml`: `release` (size-optimized: `opt-level=z`,
LTO, strip, panic=abort) and `release-fast` (3–5× faster builds for iteration).
Docker and packaging are covered in `docs/archive/DOCKER.md`,
`docs/archive/DEPLOYMENT.md`, and `docs/archive/BUILD.md`.

---

## 11. Maintaining this document

This document and `README.md` are the project's two living documents. **In any
change that does one of the following, update this file (and `README.md` if the
overview/quick-start is affected) as part of the same commit:**

- adds, removes, or renames a crate, service, route module, or Pinia store;
- changes a public REST/WebSocket payload or the frontend⇄backend contract;
- changes the data/persistence model or the watcher → index → broadcast flow;
- changes auth, authorization, or filesystem-safety behavior;
- changes configuration keys, build steps, or run commands;
- bumps the project version.

When a design note here is fully superseded, move the long-form detail into
`docs/archive/` and leave the summary here pointing to it. Keep this document
describing **the system as it is now**, not its history.
