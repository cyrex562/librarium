use clap::Parser;

/// Command-line arguments for the Librarium knowledge server.
#[derive(Parser, Debug)]
#[command(name = "librarium", about = "Librarium knowledge server", version)]
struct Args {
    /// Path to config.toml.
    /// Precedence: --config flag > LIBRARIUM_CONFIG env var > exe-adjacent config.toml > ./config.toml
    #[arg(
        short,
        long,
        default_value = "./config.toml",
        env = "LIBRARIUM_CONFIG",
        value_name = "PATH"
    )]
    config: std::path::PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Offline account administration (works with the server stopped).
    #[command(subcommand)]
    Admin(librarium::cli::AdminCommand),
}

/// Resolve the config file path.
///
/// For relative paths (including the `./config.toml` default) we first check
/// whether the same filename exists beside the executable. This makes the
/// portable layout work when the user double-clicks the exe from Explorer and
/// the working directory happens to differ from the exe's directory.
fn locate_config(specified: std::path::PathBuf) -> std::path::PathBuf {
    if specified.is_absolute() {
        return specified;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside_exe = dir.join(&specified);
            if beside_exe.exists() {
                return beside_exe;
            }
        }
    }
    specified
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    if std::env::var_os("LIBRARIUM_CONFIG").is_none() {
        if let Some(legacy_config) = std::env::var_os("CODEX_CONFIG") {
            std::env::set_var("LIBRARIUM_CONFIG", legacy_config);
        }
    }

    let args = Args::parse();
    let command = args.command;
    let config_path = locate_config(args.config);
    let config = librarium::config::AppConfig::load_from_file(config_path).unwrap_or_else(|e| {
        eprintln!(
            "Warning: {e}. Using default configuration plus LIBRARIUM__* environment overrides."
        );
        librarium::config::AppConfig::load_from_env_or_default().unwrap_or_else(|env_err| {
            eprintln!("Warning: {env_err}. Using built-in default configuration.");
            librarium::config::AppConfig::default()
        })
    });
    // `admin` subcommands operate on the database directly and must run
    // instead of the server, not alongside it.
    if let Some(Command::Admin(admin_cmd)) = command {
        return librarium::cli::run_admin(admin_cmd, &config).await;
    }

    librarium::run(config).await
}
