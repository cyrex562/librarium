# Librarium

**Version 0.100.0** · A self-hosted knowledge base and vault manager for
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
first login. See the [Deployment guide](docs/archive/DEPLOYMENT.md).

### Desktop app

```bash
cargo tauri dev      # from crates/librarium-tauri — dev with auto-reload
cargo tauri build    # release desktop bundle
```

---

## Repository layout

```text
crates/
  librarium-server   Actix Web backend + binary (default workspace member)
  librarium-types    Shared Rust DTOs / contracts
  librarium-client   HTTP + WebSocket client crate
  librarium-tauri    Tauri 2 desktop shell
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
- **[docs/archive/](docs/archive/)** — historical design notes, feature plans,
  and reference specs. Useful for context, but may describe superseded behavior.
  Notable references:
  [Build](docs/archive/BUILD.md) ·
  [Deployment](docs/archive/DEPLOYMENT.md) ·
  [Docker](docs/archive/DOCKER.md) ·
  [Configuration](docs/archive/CONFIGURATION.md) ·
  [API](docs/archive/API.md) ·
  [User Guide](docs/archive/USER_GUIDE.md) ·
  [Plugin API](docs/archive/PLUGIN_API.md)

---

## License

MIT — see [LICENSE](LICENSE).
