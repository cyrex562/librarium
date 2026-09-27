# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versioning is
[semver](https://semver.org/), pre-1.0 so minor bumps may include breaking
changes.

Tracking starts at 0.102.2 — earlier history is in `git log`.

## [Unreleased]

### Added

- Formatted mode renders Markdown tables as a grid (borders, shaded header
  row, column alignment) that you click into and type in. The file is still
  plain Markdown; Plain mode shows the `| … |` source.
- Merged table cells in Formatted mode: `^^` in a cell merges it into the
  cell above, `<<` into the cell to its left.
- `cargo xtask docker-build` / `docker-publish` for building the server image
  and pushing it to GHCR by hand.
- `scripts/release-for-windows.ps1` — builds the Windows installer and both
  portable packages and publishes them to a GitHub release.
- `librarium-upgrade` upgrades a systemd-managed server to a published GitHub
  release, with no toolchain on the server. It verifies `SHA256SUMS.txt`,
  backs up the database, and rolls back if the new version isn't healthy.
  It handles the server `.deb`, plain-binary and `cargo xtask deploy`
  layouts. Shipped in the `.deb`; attach it to each release as
  `librarium-upgrade.sh`.
- Typst notes (#138): `.typ` files show in the file tree with their own icon
  and open in an editor with Typst syntax highlighting (Formatted mode) or as
  plain text (Plain mode). They autosave like Markdown notes. Name a new note
  `something.typ` to create one; a name with no extension is still Markdown.
- Typst Preview (#139): the server compiles Typst notes with the Typst
  compiler (the `typst` crate, cargo feature `typst`, on by default) and the
  editor's Preview shows the result. Compile errors are listed with their
  line; click one to jump there. `#include` and `image()` read files from the
  vault; packages aren't supported.
- Export Typst notes as PDF (#140): the PDF button in the Typst editor (exports
  the current text), or "Export as PDF" on a `.typ` file in the file tree.
- Markdown → Typst conversion (#141): "Convert to Typst" (file-tree menu, or
  the Markdown editor's ⋯ menu) creates a `.typ` copy of a note and lists
  anything that didn't carry over. Markdown notes can now be exported to PDF
  as well (#140).

### Changed

- The editor toolbar is now two fixed rows on desktop: everyday tools on
  top, and a context row below that shows table controls while the cursor
  is in a table and less-used tools (folding, blockquote, indent, code block,
  horizontal rule) otherwise. Clicking into a table no longer shifts the page.
- Sidebar panels (Tags, Outline, AI Insights, Outgoing Links, Backlinks,
  Neighboring Files, Favorites, Bookmarks, Recent Files) and the Frontmatter
  panel now start collapsed.

### Fixed

- Markdown editor: typing a bracket or quote no longer adds a second closing
  one (`[a](b)` saved as `[a](b))]`, and apostrophes doubled) (#155). Also
  removed two claims from the user guide that didn't match the app: `[[`
  wiki-link autocomplete, and dropping an image onto the editor.
- Preview tables: body cells were rendered as header cells and column
  alignment was dropped (#123). Preview now also shows `^^` / `<<` merged
  cells, and an image's alt text goes in its `alt` attribute instead of
  appearing as text after the image.
- Importing files now refreshes the file tree immediately instead of waiting
  for a WebSocket change event.
- The admin "Temporary password" field is now masked like other password
  fields.
- The Windows PowerShell scripts no longer fail to parse on Windows
  PowerShell 5.1 (non-ASCII characters) or crash on an expected non-zero
  exit from `gh`.

## [0.102.4] - 2026-09-16

### Added

- `librarium admin set-password` / `create-user` / `list-users` — offline
  account recovery that works with the server stopped (opens SQLite
  directly), for when a password is forgotten and there's no other way in.
- Desktop: a "Forgot password?" reset on the login screen, and a Settings →
  Security panel to turn password protection on/off and change the password
  without hand-editing `config.toml`.
- A startup warning when binding a routable (non-loopback) address without
  authentication enabled, and always for plain HTTP on such a bind.
- `cargo xtask ci` — the local verification gate, replacing hosted CI (see
  Removed).

### Changed

- Password changes now revoke every session for that user. Previously a
  reset left existing sessions (including a compromised one) alive.
- A single failed token refresh no longer ends a session outright: the
  desktop client retries once before falling back to a (non-destructive)
  local sign-out, rather than immediately revoking the server-side session
  and the durable refresh token.
- `frontend/tests/e2e/` and `frontend/tests/ui/` now run against separate,
  isolated servers instead of sharing one — eliminates state leaking from
  the real-server `e2e/` specs into the mocked `ui/` ones.

### Removed

- Hosted CI (`.github/workflows/`). GitHub Actions was not working well for
  this project; verification moved into the repo itself
  (`cargo xtask ci`/`--full`).
- The `@tiptap/*` dependency (five packages, unused — nothing imported the
  component backed by it) and the one remaining vulnerable dependency it
  carried.

### Fixed

- Typing a Markdown table header and pressing Enter now inserts the
  required separator row — previously it inserted another content row,
  leaving an invalid table.
- Several stale Playwright specs corrected to match current UI behaviour
  (dialog-based rename, table/folder default-state assumptions, a couple of
  renamed selectors).

## [0.102.3] - 2026-09-13

### Added

- A contextual toolbar for editing Markdown tables in place: add/remove
  rows and columns, column alignment, row/column reordering, delete table,
  and a drag-or-type grid picker for creating a new table.

## [0.102.2] - 2026-09-12

- Version bump alongside table-editing groundwork; see 0.102.3 for the
  user-facing result.
