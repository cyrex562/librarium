use clap::Parser;

/// Command-line arguments for the Codex knowledge server.
#[derive(Parser, Debug)]
#[command(name = "codex", about = "Codex knowledge server", version)]
struct Args {
    /// Path to config.toml.
    /// Precedence: --config flag > CODEX_CONFIG env var > ./config.toml default.
    #[arg(
        short,
        long,
        default_value = "./config.toml",
        env = "CODEX_CONFIG",
        value_name = "PATH"
    )]
    config: std::path::PathBuf,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = codex::config::AppConfig::load_from_file(args.config).unwrap_or_else(|e| {
        eprintln!("Warning: {e}. Using default configuration plus CODEX__* environment overrides.");
        codex::config::AppConfig::load_from_env_or_default().unwrap_or_else(|env_err| {
            eprintln!("Warning: {env_err}. Using built-in default configuration.");
            codex::config::AppConfig::default()
        })
    });
    codex::run(config).await
}
