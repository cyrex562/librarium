pub mod assets;

pub mod config;
pub mod db;
pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod watcher;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use anyhow::Context as _;
use config::AppConfig;
use db::Database;
use routes::AppState;
use services::{
    EntityTypeRegistry, LabelService, MarkdownParser, ReindexService, RelationTypeRegistry,
    SchemaService, SearchIndex,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;
use watcher::FileWatcher;

// TLS support — only imported when actually needed at runtime
use rustls::ServerConfig as RustlsServerConfig;
use rustls_pemfile::{certs, private_key};

#[cfg(debug_assertions)]
use actix_files::NamedFile;
#[cfg(debug_assertions)]
use actix_web::Result as WebResult;
#[cfg(not(debug_assertions))]
use actix_web::{HttpResponse, Result as WebResult};
#[cfg(not(debug_assertions))]
use assets::Assets;
#[cfg(not(debug_assertions))]
use mime_guess::from_path;

#[cfg(not(debug_assertions))]
async fn serve_embedded_file(path: web::Path<String>) -> WebResult<HttpResponse> {
    let path_str = path.into_inner();
    let file_path = if path_str.is_empty() {
        "index.html"
    } else {
        &path_str
    };
    match Assets::get(file_path) {
        Some(content) => {
            let mime_type = from_path(file_path).first_or_octet_stream();
            Ok(HttpResponse::Ok()
                .content_type(mime_type.as_ref())
                .body(content.data.into_owned()))
        }
        None if is_spa_route(file_path) => match Assets::get("index.html") {
            Some(content) => Ok(HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(content.data.into_owned())),
            None => Ok(HttpResponse::NotFound().body("404 Not Found")),
        },
        None => Ok(HttpResponse::NotFound().body("404 Not Found")),
    }
}

#[cfg(not(debug_assertions))]
fn is_spa_route(path: &str) -> bool {
    !path.starts_with("api/") && !path.contains('.')
}

#[cfg(not(debug_assertions))]
async fn serve_embedded_index() -> WebResult<HttpResponse> {
    serve_embedded_file(web::Path::from("index.html".to_string())).await
}

#[cfg(not(debug_assertions))]
fn configure_static(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(serve_embedded_index))
        .route("/{filename:.*}", web::get().to(serve_embedded_file));
}

#[cfg(debug_assertions)]
async fn serve_dev_file(path: web::Path<String>) -> WebResult<NamedFile> {
    let base = std::fs::canonicalize("./target/frontend")
        .context("Failed to resolve frontend build output directory")
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let requested = path.into_inner();
    let candidate = if requested.is_empty() {
        base.join("index.html")
    } else {
        base.join(&requested)
    };

    if let Ok(resolved) = std::fs::canonicalize(&candidate) {
        if resolved.starts_with(&base) && resolved.is_file() {
            return NamedFile::open(resolved).map_err(actix_web::error::ErrorInternalServerError);
        }
    }

    NamedFile::open(base.join("index.html")).map_err(actix_web::error::ErrorInternalServerError)
}

#[cfg(debug_assertions)]
async fn serve_dev_index() -> WebResult<NamedFile> {
    serve_dev_file(web::Path::from(String::new())).await
}

#[cfg(debug_assertions)]
fn configure_static(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(serve_dev_index))
        .route("/{filename:.*}", web::get().to(serve_dev_file));
}

/// Write the first-run administrator credentials to `FIRST-RUN-CREDENTIALS.txt`
/// alongside the database file.
///
/// The directory is taken from the database path's parent (falling back to the
/// current directory for a bare filename). On Unix the file is created with
/// `0600` permissions so other local users cannot read it.
fn write_first_run_credentials(
    db_path: &str,
    username: &str,
    password: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let dir = std::path::Path::new(db_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create credentials directory {}", dir.display()))?;

    let path = dir.join("FIRST-RUN-CREDENTIALS.txt");
    let contents = format!(
        "Librarium — first-run administrator account\n\
         ============================================\n\n\
         username: {username}\n\
         password: {password}\n\n\
         You will be required to choose a new password the first time you log in.\n\
         Once you have logged in and changed the password, delete this file.\n"
    );
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(path)
}

/// Start the Librarium HTTP server with the given configuration.
///
/// Sets up logging, initialises the database, file watcher, search index,
/// and plugin registries, then runs the Actix-web server until a shutdown
/// signal is received.
///
/// This function is callable from both the standalone binary (`main.rs`)
/// and from a future Tauri shell (which will run it on a background thread
/// with its own `actix_web::rt::System`).
pub async fn run(config: AppConfig) -> anyhow::Result<()> {
    // --- Logging -----------------------------------------------------------
    // Write logs next to the database so they're findable regardless of the
    // process working directory (portable: ./data/logs; installed: alongside
    // the app data dir). Falls back to ./logs only if the DB path has no parent.
    // Best-effort dir creation: a logging-setup hiccup must never abort startup.
    let log_dir = std::path::Path::new(&config.database.path)
        .parent()
        .map(|p| p.join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("./logs"));
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "librarium.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v.to_lowercase() == "json")
        .unwrap_or(false);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "warn,librarium=info,actix_web=info,actix_server=info".into());

    // try_init instead of init so the function is safe to call multiple times
    // (e.g. from tests or when Tauri sets up its own subscriber first).
    let _ = if use_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .json()
                    .with_writer(non_blocking)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true),
            )
            .with(fmt::layer().with_writer(std::io::stdout).with_ansi(true))
            .try_init()
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_writer(non_blocking)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false),
            )
            .with(fmt::layer().with_writer(std::io::stdout).with_ansi(true))
            .try_init()
    };

    info!(
        "Logging initialized (format: {})",
        if use_json { "JSON" } else { "text" }
    );

    // --- Config validation -------------------------------------------------
    let mut config = config;

    info!("Starting Librarium server...");
    info!(
        "Server config: {}:{}",
        config.server.host, config.server.port
    );

    if config.auth.jwt_secret.trim().is_empty() {
        let generated_secret = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "");
        config.auth.jwt_secret = generated_secret;
        warn!(
            "auth.jwt_secret is empty; generated an ephemeral runtime secret. \
             Set [auth].jwt_secret in config.toml for persistent tokens across restarts."
        );
    } else if config.auth.jwt_secret.trim() == config::DEFAULT_DEV_JWT_SECRET {
        warn!(
            "Using insecure default JWT secret. \
             Set [auth].jwt_secret in config.toml before production use."
        );
    }

    // --- Database ----------------------------------------------------------
    let db_url = format!("sqlite:{}", config.database.path);
    let db = Database::new(&db_url)
        .await
        .expect("Failed to initialize database");
    db.run_recent_files_migration()
        .await
        .expect("Failed to run recent_files user_id migration");
    info!("Database initialized at {}", config.database.path);

    // Prime the ML embedder once at startup (LIB-059). When the embeddings tier
    // is active and a model is available this loads it now so the off-request
    // reindex path can use `embedder_if_ready()`; otherwise it logs once and the
    // feature degrades to Tier 1.
    if config.ml.enabled {
        let _ = services::embedding_service::embedder(&config.ml);
    }

    // --- First-run admin bootstrap ----------------------------------------
    // Precedence:
    //   1. Explicit username + password in config  → create that admin as-is.
    //   2. Auth enabled, password omitted          → generate a random password,
    //      force a change at first login, and write it to a credentials file
    //      next to the database (portable / first-launch flow).
    //   3. Otherwise                               → nothing to bootstrap.
    let bootstrap_username = config
        .auth
        .bootstrap_admin_username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let bootstrap_password = config
        .auth
        .bootstrap_admin_password
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match (bootstrap_username, bootstrap_password) {
        (Some(username), Some(password)) => {
            match db.bootstrap_admin_if_empty(Some(username), Some(password)).await {
                Ok(true) => info!(
                    "No users were found. Bootstrapped admin user '{username}' from config.toml"
                ),
                Ok(false) => info!("User bootstrap skipped (existing users found)"),
                Err(e) => warn!("User bootstrap skipped: {e}"),
            }
        }
        _ if config.auth.enabled => {
            let username = bootstrap_username.unwrap_or("admin");
            match db.bootstrap_admin_generated_if_empty(username).await {
                Ok(Some(password)) => {
                    match write_first_run_credentials(&config.database.path, username, &password) {
                        Ok(path) => info!(
                            "No users were found. Generated first-run admin '{username}'. \
                             Credentials written to {} — you must change the password at \
                             first login.",
                            path.display()
                        ),
                        Err(e) => warn!(
                            "Generated first-run admin '{username}' but could not write the \
                             credentials file: {e}. One-time password: {password}"
                        ),
                    }
                }
                Ok(None) => info!("User bootstrap skipped (existing users found)"),
                Err(e) => warn!("First-run admin bootstrap failed: {e}"),
            }
        }
        _ => {
            info!(
                "No bootstrap admin configured and auth is disabled; skipping first-run admin. \
                 Set [auth] enabled=true (and optionally bootstrap_admin_username) to create one."
            );
        }
    }

    // Seed core labels (idempotent — safe to run on every startup)
    if let Err(e) = LabelService::seed_core_labels(&db).await {
        warn!("Failed to seed core labels: {e}");
    } else {
        info!("Core labels seeded");
    }

    // --- Search & watcher --------------------------------------------------
    let search_index = SearchIndex::new();
    info!("Search index initialized");

    let (watcher, mut change_rx) = FileWatcher::new().expect("Failed to create file watcher");
    let watcher = Arc::new(Mutex::new(watcher));
    info!("File watcher initialized");

    // --- Event loop --------------------------------------------------------
    let (event_tx, _) = broadcast::channel::<models::FileChangeEvent>(100);
    let event_tx_clone = event_tx.clone();
    let (ws_tx, _) = broadcast::channel::<models::WsMessage>(64);

    let search_index_clone = search_index.clone();
    let db_clone = db.clone();
    let ws_batch = ws_tx.clone();
    tokio::spawn(async move {
        while let Some(first_event) = change_rx.recv().await {
            // Drain queued events so we can batch search-index commits.
            // A single Tantivy commit covers all docs added in one writer session;
            // without batching, 100 uploads produce 100 separate fsyncs.
            // LIB-100: cap the drain so a sustained high event rate can't grow
            // `events` without bound (and bloat memory / one over-long commit).
            // Anything beyond the cap stays queued and is handled by the next
            // outer-loop iteration.
            const MAX_BATCH: usize = 512;
            let mut events = vec![first_event];
            while events.len() < MAX_BATCH {
                match change_rx.try_recv() {
                    Ok(next) => events.push(next),
                    Err(_) => break,
                }
            }

            if events.len() > 1 {
                info!("Processing batch of {} file change events", events.len());
            }

            // ── 1. Batch-read markdown content and group by vault ─────────────
            // We collect (vault_id, path, content) for Created/Modified .md files
            // so we can do one Tantivy commit per vault instead of one per file.
            let mut vault_cache: HashMap<String, String> = HashMap::new(); // vault_id → vault.path
            let mut batch_by_vault: HashMap<String, Vec<(String, String)>> = HashMap::new();

            for event in &events {
                match &event.event_type {
                    models::FileChangeType::Created | models::FileChangeType::Modified => {
                        if event.path.ends_with(".md") {
                            let vault_path = if let Some(p) = vault_cache.get(&event.vault_id) {
                                Some(p.clone())
                            } else if let Ok(vault) = db_clone.get_vault(&event.vault_id).await {
                                vault_cache.insert(event.vault_id.clone(), vault.path.clone());
                                Some(vault.path)
                            } else {
                                None
                            };

                            if let Some(vpath) = vault_path {
                                if let Ok(content) =
                                    services::FileService::read_file(&vpath, &event.path)
                                {
                                    batch_by_vault
                                        .entry(event.vault_id.clone())
                                        .or_default()
                                        .push((event.path.clone(), content.content));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // One Tantivy commit per vault covers all files in this batch.
            for (vault_id, files) in &batch_by_vault {
                let _ = ws_batch.send(models::WsMessage::IndexingStatus {
                    vault_id: vault_id.clone(),
                    active: true,
                });
                if let Err(e) = search_index_clone.update_files_batch(vault_id, files) {
                    warn!("Batch search index update failed for vault {vault_id}: {e}");
                }
                let _ = ws_batch.send(models::WsMessage::IndexingStatus {
                    vault_id: vault_id.clone(),
                    active: false,
                });
            }

            // ── 2. Per-event processing (entity DB writes + non-md removes) ───
            // Entity reindex is the disk/DB-heavy step. Pace it so a big burst
            // (e.g. a bulk import of hundreds of files) is spread over time and
            // the OS disk scheduler can interleave foreground work, rather than
            // hammering SQLite in one uninterruptible loop.
            // LIB-097 (accepted tradeoff): the pause defers the `FileChanged` WS
            // broadcast + entity write for events *after* each throttle point —
            // up to ~`(len/EVERY) * PAUSE_MS` of added latency for the last event
            // in a very large batch. This is intentional: correctness is
            // unaffected (search index already committed in step 1), and the lag
            // only applies to bulk bursts, not interactive single edits.
            const REINDEX_THROTTLE_EVERY: usize = 40;
            const REINDEX_THROTTLE_PAUSE_MS: u64 = 25;
            for (event_idx, change_event) in events.iter().enumerate() {
                if event_idx > 0 && event_idx % REINDEX_THROTTLE_EVERY == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        REINDEX_THROTTLE_PAUSE_MS,
                    ))
                    .await;
                }
                match &change_event.event_type {
                    models::FileChangeType::Created | models::FileChangeType::Modified => {
                        if change_event.path.ends_with(".md") {
                            let vault_path = if let Some(p) = vault_cache.get(&change_event.vault_id) {
                                Some(p.clone())
                            } else if let Ok(vault) = db_clone.get_vault(&change_event.vault_id).await {
                                vault_cache.insert(change_event.vault_id.clone(), vault.path.clone());
                                Some(vault.path)
                            } else {
                                None
                            };

                            if let Some(vpath) = vault_path {
                                let abs_path = format!(
                                    "{}/{}",
                                    vpath.trim_end_matches('/'),
                                    change_event.path
                                );
                                if let Err(e) = ReindexService::index_file(
                                    &db_clone,
                                    &change_event.vault_id,
                                    &change_event.path,
                                    &abs_path,
                                )
                                .await
                                {
                                    warn!("Entity index_file failed for {}: {e}", change_event.path);
                                }
                            }
                        }
                    }
                    models::FileChangeType::Deleted => {
                        let _ = search_index_clone
                            .remove_file(&change_event.vault_id, &change_event.path);
                        if let Err(e) = ReindexService::remove_file(
                            &db_clone,
                            &change_event.vault_id,
                            &change_event.path,
                        )
                        .await
                        {
                            warn!("Entity remove_file failed for {}: {e}", change_event.path);
                        }
                    }
                    models::FileChangeType::Renamed { from, to } => {
                        let _ =
                            search_index_clone.remove_file(&change_event.vault_id, from);
                        if let Err(e) =
                            ReindexService::remove_file(&db_clone, &change_event.vault_id, from)
                                .await
                        {
                            warn!("Entity remove_file (rename from) failed for {from}: {e}");
                        }
                        if to.ends_with(".md") {
                            let vault_path = if let Some(p) = vault_cache.get(&change_event.vault_id) {
                                Some(p.clone())
                            } else if let Ok(vault) =
                                db_clone.get_vault(&change_event.vault_id).await
                            {
                                vault_cache.insert(change_event.vault_id.clone(), vault.path.clone());
                                Some(vault.path)
                            } else {
                                None
                            };

                            if let Some(vpath) = vault_path {
                                if let Ok(content) =
                                    services::FileService::read_file(&vpath, to)
                                {
                                    let _ = search_index_clone.update_file(
                                        &change_event.vault_id,
                                        to,
                                        content.content,
                                    );
                                }
                                let abs_path =
                                    format!("{}/{}", vpath.trim_end_matches('/'), to);
                                if let Err(e) = ReindexService::index_file(
                                    &db_clone,
                                    &change_event.vault_id,
                                    to,
                                    &abs_path,
                                )
                                .await
                                {
                                    warn!("Entity index_file (rename to) failed for {to}: {e}");
                                }
                            }
                        }
                    }
                }

                if let Err(e) = event_tx_clone.send(change_event.clone()) {
                    error!("Failed to broadcast event: {}", e);
                }
            }
        }
    });

    // --- Vault loading -----------------------------------------------------
    let vaults = db.list_vaults().await.expect("Failed to list vaults");
    for vault in vaults {
        info!("Loading vault: {} at {}", vault.name, vault.path);

        if !std::path::Path::new(&vault.path).exists() {
            warn!(
                "Removing vault {} because path is missing: {}",
                vault.id, vault.path
            );
            if let Err(e) = search_index.remove_vault(&vault.id) {
                error!(
                    "Failed to remove vault {} from search index: {}",
                    vault.id, e
                );
            }
            if let Err(e) = db.delete_vault(&vault.id).await {
                error!("Failed to delete missing vault {} from DB: {}", vault.id, e);
            }
            continue;
        }

        let mut w = watcher.lock().await;
        if let Err(e) = w.watch_vault(vault.id.clone(), vault.path.clone().into()) {
            error!("Failed to watch vault {}: {}", vault.id, e);
        }
        drop(w);

        // Index the vault in the background so the HTTP server can start
        // serving (and the desktop window can load) immediately instead of
        // blocking on a full re-index of a large vault. Incremental indexing
        // keeps subsequent restarts cheap; the IndexingStatus broadcasts let the
        // UI surface an "Indexing…" indicator while the work runs.
        let search_bg = search_index.clone();
        let db_reindex = db.clone();
        let ws_index = ws_tx.clone();
        let vid = vault.id.clone();
        let vname = vault.name.clone();
        let vpath = vault.path.clone();
        tokio::spawn(async move {
            let _ = ws_index.send(models::WsMessage::IndexingStatus {
                vault_id: vid.clone(),
                active: true,
            });

            // Search index is a blocking (disk-bound) operation — run it on the
            // blocking pool so it never stalls async worker threads.
            let s = search_bg.clone();
            let vp = vpath.clone();
            let vi = vid.clone();
            match tokio::task::spawn_blocking(move || s.index_vault(&vi, &vp)).await {
                Ok(Ok(count)) => info!("Indexed {} files in vault {}", count, vname),
                Ok(Err(e)) => error!("Failed to index vault {}: {}", vid, e),
                Err(e) => error!("Index task panicked for vault {}: {}", vid, e),
            }

            if let Err(e) = ReindexService::reindex_vault(&db_reindex, &vid, &vpath).await {
                error!("Entity reindex failed for vault {vid}: {e}");
            }

            let _ = ws_index.send(models::WsMessage::IndexingStatus {
                vault_id: vid.clone(),
                active: false,
            });
        });
    }

    // --- Plugin schemas ----------------------------------------------------
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let plugins_dir = services::resolve_plugins_dir();
    info!("Using plugins directory: {}", plugins_dir.display());

    let entity_type_registry = EntityTypeRegistry::new();
    let relation_type_registry = RelationTypeRegistry::new();
    {
        use services::PluginService;
        let mut plugin_svc = PluginService::new(plugins_dir.clone());
        match plugin_svc.discover_plugins() {
            Ok(plugins) => {
                if let Err(e) = SchemaService::load_plugin_schemas(
                    &db,
                    &plugins,
                    &entity_type_registry,
                    &relation_type_registry,
                )
                .await
                {
                    warn!("Schema loading error: {e}");
                }
            }
            Err(e) => {
                warn!("Plugin discovery failed during schema load: {e}");
            }
        }
    }
    info!("Plugin schemas loaded");

    // --- HTTP server -------------------------------------------------------
    let app_state = web::Data::new(AppState {
        db,
        search_index,
        watcher,
        event_broadcaster: event_tx,
        ws_broadcaster: ws_tx,
        change_log_retention_days: config.sync.change_log_retention_days,
        ml_undo_store: Arc::new(Mutex::new(HashMap::new())),
        entity_type_registry,
        relation_type_registry,
        plugins_dir: plugins_dir.clone(),
        shutdown_tx: shutdown_tx.clone(),
        document_parser: Arc::new(MarkdownParser),
    });
    let app_config = web::Data::new(config.clone());

    let server_host = config.server.host.clone();
    let server_port = config.server.port;
    let cors_allowed_origins = config.cors.allowed_origins.clone();
    let tls_config = config.tls.clone();

    let http_server = HttpServer::new(move || {
        let mut cors = Cors::default()
            .allow_any_header()
            .allow_any_method()
            .max_age(3600);

        if cors_allowed_origins.is_empty() {
            cors = cors.allow_any_origin();
        } else {
            for origin in &cors_allowed_origins {
                cors = cors.allowed_origin(origin);
            }
        }

        App::new()
            .app_data(app_state.clone())
            .app_data(app_config.clone())
            .wrap(cors)
            .wrap(middleware::RequestLogging)
            .wrap(middleware::RequestIdMiddleware)
            .wrap(middleware::RateLimitMiddleware)
            .wrap(middleware::AuthMiddleware)
            .wrap(actix_web::middleware::Compress::default())
            .configure(routes::health::configure)
            .configure(routes::version::configure)
            .configure(routes::auth::configure)
            .configure(routes::admin::configure)
            .configure(routes::groups::configure)
            .configure(routes::vaults::configure)
            .configure(routes::files::configure)
            .configure(routes::search::configure)
            .configure(routes::ml::configure)
            .configure(routes::ws::configure)
            .configure(routes::markdown::configure)
            .configure(routes::preferences::configure)
            .configure(routes::entities::configure)
            .configure(routes::plugins::configure)
            .configure(routes::bookmarks::configure)
            .configure(routes::tags::configure)
            .configure(routes::api_keys::configure)
            .configure(routes::totp::configure)
            .configure(routes::invitations::configure)
            .configure(routes::oidc::configure)
            .configure(configure_static)
    })
    .shutdown_timeout(10);

    // Bind with TLS if cert_file + key_file are both configured; otherwise plain HTTP.
    let server = match (
        tls_config.cert_file.as_deref(),
        tls_config.key_file.as_deref(),
    ) {
        (Some(cert_path), Some(key_path)) => {
            info!("TLS enabled — loading certificate from {cert_path}");
            let cert_bytes = std::fs::read(cert_path)
                .with_context(|| format!("Failed to read TLS certificate: {cert_path}"))?;
            let key_bytes = std::fs::read(key_path)
                .with_context(|| format!("Failed to read TLS private key: {key_path}"))?;

            let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
                certs(&mut std::io::BufReader::new(cert_bytes.as_slice()))
                    .collect::<Result<Vec<_>, _>>()
                    .with_context(|| "Failed to parse TLS certificate chain")?;

            let private_key = private_key(&mut std::io::BufReader::new(key_bytes.as_slice()))
                .with_context(|| "Failed to parse TLS private key")?
                .ok_or_else(|| anyhow::anyhow!("No private key found in {key_path}"))?;

            let rustls_cfg = RustlsServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain, private_key)
                .with_context(|| "Failed to build rustls server config")?;

            info!("Starting HTTPS server on {}:{}", server_host, server_port);
            http_server
                .bind_rustls_0_23((server_host.as_str(), server_port), rustls_cfg)?
                .run()
        }
        (None, None) => {
            info!("Starting HTTP server on {}:{}", server_host, server_port);
            http_server.bind((server_host.as_str(), server_port))?.run()
        }
        _ => {
            anyhow::bail!(
                "TLS configuration error: both `tls.cert_file` and `tls.key_file` must be set (or neither)"
            );
        }
    };

    let server_handle = server.handle();

    // Spawn signal listener → graceful shutdown.
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        info!("Shutdown signal received — notifying WebSocket clients and draining requests");
        let _ = shutdown_tx.send(());
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        server_handle.stop(true).await;
    });

    server.await?;
    Ok(())
}

/// Waits for SIGTERM (Unix) or Ctrl+C (all platforms).
async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
