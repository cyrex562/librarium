use clap::Parser;

/// Command-line arguments for the Librarium knowledge server.
#[derive(Parser, Debug)]
#[command(name = "librarium", about = "Librarium knowledge server", version)]
struct Args {
    /// Path to config.toml.
    /// Precedence: --config flag > LIBRARIUM_CONFIG env var > ./config.toml default.
    #[arg(
        short,
        long,
        default_value = "./config.toml",
        env = "LIBRARIUM_CONFIG",
        value_name = "PATH"
    )]
    config: std::path::PathBuf,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    if std::env::var_os("LIBRARIUM_CONFIG").is_none() {
        if let Some(legacy_config) = std::env::var_os("CODEX_CONFIG") {
            std::env::set_var("LIBRARIUM_CONFIG", legacy_config);
        }
    }

    let args = Args::parse();
    let config = librarium::config::AppConfig::load_from_file(args.config).unwrap_or_else(|e| {
        eprintln!("Warning: {e}. Using default configuration plus LIBRARIUM__* environment overrides.");
        librarium::config::AppConfig::load_from_env_or_default().unwrap_or_else(|env_err| {
            eprintln!("Warning: {env_err}. Using built-in default configuration.");
            librarium::config::AppConfig::default()
        })
    });
    librarium::run(config).await
}
