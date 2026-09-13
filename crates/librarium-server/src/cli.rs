//! `librarium admin …` — offline credential repair.
//!
//! These subcommands open the SQLite database directly rather than calling the
//! HTTP API, because they must work when you cannot authenticate (a forgotten
//! password) or when the server will not start. Recovery tooling must not
//! depend on the thing that is broken.
//!
//! Filesystem access to the database *is* the authorization model here, which
//! is the right trust boundary for a self-hosted app: anyone who can read the
//! SQLite file can already read every vault file it indexes.

use crate::config::AppConfig;
use crate::db::Database;
use crate::services::CredentialService;
use anyhow::{anyhow, Context};

#[derive(clap::Subcommand, Debug)]
pub enum AdminCommand {
    /// Set (or reset) a user's password. Prompts; never takes it on the command line.
    SetPassword {
        username: String,
        /// Test-only escape hatch, not a CLI flag: a password passed on the
        /// command line is visible in `ps` output and lands in shell history.
        #[arg(skip)]
        password: Option<String>,
    },
    /// Create a user account. Prompts for the password.
    CreateUser {
        username: String,
        /// Grant administrator rights.
        #[arg(long)]
        admin: bool,
        /// Test-only escape hatch — see `SetPassword`.
        #[arg(skip)]
        password: Option<String>,
    },
    /// List every user account.
    ListUsers,
}

/// Prompt twice for a password, with no echo, and require the two to match.
fn prompt_new_password() -> anyhow::Result<String> {
    let first = rpassword::prompt_password("New password: ").context("Failed to read password")?;
    let second = rpassword::prompt_password("Confirm password: ")
        .context("Failed to read password confirmation")?;
    if first != second {
        return Err(anyhow!("Passwords did not match"));
    }
    if first.trim().is_empty() {
        return Err(anyhow!("Password cannot be empty"));
    }
    Ok(first)
}

async fn open_db(config: &AppConfig) -> anyhow::Result<Database> {
    let url = if config.database.path.starts_with("sqlite:") {
        config.database.path.clone()
    } else {
        format!("sqlite:{}?mode=rwc", config.database.path)
    };
    Database::new(&url)
        .await
        .with_context(|| format!("Failed to open the database at {}", config.database.path))
}

pub async fn run_admin(cmd: AdminCommand, config: &AppConfig) -> anyhow::Result<()> {
    let db = open_db(config).await?;
    let svc = CredentialService::new(&db, &config.auth);

    match cmd {
        AdminCommand::SetPassword { username, password } => {
            let password = match password {
                Some(p) => p,
                None => prompt_new_password()?,
            };
            svc.set_password(&username, &password).await?;
            println!("Password updated for '{username}'. All existing sessions were revoked.");
        }
        AdminCommand::CreateUser {
            username,
            admin,
            password,
        } => {
            let password = match password {
                Some(p) => p,
                None => prompt_new_password()?,
            };
            svc.create_user(&username, &password, admin).await?;
            println!(
                "Created {} user '{username}'.",
                if admin { "admin" } else { "standard" }
            );
        }
        AdminCommand::ListUsers => {
            let users = svc.list_users().await?;
            if users.is_empty() {
                println!("No users exist yet.");
                return Ok(());
            }
            println!("{:<28} {:<8} {:<8}", "USERNAME", "ADMIN", "ACTIVE");
            for u in users {
                println!(
                    "{:<28} {:<8} {:<8}",
                    u.username,
                    if u.is_admin { "yes" } else { "no" },
                    if u.is_active { "yes" } else { "no" }
                );
            }
        }
    }
    Ok(())
}
