//! Tauri app library — `main.rs` is a thin shim calling [`run`]. Split out so
//! Android/iOS (which load the app as a native library rather than executing
//! a binary) can link against it via `tauri::mobile_entry_point`; see the
//! `[lib]` section in Cargo.toml.
//!
//! Desktop embeds `librarium-server` and drives it exactly as before this
//! split — that logic (config loading, JWT-secret persistence, the tray,
//! deep links, the actix thread, health-poll-then-navigate, and the
//! `sync_bridge` commands) is gated behind `#[cfg(desktop)]`/`#[cfg(mobile)]`
//! (Tauri's own OS-based cfg flags — true `desktop` = not android/ios).
//! Cargo.toml gates the underlying dependencies (`librarium-server`,
//! `actix-web`, `reqwest`, `librarium-sync`, `dirs`) the same way, by
//! *target* rather than a Cargo feature: `cargo tauri android build` has no
//! flag to disable default features, so exclusion has to be automatic for
//! any android/ios build rather than something a flag opts out of (verify
//! with `cargo tree -p librarium-tauri --target aarch64-linux-android -e
//! normal`, which should show neither crate). `librarium-server`'s
//! actix/sqlx/ring dependency chain was never verified for Android, unlike
//! `librarium-core`/`librarium-sync`/`librarium-mobile` (#47/48) — excluding
//! it outright matches that established boundary rather than trying to
//! cross-compile it.
//!
//! Mobile registers `librarium-mobile`'s commands (`invoke_handler`) and
//! constructs the Tauri-managed state they need (`SearchIndex`, `MobileDb`,
//! `SyncHandle` — see that crate's own doc comments for why each is
//! stateful) in the `#[cfg(mobile)]` `run_setup` below — the first real app
//! host for that crate; only tests constructed this state before. The
//! frontend log is the one piece of desktop setup mobile also does, since
//! it has no server dependency and its commands are registered on every
//! platform.
//!
//! **Known gap:** `librarium_mobile::OsKeyringStore` (used here for
//! `SyncHandle`'s API-key storage) needs a default `keyring_core` store
//! registered once at startup on Android — `keyring` v4's own
//! auto-detection deliberately excludes it. I tried wiring
//! `android_native_keyring_store::Store::new()` +
//! `keyring_core::set_default_store(...)` into this `run_setup` during #63's
//! device bring-up; it compiled and linked fine, but crashed the app on
//! launch with `SIGABRT` inside `run()`'s FFI boundary (`stop_unwind`,
//! `tao::platform_impl::platform::ndk_glue::create`'s thread) before any
//! frontend/tracing output appeared — likely a JNI thread-attachment issue
//! (the crate's `Store::new()` assumes `ndk-context`'s global is both set
//! *and* the calling thread is JNI-attached; Tauri's mobile bootstrap may
//! only guarantee the former). Reverted rather than debug further under
//! time pressure — filed as its own follow-up with the full backtrace.
//! Without it, `pairing_set`/`sync_add_remote` fail cleanly (no crash) with
//! a "no default store" style error instead of persisting the key.

#[cfg(mobile)]
mod background_sync;
mod frontend_log;
#[cfg(target_os = "android")]
mod headless_bridge_stub;
#[cfg(desktop)]
mod paths;
#[cfg(desktop)]
mod secrets;
#[cfg(desktop)]
mod session_store;
#[cfg(desktop)]
mod sync_bridge;

use anyhow::Context;
use frontend_log::{FrontendLog, LogRecord};
use tauri::{AppHandle, Manager};
#[cfg(desktop)]
use {
    librarium::config::AppConfig,
    paths::{create_dirs, resolve_paths},
    secrets::OsKeyringStore,
    session_store::SessionStore,
    sync_bridge::{RemoteDto, SyncHandle},
    tauri::menu::{Menu, MenuItem, PredefinedMenuItem},
    tauri::tray::{TrayIconBuilder, TrayIconEvent},
    tracing::{error, info, warn},
};

// ── Tray icon pixel data ──────────────────────────────────────────────────────

#[cfg(desktop)]
const TRAY_ICON_YELLOW: &[u8] = include_bytes!("../icons/tray-yellow.png");
#[cfg(desktop)]
const TRAY_ICON_GREEN: &[u8] = include_bytes!("../icons/tray-green.png");
#[cfg(desktop)]
const TRAY_ICON_RED: &[u8] = include_bytes!("../icons/tray-red.png");

// ── Tauri commands (generic — no server dependency, registered on every platform) ──

/// Open a native directory picker dialog and return the selected path.
///
/// Desktop-only: `tauri-plugin-dialog`'s mobile implementation has no
/// `pick_folder` (only `pick_file`) — Android has no equivalent native
/// folder picker in the same sense, and mobile vault management doesn't go
/// through this anyway (`librarium-mobile`'s local JSON vault registry is
/// populated by the pairing flow, not a folder picker).
///
/// Called from the Vue frontend via `window.__TAURI__.core.invoke()`.
#[cfg(desktop)]
#[tauri::command]
async fn open_directory_dialog(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog()
        .file()
        .set_title("Select Vault Directory")
        .pick_folder(move |folder| {
            let path = folder.map(|f| f.to_string());
            let _ = tx.send(path);
        });
    rx.await.unwrap_or(None)
}

/// Send a native desktop notification.
///
/// Falls back silently when the platform does not support notifications or when
/// permission has not been granted — the caller should not treat this as fatal.
#[tauri::command]
async fn notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
        .map_err(|e| e.to_string())
}

/// Open `url` in the OS default browser / handler.
///
/// The frontend routes clicks on external links (`http`, `https`, `mailto`,
/// `tel`) in rendered markdown to this command so the WebView never navigates
/// out of the app shell. Rejects unknown or unsafe schemes (e.g. `file://`,
/// `javascript:`) so a malicious note can't ask the shell to open an
/// arbitrary local target.
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err(format!(
            "refusing to open URL with unsupported scheme: {url}"
        ));
    }
    open::that_detached(&url).map_err(|e| format!("open failed: {e}"))
}

fn is_allowed_external_url(url: &str) -> bool {
    ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

/// Write base64-encoded bytes to an absolute path.
///
/// Used by the frontend's feedback-bundle export: `path` always comes back
/// from the user's own native save-dialog selection (`saveFileDialog` in
/// `tauri.ts`), the same trust boundary as any other app's "Save As" — no
/// general-purpose filesystem plugin is needed just for this one write.
#[tauri::command]
fn write_binary_file(path: String, data_base64: String) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("invalid base64: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("write failed: {e}"))
}

// ── Frontend logging ─────────────────────────────────────────────────────────

/// Append a single record to the rotating frontend log at
/// `{data_dir}/logs/frontend.log`. Called from the frontend `logger.ts`
/// wrapper for every significant lifecycle event (login, refresh, logout,
/// WebSocket reconnect, router guard decision) so a post-mortem after an
/// unexpected drop to the login page has a durable record of what happened.
///
/// Silently returns the error string on IO failure — the frontend logs it to
/// the JS console; we don't want a broken filesystem to bring down real work.
#[tauri::command]
fn frontend_log(log: tauri::State<'_, FrontendLog>, record: LogRecord) -> Result<(), String> {
    log.append(&record).map_err(|e| e.to_string())
}

/// Return the resolved log file path so the frontend can surface it in a
/// "Copy log path" affordance (Settings → Diagnostics).
#[tauri::command]
fn frontend_log_path(log: tauri::State<'_, FrontendLog>) -> String {
    log.path_str()
}

// ── Durable refresh-token store (desktop-only: no server-auth flow exists on
// the mobile local transport at all — see router/index.ts's own comment on
// why it bypasses the whole token lifecycle) ─────────────────────────────────

/// Read the persisted refresh token, if any.
///
/// The frontend calls this on boot when WebView localStorage is empty; a
/// missing/empty file returns `None` so the caller falls through to /login.
#[cfg(desktop)]
#[tauri::command]
fn auth_token_get(store: tauri::State<'_, SessionStore>) -> Result<Option<String>, String> {
    store.get().map_err(|e| e.to_string())
}

/// Mirror the current refresh token to disk. Called after every successful
/// login/refresh so the disk copy stays in lockstep with WebView localStorage.
#[cfg(desktop)]
#[tauri::command]
fn auth_token_set(store: tauri::State<'_, SessionStore>, token: String) -> Result<(), String> {
    store.set(token).map_err(|e| e.to_string())
}

/// Wipe the disk copy on logout. Called from the auth store's `logout()`
/// alongside the localStorage clear so a subsequent boot cannot restore a
/// revoked token from the durable fallback.
#[cfg(desktop)]
#[tauri::command]
fn auth_token_clear(store: tauri::State<'_, SessionStore>) -> Result<(), String> {
    store.clear().map_err(|e| e.to_string())
}

/// The loaded desktop config, so commands can reach the database path and the
/// password policy without going through the HTTP server.
#[cfg(desktop)]
struct DesktopConfig {
    config: librarium::config::AppConfig,
    config_file: std::path::PathBuf,
}

/// Reset the local admin's password without knowing the old one.
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
    let cfg = config.config.clone();
    let url = if cfg.database.path.starts_with("sqlite:") {
        cfg.database.path.clone()
    } else {
        format!("sqlite:{}?mode=rwc", cfg.database.path)
    };
    let db = librarium::db::Database::new(&url)
        .await
        .map_err(|e| format!("Could not open the database: {e}"))?;
    librarium::services::CredentialService::new(&db, &cfg.auth)
        .set_password(&username, &new_password)
        .await
        .map_err(|e| e.to_string())
}

/// Turn password protection on or off for this desktop instance.
///
/// Writes `auth.enabled` to config.toml through the same `write_to_file` path
/// that already persists the JWT secret, and creates the account when enabling.
///
/// `AppConfig` is read once at startup, so this takes effect on the next
/// launch — the UI says so rather than implying an immediate change.
#[cfg(desktop)]
#[tauri::command]
async fn auth_set_local_enabled(
    config: tauri::State<'_, DesktopConfig>,
    enabled: bool,
    username: Option<String>,
    new_password: Option<String>,
) -> Result<(), String> {
    let mut cfg = config.config.clone();

    if enabled {
        let username = username.unwrap_or_else(|| "admin".to_string());
        let password = new_password
            .ok_or_else(|| "A password is required to enable password protection".to_string())?;

        let url = if cfg.database.path.starts_with("sqlite:") {
            cfg.database.path.clone()
        } else {
            format!("sqlite:{}?mode=rwc", cfg.database.path)
        };
        let db = librarium::db::Database::new(&url)
            .await
            .map_err(|e| format!("Could not open the database: {e}"))?;

        // Reuse the account if it already exists (auth was previously on);
        // otherwise create it. Either way CredentialService enforces policy.
        let svc = librarium::services::CredentialService::new(&db, &cfg.auth);
        let exists = db
            .get_user_auth_by_username(&username)
            .await
            .map_err(|e| e.to_string())?
            .is_some();
        if exists {
            svc.set_password(&username, &password)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            svc.create_user(&username, &password, true)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    cfg.auth.enabled = enabled;
    cfg.write_to_file(&config.config_file)
        .map_err(|e| format!("Could not save the configuration: {e}"))
}

// ── Sync commands (desktop-only: librarium.db-resolved vault paths) ─────────

/// Register a remote server to sync with. Returns the generated remote id.
#[cfg(desktop)]
#[tauri::command]
async fn sync_add_remote(
    handle: tauri::State<'_, SyncHandle>,
    base_url: String,
    api_key: String,
) -> Result<String, String> {
    handle
        .add_remote(base_url, api_key)
        .await
        .map_err(|e| e.to_string())
}

/// Map a local vault to a remote vault under a registered remote.
#[cfg(desktop)]
#[tauri::command]
async fn sync_map_vault(
    handle: tauri::State<'_, SyncHandle>,
    remote_id: String,
    local_vault_id: String,
    remote_vault_id: String,
) -> Result<(), String> {
    handle
        .map_vault(remote_id, local_vault_id, remote_vault_id)
        .await
        .map_err(|e| e.to_string())
}

/// List configured remotes (never exposes API keys).
#[cfg(desktop)]
#[tauri::command]
async fn sync_list_remotes(handle: tauri::State<'_, SyncHandle>) -> Result<Vec<RemoteDto>, String> {
    handle.list_remotes().await.map_err(|e| e.to_string())
}

/// List the vaults available on a registered remote.
#[cfg(desktop)]
#[tauri::command]
async fn sync_list_remote_vaults(
    handle: tauri::State<'_, SyncHandle>,
    remote_id: String,
) -> Result<Vec<librarium::models::Vault>, String> {
    handle
        .list_remote_vaults(remote_id)
        .await
        .map_err(|e| e.to_string())
}

/// Create a new vault on a registered remote.
#[cfg(desktop)]
#[tauri::command]
async fn sync_create_remote_vault(
    handle: tauri::State<'_, SyncHandle>,
    remote_id: String,
    name: String,
) -> Result<librarium::models::Vault, String> {
    handle
        .create_remote_vault(remote_id, name)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a registered remote and everything mapped to it.
#[cfg(desktop)]
#[tauri::command]
async fn sync_remove_remote(
    handle: tauri::State<'_, SyncHandle>,
    remote_id: String,
) -> Result<(), String> {
    handle
        .remove_remote(remote_id)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a single local-to-remote vault mapping.
#[cfg(desktop)]
#[tauri::command]
async fn sync_unmap_vault(
    handle: tauri::State<'_, SyncHandle>,
    remote_id: String,
    local_vault_id: String,
) -> Result<(), String> {
    handle
        .unmap_vault(remote_id, local_vault_id)
        .await
        .map_err(|e| e.to_string())
}

/// Per-vault sync status snapshots.
#[cfg(desktop)]
#[tauri::command]
async fn sync_status(
    handle: tauri::State<'_, SyncHandle>,
) -> Result<Vec<librarium_sync::VaultStatus>, String> {
    Ok(handle.status().await)
}

/// (Re)start the background sync tasks.
#[cfg(desktop)]
#[tauri::command]
async fn sync_start(handle: tauri::State<'_, SyncHandle>) -> Result<(), String> {
    handle.start().await.map_err(|e| e.to_string())
}

/// Stop the background sync tasks.
#[cfg(desktop)]
#[tauri::command]
async fn sync_stop(handle: tauri::State<'_, SyncHandle>) -> Result<(), String> {
    handle.stop().await;
    Ok(())
}

// ── invoke_handler ───────────────────────────────────────────────────────────
//
// Two cfg'd definitions of the same function (rather than cfg-gating items
// inside a single `generate_handler!` list, which the macro doesn't support).
// Concrete `Wry`, not generic over `R: tauri::Runtime` like
// `librarium-mobile::invoke_handler` — this crate's commands take the plain
// `AppHandle`/`AppHandle<Wry>` alias (unchanged from before this split) since
// `run()` below always builds a concrete `tauri::Builder<Wry>`, unlike
// `librarium-mobile`, which is written to be embeddable under any runtime.

#[cfg(desktop)]
fn invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool {
    tauri::generate_handler![
        open_directory_dialog,
        notify,
        open_external_url,
        write_binary_file,
        frontend_log,
        frontend_log_path,
        auth_token_get,
        auth_token_set,
        auth_token_clear,
        auth_reset_local_password,
        auth_set_local_enabled,
        sync_add_remote,
        sync_map_vault,
        sync_list_remotes,
        sync_list_remote_vaults,
        sync_create_remote_vault,
        sync_remove_remote,
        sync_unmap_vault,
        sync_status,
        sync_start,
        sync_stop
    ]
}

// `Invoke<R>` isn't `Clone`, so two separately built `Fn(Invoke<R>) -> bool`
// handlers (the generic commands' `generate_handler!` closure and
// `librarium_mobile::invoke_handler()`, the crate's only public entry point
// — its individual command fns are deliberately private, so they can't be
// merged into one `generate_handler!` list) can't just be tried in sequence
// against the same value. Peek at the command name first (a `&str` borrow,
// no move) to pick exactly one to hand `invoke` to.
#[cfg(mobile)]
const GENERIC_COMMAND_NAMES: &[&str] = &[
    "notify",
    "open_external_url",
    "write_binary_file",
    "frontend_log",
    "frontend_log_path",
];

// `generate_handler!`'s expansion is an untyped `move |invoke| {...}`
// closure that relies entirely on external context to pin its parameter's
// runtime generic — that only works when the macro invocation is the
// direct return expression of a function with an explicit `impl
// Fn(Invoke<Wry>) -> bool` return type (as `generic_handler` is here and
// desktop's `invoke_handler` already was); storing it in a `let` first, or
// calling it inline before it has a concrete type, leaves the type
// unresolved.
#[cfg(mobile)]
fn generic_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool {
    tauri::generate_handler![
        notify,
        open_external_url,
        write_binary_file,
        frontend_log,
        frontend_log_path
    ]
}

#[cfg(mobile)]
fn invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool {
    let generic = generic_handler();
    let mobile = librarium_mobile::invoke_handler();

    move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
        if GENERIC_COMMAND_NAMES.contains(&invoke.message.command()) {
            generic(invoke)
        } else {
            mobile(invoke)
        }
    }
}

// ── run ───────────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_deep_link::init());

    // Android background reconcile service (#64) — mobile-only; desktop
    // keeps its current always-on sync_bridge.rs model. Registered here
    // rather than in run_setup because it's a `.plugin()`, not managed
    // state. `tauri-plugin-background-service`'s own docs require
    // `tauri-plugin-notification` to already be registered (it is, above).
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_device_info::init()).plugin(
        tauri_plugin_background_service::init_with_service(|| {
            background_sync::MobileSyncService::new()
        }),
    );

    builder
        .invoke_handler(invoke_handler())
        .setup(|app| run_setup(app).map_err(|e| e.into()))
        .run(tauri::generate_context!())
        .expect("error while running Librarium");
}

/// Main setup logic extracted from the Tauri setup hook.
///
/// Returning `anyhow::Result` makes it easy to use `?` throughout; the
/// closure converts the error to `Box<dyn std::error::Error>` at the boundary.
#[cfg(desktop)]
fn run_setup(app: &mut tauri::App) -> anyhow::Result<()> {
    let handle = app.handle().clone();

    // 1. Resolve directories (portable: exe-relative; installed: platform dirs).
    let paths = resolve_paths(&handle)?;
    create_dirs(&paths)?;
    info!(
        "App directories: config={:?} data={:?}",
        paths.config_dir, paths.data_dir
    );

    // Initialise the rotating frontend log as early as possible so any
    // subsequent boot-time diagnostic message the frontend emits ends up on
    // disk rather than lost in a WebView console. Rotation happens here
    // (previous run's file becomes `frontend.log.1`).
    match FrontendLog::init(paths.data_dir.join("logs")) {
        Ok(fl) => {
            info!("Frontend log initialised at {}", fl.path_str());
            app.manage(fl);
        }
        Err(e) => {
            warn!("Failed to initialise frontend log: {e:#}");
        }
    }

    // Register the durable refresh-token store. The frontend queries this on
    // boot when its WebView localStorage is empty, so a wipe of the WebView
    // UserData folder does not force the user to log in again as long as the
    // portable/installed data_dir survives.
    app.manage(SessionStore::new(paths.data_dir.clone()));

    // Pin the search index (and its incremental-indexing manifest) to a stable
    // absolute path under the data dir. Without this it defaults to a
    // working-directory-relative path, so incremental indexing would only find
    // its prior manifest when the app happens to launch from the same folder.
    std::env::set_var("LIBRARIUM_INDEX_DIR", paths.data_dir.join("indices"));

    // 2. Load or create configuration (first-launch branch).
    let config_file = paths.config_dir.join("config.toml");
    let mut config = if config_file.exists() {
        info!("Loading existing config from {:?}", config_file);
        AppConfig::load_from_dirs(&paths)?
    } else {
        warn!("No config.toml found — first launch. Writing default config.");
        // Create the default vault directory before writing config.
        std::fs::create_dir_all(&paths.default_vault_dir)
            .context("Failed to create default vault directory")?;
        AppConfig::write_default(&paths)?
    };

    // LIB-112: Persist a stable JWT secret on first launch (or whenever the
    // config has an empty one). The server generates an *ephemeral* random secret
    // at startup when jwt_secret is blank — meaning every restart invalidates all
    // previous tokens and forces the user to log in again. Writing a stable secret
    // here before the server starts ensures tokens survive across restarts.
    if config.auth.jwt_secret.trim().is_empty() {
        let secret = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).replace('-', "");
        config.auth.jwt_secret = secret;
        config
            .write_to_file(&config_file)
            .context("Failed to persist JWT secret to config.toml")?;
        info!("Generated and persisted a stable JWT secret to config.toml");
    }

    // LIB-080: the desktop app should stay signed in essentially indefinitely.
    // Enforce a long refresh-token lifetime floor (10 years) so the embedded
    // session does not expire between launches, regardless of whether a
    // config.toml is present. The short-lived access token still rotates
    // normally. A config that already asks for a longer lifetime is respected.
    //
    // Scope (LIB-089): this floor is applied ONLY by the desktop binary — a
    // standalone/server deployment keeps its configured lifetime. The desktop
    // server binds 127.0.0.1 (loopback, single local user), and the refresh
    // token itself is delivered to the WebView as an HttpOnly cookie (see
    // routes::auth::build_refresh_cookie) rather than being readable by page JS,
    // so the long-lived credential can't be exfiltrated by XSS.
    const DESKTOP_REFRESH_TTL_SECS: u64 = 10 * 365 * 24 * 60 * 60;
    config.auth.refresh_token_ttl = config.auth.refresh_token_ttl.max(DESKTOP_REFRESH_TTL_SECS);

    // Managed after every mutation above, so `auth_reset_local_password` sees
    // the same database path and password policy the server itself is using.
    app.manage(DesktopConfig {
        config: config.clone(),
        config_file: config_file.clone(),
    });

    // 3. Set up the system tray (yellow = starting).
    setup_tray(app)?;

    // 4. Register deep-link handler for librarium:// URLs.
    setup_deep_links(&handle)?;

    // 5. Propagate server startup errors back to this thread.
    let (err_tx, err_rx) = std::sync::mpsc::channel::<String>();

    // 6. Spawn the Actix server on a dedicated OS thread with its own runtime.
    //    Tauri must own the main thread on Linux/macOS; Actix is kept separate.
    let config_for_server = config.clone();
    std::thread::spawn(move || {
        if let Err(e) =
            actix_web::rt::System::new().block_on(async { librarium::run(config_for_server).await })
        {
            error!("Server thread exited with error: {e:#}");
            let _ = err_tx.send(format!("{e:#}"));
        }
    });

    // 7. Get the main WebView window (title defined in tauri.conf.json).
    let window = handle
        .get_webview_window("main")
        .context("main webview window not found")?;

    // 8. Poll /api/health asynchronously; navigate the WebView on success.
    let port = config.server.port;
    let handle_for_poll = handle.clone();
    tauri::async_runtime::spawn(async move {
        poll_until_healthy_then_navigate(port, window, handle_for_poll, err_rx).await;
    });

    // 9. Set up the background sync engine. Its state DB lives alongside the
    //    metadata DB but is kept separate from it. The engine is started only
    //    once the embedded server is healthy, so local vault paths can be
    //    resolved from librarium.db.
    let sync_db_path = paths.data_dir.join("sync.db");
    let db_url = format!("sqlite:{}", config.database.path);
    let sync_handle = SyncHandle::new(sync_db_path, db_url, std::sync::Arc::new(OsKeyringStore));
    app.manage(sync_handle.clone());

    let remotes = config.sync.remotes.clone();
    tauri::async_runtime::spawn(async move {
        wait_for_health(port).await;
        if let Err(e) = sync_handle.init_and_start(&remotes).await {
            warn!("Sync engine failed to start: {e:#}");
        }
    });

    Ok(())
}

/// The first real app host for `librarium-mobile`'s command layer (#62) —
/// only tests constructed this Tauri-managed state before now (see
/// `search.rs`/`metadata.rs`/`sync.rs` in that crate, each of which
/// explicitly documents what "the eventual Tauri app" is expected to do
/// here).
#[cfg(mobile)]
fn run_setup(app: &mut tauri::App) -> anyhow::Result<()> {
    let handle = app.handle().clone();

    let data_dir = handle
        .path()
        .app_data_dir()
        .context("resolve app data dir")?;
    let config_dir = handle
        .path()
        .app_config_dir()
        .context("resolve app config dir")?;
    std::fs::create_dir_all(&data_dir).context("create app data dir")?;
    std::fs::create_dir_all(&config_dir).context("create app config dir")?;

    // No server dependency, and frontend_log/frontend_log_path are
    // registered on every platform (see invoke_handler above) — leaving
    // this unmanaged would make those commands panic on unmanaged state.
    if let Ok(fl) = FrontendLog::init(data_dir.join("logs")) {
        app.manage(fl);
    }

    // A `SearchIndex` holds open Tantivy handles per vault in memory, so one
    // long-lived instance must be shared across calls (search.rs's doc
    // comment) — hence managed state rather than constructed per-command.
    app.manage(librarium_core::search_service::SearchIndex::with_index_dir(
        Some(data_dir.join("search-index")),
    ));

    // API keys are never persisted in plaintext (#54): `OsKeyringStore`
    // backs onto the platform's native credential store. On Android that
    // store needs a one-time bootstrap this doesn't do yet — see the
    // module doc's "Known gap" note (attempted during #63, reverted after
    // it crashed the app; tracked as its own follow-up); `pairing_set`/
    // `sync_add_remote` will fail cleanly (no crash) on a real device
    // until that's resolved.
    let secrets: std::sync::Arc<dyn librarium_mobile::SecretStore> =
        std::sync::Arc::new(librarium_mobile::OsKeyringStore);
    app.manage(librarium_mobile::SyncHandle::new(
        data_dir.join("sync.db"),
        config_dir,
        secrets,
    ));

    // `MobileDb::open` runs its (idempotent) migrations before returning —
    // block on it rather than spawning, since any metadata command
    // (preferences/recent/favorites/bookmarks) needs it already managed.
    // Desktop's setup hook already does comparable synchronous startup I/O.
    match tauri::async_runtime::block_on(librarium_mobile::MobileDb::open(
        &data_dir.join("mobile.db"),
    )) {
        Ok(db) => {
            app.manage(db);
        }
        Err(e) => tracing::warn!("Failed to open mobile.db: {e:#}"),
    }

    Ok(())
}

/// Block until the embedded server answers `GET /api/health`, or a 30s deadline
/// passes. Used to gate sync startup on the metadata DB being ready.
#[cfg(desktop)]
async fn wait_for_health(port: u16) {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

// ── System tray ───────────────────────────────────────────────────────────────

#[cfg(desktop)]
fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Open Librarium", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &separator, &quit_item])?;

    let yellow_icon = tauri::image::Image::from_bytes(TRAY_ICON_YELLOW)
        .context("Failed to load starting tray icon")?;

    TrayIconBuilder::with_id("main-tray")
        .icon(yellow_icon)
        .tooltip("Librarium — Starting…")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                info!("Quit requested from tray menu");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click on the tray icon brings the window to focus.
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)
        .context("Failed to build system tray")?;

    Ok(())
}

/// Update the tray icon and tooltip to reflect the current server status.
///
/// `status` is one of `"starting"`, `"healthy"`, or `"error"`.
#[cfg(desktop)]
fn update_tray_status(app: &AppHandle, status: &str) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };

    let (icon_bytes, tooltip) = match status {
        "healthy" => (TRAY_ICON_GREEN, "Librarium — Running"),
        "error" => (TRAY_ICON_RED, "Librarium — Error"),
        _ => (TRAY_ICON_YELLOW, "Librarium — Starting…"),
    };

    if let Ok(icon) = tauri::image::Image::from_bytes(icon_bytes) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(tooltip));
}

// ── Deep links ────────────────────────────────────────────────────────────────

#[cfg(desktop)]
fn setup_deep_links(handle: &AppHandle) -> anyhow::Result<()> {
    use tauri_plugin_deep_link::DeepLinkExt;

    // Register the librarium:// scheme at runtime (required on Linux/Windows;
    // on macOS registration is done via Info.plist bundled at build time).
    #[cfg(not(target_os = "macos"))]
    handle
        .deep_link()
        .register("librarium")
        .context("Failed to register librarium:// deep link scheme")?;

    let handle_clone = handle.clone();
    handle.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            info!("Deep link received: {url}");
            // Navigate the main window to the path encoded in the librarium:// URL.
            // E.g. librarium://open/vault/abc/file/note.md → /vault/abc/file/note.md
            if let Some(win) = handle_clone.get_webview_window("main") {
                let nav_path = deep_link_to_app_path(url.as_str());
                let js = format!("window.location.hash = {nav_path:?}");
                let _ = win.eval(&js);
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
    });

    Ok(())
}

/// Convert a `librarium://` URL to an in-app hash-router path.
///
/// `librarium://open/vault/abc/file/note.md` → `#/vault/abc/file/note.md`
///
/// Any URL that doesn't match `librarium://open/...` is mapped to `#/`.
#[cfg(desktop)]
pub(crate) fn deep_link_to_app_path(url: &str) -> String {
    // Strip the scheme and host part ("librarium://open"), keep the path.
    let stripped = url
        .strip_prefix("librarium://open")
        .or_else(|| url.strip_prefix("librarium://"))
        .unwrap_or("/");
    // The bare-`librarium://` fallback strips the separator along with the
    // scheme, so `librarium://vault/xyz` arrives here as `vault/xyz` — emitting
    // that verbatim yields `#vault/xyz`, which the router cannot match. Root the
    // path (which also covers the empty case, `librarium://` → `#/`).
    let mut path = String::with_capacity(stripped.len() + 2);
    path.push('#');
    if !stripped.starts_with('/') {
        path.push('/');
    }
    path.push_str(stripped);
    path
}

// ── Health polling ────────────────────────────────────────────────────────────

/// Poll `GET /api/health` every 100 ms for up to 10 s.
///
/// On a successful response, navigates the WebView to the running Librarium app
/// and updates the tray icon to green.
/// On timeout or a server startup error received via `err_rx`, shows an inline
/// error screen and updates the tray icon to red.
#[cfg(desktop)]
async fn poll_until_healthy_then_navigate(
    port: u16,
    window: tauri::WebviewWindow,
    app: AppHandle,
    err_rx: std::sync::mpsc::Receiver<String>,
) {
    // Use 127.0.0.1 rather than "localhost": the embedded server binds the IPv4
    // loopback, but "localhost" can resolve to ::1 (IPv6) first. On hosts where
    // ::1 is dropped (not refused), each 2s poll request can hang on IPv6 and
    // never fall through to IPv4, leaving the shell stuck on "failed to start"
    // even though the server is healthy. Pinning IPv4 removes that ambiguity.
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .expect("failed to build reqwest client");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

    loop {
        // Check for an error reported by the server thread.
        if let Ok(err) = err_rx.try_recv() {
            let message = classify_server_error(&err, port);
            update_tray_status(&app, "error");
            show_error_screen(&window, &message);
            return;
        }

        if std::time::Instant::now() > deadline {
            update_tray_status(&app, "error");
            show_error_screen(
                &window,
                "Librarium did not become ready within 10 seconds.\nCheck the application logs for details.",
            );
            return;
        }

        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                info!("Server healthy — navigating WebView to http://127.0.0.1:{port}");
                update_tray_status(&app, "healthy");
                let _ = window.eval(format!(
                    "window.location.replace('http://127.0.0.1:{port}')"
                ));
                return;
            }
            _ => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Classify a server startup error string into a user-readable message.
#[cfg(desktop)]
pub(crate) fn classify_server_error(raw: &str, port: u16) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("address already in use")
        || lower.contains("already in use")
        || lower.contains("os error 98")  // EADDRINUSE on Linux
        || lower.contains("os error 48")  // EADDRINUSE on macOS
        || lower.contains("only one usage")
    // Windows WSAEADDRINUSE
    {
        format!(
            "Port {port} is already in use.\n\
            Another instance of Librarium may be running, or a different program is \
            occupying that port.\n\nClose the other application and relaunch Librarium."
        )
    } else if lower.contains("permission denied") || lower.contains("os error 13") {
        format!(
            "Permission denied when binding to port {port}.\n\
            Ports below 1024 require elevated privileges. \
            Change the port in your config.toml to a value above 1024."
        )
    } else {
        format!("Server failed to start:\n{raw}")
    }
}

/// Replace the WebView content with a simple error screen.
#[cfg(desktop)]
fn show_error_screen(window: &tauri::WebviewWindow, message: &str) {
    let escaped = message
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "<br>");
    let js = format!(
        r#"document.body.innerHTML = '<div style="display:flex;height:100vh;\
flex-direction:column;align-items:center;justify-content:center;\
font-family:system-ui,sans-serif;color:#c00;padding:24px;text-align:center">\
<h2 style="margin-bottom:12px">Librarium failed to start</h2>\
<p style="max-width:480px;color:#444">{escaped}</p></div>';"#
    );
    let _ = window.eval(&js);
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    // ── open_external_url scheme allow-list ───────────────────────────────────

    #[test]
    fn allowed_url_schemes_pass_the_gate() {
        for url in [
            "http://example.com",
            "https://example.com/path?q=1",
            "mailto:alice@example.com",
            "tel:+1-555-0100",
        ] {
            assert!(is_allowed_external_url(url), "expected {url} to be allowed");
        }
    }

    #[test]
    fn disallowed_url_schemes_are_rejected() {
        // file:// would let a rendered note trigger a local-file open — the
        // main reason we filter here rather than blindly forwarding to `open`.
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "ftp://example.com",
            "vbscript:msgbox",
            "",
            "example.com",
        ] {
            assert!(
                !is_allowed_external_url(url),
                "expected {url} to be rejected"
            );
        }
    }

    // ── write_binary_file ──────────────────────────────────────────────────

    #[test]
    fn write_binary_file_decodes_base64_and_writes_bytes() {
        use base64::Engine;
        let dir = std::env::temp_dir().join(format!(
            "librarium-write-binary-file-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bundle.zip");

        let data = b"not really a zip, just some bytes\x00\x01\x02";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);

        write_binary_file(path.to_string_lossy().to_string(), encoded).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_binary_file_rejects_invalid_base64() {
        let path = std::env::temp_dir().join(format!(
            "librarium-write-binary-file-invalid-{}",
            uuid::Uuid::new_v4()
        ));
        let result = write_binary_file(
            path.to_string_lossy().to_string(),
            "not-base64!!".to_string(),
        );
        assert!(result.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn classify_port_in_use_linux() {
        let msg = classify_server_error(
            "Os { code: 98, kind: AddrInUse, message: \"Address already in use\" }",
            8080,
        );
        assert!(
            msg.contains("Port 8080 is already in use"),
            "expected port-conflict message, got: {msg}"
        );
        assert!(
            msg.contains("relaunch Librarium"),
            "expected suggestion, got: {msg}"
        );
    }

    #[test]
    fn classify_port_in_use_macos() {
        let msg = classify_server_error("os error 48", 8080);
        assert!(
            msg.contains("Port 8080 is already in use"),
            "macOS EADDRINUSE: {msg}"
        );
    }

    #[test]
    fn classify_port_in_use_windows() {
        let msg = classify_server_error("Only one usage of each socket address", 8080);
        assert!(
            msg.contains("Port 8080 is already in use"),
            "Windows WSAEADDRINUSE: {msg}"
        );
    }

    #[test]
    fn classify_permission_denied() {
        let msg = classify_server_error("permission denied binding port 80", 80);
        assert!(
            msg.contains("Permission denied"),
            "expected permission message, got: {msg}"
        );
        assert!(
            msg.contains("config.toml"),
            "expected config hint, got: {msg}"
        );
    }

    #[test]
    fn classify_generic_error_is_passthrough() {
        let raw = "some unexpected database initialization failure";
        let msg = classify_server_error(raw, 8080);
        assert!(msg.contains(raw), "raw error should be included: {msg}");
        assert!(msg.starts_with("Server failed to start:"));
    }

    #[test]
    fn classify_preserves_port_in_messages() {
        for port in [80u16, 443, 3000, 8080, 51234] {
            let msg = classify_server_error("address already in use", port);
            assert!(
                msg.contains(&port.to_string()),
                "port {port} not in message: {msg}"
            );
        }
    }

    // ── deep link tests ───────────────────────────────────────────────────────

    #[test]
    fn deep_link_open_vault_file() {
        let path = deep_link_to_app_path("librarium://open/vault/abc/file/note.md");
        assert_eq!(path, "#/vault/abc/file/note.md");
    }

    #[test]
    fn deep_link_bare_scheme() {
        let path = deep_link_to_app_path("librarium://");
        assert_eq!(path, "#/");
    }

    #[test]
    fn deep_link_open_root() {
        let path = deep_link_to_app_path("librarium://open");
        assert_eq!(path, "#/");
    }

    #[test]
    fn deep_link_open_with_query() {
        let path = deep_link_to_app_path("librarium://open/search?q=hello");
        assert_eq!(path, "#/search?q=hello");
    }

    #[test]
    fn deep_link_unknown_host_prefix_stripped() {
        // Anything under librarium:// that doesn't start with /open falls through
        // to the librarium:// fallback.
        let path = deep_link_to_app_path("librarium://vault/xyz");
        assert_eq!(path, "#/vault/xyz");
    }
}
