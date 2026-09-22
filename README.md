# Librarium

**Version 0.102.12** · A self-hosted knowledge base and vault manager for
Obsidian-compatible Markdown vaults.

Librarium keeps your notes as plain Markdown files on disk — they stay portable
and tool-agnostic — and layers a fast multi-user web app (and an optional native
desktop app) on top. Search index, entity graph, and metadata are all derived
state that can be rebuilt from the files at any time.

> **New here?** Start with the [Design & Architecture document](docs/DESIGN.md)
> for how the system fits together.

---

## Features

- **Multi-vault** — manage multiple Obsidian vaults from one interface.
- **File management** — browse, create, edit, move, and delete files and folders.
- **Live sync** — two-way sync between the filesystem and the UI via file
  watching; external edits (git, other editors, sync tools) flow into the app.
- **Conflict handling** — automatic conflict detection with backups.
- **Full-text search** — fast search across Markdown powered by Tantivy.
- **Multiple editor modes** — raw Markdown, side-by-side preview, formatted, and
  fully rendered views (Tiptap + CodeJar).
- **Obsidian syntax** — wiki links, embeds, tags, frontmatter.
- **Tabs & split view** — work with multiple files at once.
- **Entities & relations** — user-defined typed entities and a relation graph,
  driven from frontmatter.
- **Auth & multi-user** — password / LDAP / OIDC login, TOTP 2FA, API keys, and
  per-vault Owner / Editor / Viewer roles with groups, sharing, and invitations.
- **Plugins** — capability-gated JavaScript plugins (backlinks, daily notes,
  word count, worldbuilding, and more).
- **Local organization (ML)** — offline keyphrase extraction and optional
  embeddings; no data leaves the machine.
- **Desktop app** — optional Tauri 2 shell that runs the whole stack locally.

---

## Tech stack

| Layer | Technology |
| --- | --- |
| Backend | Rust · Actix Web · Tokio |
| Storage | Markdown files on disk (source of truth) · SQLite (SQLx) for metadata · Tantivy for full-text search |
| File watching | `notify` + debouncer (500 ms) |
| Markdown | `pulldown-cmark` |
| Frontend | Vue 3 (Composition API) · TypeScript · Vuetify 3 · Pinia · Vite |
| Realtime | WebSocket file-change notifications |
| Desktop | Tauri 2 (embeds the server on `127.0.0.1`) |

A single binary serves the API and the embedded frontend. Details and diagrams
are in [docs/DESIGN.md](docs/DESIGN.md).

---

## Quick start

```bash
# 1. Build the frontend (embedded into the server binary)
npm --prefix frontend install
npm --prefix frontend run build

# 2. Build and run the server
cargo run -p librarium-server

# 3. Open the app
#    http://localhost:8080
```

On first run with auth enabled, Librarium bootstraps an admin account and writes
the generated credentials next to the database, then forces a password change at
first login. See the [Deployment guide](docs/DEPLOYMENT.md).

### Forgot your password?

No email or network is involved — recovery is local, and works even when the
server will not start.

- **Desktop app:** choose **Forgot password?** on the sign-in screen. Anyone who
  can use the computer can already open the vault files directly, so no old
  password is needed.
- **Server:** run this on the machine hosting Librarium:

  ```bash
  librarium admin set-password <username>
  ```

  It prompts for the new password (no echo), works with the server stopped, and
  revokes every existing session. `librarium admin list-users` and
  `librarium admin create-user <username> [--admin]` are also available.

Changing a password signs out that user's other sessions. It does **not** revoke
API keys — those are separate credentials with their own list in
**Settings → API Keys**; revoke them there if a key may have been exposed.

**Running the server on its own box (a VM, a home server)?** Clone the repo
there and install with `cargo xtask local-install` (or the equivalent
`python scripts/librarium.py local-install`) — builds the binary and installs
it to `~/.local/bin`, writing a starter `config.toml` to `~/.config/librarium`
without touching one that's already there. To keep it current afterward, run
`cargo xtask update` on that same box any time: it pulls the latest commits
(refusing outright if the working tree is dirty, never discarding local
changes), rebuilds, reinstalls, and restarts the systemd service if you've set
one up (`deploy/systemd/librarium.service.template`) — safe to rerun anytime,
and it never touches your database or overwrites an existing config. This is
the local counterpart to `cargo xtask deploy`, which manages a *remote* target
over SSH from a separate machine instead.

If the box was instead set up via `cargo xtask deploy` against a
`targets.toml` entry with `deployment = "docker"` (its `app_dir`, e.g.
`/opt/librarium`, is where that install actually lives — not
`~/.local/bin`), point `update` at that same target definition instead:
`cargo xtask update --target <name>` pulls, rebuilds the Docker image, and
runs `docker compose up -d --build` directly in `app_dir`, in place — the
exact same layout and compose file `deploy` manages remotely, just run
locally instead of over SSH.

### Desktop app

```bash
cargo tauri dev      # from crates/librarium-tauri — dev with auto-reload
cargo tauri build    # release desktop bundle (installer)
```

#### Windows (via `cargo xtask`)

`cargo xtask` (the `xtask/` crate, aliased in `.cargo/config.toml`) wraps
the frontend + Tauri build into one command and works the same way on
Windows, WSL, or Linux/macOS. Prerequisites: [Rust](https://rustup.rs) and
[Node.js](https://nodejs.org); Windows 11 (and most Windows 10 installs)
already has the WebView2 runtime Tauri needs — if not, get it from
[Microsoft's WebView2 page](https://developer.microsoft.com/microsoft-edge/webview2/).

From a PowerShell or Command Prompt at the repo root:

```powershell
cargo xtask build-desktop            # release build -> target\release\librarium-tauri.exe
cargo xtask build-desktop --debug    # faster, unoptimized build
cargo xtask run-desktop              # build, then launch it directly
```

This produces a runnable binary, not an installer — good for local testing
without installing anything.

For a distributable, re-runnable installer (NSIS `.exe` setup on Windows),
use `cargo xtask build-installer` instead (needs `cargo install tauri-cli
--version '^2' --locked` first). The resulting installer lands under
`target\release\bundle\nsis\`. Tauri's NSIS installer is idempotent by
design: re-running it over an existing install detects it and upgrades in
place (replaces the binary, keeps your vaults/config untouched) — so the
day-to-day loop after pulling new code is just
`cargo xtask build-installer` followed by running the installer it
produces, no need to uninstall first.

`cargo xtask` has other subcommands too — run `cargo xtask help` (or see
`xtask/src/main.rs`'s module doc) for `build-frontend`, `deploy`/`status`/
`logs`/`doctor` for managing a *remote* server target over SSH, and
`local-install`/`update` for installing/updating the server locally on the
box that's running it (see [Quick start](#quick-start) above).

### Android app

Librarium's Android app is a thin sync client — it doesn't embed a server;
pair it with a Librarium server you already run (see [Quick start](#quick-start)
above). Distribution is sideload-only for now (see
[docs/DESIGN.md](docs/DESIGN.md) for why): download the latest
`Librarium-*-android-universal.apk` from the
[Releases page](https://github.com/cyrex562/librarium/releases), then on
your device:

1. Open the downloaded APK (from your browser's downloads or a file
   manager). Android will prompt to allow installs from that app if it's
   the first time — allow it, install, then open Librarium.
2. On first launch, pair with your Librarium server: enter its URL (e.g.
   `https://librarium.example.com`) and an API key from that server's
   admin settings.
3. Map a local vault to a vault on the server to start syncing.

Building it yourself: see `AGENTS.md`'s "Android build" and "Android
release signing" sections.

---

## Repository layout

```text
crates/
  librarium-server   Actix Web backend + binary (default workspace member)
  librarium-core     Platform-independent core (errors, FileService, frontmatter, Markdown render, Tantivy search); no actix/sqlx/tokio by default
  librarium-types    Shared Rust DTOs / contracts
  librarium-client   HTTP + WebSocket client crate
  librarium-tauri    Tauri 2 desktop shell
  librarium-mobile   Route C thin-client command layer (vault, file, render, links, tags, frontmatter, on-device search, local metadata, sync bridge, secure remote pairing); no frontend wiring yet
frontend/            Vue 3 + TypeScript + Vuetify SPA
plugins/             Bundled first-party plugins
benches/  tests/  scripts/  docs/
```

---

## Development

```bash
cargo check --workspace
cargo test -p librarium-server        # backend tests
cargo test --workspace                # all Rust tests
npm --prefix frontend test            # Vitest unit tests
npm --prefix frontend run test:e2e    # Playwright E2E
```

Contributor conventions live in [AGENTS.md](AGENTS.md) and
[CLAUDE.md](CLAUDE.md).

---

## Documentation

- **[Design & Architecture](docs/DESIGN.md)** — the canonical, current system
  overview. Kept up to date alongside this README.
- [Build](docs/BUILD.md) ·
  [Deployment](docs/DEPLOYMENT.md) ·
  [Configuration](docs/CONFIGURATION.md) ·
  [API](docs/API.md) ·
  [User Guide](docs/USER_GUIDE.md) ·
  [WebKitGTK compatibility](docs/WEBKITGTK_COMPAT.md)
- [Contributing](CONTRIBUTING.md) · [Security policy](SECURITY.md) ·
  [Changelog](CHANGELOG.md)
- **[docs/archive/](docs/archive/)** — historical design notes and feature
  plans. May describe superseded behavior; not kept up to date.

---

## License

MIT — see [LICENSE](LICENSE).
