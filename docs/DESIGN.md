# Librarium — Design & Architecture

> **Status:** Canonical design document. This file and the root `README.md` are the
> two documents kept current as the project evolves. When a change alters
> architecture, data flow, public APIs, configuration, or build/run steps, update
> this document in the same change (see [Maintaining this document](#11-maintaining-this-document)).
>
> Historical design notes, feature plans, and superseded specs live in
> [`docs/archive/`](archive/). Treat archived files as background, not as a
> description of the current system.

**Version:** 0.102.13

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
| `librarium-core` | `crates/librarium-core` | Platform-independent core shared with non-server consumers: `AppError`/`AppResult`, `FileService` (path-safe disk I/O), frontmatter read/write, Markdown render + Obsidian wiki-link parsing/resolution, Tantivy full-text search (behind a default-on `search` feature). No actix/sqlx/tokio in its default feature set; `librarium-server` enables its `actix` and `sqlx` features. |
| `librarium-types` | `crates/librarium-types` | Shared Rust DTOs and parser/contract types used across crates. |
| `librarium-client` | `crates/librarium-client` | Reusable HTTP + WebSocket client for the Librarium API. |
| `librarium-tauri` | `crates/librarium-tauri` | Tauri 2 desktop shell embedding the server + frontend, **and** the Android app host (Route C 19, #62). Has both a `[lib]` (`librarium_tauri_lib`, `crate-type = ["staticlib", "cdylib", "rlib"]`) and a `[[bin]]` (`main.rs`, a two-line shim calling `librarium_tauri_lib::run()`) — required for the Android/iOS host to load the app as a native library (Route C 18, #61). Desktop vs. mobile setup is `#[cfg(desktop)]`/`#[cfg(mobile)]`-gated in `run_setup` (Tauri's own OS-based cfg, not a Cargo feature — desktop drives config/JWT/tray/deep-links/actix-spawn/health-poll and the `sync_*` commands; mobile registers `librarium-mobile`'s `invoke_handler` and constructs its `SearchIndex`/`MobileDb`/`SyncHandle` managed state, the first real app host for that crate). The underlying dependencies (`librarium-server`, `actix-web`, `reqwest`, `librarium-sync`, `dirs`) are excluded from Android/iOS builds by **target** cfg in `Cargo.toml` (`[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`) rather than a Cargo feature — `cargo tauri android build` has no flag to disable default features, so exclusion has to be automatic for the real target (verify: `cargo tree -p librarium-tauri --target aarch64-linux-android -e normal`, no `librarium-server`/`actix-web`). `tauri.android.conf.json` overrides `frontendDist` to the real Vite build output (desktop's `tauri.conf.json` keeps its `"./loading"` stub — desktop never uses Tauri's static-asset serving for the real UI, only the embedded server over HTTP). `gen/android` (from `cargo tauri android init`) is committed; its default scaffold already handles cleartext-HTTP-in-debug-only (`android:usesCleartextTraffic` manifest placeholder, flipped per Gradle build type) and the `INTERNET` permission, so no manual manifest edits were needed for #62's requirements. Mobile-only (target-cfg-gated) deps also include `tauri-plugin-background-service` + `tauri-plugin-device-info` (Route C 21, #64): `background_sync.rs`'s `MobileSyncService` implements the former's `BackgroundService` trait to run `librarium-mobile`'s `reconcile_once` on a 15-minute timer inside an Android foreground service (persistent notification, `START_STICKY`) instead of the always-live WebSocket task the foreground case uses — Android kills an ordinary backgrounded process. The policy gate (Wi-Fi-only / battery-threshold, read from `MobileDb`'s `sync_policy` table) uses the latter plugin's `DeviceInfoExt` trait, callable directly from Rust with no webview involved. The frontend starts the service once during local-mode bootstrap via `plugin:background-service|start` (`utils/tauri.ts`'s `startBackgroundSyncService`); `tauri-plugin-background-service`'s Android module bundles its own manifest permissions (`FOREGROUND_SERVICE*`, `POST_NOTIFICATIONS`, plus unrelated telecom/camera ones from its broader feature set — no manual manifest edits needed, confirmed via the merged-manifest build output). |
| `librarium-mobile` | `crates/librarium-mobile` | Route C thin-client command layer: vault listing (local JSON registry, not SQLite), file/directory commands over `librarium-core::FileService`, Markdown render, wiki-link resolution/backlinks/outgoing-links, tags, standalone frontmatter read/write, on-device Tantivy search (build/rebuild/incremental-update + search, behind a `local_search_enabled` switch), a local metadata store (preferences, recent files, favorites, bookmarks) in its own `mobile.db` — device-local, not synced — and a `librarium-sync` bridge (`SyncHandle`: add/list/remove remotes, map/unmap vaults, start/stop/status, plus opinionated single-remote `pairing_set`/`pairing_get`/`pairing_clear` commands) ported from `librarium-tauri/src/sync_bridge.rs`, with local-vault-path resolution swapped from the desktop's `librarium.db` to the same JSON registry `vault` uses. API keys are never persisted in plaintext: a `SecretStore` trait backs `pairing_set`/`sync_add_remote` (`OsKeyringStore` in production — platform Keychain/Credential Manager/Secret Service/Android Keystore via the `keyring` crate; `InMemorySecretStore` in tests), and `librarium-sync`'s `SyncEngine` reads each remote's key from an injected `ApiKeyProvider` closure at connection time rather than from its own database — the one deliberate, minimal change made to `librarium-sync` (previously untouched) for this. Plain, Tauri-free `pub async fn`s for the actual logic; private `#[tauri::command]` wrappers resolve paths via Tauri's path API. Wired into `librarium-tauri`'s mobile entry point (#62) — `run_setup` builds its `SearchIndex`/`MobileDb`/`SyncHandle` state under app-private storage dirs from Tauri's own path API. **Known gap**: `OsKeyringStore` needs a one-time JNI bootstrap (`keyring_core::set_default_store(...)`) from the running Android Activity that isn't wired up yet — `pairing_set`/`sync_add_remote` will error on a real device until whichever issue does the device bring-up adds it. `SyncHandle::reconcile_once` (Route C 21, #64) is a coarse one-shot reconcile — full manifest reconcile + outbox drain + catch-up, no watcher, no live socket — for the Android background service and the manual "sync now" command (`sync_reconcile_once`); it wraps a new `librarium_sync::SyncEngine::reconcile_once`, extracted from the same `full_reconcile`/`drain_outbox`/`pull_changes` sequence the always-live per-vault task already runs once before going live, so the two paths share identical reconcile logic. `mobile.db` also gained a `sync_policy` table (`SyncPolicy { wifi_only, battery_threshold }`, `sync_get_policy`/`sync_set_policy` commands) — deliberately its own table rather than a `UserPreferences` field, since those two settings are Android-background-sync-specific and `UserPreferences` is also the server's synced `/api/preferences` type. |
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
| `error.rs` | `AppError` / `AppResult`, re-exported from `librarium-core` (`impl ResponseError` lives there, behind its `actix` feature). |

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
| `file_service` | All disk I/O. **Owns path-traversal protection** (canonicalize + containment checks), conflict detection, trash/backup on conflict, move/rename. Lives in `librarium-core`, re-exported here. |
| `search_service` | Tantivy wrapper: per-vault index, incremental updates, query + snippet highlighting. Lives in `librarium-core` (behind its `search` feature), re-exported here. Index directory is resolved here (`LIBRARIUM_INDEX_DIR`/`CODEX_INDEX_DIR`, default `./data/indices`) and passed in explicitly — the core crate has no stable notion of an environment or current directory. |
| `reindex_service` | Two-pass entity/relation indexer from frontmatter; single source of truth for entity state (distinct from full-text search). |
| `markdown_service` | Markdown parsing/rendering (`pulldown-cmark`), link rewriting. Lives in `librarium-core`, re-exported here. |
| `wiki_link_service` | Obsidian `[[wiki link]]` parsing and rewriting. Lives in `librarium-core`, re-exported here. |
| `frontmatter_service` | YAML frontmatter read/write. Lives in `librarium-core`, re-exported here. |
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
4. **Logs** — `{data_dir}/logs/` holds two file streams. The server-side
   tracing output lives at `librarium.log.YYYY-MM-DD` (one file per calendar
   day). The desktop-only frontend diagnostic log lives at `frontend.log`
   and rotates on every app start (current → `.1`, `.1` → `.2`, `.2` → `.3`,
   `.3` dropped) — the last four full runs of the shell are always available
   for post-mortem after an unexpected drop to `/login`. The rotation is
   driven by `librarium-tauri`'s `FrontendLog::init`; the frontend writes
   through the `frontend_log` Tauri command from `frontend/src/utils/logger.ts`.

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
Vite 6. Editing uses **CodeJar** (a `contenteditable`) over the Markdown source,
with a bespoke line-based highlighter (`utils/highlight.ts`) providing the
`formatted_raw` mode. Tiptap is present in `package.json` and
`components/editor/TiptapEditor.vue` exists, but **nothing imports it** — it is
currently dead code, not the editing path. Rendering helpers: `highlight.js`
(code), `mermaid` (diagrams), `pdfjs-dist` (PDF preview),
`d3-force`/`d3-selection` (graph view), `dompurify` (sanitization), `yaml`
(frontmatter).

**Markdown tables** (#122) are handled entirely as source text. `editor/table.ts`
is a pure-function module — parse a table block into a `ParsedTable`
(`{indent, header, alignments, rows, blockStart, blockEnd}`), mutate the
structure, serialize back with columns padded to their widest cell.
`findTableAt` returns non-null only when a valid separator row is present, and
that single predicate drives both the contextual toolbar's visibility and the
commands themselves, so the two cannot disagree. Every table operation
re-serializes the whole block, so tables are auto-aligned on every edit.
Enter and Tab behaviour lives here too as pure functions, with
`MarkdownEditor.vue` reduced to a thin adapter; the table logic previously sat
inside that component where it could not be unit-tested, which is how the
separator-row bug reached users. Deliberately, `formatted_raw` shows aligned
Markdown source rather than a rendered grid: files on disk are the source of
truth, and a rich-text document model would round-trip the whole document and
risk reformatting content the user never touched.

### Source layout (`frontend/src/`)

| Directory | Contents |
| --- | --- |
| `api/` | REST client modules + WebSocket wiring; TypeScript types mirroring backend JSON. |
| `stores/` | Pinia stores: `auth`, `vaults`, `files`, `editor`, `tabs`, `preferences`, `graph`, `plugins`, `indexing`, `ui`, `sync`. |
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

### Build/version display

Settings → About (`AboutPanel.vue`) shows the running server's version, short
git commit hash, and build date by calling the pre-existing, unauthenticated
`GET /api/version` (`version.rs` — all three fields embedded at compile time
via `librarium-server/build.rs`'s `git rev-parse --short HEAD`). Keeping that
version current is `cargo xtask bump-version`'s job — see CLAUDE.md's
"Documentation & versioning" section.

### Capability set (Route C thin mobile client)

`request()` in `api/client.ts` goes through a pluggable `Transport`
(`httpTransport`, today's `fetch`-based default, vs. `localTransport`,
dispatching to `librarium-mobile` Tauri commands via `api/localDispatcher.ts`
— see crate table above). The local transport implements the core editing
loop (vaults, files, render, search, tags, backlinks, preferences, recent
files, favorites, bookmarks, random/daily notes) but not the server-only
features below — those only work against a real `librarium-server`, and the
capability set is what hides them under the local transport instead of
letting them fail at runtime:

| Capability (`useCapabilities()`) | Hidden UI | Why not implemented locally |
| --- | --- | --- |
| `canUseAdmin` | `/admin/users` route + its `TopBar` menu item | User/role administration is inherently multi-user server state. |
| `canUseGroupsAndSharing` | `VaultManager`'s "Sharing & Groups" section (shows "not available offline" instead) | Vault sharing, groups, and invitations require a multi-user server; mobile is single-user by construction (#52). |
| `canUsePlugins` | `PluginManager` modal + its `TopBar` triggers | Plugin execution is server-hosted JavaScript; not ported to the mobile command layer. |
| `canUseMlOrganize` | `MlInsightsPanel` (outline/analysis/organize-vault) | ML organization (keyphrase extraction, embeddings) is server-only. |
| `canUseEntityGraph` | `EntityRelationsPanel`, `NewEntityDialog`, graph view, "New entity" (`SidebarActions`) | Entity/relation modeling and the graph view aren't in the mobile command surface. |
| `canUseReindex` | Reindex buttons (`StructuralEditor`; admin's per-vault reindex is already covered by `canUseAdmin`) | Manual reindex triggers a full Tantivy rebuild via a server-only route. |
| `canUseArchiveImportExport` | ZIP/tar.gz export menus (`SidebarActions`, `FileTreeNode`); archive (`.zip`/`.tar`/`.tar.gz`/`.tgz`) import fails with a clear message rather than being hidden, since the generic import picker can't tell a file is an archive until after selection | `apiImportArchive`/`apiDownloadZip`/`apiDownloadTar` bypass the transport for binary bodies; not implemented locally. |

All gates are driven off `isLocalTransportActive()` (the active transport),
**not** `useMobile`'s `isMobile` (viewport size) — a narrow desktop window is
mobile-*sized* but fully capable, so conflating the two would hide these
features on desktop too. Every gate hides rather than proxies to a paired
remote: a mobile session's remote credentials live in Rust secure storage
(#54) and never reach the WebView, so there's no mechanism today for the
frontend to call the remote's admin/plugin/ML endpoints directly — that
would be a separate, larger proxying feature, not a hiding one.

### Contract test suite (Route C, #59)

The capability-set table above documents *which* routes the local transport
implements; the contract test suite guards that its implementation actually
*agrees* with the server's. Two implementations of the same route table exist
— `librarium-server/src/routes/` (HTTP) and `librarium-mobile/src/` (Tauri
commands, dispatched via `localDispatcher.ts`) — so nothing structurally
prevents them from drifting apart as either side changes independently.

- **`crates/librarium-mobile/tests/contract_test.rs`** is the real
  drift-detector: it starts an in-process `librarium-server` and drives every
  route in #56/#57's scope against both a real HTTP call (`ObsidianClient`, or
  a raw authenticated request for the handful of routes `ObsidianClient` has
  no method for) and the equivalent `librarium-mobile` function, operating on
  the *same on-disk vault directory* for both sides. Responses are compared
  after stripping a short, reviewed set of legitimately-differing fields:
  - `created_at`, `updated_at`, `modified` — each side stamps these
    independently (separate clock/`mtime` reads for the same operation), so
    the value differs even though both are valid for the same event.
  - `user_id` — present on the server's `Favorite` (multi-user), *absent* on
    `librarium-mobile`'s (single-user by construction, #52) — an absence, not
    a differing value.

  Covered by `cargo xtask ci`, and worth running alone — separate from the
  general `cargo test --workspace` run. See AGENTS.md's Build And Test section for the
  local command, and the test file's module doc for the full design
  rationale (including two non-obvious traps it works around: the fire-and-
  forget `/reindex` route, and `SearchIndex::update_file`'s silent no-op for
  an unregistered vault).
- **`frontend/src/api/localDispatcherCoverage.test.ts`** is a lighter,
  frontend-only static coverage check: it drives every #56/#57 `apiXxx` call
  in `client.ts` through the real `localDispatcher.ts` route table (via
  `localTransport`) and asserts none of them throws
  `LocalTransportUnsupportedError`. It does not compare response values —
  that's the Rust suite's job — it only catches an `apiXxx` call site
  drifting to a method/path the dispatcher's table no longer matches.

### Offline UX (Route C, #60)

`librarium-mobile`'s pairing (`pairing_set/get/clear`, #54) and sync
(`sync_status/start/stop`, #53) commands existed with no frontend caller
until this issue — this is that wiring, entirely gated on
`useCapabilities().isLocalMode` (#58) and inert until a mobile app host
(#61-#63) calls `setTransport(localTransport)`.

- **`stores/sync.ts`** owns pairing/status state and a single shared 3s
  status poll (`startPolling`/`stopPolling`), consumed by both the TopBar
  chip and the Settings panel rather than each polling independently.
  `VaultStatus` has no wall-clock "last synced" field (only
  `last_synced_seq`, a sequence number) — `lastSyncedAt` is a client-side
  timestamp stamped whenever a poll observes a vault as `live`, the
  practical proxy for it.
- **`components/sync/PairingGate.vue`**, rendered by `MainLayout.vue` ahead
  of the normal vault/editor UI whenever `isLocalMode` is true and the
  device isn't yet paired with at least one vault mapped, blocks first use
  until sync is set up. It embeds `PairingSection.vue`, which also doubles
  as the paired-state view (re-pair status, unpair, map another vault) —
  reusing `components/settings/sync/VaultMappingSection.vue` as-is for the
  vault-mapping step, since mobile pairs to the fixed remote id `"primary"`
  but otherwise shares the exact command surface #53 already gave the
  desktop bridge.
- **Conflict surfacing has no dedicated backend command.**
  `librarium-sync`'s keep-both resolution writes ordinary
  `conflict_<stem>_<YYYYMMDD_HHMMSS>.<ext>` sibling files
  (`crates/librarium-sync/src/engine.rs`'s `conflict_name()`), so
  `stores/sync.ts`'s `scanConflicts()` just walks each mapped vault's
  existing `file_tree` command and filters on that filename convention —
  cheap enough given it only runs on demand (when the Settings panel opens
  or a conflict is resolved), not on the 3s status poll. Resolving a
  conflict is `mobileFileDelete`/`mobileFileRename` against the existing
  file commands; no new backend surface was needed for any of this.
- **`components/modals/SearchModal.vue`** now catches
  `LocalSearchUnavailableError` (`api/localDispatcher.ts`, thrown when no
  on-device index exists for a vault) and shows an explicit "not available
  offline" state instead of the previously-uncaught failure. The rest of the
  capability-gated empty states (admin, groups/sharing, plugins, ML,
  entity graph, reindex, archive import/export) already existed from #58.
- Vitest gained real `@vue/test-utils` component-mount coverage for the
  first time (`vitest.config.ts` now runs `vite-plugin-vuetify`'s
  `autoImport` — with `styles: 'none'`, since per-component CSS imports
  don't resolve under Vitest's transform — and `server.deps.inline` for
  `vuetify` itself, needed for the same reason; `test-setup.ts` stubs
  `window.visualViewport`, which happy-dom doesn't implement but Vuetify's
  overlay positioning references unconditionally).

Verified end-to-end on an Android emulator (#63/#64): launch, pair, sync,
browse/edit, and background sync via an Android foreground service all
work, modulo the still-open Android Keystore JNI bootstrap gap noted in the
crate table above (pairing/sync currently need a temporary in-memory secret
store swap for device testing until that lands). Physical-device
verification remains outstanding — the sandboxed environment these issues
were developed in has no attached hardware.

**Distribution (#67): sideload-only**, not the Play Store. Librarium talks
only to a server the user already configures — there's no first-party
backend, telemetry, or data collection for a store listing to review, and
self-hosted-tool users are already comfortable enabling "install unknown
apps" for the desktop build's package-manager-free installers. Sideloading
avoids the Play Store's review/policy surface and data-safety declaration
process entirely. Release APKs are signed (`gen/android/app/build.gradle.kts`'s
`signingConfigs["release"]`, keyed off a gitignored `keystore.properties` —
see `AGENTS.md`'s "Android release signing" section) and attached to GitHub
releases alongside the desktop/server artifacts. Release artifacts are built
locally — this repo deliberately has no hosted CI, so `cargo xtask ci` is the
verification gate and release builds are run by hand. Revisit Play Store
distribution only if sideload friction turns out to matter in practice —
`--aab` (Play Store's required bundle format) is a one-flag addition to the
same `cargo tauri android build` invocation whenever that's decided.

---

## 6. Desktop (`librarium-tauri`)

The desktop app is a thin native shell, not a reimplementation:

1. Resolves platform paths (portable / installed) and loads-or-creates `config.toml`.
2. Enforces a long-lived, non-rotating refresh token so the single local user
   stays signed in across restarts. On desktop the refresh token is persisted in
   the WebView's `localStorage` and sent in the `/api/auth/refresh` body (the
   HttpOnly cookie is still set but treated as best-effort — the Tauri WebView
   does not reliably persist HttpOnly cookies across app restarts). This is
   loopback-only, single-user, no third-party content, so `localStorage` is an
   acceptable durable store. See `archive/PLAN-desktop-sync-multiuser.md` for
   the original rationale.
3. Sets up a system tray (starting / running / error states).
4. Registers the `librarium://` deep-link handler.
5. Spawns `librarium-server` bound to `127.0.0.1`.
6. Opens a WebView at the local server URL.

Native capabilities exposed to the frontend (via Tauri commands / optional
`@tauri-apps/*` packages): folder picker dialog, desktop notifications, and
writing base64-encoded bytes to a path the user already chose via the native
save dialog (`write_binary_file` — used by the in-app feedback exporter below;
the path always comes from the user's own dialog selection, so no
general-purpose filesystem plugin is needed).

### In-app feedback capture

The top bar's feedback button (`FeedbackModal.vue`, backed by
`composables/useFeedback.ts`) bundles a free-text report with diagnostic
context — the active vault/open tabs/panes, the last 500 entries from the
frontend logger's ring buffer, and app/browser environment info — plus an
optional DOM screenshot (`html2canvas`, capturing the app's own rendered UI,
not a true OS-level screen capture) into a zip (`jszip`) named
`librarium-feedback-<timestamp>.zip`. Entirely client-side: nothing is ever
sent to a server. On desktop the zip is written straight to disk via the
native save dialog + `write_binary_file`; in a plain browser it's a normal
download. The resulting file is meant to be attached/pasted into an AI coding
assistant by hand.

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

- **Capabilities** and **hooks** (`on_load`, `on_file_open`, `on_file_save`,
  `on_editor_change`) are declared per plugin in `manifest.json`.
- **Config schema** (JSON Schema) auto-generates a settings UI.

**Known gap, found during documentation triage (2026-09-16):** the frontend
plugin loader (`frontend/src/composables/usePlugins.ts`) only actually invokes
`onLoad` — `onFileOpen`, `onEditorChange`, and `onFileSave` are declared in
manifests and implemented by bundled plugins (e.g. `word-count`) but never
called, so those plugins only run their load-time logic and never react to
file switches or edits. Capabilities are not enforced (declarative only).
Separately, `crates/librarium-server/src/services/plugin_api.rs`'s `PluginApi`
struct — a different, more structured host-API surface with file/storage/HTTP/
event methods — is never constructed anywhere; the live plugin API is the
small ad-hoc object built inline in `usePlugins.ts` (`addRibbonIcon`,
`addStatusBarItem`, `storage_get/set`, `read_file`/`write_file`, `show_notice`,
`getContext`, `register_command`), several of whose methods are stubs
(`show_notice` is a bare `alert()`, `register_command` only logs). The
previous plugin-authoring docs (`docs/archive/PLUGIN_API.md`,
`PLUGIN_ARCHITECTURE.md`) described neither of these accurately — they predate
this implementation and describe a `window.app`-based model that was never
built. Left archived rather than corrected in this pass: an accurate plugin-
authoring guide needs its own verification pass once the hook-dispatch and
dead-code questions above are resolved one way or the other, not a rewrite
that documents known-broken behavior as if it works.

---

## 8. Authentication & security

- **Auth methods:** local password (Argon2), LDAP/AD, OIDC (OAuth2 Authorization
  Code). Optional **TOTP 2FA** and **API keys** (prefix-indexed, optionally
  expiring, revocable).
- **Tokens:** short-lived JWT access tokens + longer-lived refresh tokens; the
  router guard keeps the access token fresh. Browser deployments hold the
  refresh token in an HttpOnly, SameSite=Strict cookie (JS cannot read it).
  Desktop uses a long-lived (10-year floor, non-rotating) refresh token stored
  in the WebView's `localStorage` and sent in the refresh request body — the
  Tauri WebView doesn't reliably persist HttpOnly cookies across restarts, and
  loopback-only + no third-party content makes `localStorage` acceptable.
  Desktop also mirrors the refresh token to a second, disk-backed copy at
  `{data_dir}/session.json` (`session_store.rs`, atomic write, `0600` on Unix)
  so a wipe of the WebView's UserData folder — an uninstall/reinstall of the
  shell, a runtime reset — doesn't force a re-login as long as the portable
  data dir survives; the frontend restores from it at boot
  (`authStore.bootstrapPersistence()`) whenever `localStorage` is empty.
- **Session-clear contract:** every call site that can throw while checking or
  refreshing a session (`App.vue`'s mount hook, the router guard,
  `useWebSocket`'s connect, `MainLayout`'s mount hook) acts only when the server
  positively said the session is invalid (`isSessionInvalid()` in
  `api/client.ts` — an `ApiError` with `status === 401`). Any other failure (a
  loopback hiccup on wake-from-sleep, a brief WebSocket reconnect race) is
  treated as transient and left alone rather than wiping tokens;
  `authStore.refresh()` itself retries transient failures 3× with a short
  backoff before giving up.
- **Voluntary vs. involuntary session end:** `authStore.logout()` is the
  *voluntary* path — it revokes the session server-side and deletes the
  disk-backed token. `authStore.clearLocalSession()` is the *involuntary* path
  and clears only in-memory + `localStorage` state. Involuntary callers (the
  401 handler, `useWebSocket`) must use the latter. The distinction is load
  bearing: `ensureFreshForRequest` deliberately lets a request proceed when its
  refresh failed, so a transient blip surfaces as a 401 carrying a stale token.
  Treating that single 401 as fatal — and taking the destructive path — is what
  ended desktop sessions after days or weeks despite the 10-year TTL: one
  unlucky moment in weeks of hourly refreshes destroyed the credential
  permanently. `handleUnauthorized` now forces one refresh and replays the
  request once, clearing only local state if that also 401s.
- **Credential mutation** funnels through one service,
  `services/credentials.rs`'s `CredentialService`: validate policy → Argon2 hash
  → update the row → **revoke every session the user holds** → audit. The
  revocation matters — before this existed, resetting a compromised account's
  password left the attacker's session alive, and on desktop with a 10-year
  non-rotating refresh token that meant indefinitely. It takes `&Database`
  rather than `AppState` so callers without a running server can use it. Note
  that a password change does **not** revoke API keys: those are independently
  managed credentials with their own revocation UI, and breaking every
  automation on a password change would be wrong.
- **Password recovery** has three transports onto that one service, chosen
  deliberately:
  - **CLI** (`cli.rs`, `librarium admin set-password|create-user|list-users`)
    opens SQLite directly. Recovery tooling must not depend on the thing that is
    broken — an HTTP-based tool is useless when you cannot authenticate or the
    server will not start. Passwords are prompted with no echo, never taken from
    `argv` (visible in `ps`, kept in shell history). Filesystem access to the
    database *is* the authorization model, which is the right boundary for
    self-hosting: anyone who can read that file can already read every vault
    file it indexes.
  - **Desktop reset** (`auth_reset_local_password`, `#[cfg(desktop)]`) is a
    Tauri command, **not** an HTTP route: the embedded server is loopback, where
    any local process and the browser build could reach an endpoint, whereas a
    Tauri command is callable only from this app's own WebView. It needs no
    further gate because the server is single-user and loopback-only and the
    vault files are already readable by that OS account.
  - **HTTP** (`/api/auth/change-password`, admin reset) for authenticated
    in-app changes.
  There is deliberately no email/SMTP reset: it would require outbound network
  and conflict with the offline-first stance.
- **Desktop password setup:** `SecurityPanel.vue` (Settings → Security, desktop
  only) turns password protection on or off and changes the password, writing
  `auth.enabled` plus the account through the same `write_to_file` path that
  persists the JWT secret. `AppConfig` is read once at startup, so enabling or
  disabling takes effect on the next launch and the panel says so.
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
CORS, TLS, and ML tiers. Full reference: [`docs/CONFIGURATION.md`](CONFIGURATION.md).

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
Docker and packaging are covered in `docs/DEPLOYMENT.md` and
`docs/BUILD.md`.

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
