# AGENTS.md

This repository is a Rust workspace for a self-hosted Obsidian-compatible knowledge app with a Vue frontend and a Tauri desktop shell.

## Repository Map

- `crates/librarium-server`: main Actix Web backend, default workspace member
- `crates/librarium-core`: platform-independent core (`AppError`, `FileService`, frontmatter read/write, Markdown render + wiki-link resolution, Tantivy search behind a default-on `search` feature) shared with non-server consumers; no actix/sqlx/tokio by default
- `crates/librarium-types`: shared Rust DTOs and parser traits
- `crates/librarium-client`: HTTP and WebSocket client crate
- `crates/librarium-tauri`: desktop shell that embeds the frontend and server, **and** the Android app host (#61/#62). Has a `[lib]` (`librarium_tauri_lib`) alongside its `[[bin]]` for the Android/iOS native-library entry point; desktop-only setup (config/JWT/tray/deep-links/actix/health-poll, `sync_*` commands) vs. mobile setup (registers `librarium-mobile`'s commands, constructs its `SearchIndex`/`MobileDb`/`SyncHandle` state) is gated by `#[cfg(desktop)]`/`#[cfg(mobile)]` in `src/lib.rs`, with the underlying dependencies gated by *target* in `Cargo.toml` (not a Cargo feature — see Build And Test below for why). `gen/android` (from `cargo tauri android init`) is committed, not regenerated. Mobile-only `background_sync.rs` (#64) runs a periodic background reconcile via `tauri-plugin-background-service` (Android foreground service) + `tauri-plugin-device-info` (Wi-Fi/battery policy checks) — both target-cfg-gated the same way as the desktop-only deps, just inverted
- `crates/librarium-mobile`: Route C thin-client command layer (vault list/get from a local JSON registry, file/directory ops over `librarium-core::FileService`, Markdown render, wiki-link/backlinks/outgoing-links, tags, frontmatter read/write, on-device Tantivy search, local metadata store — preferences/recent/favorites/bookmarks/sync policy (#64) — in its own `mobile.db`, and a `librarium-sync` bridge — add/list/remove remotes, map/unmap vaults, start/stop/status, one-shot `reconcile_once` (#64, for the background service and manual "sync now"), plus single-remote `pairing_set`/`pairing_get`/`pairing_clear` — resolving local vault ids via the same JSON registry instead of the desktop's `librarium.db`, and storing API keys in platform secure storage via a `SecretStore` trait rather than any plaintext file); wired into `librarium-tauri`'s mobile entry point (#62), but Android Keystore registration still needs a one-time JNI bootstrap from the host Activity that hasn't been added yet (`crates/librarium-mobile/src/secrets.rs`) — pairing/sync will error on a real device until that lands
- `frontend`: Vue 3 + TypeScript + Vuetify SPA
- `plugins`: built-in plugin manifests and scripts
- `tests`: workspace-level Rust integration tests
- `docs/DESIGN.md`: the canonical, current design & architecture document
- `docs/archive`: historical design notes, feature plans, and superseded specs (background only)

## Working Style

- Prefer minimal, targeted changes that fit existing module boundaries.
- Treat `crates/librarium-server/src/services` as business logic, `routes` as thin transport adapters, and `models` / `librarium-types` as shared contracts.
- Keep frontend API types aligned with backend JSON shapes.
- Avoid large refactors unless the task explicitly calls for them.
- Do not edit generated build outputs in `dist/` unless the task is specifically about release artifacts.

## Build And Test

**This repo has no hosted CI.** `cargo xtask ci` is the verification gate —
run it before every push, and treat a red run the way you would a red build
badge.

```bash
cargo xtask ci            # fmt, clippy, cargo test --workspace, vitest, vue-tsc
cargo xtask ci --quick    # Rust only (skips the frontend gates)
cargo xtask ci --full     # adds Android cross-compile and the Playwright E2E suite
```

It runs every gate rather than stopping at the first failure, then prints one
summary. A gate whose tooling is missing reports **SKIPPED**, never PASSED —
a skipped check is an absence of evidence, so read the summary's skip count
before concluding the tree is clean.

The individual commands, if you want to run one directly:

- Rust workspace check: `cargo check --workspace`
- Backend tests: `cargo test -p librarium-server`
- **Offline account admin** (works with the server stopped — this is the
  password-recovery backstop, so it deliberately opens SQLite directly instead
  of calling the HTTP API):
  ```bash
  librarium admin set-password <username>   # prompts twice, no echo
  librarium admin create-user <username> [--admin]
  librarium admin list-users
  ```
  Resolves `config.toml` the same way the server does, so it targets the same
  database. Every password change revokes that user's sessions (but not their
  API keys — separate credentials, revoked in Settings → API Keys).
- Workspace tests: `cargo test --workspace`
- Frontend install: `npm --prefix frontend install`
- Frontend unit tests: `npm --prefix frontend test`
- Frontend build: `npm --prefix frontend run build`
- Frontend E2E: `npm --prefix frontend run test:e2e`
- Android cross-compile check (also run by `cargo xtask ci --full`; needs an
  installed Android NDK, e.g. via Android Studio's SDK Manager —
  `cargo-ndk` auto-detects it from `$ANDROID_HOME`/`$ANDROID_NDK_HOME`):
  ```bash
  rustup target add aarch64-linux-android x86_64-linux-android
  cargo install cargo-ndk --locked
  cargo ndk -t aarch64-linux-android -P 21 build -p librarium-core -p librarium-sync -p librarium-mobile
  cargo ndk -t x86_64-linux-android -P 21 build -p librarium-core -p librarium-sync -p librarium-mobile
  ```
  To confirm no C-toolchain dependency (`openssl-sys`, `onig`) has crept back
  into these three crates:
  ```bash
  cargo tree -p librarium-core -p librarium-sync -p librarium-mobile --target aarch64-linux-android -i openssl-sys
  cargo tree -p librarium-core -p librarium-sync -p librarium-mobile --target aarch64-linux-android -i onig
  ```
  Both should error with "did not match any packages" (i.e. not found).
- `librarium-tauri` mobile dependency-graph check (#61/#62): confirms the
  desktop-only setup (config loading, JWT persistence, the tray, deep links,
  the actix thread, health polling, `sync_*` commands) is fully excluded
  from an Android/iOS build. Gated by *target* (`[target.'cfg(not(any(
  target_os = "android", target_os = "ios")))'.dependencies]` in
  `crates/librarium-tauri/Cargo.toml`), not a Cargo feature — `cargo tauri
  android build` has no flag to disable default features, so exclusion has
  to be automatic for the real target rather than something a flag opts out
  of. Verify against the actual Android target (needs the Rust target
  installed via `rustup target add aarch64-linux-android`, but not the full
  NDK — `cargo tree`/`cargo check` don't link):
  ```bash
  cargo check -p librarium-tauri --target aarch64-linux-android
  cargo tree -p librarium-tauri --target aarch64-linux-android -e normal | grep -E "librarium-server|actix-web"
  ```
  The `grep` should find nothing. (`cargo check --target aarch64-linux-android`
  alone will still fail without NDK env vars set — `ring`'s build script
  needs a C compiler — that's expected; it's `cargo ndk`/`cargo tauri
  android` that set those up, see below.)
- **Android build (#62)** — prerequisites: JDK 17+ (`java -version`), the
  Android SDK (`$ANDROID_HOME`) with build-tools/platform-tools installed,
  NDK 27+ (via Android Studio's SDK Manager, or `sdkmanager
  "ndk;27.2.12479018"`), `cargo install cargo-ndk --locked` (this crate's
  Android cross-compile check above also needs it), `cargo install
  tauri-cli --locked` (or `cargo binstall tauri-cli`), and the Rust targets:
  ```bash
  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
  ```
  `ANDROID_NDK_HOME` (or `NDK_HOME`) must point at the versioned NDK dir
  (`$ANDROID_HOME/ndk/<version>`) — unlike `ANDROID_HOME`, this is rarely
  set by default installers. From `crates/librarium-tauri/`:
  ```bash
  cargo tauri android init   # generates gen/android — already committed, do not regenerate over local edits
  cargo tauri android build --apk --debug -t aarch64 x86_64
  ```
  Produces one universal debug APK (containing both ABIs' native libs) at
  `gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`
  — `arm64-v8a` for real devices, `x86_64` for emulators on Intel/AMD hosts
  (Apple Silicon Macs: `armv7`/`aarch64` targets in an ARM emulator image
  instead). Debug builds allow cleartext HTTP (`android:usesCleartextTraffic`
  manifest placeholder, `crates/librarium-tauri/gen/android/app/build.gradle.kts` —
  `cargo tauri android init`'s own default, not something this repo added)
  so syncing with a plain-`http://` LAN server works; release builds keep it
  blocked. From inside an Android **emulator**, the host machine's loopback
  is `10.0.2.2`, not `127.0.0.1` or `localhost` — needed when pointing a
  paired remote at a Librarium server running on the same dev machine.
  `cargo tauri android init`'s `beforeBuildCommand`/`beforeDevCommand`
  (`crates/librarium-tauri/tauri.android.conf.json`) run with a working
  directory one level up from `crates/librarium-tauri/` (empirically —
  `npm --prefix ../frontend`, not `../../frontend` as you'd expect from the
  config file's own location), unlike desktop's build, which has no
  `beforeBuildCommand` at all (desktop's real UI is served by the embedded
  server, not Tauri's static-asset pipeline — see `docs/DESIGN.md`).
  To build and install onto a connected device/emulator in one step
  (replacing whatever's currently installed), use
  `scripts/android-deploy.sh` instead of the raw `cargo tauri`/`adb`
  commands above — `bash scripts/android-deploy.sh --help` for options
  (target arch, `--release`, device serial when more than one is attached).
- **Android release signing (#67)** — `gen/android/app/build.gradle.kts`'s
  `signingConfigs["release"]` reads a gitignored `keystore.properties` at
  the Android project root (`gen/android/keystore.properties` — see
  `gen/android/.gitignore`); its absence is the fallback to AGP's default
  unsigned release build, so a release build works either way, signed or
  not. To sign one locally, generate a keystore once:
  ```bash
  keytool -genkeypair -v -keystore /path/to/librarium-release.jks \
    -alias librarium -keyalg RSA -keysize 2048 -validity 10000
  ```
  then create `crates/librarium-tauri/gen/android/keystore.properties`:
  ```properties
  storeFile=/path/to/librarium-release.jks
  storePassword=...
  keyAlias=librarium
  keyPassword=...
  ```
  **Never commit the `.jks`/`.keystore` file or `keystore.properties`** —
  both are gitignored; store the keystore and its passwords somewhere
  durable outside the repo (a password manager, not a chat log or a
  throwaway note) — losing it means every future release needs a new app
  identity, since Android refuses to install an update signed by a
  different key over an existing install. Then build a release (not debug)
  APK:
  ```bash
  cargo tauri android build --apk -t aarch64
  ```
  Output lands at
  `gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`.
  Verify it's actually signed (not just built) with
  `apksigner verify --print-certs <apk>` (part of the Android SDK
  build-tools) or `jarsigner -verify -verbose -certs <apk>`.
  Release APKs are built locally (this repo has no hosted CI — see
  "Verification" above); keep the `.jks` and `keystore.properties` outside
  the repo and point at them from `gen/android/keystore.properties`.
- Mobile contract test (#59): asserts every route in #56/#57's scope
  (vault/file/render/resolve-link/backlinks, search, tags, preferences,
  recent files, favorites, bookmarks, random/daily notes) produces
  structurally equivalent output from the real `librarium-server` HTTP routes
  and the `librarium-mobile` functions `localDispatcher.ts` calls into. Also
  covered by `cargo test --workspace`, but worth running alone when you are
  specifically chasing route drift:
  ```bash
  cargo test -p librarium-mobile --test contract_test
  ```
  See `crates/librarium-mobile/tests/contract_test.rs`'s module doc for the
  normalized-field list (fields stripped before comparison, with
  justification per field) and design notes.
- Frontend static coverage check for the same issue: asserts every #56/#57
  `apiXxx` call in `frontend/src/api/client.ts` still resolves to a route in
  `localDispatcher.ts`'s table (no live-response comparison — that's the Rust
  suite's job):
  ```bash
  npm --prefix frontend test -- localDispatcherCoverage
  ```

## Config And Runtime Notes

- The server reads `config.toml` by default, or `LIBRARIUM_CONFIG` / `--config`.
- Auth, JWT, LDAP, OIDC, CORS, vault paths, and TLS are configured in `crates/librarium-server/src/config/mod.rs`.
- The committed root `config.toml` is development-oriented, not a production baseline.
- File and vault operations must preserve path-safety checks in `FileService`.
- Search indexing and watcher behavior are tightly coupled to vault file mutations; changes here should be verified with integration tests.

## Code Areas To Inspect Carefully

- Auth and session behavior: `crates/librarium-server/src/routes/auth.rs`, `middleware/auth.rs`, `routes/totp.rs`
- Filesystem mutation paths: `crates/librarium-core/src/file_service.rs` (re-exported as `librarium::services::file_service`)
- Search index consistency: `crates/librarium-core/src/search_service.rs` (re-exported as `librarium::services::search_service`)
- Reindex and entity sync: `crates/librarium-server/src/services/reindex_service.rs`
- Frontend editor state and tab behavior: `frontend/src/stores`, `frontend/src/components/editor`, `frontend/src/components/tabs`
- Tauri command ACL: every `#[tauri::command]` reachable via `invoke()` (desktop's `crates/librarium-tauri/src/lib.rs` and `crates/librarium-mobile/src/commands.rs`) must be listed in `crates/librarium-tauri/build.rs`'s `COMMANDS` const *and* have its generated `allow-<kebab-case-name>` permission referenced in `crates/librarium-tauri/capabilities/default.json` (desktop) or `mobile.json` (mobile-only commands). Missing either step means Tauri silently denies every call to that command in release builds ("Command X not allowed by ACL") — this bit the whole `sync_*`/`pairing_*`/local-dispatcher command surface at once before it was caught. `cargo build -p librarium-tauri` fails loudly if a capability references a permission that doesn't exist, but adding a *new* command with no capability entry at all builds cleanly and fails only at runtime — there's no build-time check for that omission, so don't rely on the build succeeding as proof a new command is reachable.
  A second, independent gotcha in the same area: `default.json`'s permissions only apply to Tauri-"local" content by default. Desktop's window doesn't load via Tauri's own asset protocol — `lib.rs`'s `poll_until_healthy_then_navigate` does a runtime `window.eval("window.location.replace('http://127.0.0.1:{port}')")` to point the WebView at the embedded actix server instead — so every desktop `invoke()` call executes from a `http://127.0.0.1:<port>` origin, which Tauri's ACL treats as *remote*, not local. `default.json` has an explicit `"remote": {"urls": ["http://127.0.0.1:*"]}` entry to cover this (port is user-configurable via `config.toml`, hence the wildcard) — don't remove it, and don't assume adding a permission to `default.json`'s `permissions` list alone is sufficient on desktop.

## Documentation Guardrails

- `docs/DESIGN.md` and the root `README.md` are living documents. Keep them
  describing the system **as it is now**.
- When a change is breaking or otherwise alters architecture, data flow, public
  REST/WebSocket payloads, the frontend⇄backend contract, the persistence model,
  auth/authorization, config keys, or build/run commands, **update `docs/DESIGN.md`
  in the same change** — and `README.md` too if the overview or quick start is
  affected.
- Bump the project version across `crates/*/Cargo.toml`, `frontend/package.json`,
  and `crates/librarium-tauri/tauri.conf.json` together; the `/api/version`
  endpoint derives from `CARGO_PKG_VERSION`.
- When a design note in `docs/DESIGN.md` is fully superseded, move the long-form
  detail into `docs/archive/` and leave a short pointer behind. Do not resurrect
  archived docs as the source of truth.

## Change Guardrails

- Preserve backward compatibility for API payloads unless the task explicitly includes coordinated frontend and backend changes.
- Add or update tests when modifying auth, file mutation, reindexing, search, or editor state behavior.
- Prefer fixing root causes over patching symptoms, but avoid unrelated cleanup.
- Be careful with default credentials, secrets, and security-sensitive defaults in committed config files.
