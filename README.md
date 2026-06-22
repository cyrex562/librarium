# Librarium

A self-hosted knowledge base and vault manager for Obsidian-compatible vaults.

## Features

- **Multi-vault support**: Manage multiple Obsidian vaults from a single interface
- **File management**: Browse, create, edit, and delete files and folders
- **Real-time sync**: Two-way synchronization between filesystem and web UI using file watching
- **Conflict resolution**: Automatic conflict detection with backup file creation
- **Full-text search**: Fast search across all markdown files powered by Tantivy
- **Multiple editor modes**:
  - Raw markdown editor
  - Side-by-side (editor + live preview)
  - Formatted raw (with syntax highlighting)
  - Fully rendered view
- **Obsidian syntax support**: Wiki links, embeds, tags, and frontmatter
- **Split view**: Work with multiple files simultaneously in split panes
- **Tab management**: Open multiple files with tab interface
- **Dark/Light themes**: Toggle between themes for comfortable viewing
- **Authentication**: Password, LDAP, and OIDC login; TOTP two-factor auth; API keys
- **Multi-user**: Role-based vault access (Owner / Editor / Viewer), groups, sharing, and invitations
- **Plugin system**: Bundled plugins for backlinks, daily notes, word count, and more

## Tech Stack

### Backend
- **Rust** with Actix Web framework
- **SQLite** (via SQLx) for metadata storage
- **Tantivy** for full-text search
- **notify** + **notify-debouncer-full** for filesystem watching
- **pulldown-cmark** for Markdown parsing

### Frontend
- **Vue 3** (Composition API) single-page application
- **TypeScript** for type-safe client code
- **Vuetify** component library
- **Pinia** for state management
- **WebSocket** for real-time file change notifications

### Desktop
- **Tauri 2** shell (optional) — embeds the server in a native desktop binary

## Documentation

-   [User Guide](docs/USER_GUIDE.md) - How to use the interface.
-   [Installation & Build](docs/BUILD.md) - Building from source and cross-compilation.
-   [Docker Deployment](docs/DOCKER.md) - Running with Docker.
-   [Deployment](docs/DEPLOYMENT.md) - Production setup, auth bootstrapping, TLS.
-   [API Reference](docs/API.md) - API endpoints documentation.
-   [Architecture](docs/ARCHITECTURE.md) - System overview.
-   [Configuration](docs/CONFIGURATION.md) - Config file and env vars.

## Quick Start
1.  **Download** the latest release or build from source (see [Build Guide](docs/BUILD.md)).
2.  **Configure** auth credentials (see [Deployment Guide](docs/DEPLOYMENT.md)).
3.  **Run** the binary or Docker container.
4.  **Open** `http://localhost:8080`.

## How It Works

- File watching with `notify` monitors vault changes (500 ms debounce)
- WebSocket broadcasts updates to connected clients
- Tantivy search index and entity/relation state are kept in sync by a dedicated reindex service
- Automatic conflict resolution with backup file creation
- Path traversal protection on all filesystem operations

## License

MIT
