# User guide

Librarium hosts and edits your Obsidian-compatible vaults through a web
interface (or the desktop app, which embeds the same interface).

## Getting started

1. **Launch**: run the `librarium` binary, start the Docker container, or
   open the desktop app.
2. **Open the web UI**: `http://localhost:8080` (or your configured port).
3. **Add a vault**: open the vault switcher (top left) → "Manage Vaults" →
   give it a name and, optionally, a directory path (leave it blank and
   Librarium creates one for you).
4. **Select the vault** from the switcher to open it.

## Interface

- **Sidebar** (left): the file tree for the current vault. Resizable and
  collapsible, with Favorites, Bookmarks, Recent Files, and Tags panels
  beneath it.
- **Main area**: tabbed editor/preview, one tab per open file.
- **Top bar**: search, plugins, theme toggle, settings, vault switcher.

## Working with files

### Navigation

- Click a file in the sidebar to open it; click a folder to expand or
  collapse it (folders start collapsed).
- **Quick switcher** (`Ctrl/Cmd+P` or `Ctrl/Cmd+K`) — jump to a file by name.
- **Search** (`Ctrl/Cmd+Shift+F`) — full-text search across the vault, with
  match highlights; click a result to open the file at that point.

### Editing

Four modes, switchable from the toolbar or `Ctrl/Cmd+1`/`2`/`3`:

- **Plain** (`1`) — raw Markdown source, no formatting.
- **Formatted** (`2`) — Markdown source with inline styling (headings, bold,
  tables, etc. rendered in place) while still editing plain text.
- **Preview** (`3`) — fully rendered, read-only.
- **Structural** — a form-style editor for structured entities (see
  Worldbuilding/entities below), toggled from the mode selector rather than
  a number shortcut.

Changes save automatically. `Ctrl/Cmd+S` saves immediately rather than
waiting for the debounce. Type `[[` for wiki-link autocomplete. Drag and drop
an image into the editor to upload and embed it.

### File operations

- **Create**: right-click a folder → New File / New Folder (or the sidebar's
  toolbar buttons).
- **Rename**: right-click a file → Rename (opens a dialog).
- **Delete**: right-click a file → Delete — moves it to the vault's
  `.trash/` folder rather than deleting it outright.
- **Upload**: drag files into the sidebar, or use the upload button.

### Supported file types

| Type | Extensions | Support |
| --- | --- | --- |
| Markdown | `.md` | Full editing |
| Images | `.png .jpg .jpeg .gif .svg .webp` | Viewer with zoom/pan |
| PDF | `.pdf` | Native viewer, search, metadata |
| Audio | `.mp3 .wav .ogg` | Playback |
| Video | `.mp4 .webm` | Playback |
| Code | `.js .ts .py .rs .java .c .cpp .css .html .xml .json .yaml .sh` and more | Syntax highlighting |
| Everything else | — | Download link |
| Canvas | `.canvas` | Read-only node/edge graph view |

## Settings

Theme (light/dark), editor font size and default mode, and window/pane
layout are all under Settings, persisted per user.

## Multi-user

- **Auth**: password, LDAP, or OIDC (server config); TOTP two-factor and API
  keys are available regardless of provider. Off by default — see
  [CONFIGURATION.md](CONFIGURATION.md#authentication).
- **Roles**, per vault: Owner (can share and manage members), Editor
  (read/write), Viewer (read-only).
- **Groups**: bulk-share a vault with a group instead of one user at a time.
- **Public vaults**: a vault can allow unauthenticated read-only access.
- **Admin panel** (`/admin`, admin role required): user management, the
  audit log, and manual vault reindexing.

Setting up the first admin account: see [DEPLOYMENT.md](DEPLOYMENT.md) or,
for a forgotten password, the README's
["Forgot your password?"](../README.md#forgot-your-password) section.

## Troubleshooting

**Vault not loading** — confirm the path exists and is readable by the
process running Librarium; check server logs for permission errors.

**Changes not syncing across tabs/devices** — check the browser console for
WebSocket errors; confirm nothing is blocking the WebSocket (it shares the
HTTP port, so a firewall issue here usually means HTTP is blocked too); in
Docker, confirm the volume is actually mounted where you think it is.

**Images not showing** — the image must be inside the vault's directory
tree; very unusual characters in the filename may not be handled yet.
