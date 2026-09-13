//! Cross-platform build + deploy automation for Librarium.
//!
//! Run via the cargo alias defined in `.cargo/config.toml`:
//!
//!   cargo xtask build-desktop           # release build of the desktop app
//!   cargo xtask build-desktop --debug   # faster, unoptimized build
//!   cargo xtask build-frontend          # just (re)build the Vue SPA
//!   cargo xtask run-desktop [--debug]   # build + launch the desktop app
//!   cargo xtask build-installer         # build a distributable installer (NSIS on Windows)
//!   cargo xtask deploy [TARGET] [...]   # deploy the server to a remote target
//!   cargo xtask status [TARGET]         # check a target's health/version
//!   cargo xtask logs   [TARGET]         # stream a target's logs
//!   cargo xtask doctor [TARGET]         # preflight a target
//!   cargo xtask update [...]            # idempotently update a *local* checkout in place
//!   cargo xtask bump-version [kind]      # bump the app version everywhere it's recorded
//!
//! The build commands are self-contained. The deploy/status/logs/doctor/update
//! commands are thin pass-throughs to `scripts/librarium.py`, so this crate is
//! the single entry point for the whole build→deploy→observe loop (build
//! commands need Node+Rust; the ops commands additionally need Python 3 with the
//! script's deps installed).
//!
//! `bump-version` should run before every push to a PR or main (see
//! CLAUDE.md's "Documentation & versioning" section) — it's how the version
//! shown in the app's Settings → About panel actually increments.
//!
//! `deploy` manages a *remote* target over SSH from a separate machine.
//! `update` is its local counterpart: run it ON the box hosting the server
//! after `git clone`ing this repo there — it pulls, rebuilds, reinstalls, and
//! restarts the systemd service in one idempotent step.
//!
//! The desktop app embeds the server, which embeds the Vue SPA from
//! `target/frontend/` via rust-embed — so the frontend MUST be built before the
//! Rust build. `build-desktop`/`run-desktop` handle that ordering for you.
//!
//! Prerequisites for the build commands: Rust (rustup), Node.js + npm, and the
//! platform's Tauri/WebView deps. On Windows that means the WebView2 runtime
//! (preinstalled on Win11 and most Win10); on Linux the webkit2gtk-4.1 / gtk3
//! dev packages.

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let release = !args.iter().any(|a| a == "--debug");

    match cmd {
        "build-frontend" => build_frontend(),
        "build-desktop" => {
            build_frontend();
            build_desktop(release);
        }
        "run-desktop" => {
            build_frontend();
            run_desktop(release);
        }
        "build-installer" => {
            build_frontend();
            build_installer();
        }
        "bump-version" => bump_version(&args),
        // Ops commands: forward verbatim (including the command name and any
        // target/flags) to the deployment CLI.
        "deploy" | "status" | "logs" | "doctor" | "targets" | "update" | "local-install" => {
            librarium_cli(&args)
        }
        "help" | "-h" | "--help" => help(),
        other => {
            eprintln!("unknown command: {other}\n");
            help();
            exit(2);
        }
    }
}

fn help() {
    println!(
        "Librarium tasks:\n\
         \n  Build:\
         \n    cargo xtask build-desktop [--debug]   Build the desktop app (frontend + Tauri)\
         \n    cargo xtask run-desktop   [--debug]   Build then launch the desktop app\
         \n    cargo xtask build-frontend            Build only the Vue SPA into target/frontend\
         \n    cargo xtask build-installer           Build a distributable installer (NSIS on Windows)\
         \n                                          Re-running the installer over an existing install\
         \n                                          upgrades it in place (Tauri/NSIS default behavior) —\
         \n                                          no uninstall step needed after a code change.\
         \n                                          Needs the Tauri CLI: cargo install tauri-cli --version '^2' --locked\
         \n    cargo xtask bump-version [kind]       Bump the app version everywhere it's recorded\
         \n                                          (default kind: patch; also accepts minor, major).\
         \n                                          Run this before every push to a PR or main —\
         \n                                          it's how the version shown in the app's\
         \n                                          Settings -> About panel increments. Does not\
         \n                                          commit; review `git diff` and commit yourself.\
         \n  Deploy / observe (via scripts/librarium.py; needs: pip install -r scripts/requirements.txt):\
         \n    cargo xtask deploy [TARGET] [flags]   Deploy the server to a remote target over SSH\
         \n    cargo xtask status [TARGET]           Show a target's running version/health\
         \n    cargo xtask logs   [TARGET]           Stream a target's logs\
         \n    cargo xtask doctor [TARGET]           Preflight a target\
         \n    cargo xtask targets                   List configured deploy targets\
         \n    cargo xtask local-install [flags]     Build + install the server locally (this box)\
         \n    cargo xtask update [flags]            Idempotently update a local checkout in place:\
         \n                                          git pull, rebuild, reinstall, restart the\
         \n                                          systemd service if one's installed. Run this ON\
         \n                                          the box hosting the server after `git clone`ing\
         \n                                          this repo there — the local counterpart to\
         \n                                          `deploy`. Pass -h after any of these for details,\
         \n                                          e.g. `cargo xtask update -h`.\
         \n\nBuild profile defaults to release; pass --debug for a faster, unoptimized build.\
         \nTARGET names come from targets.toml (omit to be prompted)."
    );
}

/// Workspace root, derived from this crate's location (xtask/ lives at the root).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

fn build_frontend() {
    let frontend = repo_root().join("frontend");
    // Install deps on first run (or after a clean checkout).
    if !frontend.join("node_modules").exists() {
        eprintln!("→ frontend/node_modules missing — running npm ci");
        npm(&frontend, &["ci"]);
    }
    eprintln!("→ building frontend (vue-tsc + vite) into target/frontend");
    npm(&frontend, &["run", "build"]);
}

fn build_desktop(release: bool) {
    let root = repo_root();
    let mut args = vec!["build", "-p", "librarium-tauri"];
    if release {
        args.push("--release");
    }
    eprintln!("→ cargo {}", args.join(" "));
    run(
        Command::new("cargo").args(&args).current_dir(&root),
        "cargo build",
    );

    let profile = if release { "release" } else { "debug" };
    let bin = if cfg!(windows) {
        "librarium-tauri.exe"
    } else {
        "librarium-tauri"
    };
    println!("\n✓ Desktop build ready: target/{profile}/{bin}");
    println!(
        "  Run it directly, or for a distributable installer use the Tauri CLI\n\
         (cargo install tauri-cli && cargo tauri build) which emits an NSIS \
         installer on Windows."
    );
}

/// Build a distributable installer via the Tauri CLI — an NSIS `.exe` on
/// Windows (`cargo tauri build --bundles nsis`, matching exactly what
/// `.github/workflows/release.yml`'s `desktop-windows` job runs), or the
/// platform's default bundle(s) elsewhere (`cargo tauri build` with no
/// `--bundles` filter lets Tauri pick whichever of `tauri.conf.json`'s
/// `bundle.targets` apply to the host OS — e.g. deb+appimage on Linux, dmg
/// on macOS).
///
/// Tauri's NSIS installer is idempotent by design: running the same
/// installer again over an already-installed copy detects it (via the
/// registry uninstall entry it wrote) and upgrades in place — replaces the
/// binary, keeps user data untouched. That's the whole point of this
/// command: build once after each code change, re-run the resulting
/// installer to patch the installed copy, no custom update logic needed.
fn build_installer() {
    let root = repo_root();
    let tauri_dir = root.join("crates").join("librarium-tauri");

    if !has_tauri_cli() {
        eprintln!(
            "✗ Tauri CLI not found. Install it first:\n\
             \n    cargo install tauri-cli --version '^2' --locked\n"
        );
        exit(1);
    }

    let mut args = vec!["tauri", "build"];
    if cfg!(windows) {
        args.push("--bundles");
        args.push("nsis");
    }
    eprintln!("→ cargo {} (in crates/librarium-tauri)", args.join(" "));
    run(
        Command::new("cargo").args(&args).current_dir(&tauri_dir),
        "cargo tauri build",
    );

    let bundle_dir = root.join("target").join("release").join("bundle");
    println!("\n✓ Installer built under {}", bundle_dir.display());
    if cfg!(windows) {
        println!(
            "  Look under bundle\\nsis\\ for the .exe setup file. Re-run it any time to\n\
             \x20 upgrade an existing install in place — no need to uninstall first."
        );
    }
}

/// Whether `cargo tauri` (the Tauri CLI subcommand) is available.
fn has_tauri_cli() -> bool {
    Command::new("cargo")
        .args(["tauri", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_desktop(release: bool) {
    let root = repo_root();
    let mut args = vec!["run", "-p", "librarium-tauri"];
    if release {
        args.push("--release");
    }
    eprintln!("→ cargo {}", args.join(" "));
    run(
        Command::new("cargo").args(&args).current_dir(&root),
        "cargo run",
    );
}

/// Forward a command to the deployment CLI (`scripts/librarium.py`), passing the
/// xtask args through verbatim so `cargo xtask deploy librarium-01 --skip-backup`
/// becomes `python scripts/librarium.py deploy librarium-01 --skip-backup`.
fn librarium_cli(args: &[String]) {
    let root = repo_root();
    let script = root.join("scripts").join("librarium.py");
    if !script.exists() {
        eprintln!("✗ deployment CLI not found at {}", script.display());
        exit(1);
    }
    let mut cmd = Command::new(python_exe());
    cmd.arg(&script).args(args).current_dir(&root);
    run(&mut cmd, "librarium.py");
}

/// Pick a Python interpreter: prefer `python3`, fall back to `python` (Windows).
fn python_exe() -> &'static str {
    for cand in ["python3", "python"] {
        let ok = Command::new(cand)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return cand;
        }
    }
    // Neither found; return the common default so `run` surfaces a clear error.
    "python3"
}

// ── bump-version ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum BumpKind {
    Major,
    Minor,
    Patch,
}

/// Every crate whose `Cargo.toml` `version` is meant to track the app
/// version (per CLAUDE.md's "Documentation & versioning" section).
const VERSIONED_CARGO_TOMLS: &[&str] = &[
    "crates/librarium-server/Cargo.toml",
    "crates/librarium-tauri/Cargo.toml",
    "crates/librarium-core/Cargo.toml",
    "crates/librarium-types/Cargo.toml",
    "crates/librarium-client/Cargo.toml",
    "crates/librarium-mobile/Cargo.toml",
    "crates/librarium-sync/Cargo.toml",
];

fn bump_version(args: &[String]) {
    let kind = match args.get(1).map(String::as_str) {
        Some("major") => BumpKind::Major,
        Some("minor") => BumpKind::Minor,
        Some("patch") | None => BumpKind::Patch,
        Some(other) => {
            eprintln!("unknown bump kind '{other}' (expected patch, minor, or major)");
            exit(2);
        }
    };

    let root = repo_root();
    let canonical = root.join(VERSIONED_CARGO_TOMLS[0]);
    let old_version = read_cargo_toml_version(&canonical);
    let new_version = bump_semver(&old_version, kind);

    eprintln!("→ bumping version {old_version} → {new_version}");

    let mut touched = Vec::new();

    for rel in VERSIONED_CARGO_TOMLS {
        let path = root.join(rel);
        replace_first(
            &path,
            &format!("version = \"{old_version}\""),
            &format!("version = \"{new_version}\""),
        );
        touched.push((*rel).to_string());
    }

    for rel in [
        "frontend/package.json",
        "crates/librarium-tauri/tauri.conf.json",
    ] {
        let path = root.join(rel);
        replace_first(
            &path,
            &format!("\"version\": \"{old_version}\""),
            &format!("\"version\": \"{new_version}\""),
        );
        touched.push(rel.to_string());
    }

    for (rel, old_snippet, new_snippet) in [
        (
            "README.md",
            format!("**Version {old_version}**"),
            format!("**Version {new_version}**"),
        ),
        (
            "docs/DESIGN.md",
            format!("**Version:** {old_version}"),
            format!("**Version:** {new_version}"),
        ),
        (
            "CLAUDE.md",
            format!("Current version: **{old_version}**"),
            format!("Current version: **{new_version}**"),
        ),
    ] {
        let path = root.join(rel);
        replace_first(&path, &old_snippet, &new_snippet);
        touched.push(rel.to_string());
    }

    eprintln!("→ npm install --package-lock-only (refresh frontend/package-lock.json)");
    npm(&root.join("frontend"), &["install", "--package-lock-only"]);

    eprintln!("→ cargo check --workspace -q (refresh Cargo.lock)");
    run(
        Command::new("cargo")
            .args(["check", "--workspace", "-q"])
            .current_dir(&root),
        "cargo check",
    );

    println!(
        "\n✓ Bumped {old_version} → {new_version} across {} files (+ Cargo.lock, package-lock.json):",
        touched.len()
    );
    for t in &touched {
        println!("    {t}");
    }
    println!("\nDoes not commit — review the diff (`git diff`) and commit it yourself.");
}

fn bump_semver(version: &str, kind: BumpKind) -> String {
    let parts: Vec<u64> = version
        .split('.')
        .map(|p| {
            p.parse().unwrap_or_else(|_| {
                eprintln!("✗ expected a numeric version component, got {version:?}");
                exit(1);
            })
        })
        .collect();
    let (major, minor, patch) = match parts.as_slice() {
        [a, b, c] => (*a, *b, *c),
        _ => {
            eprintln!("✗ expected a MAJOR.MINOR.PATCH version, got {version:?}");
            exit(1);
        }
    };
    match kind {
        BumpKind::Major => format!("{}.0.0", major + 1),
        BumpKind::Minor => format!("{major}.{}.0", minor + 1),
        BumpKind::Patch => format!("{major}.{minor}.{}", patch + 1),
    }
}

/// Read the `[package]` table's own `version = "..."` line — the first line
/// in the file that starts with `version = "`, which for a crate's own
/// Cargo.toml is always this field (dependency version constraints use a
/// different syntax, and always appear later in the file, after `[package]`).
fn read_cargo_toml_version(path: &Path) -> String {
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("✗ could not read {}: {e}", path.display());
        exit(1);
    });
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("version = \"") {
            if let Some(end) = rest.find('"') {
                return rest[..end].to_string();
            }
        }
    }
    eprintln!("✗ no `version = \"...\"` line found in {}", path.display());
    exit(1);
}

/// Replace the first occurrence of `needle` with `replacement` in the file at
/// `path`. Fails loudly (rather than silently no-op-ing) if `needle` isn't
/// found — a bump-version run should never report success while quietly
/// skipping a file that was supposed to be updated.
fn replace_first(path: &Path, needle: &str, replacement: &str) {
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("✗ could not read {}: {e}", path.display());
        exit(1);
    });
    if !content.contains(needle) {
        eprintln!("✗ expected to find {needle:?} in {}", path.display());
        exit(1);
    }
    let new_content = content.replacen(needle, replacement, 1);
    std::fs::write(path, new_content).unwrap_or_else(|e| {
        eprintln!("✗ could not write {}: {e}", path.display());
        exit(1);
    });
}

/// Invoke npm portably. On Windows `npm` is a `.cmd` shim that `Command` won't
/// resolve directly, so route it through `cmd /C`.
fn npm(dir: &Path, args: &[&str]) {
    if cfg!(windows) {
        let line = format!("npm {}", args.join(" "));
        run(
            Command::new("cmd").arg("/C").arg(line).current_dir(dir),
            "npm",
        );
    } else {
        run(Command::new("npm").args(args).current_dir(dir), "npm");
    }
}

fn run(cmd: &mut Command, label: &str) {
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("✗ {label} failed with {status}");
            exit(status.code().unwrap_or(1));
        }
        Err(e) => {
            eprintln!("✗ could not launch {label}: {e}");
            exit(1);
        }
    }
}
