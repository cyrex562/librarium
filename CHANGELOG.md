# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versioning is
[semver](https://semver.org/), pre-1.0 so minor bumps may include breaking
changes.

Tracking starts at 0.102.2 — earlier history is in `git log`.

## [Unreleased]

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
