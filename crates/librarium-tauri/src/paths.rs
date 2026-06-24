use anyhow::Context;
use librarium::config::LibrariumPaths;

/// Resolve application directories, preferring portable mode when applicable.
///
/// **Portable mode** is active when a `config.toml` file exists in the same
/// directory as the executable. All paths are then resolved relative to that
/// directory, keeping the installation fully self-contained:
///
/// ```text
/// LibrariumDesktop.exe
/// config.toml          ← triggers portable mode
/// data/
///   librarium.db
/// vaults/
/// ```
///
/// **Installed mode** (no exe-adjacent config.toml) falls back to
/// platform-standard directories:
///   Linux (XDG):  ~/.config/librarium/, ~/.local/share/librarium/
///   macOS:        ~/Library/Application Support/librarium/
///   Windows:      %APPDATA%\librarium\
pub fn resolve_paths(handle: &tauri::AppHandle) -> anyhow::Result<LibrariumPaths> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            if exe_dir.join("config.toml").exists() {
                return Ok(LibrariumPaths {
                    config_dir: exe_dir.to_path_buf(),
                    data_dir: exe_dir.join("data"),
                    cache_dir: exe_dir.join("cache"),
                    default_vault_dir: exe_dir.join("vaults"),
                });
            }
        }
    }
    resolve_platform_paths(handle)
}

/// Resolve platform-standard application directories from Tauri's path API.
///
/// Linux (XDG):   config_dir  → ~/.config/librarium/
///                data_dir    → ~/.local/share/librarium/
///                cache_dir   → ~/.cache/librarium/
/// macOS:         ~/Library/Application Support/librarium/ for config & data
/// Windows:       %APPDATA%\librarium\
pub fn resolve_platform_paths(handle: &tauri::AppHandle) -> anyhow::Result<LibrariumPaths> {
    let path = handle.path();

    let config_dir = path
        .app_config_dir()
        .context("Failed to resolve app config dir")?;
    let data_dir = path
        .app_data_dir()
        .context("Failed to resolve app data dir")?;
    let cache_dir = path
        .app_cache_dir()
        .context("Failed to resolve app cache dir")?;

    // Default vault dir: ~/Documents/Librarium — prompted on first launch.
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let default_vault_dir = home.join("Documents").join("Librarium");

    Ok(LibrariumPaths {
        config_dir,
        data_dir,
        cache_dir,
        default_vault_dir,
    })
}

/// Create all required application directories (idempotent).
pub fn create_dirs(paths: &LibrariumPaths) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.config_dir).context("Failed to create config dir")?;
    std::fs::create_dir_all(&paths.data_dir).context("Failed to create data dir")?;
    std::fs::create_dir_all(paths.data_dir.join("plugins"))
        .context("Failed to create plugins dir")?;
    std::fs::create_dir_all(&paths.cache_dir).context("Failed to create cache dir")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_paths(base: &std::path::Path) -> LibrariumPaths {
        LibrariumPaths {
            config_dir: base.join("config"),
            data_dir: base.join("data"),
            cache_dir: base.join("cache"),
            default_vault_dir: base.join("vaults"),
        }
    }

    #[test]
    fn test_create_dirs_creates_expected_subdirectories() {
        let base = TempDir::new().unwrap();
        let paths = make_paths(base.path());

        create_dirs(&paths).unwrap();

        assert!(paths.config_dir.exists(), "config_dir should be created");
        assert!(paths.data_dir.exists(), "data_dir should be created");
        assert!(
            paths.data_dir.join("plugins").exists(),
            "data_dir/plugins should be created"
        );
        assert!(paths.cache_dir.exists(), "cache_dir should be created");
    }

    #[test]
    fn test_create_dirs_is_idempotent() {
        let base = TempDir::new().unwrap();
        let paths = make_paths(base.path());

        create_dirs(&paths).unwrap();
        create_dirs(&paths).unwrap(); // calling twice must not error
    }

    #[test]
    fn test_create_dirs_does_not_touch_default_vault_dir() {
        let base = TempDir::new().unwrap();
        let paths = make_paths(base.path());

        create_dirs(&paths).unwrap();

        // default_vault_dir is NOT created by create_dirs — it is created
        // only during the first-launch flow after user confirmation.
        assert!(
            !paths.default_vault_dir.exists(),
            "default_vault_dir should not be created by create_dirs"
        );
    }
}
