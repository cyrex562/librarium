# Contributing to Librarium

## Development setup

Prerequisites: Rust (via `rustup`) and a current Node.js LTS. See
[docs/BUILD.md](docs/BUILD.md) for system packages needed for the Tauri
desktop build specifically — the server alone needs nothing beyond a C
toolchain.

```bash
# Terminal 1 — backend (binds :8080)
cargo run

# Terminal 2 — frontend, hot-reloading, proxies /api to the backend above
npm --prefix frontend install
npm --prefix frontend run dev
```

Open `http://localhost:5173` (the Vite dev server, not :8080 directly) —
`vite.config.ts` proxies `/api` requests through to the Rust backend.

## Project structure

```
crates/
  librarium-server   Actix Web backend + binary (the workspace default member)
  librarium-core     Platform-independent core (FileService, Markdown, search) — no actix/sqlx by default
  librarium-types    Shared Rust DTOs
  librarium-client   HTTP + WebSocket client crate
  librarium-tauri    Desktop shell + Android app host
  librarium-mobile   Thin mobile client command layer
frontend/            Vue 3 + TypeScript + Vuetify SPA
plugins/             Bundled first-party plugins
xtask/               Build/deploy automation (`cargo xtask help`)
docs/                DESIGN.md (architecture), plus operator/user guides
```

Full detail — code-area ownership, the watcher→index→broadcast flow, and
what to check before touching auth, search, or file mutation — lives in
[AGENTS.md](AGENTS.md) and [CLAUDE.md](CLAUDE.md). Read those before a
non-trivial change; this file is the on-ramp, not a duplicate of them.

## Before you push

**This repo has no hosted CI.** `cargo xtask ci` is the gate:

```bash
cargo xtask ci            # rustfmt, clippy -D warnings, cargo test --workspace, vitest, vue-tsc
cargo xtask ci --full     # adds the Android cross-compile check and the full Playwright suite
```

It runs every check rather than stopping at the first failure and prints one
summary; a gate whose tooling is missing reports **SKIPPED**, never PASSED —
treat a skip as "not checked," not as a pass.

If your change is structurally significant — a new crate, route module, or
Pinia store; a changed REST/WebSocket contract; a data-model, auth, or
filesystem-safety change; a new config key or build step — update
[docs/DESIGN.md](docs/DESIGN.md) and [README.md](README.md) in the *same*
change. See CLAUDE.md's "Documentation & versioning" section for the full
list of what counts.

**Bump the version** before pushing: `cargo xtask bump-version` (defaults to
a patch bump; pass `minor`/`major` for a deliberate larger one). It updates
every `Cargo.toml`, `frontend/package.json`, `tauri.conf.json`, and the
version lines in README/DESIGN/CLAUDE.md, and refreshes the lockfiles. It
does not commit — review `git diff` and commit the bump yourself, in the
same commit as the change or its own.

## Pull requests

1. Branch from `main`.
2. Make the change; add or update tests alongside it.
3. `cargo xtask ci` clean.
4. Open a PR describing what changed and why. Small, focused PRs over one
   large one — easier to review, easier to revert if something's wrong.

## Coding style

Rust: `rustfmt` + `clippy -D warnings` (enforced by `cargo xtask ci`, not a
suggestion). TypeScript: strict mode, no implicit `any`. Beyond the
formatter, match the conventions already in the file you're editing —
`services/` holds business logic, `routes/` are thin transport adapters,
`models/`/`librarium-types` are shared contracts; keep frontend API types
aligned with backend JSON shapes.
