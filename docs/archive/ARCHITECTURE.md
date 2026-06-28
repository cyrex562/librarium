# Architecture Overview

Librarium is a full-stack application built with Rust (backend) and TypeScript (frontend), managing a local SQLite database for metadata.

## Tech Stack

### Backend
-   **Language**: Rust (Edition 2021)
-   **Web Framework**: Actix Web
-   **Database**: SQLx (SQLite)
-   **Search**: Tantivy (full-text search engine, persisted on disk)
-   **File Watching**: `notify` (Cross-platform filesystem events)
-   **Markdown**: `pulldown-cmark`
-   **Template Engine**: None (API only, frontend is SPA)

### Frontend
-   **Framework**: Vue 3 (Composition API) — single-page application
-   **Language**: TypeScript
-   **Component Library**: Vuetify 3
-   **State Management**: Pinia
-   **Build Tool**: Vite
-   **Editor**: CodeMirror (primary) / CodeJar (lightweight fallback)

## Core Components

### 1. `AppConfig` & `Database`
-   Application configuration is loaded from `config.toml`, ENV variables, and defaults.
-   SQLite stores:
    -   `vaults`: Registered vault paths.
    -   `preferences`: User settings.
    -   `recent_files`: History.

### 2. `FileService`
-   Handles all filesystem I/O.
-   Performs security checks (Path Traversal prevention) using `canonicalize`.
-   Operations: Read, Write, Create (recursive), Delete (move to `.trash`), Move/Rename.

### 3. `SearchIndex`
-   Wraps a Tantivy search engine instance (persisted index on disk).
-   Built on startup by scanning registered vaults.
-   Updated incrementally via file events and API-driven mutations.
-   Provides fast full-text search with snippet highlighting.

### 4. `FileWatcher`
-   Runs in a separate thread.
-   Watches all registered vault paths recursively.
-   Debounces events to prevent floods.
-   Broadcasts events (`Created`, `Modified`, `Deleted`, `Renamed`) via a Tokio broadcast channel.

### 5. `ReindexService`
-   Two-pass entity/relation indexer: first upserts all entity frontmatter, then syncs relations between them.
-   Acts as the single source of truth for entity state (distinct from the full-text `SearchIndex`).
-   Called at startup (full vault scan), on watcher events, and directly by API handlers that mutate files.

### 6. `WebSocketHandler`
-   Accepts WebSocket connections from the frontend.
-   Subscribes to the file event broadcast channel.
-   Pushes updates to clients to trigger UI refreshes (e.g., file tree update, content reload).

## Data Flow

1.  **User Edit**: Frontend sends `PUT /api/files/...`.
2.  **API Handler**: `FileService` writes to disk.
3.  **Filesystem**: OS confirms write.
4.  **Watcher**: Detects `Modify` event.
5.  **Event Loop**:
    -   Updates `SearchIndex`.
    -   Broadcasts event to WebSockets.
6.  **Frontend**: Receives event.
    -   If file is open elsewhere, warns user or updates.
    -   If file tree changed, re-fetches tree.

## Directory Structure

-   `crates/librarium-server/src/`: Backend Rust code
    -   `config/`: Configuration loading (TOML + env vars)
    -   `db/`: SQLite migrations and database layer
    -   `models/`: API and DB structs
    -   `routes/`: Actix Web request handlers
    -   `services/`: Core business logic (File, Search, Reindex, Plugin, …)
    -   `middleware/`: Auth middleware (JWT, API key, vault-role enforcement)
    -   `watcher/`: Filesystem event debouncer
-   `frontend/`: Vue 3 SPA
    -   `src/`: TypeScript source (components, pages, stores, api)
    -   `public/`: Static assets
-   `crates/librarium-tauri/`: Tauri desktop shell
-   `plugins/`: Bundled first-party plugins
