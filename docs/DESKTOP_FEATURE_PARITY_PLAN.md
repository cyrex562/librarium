# Desktop Feature Parity Plan

This document tracks the work needed for the native desktop client (`crates/obsidian-desktop`) to reach practical feature parity with the current web frontend.

## Current state

The desktop client currently provides a functional skeleton with:

- authentication against the server API
- vault loading and selection
- file tree loading
- quick create-note and create-folder actions
- rename / move actions with open-tab path remapping
- delete-to-trash flow with explicit confirmation
- recent files tracking with quick reopen actions
- selected-file tracking and note metadata header
- basic open/switch/close tab navigation
- dirty-state tracking and a status footer
- manual note open/edit/save
- frontmatter editing with JSON validation and round-trip save support
- markdown toolbar actions (heading, emphasis, list, quote, code)
- editor mode switching (raw / formatted / preview)
- rendered markdown preview in formatted/preview modes
- conflict-aware save flow with reload/force-save resolution panel
- template insertion flow from `Templates/*.md` with variable substitution and append/replace modes
- full-text search panel with paged results and open-result actions
- quick switcher panel with fuzzy path matching and one-click reopen
- outline panel with section list and refresh actions
- ML-backed outline generation integrated into the outline panel
- outgoing links, backlinks, and neighboring files panels for active note context
- bookmarks and tags panels with quick open actions
- ML suggestion inbox panel with confidence/rationale display
- ML suggestion dry-run/apply actions with undo-last workflow
- rollback refresh wiring and optimistic ML action status messaging
- preferences window with remote load/save/reset actions
- local session snapshot restore for active vault, tabs, and editor layout
- reconnecting WebSocket sync loop with footer status and retry backoff
- practical desktop keyboard shortcuts for save/search/switch/mode/sync actions
- client deployment seams for standalone, cloud, and hybrid desktop modes
- plugin manager panel with enable/disable toggle per plugin and live reload
- modularized desktop state/UI structure with reusable view sections
- tree refresh and open-tab cleanup after path mutations

## Web frontend feature inventory

The current web frontend includes these major surfaces:

### Core shell

- vault picker and vault management
- sidebar navigation and actions
- split-pane tabbed workspace
- persistent editor/preferences state

### File operations

- hierarchical file tree
- create file / create folder
- rename / move / delete
- drag-and-drop tree operations
- import dialog and archive ingestion
- recent files
- random note / daily note helpers

### Editing experience

- markdown editor with toolbar
- frontmatter editing
- rendered preview / formatted modes
- conflict resolution flow
- templates
- media viewers (image, PDF, audio/video)

### Discovery and navigation

- full-text search modal
- quick switcher
- outgoing links, backlinks, neighboring files
- outline panel
- tags panel
- bookmarks panel

### ML / smart features

- outline generation
- organization suggestions
- dry run / apply / undo workflow

### Platform and extensibility

- preferences UI
- plugin manager
- WebSocket-driven sync events

## Native desktop parity backlog

### Milestone 1 — desktop usability baseline

- [x] Replace manual note-path workflow with interactive file tree selection
- [x] Add selected-file state and file details header
- [x] Add basic tab model (open, switch, close)
- [x] Split editor and preview into reusable desktop modules
- [x] Add save state / dirty indicator / status footer

### Milestone 2 — core file management parity

- [x] Create file and create folder actions
- [x] Rename / move files and folders
- [x] Delete with confirmation
- [x] Refresh tree after mutations and remap open tabs
- [x] Add recent files and quick reopen affordances

### Milestone 3 — editing parity

- [x] Add frontmatter panel
- [x] Add markdown toolbar actions
- [x] Add editor mode switching (raw / formatted / preview)
- [x] Render proper markdown preview instead of plain text mirror
- [x] Add conflict resolution UX
- [x] Add template insertion flow

### Milestone 4 — search and navigation parity

- [x] Add full-text search UI
- [x] Add quick switcher UI
- [x] Add outline panel
- [x] Add outgoing links / backlinks / neighboring files panels
- [x] Add bookmarks and tags panels

### Milestone 5 — smart features parity

- [x] Add ML outline generation UI
- [x] Add ML suggestion inbox
- [x] Add dry run / apply / undo UX
- [x] Add rollback state refresh and optimistic messaging

### Milestone 6 — desktop-native polish

- [x] Add preferences window
- [x] Persist session state locally (active vault, tabs, layout)
- [x] Improve event sync loop and reconnect behavior
- [x] Add keyboard shortcuts matching web behavior where practical
- [x] Introduce standalone / cloud / hybrid mode architecture seams

### Milestone 7 — stretch parity

- [x] Plugin manager and plugin surface compatibility review
- [x] Import/export workflows
- [x] Media viewer parity (image, PDF, audio/video)
- [x] Daily note / random note helpers
- [x] Release hardening, feature flags, telemetry/logging hooks

## Recommended implementation order

1. interactive file tree selection
2. tab model + selected note state
3. tree mutation commands (create/rename/delete)
4. real markdown preview + editor modes
5. search + quick switcher
6. ML panels
7. preferences + persistence + sync hardening

## Current status (LIB-029)

All Milestones 1–7 complete. Tauri stabilization sprint finished:

- Persisted server URL, username, and deployment mode across restarts
- Deployment mode picker (Cloud / Standalone / Hybrid) with hybrid local mirror URL input
- Feature flags now gate ML panels (outline + suggestions) and media preview in the UI
- Vault creation UI (name input + "+ Vault" button in sidebar)
- System tray with traffic-light status icons (yellow/green/red for starting/healthy/error)
- Deep-link handler for `librarium://` URLs
- Health-polling loop with user-readable error screen on timeout or port conflict
- `classify_server_error` covering EADDRINUSE, permission-denied, and generic cases

### Packaging stabilization (LIB-029)

- [x] Enable Tauri bundler (`bundle.active = true`) targeting deb, AppImage, NSIS, dmg
- [x] Add placeholder 32×32 and 128×128 PNG app icons to satisfy bundler requirements
      (replace `icons/icon-32x32.png` and `icons/icon-128x128.png` with a proper
      branded icon set before a public release)
- [x] Declare Linux `.deb` runtime dependencies: `libwebkit2gtk-4.1-0`, `libgtk-3-0`,
      `libayatana-appindicator3-1`
- [x] Fix `capabilities/default.json` `$schema` to use the public Tauri schema URL
      instead of a local `node_modules/` path that doesn't exist in fresh checkouts

### Remaining before public release

- [ ] Replace placeholder icons with a branded icon set (32, 128, 256 px PNG + ICO/ICNS)
- [ ] Test `.deb` install on Ubuntu 22.04 and 24.04
- [ ] Test AppImage on Fedora 39+ and Arch
- [ ] Verify `librarium://` deep-link registration survives package manager install/upgrade
- [ ] Confirm `libwebkit2gtk-4.1-0` `.deb` dependency resolves on Ubuntu 22.04
      (may need `libwebkit2gtk-4.0-0` as a fallback for the older GTK stack)
