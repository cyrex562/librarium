# Librarium Platform Architecture — Implementation Spec

> A unified design and task breakdown covering the migration to a Tauri-based
> desktop client, single binary architecture, config path flag, platform data
> directories, and document format strategy. Intended as the single source of
> truth for this feature set alongside the worldbuilding spec.

---

## Design Summary

Librarium currently has three separate binaries: `librarium-server` (Actix Web backend
with embedded Vue frontend), `librarium-desktop` (Iced native GUI), and `librarium-client`
(async HTTP/WS library). Every new feature requires parallel implementation in
both the Vue frontend and the Iced desktop client.

This spec replaces the Iced desktop client with a Tauri shell that embeds the
existing Vue frontend in a native WebView, running the Actix server in the same
process. The result is one binary for desktop use, one for headless server/Docker
use, and the browser as a zero-install fallback — all sharing a single frontend
codebase.

The offline edit queue is shelved. It becomes relevant only when two Librarium
instances need to sync with each other, which is a separate future feature.

---

## Architecture Overview

```
BEFORE
├── librarium-server     Actix + embedded Vue SPA       standalone server / Docker
├── librarium-desktop    Iced GUI → HTTP → librarium-server  desktop (being retired)
└── librarium-client     reqwest + tungstenite library   used by desktop + tests

AFTER
├── librarium-server     Actix + embedded Vue SPA        standalone server / Docker
│   └── lib.rs       run(config) callable by Tauri
├── librarium-tauri      Tauri shell                     desktop app
│   ├── embeds       librarium-server::run() on bg thread
│   └── provides     OS integration only (tray, dialogs, notifications)
└── librarium-client     reqwest + tungstenite library   tests + CLI tools (kept)
```

### Key principles

- The Vue frontend is the only UI codebase. Zero Iced UI work after migration.
- The Tauri shell is intentionally thin — OS integration only, no business logic.
- Standalone server mode (`librarium-server` binary) is unchanged for Docker/headless.
- Browser access to a running server remains a first-class supported path.
- Config path is always explicit; no working-directory magic in production.
- Platform data directories are used by the Tauri app; working-directory
  conventions remain available for the standalone binary via the `--config` flag.

---

## Testing Requirements

All implemented code in this spec must pass three test gates before a phase is
considered complete. No phase is done until all three gates are green. This
applies equally to Rust backend changes, Vue frontend changes, and Tauri shell
changes.

### Gate 1 — Rust unit and integration tests

```bash
cargo test                        # all unit + integration tests
cargo test -p librarium-server        # server crate only
cargo test -p librarium-tauri         # Tauri crate only
RUST_LOG=debug cargo test -- --nocapture   # with log output for failures
```

**Unit tests** live inline in each source file under `#[cfg(test)] mod tests`.
Every public function in a new or modified module must have unit test coverage
for its happy path and principal error paths.

**Integration tests** live in `tests/`. Every new API endpoint, config loading
path, and service method introduced by this spec requires at least one
integration test exercising the full request/response cycle against a real
in-process server instance. Integration tests must use a temporary directory
fixture for config, database, and vault paths — never the developer's actual
data directories.

Minimum coverage expectations per phase:

| Area | Required tests |
|---|---|
| `AppConfig::load_from_file` | Missing file, malformed TOML, valid file, env var override |
| `AppConfig::load_from_dirs` | First launch (no file), existing file, default value generation |
| `AppConfig::write_default` | Writes valid TOML, returns correct struct, idempotent on re-run |
| `librarium-server::run()` as library | Starts, serves `/api/health`, shuts down cleanly |
| CLI flag parsing | `--config` flag, `LIBRARIUM_CONFIG` env var, default fallback, `--help` |
| Tauri `LibrariumPaths` resolution | Correct paths per platform (unit test with mock dirs) |
| `DocumentParser` trait | `MarkdownParser` implements all methods correctly |

### Gate 2 — Frontend Vitest unit tests

```bash
cd frontend && npm test           # Vitest unit tests
cd frontend && npm run test:watch # watch mode during development
```

Every new or modified Vue component, Pinia store, and composable introduced by
this spec requires Vitest unit tests covering:

- Component renders without errors given valid props
- Store actions produce correct state transitions
- Error states render the correct UI (loading screen, error screen, timeout screen)
- Tauri-specific code paths are guarded behind a capability check so tests pass
  in a non-Tauri browser environment

Tauri API calls (`window.__TAURI__`) must be mockable in Vitest. Add a
`src/utils/tauri.ts` capability wrapper:

```typescript
// src/utils/tauri.ts
export const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI__' in window;

export const openDirectoryDialog = async (): Promise<string | null> => {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  return await open({ directory: true }) as string | null;
};
```

This wrapper is importable in tests and mockable via `vi.mock('./utils/tauri')`.

### Gate 3 — Playwright end-to-end tests

```bash
cd frontend && npm run test:e2e              # full suite
cd frontend && npm run test:e2e -- --headed  # visible browser for debugging
```

Playwright tests run against a real `librarium-server` instance started in a
temporary directory. Every user-visible flow introduced or modified by this spec
requires E2E coverage.

**Required E2E tests per phase:**

| Phase | Flow | Test |
|---|---|---|
| 0 | Server starts with `--config` flag | Health endpoint returns 200 |
| 0 | Server starts with `LIBRARIUM_CONFIG` env var | Health endpoint returns 200 |
| 0 | Missing config file | Server exits with non-zero code and clear error message |
| 1 | Loading screen | Visible during server startup, disappears on ready |
| 1 | WebView navigation | App loads fully after health poll succeeds |
| 1 | Server startup error | Error screen shown with log path |
| 1 | Port conflict | Error dialog shown, app does not hang |
| 2 | Vault registration with native dialog | Vault appears in vault list after selection |
| 2 | Desktop notification | Notification fires on reindex complete event |
| 3 | Full suite on WebKitGTK 2.36 | All existing tests pass on Ubuntu 22.04 target |
| 5 | `MarkdownParser` renders existing vaults | No regressions in rendered output |

**Playwright project configuration** must include a `webkit` project for the
WebKitGTK 2.36 compatibility gate (Phase 3). Add to `playwright.config.ts`:

```typescript
projects: [
  { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  { name: 'firefox',  use: { ...devices['Desktop Firefox'] } },
  { name: 'webkit',   use: { ...devices['Desktop Safari'] } },
],
```

The `webkit` project approximates WebKitGTK behavior well enough for logic
testing. Full WebKitGTK 2.36 verification still requires the Ubuntu 22.04
Docker environment described in Phase 3.

### CI enforcement

All three gates must be enforced in CI on every pull request and merge to main.
The CI pipeline should run them in this order, failing fast:

```
1. cargo fmt --check && cargo clippy    (lint gate)
2. cargo test                           (Rust unit + integration)
3. npm test                             (Vitest)
4. npm run test:e2e                     (Playwright — chromium + firefox + webkit)
```

The Playwright E2E job requires a running `librarium-server` instance. The CI
step should start the server in the background against a temp config, wait for
`/api/health` to return 200, then run the suite.

---

## Platform Data Directories

The Tauri desktop app uses platform-standard directories via Tauri's path API.
The standalone server binary uses paths relative to `--config` file location
or explicit values in `config.toml`.

### Linux (XDG)

| Purpose | Tauri API | Path |
|---|---|---|
| Config file | `app_config_dir()` | `~/.config/librarium/` |
| Database, plugins, logs | `app_data_dir()` | `~/.local/share/librarium/` |
| Cache | `app_cache_dir()` | `~/.cache/librarium/` |
| Default vault location | user-chosen on first run | `~/Documents/Librarium/` |

### macOS

| Purpose | Path |
|---|---|
| Config, data, logs | `~/Library/Application Support/librarium/` |
| Cache | `~/Library/Caches/librarium/` |
| Default vault location | `~/Documents/Librarium/` |

### Windows

| Purpose | Path |
|---|---|
| Config, data, logs | `%APPDATA%\librarium\` |
| Cache | `%LOCALAPPDATA%\librarium\` |
| Default vault location | `%USERPROFILE%\Documents\Librarium\` |

### Vault directory convention

Vaults are user content. They default to `~/Documents/Librarium/` (prompted on
first launch) rather than being buried in XDG data directories. The chosen path
is written to `config.toml` and never changed automatically. Individual vault
paths in the database are always absolute, so vaults anywhere on disk are
supported regardless of the default.

---

## Config Loading

### Entry points

```rust
impl AppConfig {
    /// Standalone server — explicit file path from --config flag.
    /// Returns error with file path context if file is missing or malformed.
    pub fn load_from_file(path: PathBuf) -> anyhow::Result<Self>;

    /// Tauri desktop app — resolves paths from platform directories.
    /// Reads config.toml if present; uses defaults otherwise.
    pub fn load_from_dirs(paths: &LibrariumPaths) -> anyhow::Result<Self>;

    /// Called by Tauri on first launch when no config.toml exists yet.
    /// Writes a default config.toml to paths.config_dir and returns it.
    pub fn write_default(paths: &LibrariumPaths) -> anyhow::Result<Self>;

    fn default_for_dirs(paths: &LibrariumPaths) -> Self;
}
```

`AppConfig::load()` (working-directory-relative) is removed entirely. Every
call site is explicit about the source.

### `LibrariumPaths` struct

```rust
pub struct LibrariumPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub default_vault_dir: PathBuf,
}
```

### Config resolution precedence

For standalone server mode, resolution order (highest first):

1. Values in the file at `--config` path (or `LIBRARIUM_CONFIG` env var path)
2. Hard-coded defaults in `AppConfig::default()`

Environment variable override of individual config keys
(`LIBRARIUM__SERVER__PORT` etc.) continues to work on top of file values.

---

## CLI Interface (`librarium-server` binary)

```
USAGE:
    librarium [OPTIONS]

OPTIONS:
    -c, --config <PATH>    Path to config.toml
                           [env: LIBRARIUM_CONFIG]
                           [default: ./config.toml]
    -h, --help             Print help
    -V, --version          Print version
```

Precedence: `--config` flag > `LIBRARIUM_CONFIG` env var > `./config.toml` default.

Implemented via `clap`:

```rust
#[derive(Parser)]
#[command(name = "librarium", about = "Librarium knowledge server")]
struct Args {
    #[arg(short, long, default_value = "./config.toml", env = "LIBRARIUM_CONFIG")]
    config: PathBuf,
}
```

---

## Tauri Shell Design (`librarium-tauri`)

### Responsibilities

The Tauri shell does exactly four things:

1. Resolves platform data directories and builds `LibrariumPaths`
2. Handles first-launch initialization (directory creation, default config, vault dir prompt)
3. Spawns `librarium-server::run()` on a background thread
4. Polls `GET /api/health` then navigates the WebView to `http://localhost:{port}`

Everything else — all app logic, all UI — lives in the Vue frontend and Actix backend.

### OS integration surface

These are the only Tauri-specific features beyond the shell itself:

| Feature | Tauri plugin | Notes |
|---|---|---|
| Native file dialogs | `tauri-plugin-dialog` | Open/save vault dirs, import files |
| System tray | `tauri-plugin-tray` | Open, quit, server status indicator |
| Desktop notifications | `tauri-plugin-notification` | File sync events, background errors |
| Deep links / file assoc. | `tauri-plugin-deep-link` | `librarium://` URLs, `.md` file open |
| Auto-update | `tauri-plugin-updater` | Optional; wire up later |

### Startup sequence

```
main()
  │
  ├─ resolve LibrariumPaths from Tauri path API
  ├─ create dirs if absent
  ├─ if no config.toml → show vault dir prompt → AppConfig::write_default()
  ├─ else → AppConfig::load_from_dirs()
  │
  ├─ thread::spawn ──► actix_web::rt::System::new().block_on(
  │                        librarium_server::run(config)
  │                    )
  │
  └─ tauri setup hook
       ├─ show loading screen in WebView
       └─ async task: poll GET /api/health every 100ms (timeout 10s)
              on success → window.location.href = 'http://localhost:{port}'
              on timeout → show error screen with log path
```

### Port conflict handling

If the configured port is already bound, the server should attempt to bind
an ephemeral port and communicate the chosen port back to the Tauri shell
before the WebView navigates. Implementation: server writes its bound port
to a well-known file in `app_cache_dir()` after successful bind; Tauri reads
it before constructing the navigation URL.

Alternative for initial implementation: fail fast with a clear error dialog
("Librarium is already running — only one instance is supported") since multi-instance
use is not a current requirement.

### WebKitGTK compatibility targets

Primary desktop targets and their WebKitGTK versions:

| Distro | WebKitGTK | Status |
|---|---|---|
| Fedora 39/40 | 2.44+ | Full support |
| Ubuntu 22.04 LTS | 2.36 | Minimum target |
| Ubuntu 24.04 LTS | 2.44+ | Full support |

Vue frontend constraints for WebKitGTK 2.36 compatibility:

- No CSS container queries — use media queries or Vuetify breakpoints instead
- No `<dialog>` HTML element — use Vuetify modal components (already the case)
- `ResizeObserver` and `IntersectionObserver` supported — D3 graph sizing is fine
- No WebGL — D3 graph renderer must use SVG, not canvas (already the plan)
- ES2020 target in `vite.config.ts` — verify Vite build target is not newer

Ubuntu 22.04 on AWS Workspaces is a known target. Browser fallback (connecting
to a running Librarium server via Chrome/Firefox) fully covers this case without
requiring Tauri installation.

---

## Document Format Strategy

### Decision: keep `.md` as storage format

Markdown remains the canonical on-disk format. The criticisms of markdown as a
standard are real but largely apply to markdown as a *publication format* passed
between tools. Librarium owns its entire stack and is not subject to cross-tool
fragmentation.

Structured data is already moving out of prose and into frontmatter YAML + SQLite
(see worldbuilding spec). The markdown body handles narrative prose, which is
where markdown is strongest.

### MDX: deferred, architecture prepared

MDX (Markdown + JSX) is the strongest candidate for a future format upgrade.
It is a strict superset of markdown — all existing `.md` files are valid MDX —
and enables embedding live component calls in prose:

```mdx
<RelatedEntities type="event" relation="caused_by" />
<Timeline from="2157" to="2203" filter="faction:remnants" />
```

This value only materializes once the entity/component system from the worldbuilding
spec exists. MDX without components is markdown with noise.

### Parser abstraction

`MarkdownService` is refactored behind a trait so a future `MdxParser` can
slot in without touching call sites:

```rust
pub trait DocumentParser: Send + Sync {
    fn render(&self, source: &str) -> RenderedDocument;
    fn extract_frontmatter(&self, source: &str) -> Frontmatter;
    fn extract_prose(&self, source: &str) -> String;
}

pub struct MarkdownParser;   // current implementation
// pub struct MdxParser;     // future
```

The active parser is selected by vault-level config:

```toml
[vault]
document_format = "markdown"   # "markdown" | "mdx" (future)
```

### Prose sentinel format-agnosticism

The `<!-- librarium:prose:begin -->` / `<!-- librarium:prose:end -->` sentinels from the
worldbuilding spec are HTML comments, valid in both markdown and MDX. The structural
editor must not assume the content between them is *only* markdown — when MDX is
active, component syntax is valid prose content.

### Import / export

- **Import from Obsidian**: `.md` files are ingested as-is. Obsidian-specific
  syntax (`[[links]]`, `![[embeds]]`, frontmatter tags) is already handled by
  `WikiLinkService` and `FrontmatterService`. No format conversion needed.
- **Export to Obsidian-compatible markdown**: strip `librarium_type`, `librarium_labels`,
  `librarium_plugin` frontmatter keys; strip prose sentinels; render any MDX component
  calls to their markdown equivalents (e.g. a `<Timeline>` becomes a plain
  markdown list of events). `MarkdownService` is the export pipeline.
- **Export to PDF / LaTeX**: out of scope for now. LaTeX is a typesetting target,
  not a storage format. Add as a future export option via `pandoc` or similar.

---

## `librarium-server` Library Extraction

The server's `main.rs` currently contains both the Tokio runtime entry point and
all startup logic. These must be separated so `librarium-tauri` can call the startup
logic on a background thread with its own runtime.

### Target structure

```rust
// crates/librarium-server/src/lib.rs

pub async fn run(config: AppConfig) -> anyhow::Result<()> {
    setup_logging(&config)?;
    let db = Database::new(&config).await?;
    let search_index = SearchIndex::new();
    let storage = build_storage_backend(&config)?;
    let watcher = FileWatcher::new()?;
    let (event_tx, _) = tokio::broadcast::channel(256);
    let shutdown_tx = tokio::broadcast::channel(1).0;

    let state = web::Data::new(AppState {
        db, search_index, storage, watcher,
        event_broadcaster: event_tx.clone(),
        change_log_retention_days: config.sync.change_log_retention_days,
        ml_undo_store: Default::default(),
        shutdown_tx: shutdown_tx.clone(),
    });

    bootstrap_admin_if_empty(&state).await?;
    load_and_watch_vaults(&state).await?;
    start_event_loop(state.clone(), event_tx).await;

    HttpServer::new(move || build_app(state.clone()))
        .bind((config.server.host.as_str(), config.server.port))?
        .run()
        .await?;

    Ok(())
}

// crates/librarium-server/src/main.rs

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = AppConfig::load_from_file(args.config)?;
    librarium_server::run(config).await
}
```

### Actix runtime on background thread

Tauri must own the main thread on Linux/macOS. Actix is spawned via its own
system on a background thread:

```rust
// crates/librarium-tauri/src/main.rs

std::thread::spawn(move || {
    actix_web::rt::System::new()
        .block_on(async {
            librarium_server::run(config).await
                .expect("server failed");
        });
});
```

`actix_web::rt::System::new()` initializes the Actix actor system correctly,
avoiding the runtime conflict that would occur with a raw `tokio::runtime::Runtime`.

---

## Docker / Deployment Updates

### `docker-compose.yml`

Config path is now explicit. Remove any working-directory assumptions:

```yaml
services:
  librarium:
    image: librarium:latest
    command: ["--config", "/data/config.toml"]
    environment:
      - LIBRARIUM_CONFIG=/data/config.toml   # alternative to command flag
    ports:
      - "8080:8080"
    volumes:
      - ./config.toml:/data/config.toml
      - ./vaults:/data/vaults
      - librarium-db:/data/db
volumes:
  librarium-db:
```

### Systemd service example

```ini
[Unit]
Description=Librarium knowledge server
After=network.target

[Service]
ExecStart=/usr/local/bin/librarium --config /etc/librarium/config.toml
Environment=RUST_LOG=info
Restart=on-failure
User=librarium

[Install]
WantedBy=multi-user.target
```

---

## Workspace Structure (final)

```
librarium/
├── Cargo.toml               workspace root
├── crates/
│   ├── librarium-server/        Actix backend + embedded Vue SPA
│   │   ├── src/
│   │   │   ├── lib.rs       run(config) — callable by Tauri and tests
│   │   │   ├── main.rs      clap CLI entry point
│   │   │   └── ...
│   ├── librarium-tauri/         Tauri desktop shell (new)
│   │   ├── src/
│   │   │   └── main.rs      platform init, server spawn, WebView nav
│   │   └── tauri.conf.json
│   ├── librarium-types/         shared types (unchanged)
│   ├── librarium-client/        HTTP/WS client library (kept for tests + CLI)
│   └── librarium-desktop/       Iced client (frozen — retire after Tauri parity)
├── frontend/                Vue 3 SPA (unchanged — works in both modes)
├── docs/
│   └── deployment.md        updated with --config flag, env var, Docker, systemd
└── ...
```

---

## Phases and Tasks

---

### Phase 0 — Config and Library Extraction

> Prerequisite for everything. No user-visible changes. Improves testability
> immediately as a side effect.

#### 0.1 Remove `AppConfig::load()`

- [ ] Audit all call sites of `AppConfig::load()` in `librarium-server`
- [ ] Add `LibrariumPaths` struct to `librarium-types`
- [ ] Implement `AppConfig::load_from_file(path: PathBuf)` with context error
      on missing file
- [ ] Implement `AppConfig::load_from_dirs(paths: &LibrariumPaths)`
- [ ] Implement `AppConfig::write_default(paths: &LibrariumPaths)`
- [ ] Implement `AppConfig::default_for_dirs(paths: &LibrariumPaths)`
- [ ] Remove `AppConfig::load()` — fix all compile errors
- [ ] Update integration tests to use `load_from_file` with a temp dir fixture
- [ ] **Unit tests**: `load_from_file` — missing file returns descriptive error;
      malformed TOML returns parse error with file path context; valid file
      returns correct struct; omitted fields use correct defaults
- [ ] **Unit tests**: `load_from_dirs` — no config file returns default struct;
      existing config file is read and merged correctly
- [ ] **Unit tests**: `write_default` — creates file at correct path; written
      TOML round-trips back to equivalent struct; calling twice does not error
- [ ] **Unit tests**: `default_for_dirs` — all path fields resolve relative to
      provided `LibrariumPaths`; no field references working directory

#### 0.2 Add `clap` CLI to `librarium-server`

- [ ] Add `clap` dependency to `librarium-server/Cargo.toml` (features: `derive`)
- [ ] Define `Args` struct with `--config` / `LIBRARIUM_CONFIG` / `./config.toml` default
- [ ] Wire `Args::parse()` in `main.rs`
- [ ] Verify `--help` output is correct
- [ ] Verify `LIBRARIUM_CONFIG=/tmp/test.toml cargo run` resolves correctly
- [ ] **Unit tests**: `--config` flag sets path correctly; `LIBRARIUM_CONFIG` env
      var is used when flag is absent; default `./config.toml` is used when
      neither is set; `--config` takes precedence over `LIBRARIUM_CONFIG`
- [ ] **E2E test**: server starts successfully when `--config` points to a valid
      temp config file and `GET /api/health` returns 200
- [ ] **E2E test**: server exits with non-zero code and prints a clear error when
      `--config` points to a missing file

#### 0.3 Extract `run()` into `lib.rs`

- [ ] Move all startup logic from `main()` into `pub async fn run(config: AppConfig)`
      in `lib.rs`
- [ ] `main.rs` becomes: parse args → load config → call `librarium_server::run(config)`
- [ ] Verify standalone server still starts and passes all existing integration tests
- [ ] Verify `cargo build --release` produces a working binary
- [ ] **Integration test**: call `librarium_server::run(config)` directly in a test
      process, verify `/api/health` returns 200, send shutdown signal, verify
      clean exit — this test is the foundation for all future integration tests
- [ ] **Integration test**: all existing `tests/` integration tests pass without
      modification after the extraction

#### 0.4 Update Docker and deployment files

- [ ] Update `docker-compose.yml` to pass `--config /data/config.toml`
- [ ] Update `Dockerfile` CMD or ENTRYPOINT accordingly
- [ ] Add systemd unit file example to `docs/deployment.md`
- [ ] Document `LIBRARIUM_CONFIG` env var and `--config` flag precedence in docs
- [ ] **E2E test**: `docker compose up` starts successfully; `GET /api/health`
      returns 200 from the host; `docker compose down` exits cleanly

---

### Phase 1 — Tauri Shell

> Creates the `librarium-tauri` crate. At the end of this phase, the desktop app
> is a Tauri window running the full Vue frontend with the embedded server.

#### 1.1 Scaffold `librarium-tauri` crate

- [ ] Add `librarium-tauri` to workspace `Cargo.toml`
- [ ] Add Tauri dependencies: `tauri`, `tauri-build`
- [ ] Create `tauri.conf.json`:
  - `productName: "Librarium"`
  - `identifier: "com.librarium.app"`
  - `windows[0].url: "http://localhost:8080"`
  - `windows[0].title: "Librarium"`
  - `bundle.targets: ["appimage", "deb"]` (Linux)
- [ ] Create `build.rs` with `tauri_build::build()`
- [ ] Verify `cargo build -p librarium-tauri` compiles
- [ ] **Unit test**: `tauri.conf.json` contains correct `productName`,
      `identifier`, and window URL — parse and assert in a build-time test

#### 1.2 Platform path resolution

- [ ] Add `tauri-plugin-path` or use built-in `app.path()` API
- [ ] Implement `resolve_platform_paths(app: &AppHandle) -> LibrariumPaths`
- [ ] Create all required directories on startup (`config_dir`, `data_dir`,
      `data_dir/plugins`, `cache_dir`)
- [ ] Verify correct paths on Fedora and Ubuntu 22.04
- [ ] **Unit tests**: `resolve_platform_paths` with mock `AppHandle` returns
      correct XDG paths on Linux; all expected subdirectories are created;
      calling twice when dirs exist does not error

#### 1.3 First-launch flow

- [ ] On startup, check if `config_dir/config.toml` exists
- [ ] If absent: show native dialog prompting for vault directory
      (pre-filled with `~/Documents/Librarium`); call `AppConfig::write_default()`
- [ ] If present: call `AppConfig::load_from_dirs()`
- [ ] Create `default_vault_dir` if it does not exist
- [ ] **Unit tests**: first-launch branch calls `write_default` and creates
      vault dir; returning-user branch calls `load_from_dirs`; vault dir creation
      is idempotent
- [ ] **E2E test**: launching with no existing config shows the vault directory
      prompt and creates `config.toml` after confirmation

#### 1.4 Embedded server startup

- [ ] Spawn Actix server on background thread via
      `actix_web::rt::System::new().block_on(librarium_server::run(config))`
- [ ] Propagate server startup errors to main thread via `mpsc` channel;
      show error dialog if server fails to start
- [ ] **Integration test**: server thread starts, `/api/health` returns 200
      within 2s; error in `run()` is received on the `mpsc` channel within 1s

#### 1.5 Health polling and WebView navigation

- [ ] Show loading screen HTML in WebView immediately (plain centered text:
      "Starting Librarium…")
- [ ] Spawn async task polling `GET http://localhost:{port}/api/health`
      every 100ms
- [ ] On healthy response: navigate WebView to `http://localhost:{port}`
- [ ] On timeout (10s): show error screen with log file path and quit button
- [ ] Verify on Fedora — expected startup time under 1s on modern hardware
- [ ] Verify on Ubuntu 22.04 — expected startup time under 2s
- [ ] **Unit tests**: health poll resolves immediately when server is ready;
      health poll times out and returns error after 10s when server never starts
- [ ] **Vitest tests**: loading screen component renders correctly; error screen
      renders log file path and quit button; neither screen renders when app
      is already loaded
- [ ] **E2E test**: loading screen is visible on launch and replaced by the
      full app after server becomes healthy
- [ ] **E2E test**: error screen is shown with log path when server fails to
      start within the timeout window

#### 1.6 Port conflict handling (initial)

- [ ] If server fails to bind configured port, surface a clear error dialog:
      "Librarium is already running on port {port}. Only one instance is supported."
- [ ] Log full bind error to log file before showing dialog
- [ ] **Integration test**: attempting to start a second server on the same port
      returns a bind error; error message contains the port number
- [ ] **E2E test**: launching a second Tauri instance while one is running shows
      the port conflict error dialog and does not hang

---

### Phase 2 — OS Integration

> Adds native desktop features to the Tauri shell. Can be done incrementally
> in any order after Phase 1.

#### 2.1 System tray

- [ ] Add `tauri-plugin-tray` dependency
- [ ] Tray icon with menu: Open Librarium, separator, Quit
- [ ] Status indicator: green (server healthy), yellow (starting), red (error)
- [ ] Tray icon shows on startup; clicking "Open Librarium" focuses or creates window
- [ ] **Unit tests**: tray menu items are constructed with correct labels and
      actions; status transitions from yellow → green on health; red on error
- [ ] **E2E test**: tray icon is present after launch; "Open Librarium" focuses
      the window; "Quit" exits the process cleanly

#### 2.2 Native file dialogs

- [ ] Add `tauri-plugin-dialog` dependency
- [ ] Expose Tauri command `open_directory_dialog() -> Option<String>`
- [ ] Call from Vue frontend when user registers a new vault
      (replaces current browser-based path text input)
- [ ] Expose `save_file_dialog(default_name, extensions) -> Option<String>`
      for export flows
- [ ] **Vitest tests**: vault registration component calls `openDirectoryDialog`
      when running in Tauri; falls back to text input when not in Tauri;
      `isTauri()` mock controls which branch is exercised
- [ ] **E2E test**: clicking "Add Vault" in the Tauri app opens a native
      directory picker; selected path is populated in the vault registration form

#### 2.3 Desktop notifications

- [ ] Add `tauri-plugin-notification` dependency
- [ ] Expose Tauri command `notify(title, body)`
- [ ] Call from Vue on relevant WebSocket events: reindex complete, sync conflict,
      background error
- [ ] Respect system notification permissions (Tauri handles this automatically)
- [ ] **Vitest tests**: notification composable calls `notify` in Tauri context;
      falls back to in-app toast in browser context; each triggering WebSocket
      event fires the correct title and body
- [ ] **E2E test**: triggering a reindex complete event results in a notification
      being requested (verify via Tauri event log, not system UI)

#### 2.4 Deep links and file associations

- [ ] Add `tauri-plugin-deep-link` dependency
- [ ] Register `librarium://` URL scheme
- [ ] Register `.md` file association (opens file in Librarium)
- [ ] Handle deep link in main window: navigate to the linked entity or file
- [ ] **Unit tests**: deep link URL parser extracts vault id and file path
      correctly from `librarium://vault/{id}/file/{path}` format; malformed URLs
      are handled gracefully without panic
- [ ] **E2E test**: opening a `librarium://` URL while the app is running navigates
      to the correct file; opening one while the app is closed launches the app
      and then navigates

---

### Phase 3 — WebKitGTK Compatibility Verification

> Explicit QA phase for Ubuntu 22.04 / WebKitGTK 2.36.

- [ ] Audit `vite.config.ts` build target — set to `es2020` maximum
- [ ] Audit Vue components for CSS container query usage — replace with
      Vuetify breakpoints
- [ ] Verify D3 graph renderer uses SVG exclusively (no canvas, no WebGL)
- [ ] Run full Playwright E2E suite inside WebKitGTK 2.36 environment
      (Docker image: `ubuntu:22.04` with WebKitGTK installed)
- [ ] Verify file tree, editor, search, and plugin manager on Ubuntu 22.04
- [ ] Verify WebSocket connection and real-time sync on Ubuntu 22.04
- [ ] Document any remaining known issues with WebKitGTK 2.36
- [ ] **CI gate**: add a `webkit-compat` CI job that runs the Playwright webkit
      project inside an `ubuntu:22.04` Docker container; this job must pass
      before Phase 3 is marked complete and before Phase 4 (retirement) begins
- [ ] **Regression baseline**: record the full Playwright suite pass/fail result
      on WebKitGTK 2.36 before making any changes; any test that was passing
      must continue to pass after this phase

---

### Phase 4 — Desktop Client Retirement

> Retire `librarium-desktop` (Iced) once Tauri reaches feature parity.

#### 4.1 Parity checklist

Compare `librarium-desktop` feature list against Tauri implementation:

- [ ] Vault browser — handled by Vue frontend
- [ ] Multi-tab markdown editor — handled by Vue frontend
- [ ] Frontmatter editor — handled by structural editor (worldbuilding spec)
- [ ] Full-text search — handled by Vue frontend
- [ ] Real-time sync — handled by Vue frontend via WebSocket
- [ ] ML outline panel — handled by Vue frontend
- [ ] Plugin manager — handled by Vue frontend
- [ ] Preferences sync — handled by Vue frontend
- [ ] Local session persistence — handled by Vue auth store + Tauri window state
- [ ] Auto-login — handled by Vue auth store (refresh token in localStorage)
- [ ] Conflict resolution UI — handled by Vue frontend
- [ ] Native file dialogs — Phase 2.2 above
- [ ] System tray — Phase 2.1 above
- [ ] Desktop notifications — Phase 2.3 above

#### 4.2 Retirement

- [ ] Confirm all parity checklist items complete
- [ ] Remove `librarium-desktop` from workspace `Cargo.toml`
- [ ] Delete `crates/librarium-desktop/` directory
- [ ] Remove `librarium-desktop` references from `README`, `docs/`, CI config
- [ ] `librarium-client` is kept — still used by integration tests and any CLI tools
- [ ] **Regression gate**: full `cargo test`, `npm test`, and `npm run test:e2e`
      must pass with zero failures after deletion; no test may reference
      `librarium-desktop` or `librarium-client` in a way that breaks after desktop removal

---

### Phase 5 — Document Format Abstraction

> Low priority. Prepares the parser layer for a future MDX upgrade without
> committing to one.

- [ ] Define `DocumentParser` trait in `librarium-types`:
  ```rust
  pub trait DocumentParser: Send + Sync {
      fn render(&self, source: &str) -> RenderedDocument;
      fn extract_frontmatter(&self, source: &str) -> Frontmatter;
      fn extract_prose(&self, source: &str) -> String;
  }
  ```
- [ ] Refactor `MarkdownService` to implement `DocumentParser`
- [ ] Replace direct `MarkdownService` usage in route handlers with
      `Arc<dyn DocumentParser>` in `AppState`
- [ ] Add `document_format` field to vault config (`"markdown"` only for now)
- [ ] Add `document_format` to vault record in database and API response
- [ ] Verify prose sentinel handling is format-agnostic
      (sentinels are HTML comments, valid in markdown and MDX)
- [ ] Update `AppConfig` defaults to set `document_format = "markdown"`
- [ ] **Unit tests**: `MarkdownParser` implements all three `DocumentParser`
      methods correctly — `render` produces expected HTML, `extract_frontmatter`
      returns correct key/value pairs, `extract_prose` strips frontmatter and
      sentinels and returns only body content
- [ ] **Unit tests**: swapping `MarkdownParser` for a stub `DocumentParser`
      implementation in `AppState` compiles and all route handlers use the
      trait correctly — confirms the abstraction is complete
- [ ] **Integration tests**: all existing markdown rendering integration tests
      pass without modification after the refactor
- [ ] **E2E tests**: full Playwright suite passes with zero regressions after
      `MarkdownService` is replaced by the `DocumentParser` trait — rendered
      output in the Vue frontend is visually identical

---

## Suggested Build Order

```
Phase 0 (config + lib extraction)   ← do first; unblocks everything;
    │                                  improves tests immediately
    └── Phase 1 (Tauri shell)
            └── Phase 2 (OS integration)   ← in any order after Phase 1
            └── Phase 3 (WebKitGTK QA)    ← ideally before Phase 2 ships
            └── Phase 4 (retire desktop)  ← after Phase 2 + Phase 3 complete

Phase 5 (format abstraction)   ← independent; do any time before MDX needed
```

Phase 0 is worth doing immediately regardless of any other decisions in this
spec — it improves testability, removes ambiguous config loading behavior, and
costs nothing to roll back.

**Testing gates per phase transition:**

| Transition | Gate required before proceeding |
|---|---|
| Phase 0 → Phase 1 | All three CI gates green; no existing tests broken |
| Phase 1 → Phase 2 | Loading screen, health poll, and navigation E2E tests pass |
| Phase 2 → Phase 3 | All OS integration E2E tests pass on Fedora |
| Phase 3 → Phase 4 | Full Playwright suite passes on WebKitGTK 2.36 (`ubuntu:22.04`) |
| Phase 4 (retirement) | Zero test failures after `librarium-desktop` deletion |
| Any → Phase 5 | No gate dependency; run full suite after to confirm no regressions |

---

## Open Questions

1. **Auto-update**: `tauri-plugin-updater` is listed as optional. What update
   distribution channel is planned — GitHub releases, a self-hosted endpoint,
   or manual only? This affects how the Tauri bundle is signed and configured.

2. **Multi-window support**: the current design assumes one Tauri window. If a
   user wants to have two vaults open side-by-side in separate windows, does that
   require multiple windows in the same Tauri instance, or is opening a second
   browser tab the answer?

3. **AppImage vs. deb as primary Linux artifact**: AppImage is most portable;
   deb integrates better with system package managers on Debian/Ubuntu. Fedora
   users would prefer RPM. Does the CI build need to produce all three, or is
   AppImage sufficient for the current user base?

4. **Session persistence across restarts**: `librarium-desktop` had auto-login via
   a saved refresh token in `~/.config/librarium/session.json`. In the Tauri app,
   the Vue frontend stores tokens in `localStorage` which is scoped to the
   WebView origin (`localhost:{port}`). This should persist across restarts
   automatically — but needs verification, particularly if the port ever changes.

5. **Sync between two instances**: shelved for now. When this is revisited, the
   natural design is a vault-level sync using the existing file watcher +
   broadcast channel infrastructure, either via a shared network filesystem,
   rsync, or a purpose-built sync protocol. The offline edit queue concept from
   `librarium-desktop` is the right starting point for that design.

---

*Last updated: initial platform architecture session*