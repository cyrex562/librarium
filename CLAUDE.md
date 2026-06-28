# CLAUDE.md

Guidance for Claude Code (and other AI agents) working in this repository.
This file complements [AGENTS.md](AGENTS.md) — read both. Where they overlap,
they agree; AGENTS.md holds the fuller repository map, build/test commands, and
code areas to inspect carefully.

## Orientation

Librarium is a Rust workspace for a self-hosted, Obsidian-compatible knowledge
app with a Vue 3 frontend and a Tauri desktop shell. **Markdown files on disk are
the source of truth**; the SQLite database and Tantivy index are derived state.

Start from **[docs/DESIGN.md](docs/DESIGN.md)** — the canonical, current design &
architecture document — before making non-trivial changes. Historical notes live
in [`docs/archive/`](docs/archive/) and may describe superseded behavior; do not
treat them as current.

## Working style

- Prefer minimal, targeted changes that fit existing module boundaries.
- `services/` = business logic, `routes/` = thin transport adapters,
  `models/` + `librarium-types` = shared contracts. Keep frontend API types
  aligned with backend JSON shapes.
- Add or update tests when touching auth, file mutation, reindexing, search, or
  editor-state behavior. The watcher → index → broadcast loop is the most
  consistency-sensitive code in the system.
- Never bypass the path-safety checks in `FileService`.

## Documentation & versioning (keep these current)

`docs/DESIGN.md` and the root `README.md` are living documents. **In the same
change that introduces a breaking or structurally significant change, update
them.** That includes any change which:

- adds, removes, or renames a crate, service, route module, or Pinia store;
- alters a public REST/WebSocket payload or the frontend⇄backend contract;
- changes the data/persistence model or the watcher → index → broadcast flow;
- changes auth, authorization, or filesystem-safety behavior;
- changes configuration keys, build steps, or run commands;
- bumps the project version.

Update `README.md` too when the overview or quick start is affected. When a
section of `docs/DESIGN.md` is fully superseded, move the long-form detail to
`docs/archive/` and leave a short pointer.

**Version bumps** must stay in sync across all of: `crates/*/Cargo.toml`,
`frontend/package.json`, and `crates/librarium-tauri/tauri.conf.json`. The
`/api/version` endpoint reads `CARGO_PKG_VERSION`, so the crate versions feed it
directly. Current version: **0.100.0**.

## Build & test (see AGENTS.md for the full list)

```bash
cargo check --workspace
cargo test -p librarium-server
npm --prefix frontend test
npm --prefix frontend run build
```
