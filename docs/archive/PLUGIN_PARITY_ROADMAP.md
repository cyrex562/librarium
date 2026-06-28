# Plugin Parity Roadmap (Librarium)

This roadmap tracks feature parity for requested Obsidian plugin behaviors in the Vue + Rust app.

## Status Legend

- ✅ Implemented in app
- 🟡 Partial / groundwork exists
- ⏳ Planned / not implemented yet

## Requested Plugin Parity Matrix

| Plugin / Feature | Status | Notes |
|---|---:|---|
| Advanced Tables | 🟡 | Enter adds rows, Tab/Shift+Tab navigates cells — full alignment/insert toolbar still pending. |
| Automatic List Management | ✅ | Enter continues bullets, ordered, task, and roman-numeral lists; Tab/Shift+Tab indents. |
| Copy Button for Code Blocks | ✅ | Added in `MarkdownPreview.vue` (`Copy` / `Copied!` buttons on fenced code blocks). |
| Code Syntax Highlight | ✅ | Backend renderer applies syntax highlighting (`markdown_service.rs`, syntect). |
| Folder Notes | ⏳ | No folder-note resolution convention yet. |
| Iconize | ⏳ | No user-configurable per-file/folder icon metadata yet. |
| Kanban | ⏳ | No `.kanban`/board view editor yet. |
| Mermaid Tools | ✅ | Mermaid fences are rendered as diagrams in preview (`MarkdownPreview.vue`, lazy-loaded `mermaid` library, SVG sanitized via DOMPurify). |
| Mindmap | ⏳ | No mindmap parser/view yet. |
| Neighboring Files | ✅ | `NeighboringFilesPanel.vue` shows previous/next note in tree order. |
| Note Refactor | ⏳ | No heading/block extraction/refactor workflows yet. |
| Ordered List Style | 🟡 | Auto-continuation exists (decimal, alpha, roman); no configurable numbering styles (force roman, etc.). |
| Recent Files | ✅ | `RecentFilesPanel.vue` uses `filesStore.recentFiles`; backend `/recent` endpoint exists. |
| Tag Wrangler | ⏳ | No global rename/merge/delete tag operations yet. |
| Tasks | ⏳ | No dedicated task query panel/filters/scheduling UI yet. |
| Backlinks | ✅ | `BacklinksPanel.vue` calls `apiGetBacklinks()` and renders with open-file navigation. |
| Bases (Dataview-like) | 🟡 | Entity model and metadata query specs exist; full UI/query execution parity pending. |
| Bookmarks | ✅ | `BookmarksPanel.vue` with add/remove actions and open-file navigation. |
| Canvas | ✅ | `CanvasView.vue` — full interactive editor: pan/zoom, drag nodes (text/file/link/group), SVG bezier edges, inline editing, multi-select, resize, auto-save. |
| Footnotes View | 🟡 | Markdown parser supports footnotes; no dedicated footnotes panel/view yet. |
| Graph View | ✅ | `GraphView.vue` — entity relation graph via `/api/vaults/{id}/graph`. |
| Outgoing Links | ✅ | `OutgoingLinksPanel.vue` parses wiki-links and markdown links from current file content. |
| Outline | ✅ | `OutlinePanel.vue` parses headings from current file content with ML outline generation. |
| Properties View | ✅ | Frontmatter properties panel (`FrontmatterPanel.vue`). |
| Tags View | ✅ | `TagsPanel.vue` lists all vault tags with counts via `apiListTags()`. |
| Word Count | 🟡 | Hardcoded word/char count in `EditorPane.vue` status bar; not plugin-extensible yet. |

## Plugin Infrastructure

### What is implemented

- Plugin discovery: scans `plugins/` directory for `manifest.json`
- Capability-based authorization (`PluginCapability` enum; every API call checks the required capability)
- Plugin enable/disable persisted to `.plugins_config.json`
- Plugin config schema support (JSON Schema in manifest; server validates before persisting)
- Dependency resolution with topological sort and cycle detection
- Entity type, relation type, and label registration (worldbuilding plugin)
- `PluginManager.vue` admin UI: list plugins, toggle enable/disable, show schema-based config
- Plugin asset serving (`/api/plugins/{id}/assets/{file}`) with path traversal protection

### Plugin API authorization

| Endpoint | Auth Required | Notes |
|---|---|---|
| `GET /api/plugins` | Any authenticated user | List all plugins and metadata |
| `GET /api/plugins/{id}` | Any authenticated user | Plugin detail + config schema |
| `POST /api/plugins/{id}/toggle` | **Admin only** | Enable or disable a plugin |
| `PUT /api/plugins/{id}/config` | **Admin only** | Replace plugin configuration |
| `GET /api/plugins/{id}/assets/{file}` | Any authenticated user | Serve plugin static assets; path-traversal protected |

This authorization model is intentional: all users can view plugin info and load plugin assets (needed for frontend plugin JS to load), but only admins control what is installed and configured.

### What is not yet implemented

- **Frontend plugin code execution**: `main.js` files are not yet loaded or executed in the browser. All current plugin behavior (backlinks, word-count, daily-notes) is implemented natively in the Vue/Rust app; the plugin JS files are scaffolding for future runtime support.
- **Plugin event bus**: No frontend event system for `on_file_open`, `on_editor_change`, `on_load` hooks yet.
- **Plugin UI extension APIs**: No `addStatusBarItem()`, `registerCommand()`, `addRibbonIcon()`, or custom sidebar panel registration from plugin code.
- **JavaScript runtime**: The backend has a `PluginApi` struct with capability-checked methods but no JS engine (V8 / QuickJS) to execute plugin code server-side.

### Recommended future direction

The cleanest path for frontend plugin execution is a **Web Worker sandbox** per plugin:
- Each enabled plugin's `main.js` is loaded into an isolated `Worker`
- A message-passing bridge exposes the capability-checked API (`readFile`, `writeFile`, `addStatusBarItem`, etc.)
- The worker is terminated on plugin disable or vault switch
- This avoids CSP/iframe complexity and keeps plugin code off the main thread

Server-side execution (via an embedded JS engine) is an option for data-heavy plugins but adds significant dependency weight. The hybrid model (client Worker + server capability check on every API call) is preferred for this offline-first app.

## Remaining Implementation Waves

### Wave 1 — Quick wins remaining

1. Advanced Tables toolbar: column insert/delete, alignment toggle
2. Configurable ordered-list styles in editor toolbar

### Wave 2 — Advanced views

1. Tasks panel with query DSL (checkbox scanning, due-date filters)
2. Bases (Dataview-like entity query views)
3. Kanban board view (`.kanban` files)
4. Mindmap view

### Wave 3 — Plugin runtime

1. Web Worker plugin loader with message-bridge API
2. Plugin event bus (file open, editor change, save)
3. `addStatusBarItem()` — replace hardcoded word count
4. `registerCommand()` / command palette integration
5. Custom sidebar panel registration from plugin code
6. Schema-based config form in `PluginManager.vue`

### Wave 4 — Content operations

1. Tag Wrangler bulk ops (rename/merge/delete across vault)
2. Note Refactor (heading/block extraction)
3. Folder Notes resolution
4. Iconize-like per-file/folder icon metadata
