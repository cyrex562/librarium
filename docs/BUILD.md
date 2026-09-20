# Build

## Prerequisites

- **Rust**: current stable (via `rustup`)
- **Node.js**: a current LTS (18+) for the Vue frontend
- **npm**: ships with Node.js

### Linux system dependencies — server binary

Nothing beyond a standard C toolchain:

```bash
# Debian / Ubuntu
sudo apt-get install -y build-essential pkg-config libssl-dev libsqlite3-dev

# Fedora / RHEL
sudo dnf install -y gcc pkg-config openssl-devel sqlite-devel

# Arch
sudo pacman -S base-devel pkg-config openssl sqlite
```

### Linux system dependencies — desktop binary (`librarium-tauri`)

The Tauri desktop shell embeds the Vue UI in a native WebView via
**WebKitGTK**. Required at build time and at runtime on the end-user's
machine.

#### Ubuntu / Debian

```bash
# Ubuntu 22.04 LTS (WebKitGTK 4.0 — Tauri 2's minimum target)
sudo apt-get install -y \
    build-essential pkg-config libssl-dev libsqlite3-dev \
    libwebkit2gtk-4.0-dev libsoup2.4-dev libjavascriptcoregtk-4.0-dev \
    libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev

# Ubuntu 24.04 LTS / Debian 12+ (WebKitGTK 4.1)
sudo apt-get install -y \
    build-essential pkg-config libssl-dev libsqlite3-dev \
    libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
    libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

#### Fedora / RHEL

```bash
# Fedora 39+ / RHEL 9 (WebKitGTK 4.1)
sudo dnf install -y \
    gcc pkg-config openssl-devel sqlite-devel \
    webkit2gtk4.1-devel libsoup3-devel javascriptcoregtk4.1-devel \
    gtk3-devel libappindicator-gtk3-devel librsvg2-devel

# Fedora 43+ ships newer SONAMEs (libicu*.so.75, libjpeg.so.9+) that affect
# Playwright's bundled WebKit specifically, not this build — see "Playwright
# WebKit on Fedora 43+" below.
```

#### Arch Linux

```bash
sudo pacman -S base-devel pkg-config openssl sqlite \
    webkit2gtk-4.1 libsoup3 gtk3 libappindicator-gtk3 librsvg
```

#### Runtime requirements (end-user machines)

A machine running `librarium-tauri` needs the same WebKitGTK/GTK3 libraries
at runtime — present by default on Ubuntu 22.04+ and Fedora 39+. Tauri's
bundler (`cargo tauri build`) packages a `.deb`/`.rpm`/AppImage, so end users
don't need to install anything beyond what the package manager pulls in.

#### Package mapping reference

| Capability | Ubuntu 22.04 | Ubuntu 24.04 / Fedora |
| --- | --- | --- |
| WebKitGTK rendering | `libwebkit2gtk-4.0-dev` | `libwebkit2gtk-4.1-dev` / `webkit2gtk4.1-devel` |
| HTTP stack (libsoup) | `libsoup2.4-dev` | `libsoup-3.0-dev` / `libsoup3-devel` |
| JS engine | `libjavascriptcoregtk-4.0-dev` | `libjavascriptcoregtk-4.1-dev` |
| GTK3 toolkit | `libgtk-3-dev` | same |
| Tray icon | `libayatana-appindicator3-dev` | `libappindicator-gtk3-devel` |

WebKitGTK 4.0 is the Tauri 2 minimum; 4.1 is preferred where available. Both
SONAME families can coexist on one machine, but a given binary links against
whichever `-dev` package `pkg-config` finds at compile time. See
[WEBKITGTK_COMPAT.md](WEBKITGTK_COMPAT.md) for the JS/CSS constraints that
follow from targeting the older 2.36 runtime.

## Building

```bash
npm --prefix frontend ci && npm --prefix frontend run build   # embeds into the server binary
cargo build --release -p librarium-server                     # target/release/librarium
cargo build --release -p librarium-tauri                      # target/release/librarium-tauri
```

Or via `cargo xtask` (handles the frontend-then-Rust ordering for you, and
works identically on Linux/macOS/Windows/WSL) — see the
[README](../README.md#quick-start) for `build-desktop`, `build-installer`,
and the Windows-specific walkthrough.

The release profile (`[profile.release]` in the workspace `Cargo.toml`)
optimizes for size: `opt-level = "z"`, LTO on, `codegen-units = 1`,
`panic = "abort"`, symbols stripped. `[profile.release-fast]` inherits from
it with LTO off and `opt-level = 3` for a build that's ~3-5x quicker at the
cost of a larger binary — use it while iterating on a release build; keep
plain `release` for what you actually ship.

## Windows portable packages & release publishing

Three PowerShell scripts under `scripts/` (Windows-only; run with `pwsh`):

- **`build-portable.ps1`** — stages a self-contained `Librarium.exe` (server
  only) plus `config.toml`, a launcher, and a README under `dist/portable/`.
  No install, no AppData/registry writes — the whole folder is copy-anywhere
  portable.
- **`build-portable-desktop.ps1`** — the same idea for the Tauri desktop app,
  staged under `dist/portable-desktop/`.
- **`release-for-windows.ps1`** — builds all three Windows deliverables (the
  NSIS installer via `cargo xtask build-installer`, and both portable
  packages above, zipped) and publishes them to a tagged GitHub release via
  `gh release create`/`upload`, creating the release if it doesn't exist and
  merging into its `SHA256SUMS.txt` rather than clobbering entries from other
  platforms. This repo has no hosted CI (see [AGENTS.md](../AGENTS.md)), so
  this is the manual publish step for the Windows side of a release — see
  `pwsh scripts/release-for-windows.ps1 -?` for the full option list
  (comment-based help covers every parameter and gives usage examples).
  Requires `gh` authenticated (`gh auth login`) and refuses to run with a
  dirty working tree, same safety pattern as `cargo xtask update`.

## Cross-compilation

The project's own release process is to build natively per platform (see the
[README](../README.md) and the release punch list's item 4b) rather than
cross-compile — most reliable, and Windows/macOS artifacts get built on real
Windows/Mac hardware. If you want to cross-compile anyway:

- **`cargo-xwin`** and **`cargo-zigbuild`** are available in this repo's
  toolchain (already used for other workspace tooling) and can target
  `x86_64-pc-windows-msvc` / glibc Linux targets from a Linux host without a
  Windows machine.
- **[`cross`](https://github.com/cross-rs/cross)** (needs Docker) is a more
  general-purpose option: `cargo install cross`, then
  `cross build --target x86_64-unknown-linux-gnu --release` (or
  `x86_64-pc-windows-gnu`).

Either way, follow the "Building" steps above afterward using the binary from
`target/<target-triple>/release/`.

For Android, see `AGENTS.md`'s "Android build" section —
that's a real cross-compile (via `cargo-ndk`) since there's no "native"
Android host to build on directly.

## Running tests

**This repo has no hosted CI.** `cargo xtask ci` is the gate — see
`AGENTS.md`'s "Build And Test" section for the full breakdown. It runs
rustfmt, clippy, `cargo test --workspace`, vitest, and the frontend
typecheck; `cargo xtask ci --full` additionally runs the Android
cross-compile check and the full Playwright suite.

### Playwright E2E

Two isolated suites, each against its own dedicated server
(`frontend/playwright.shared.ts`):

```bash
cd frontend
npm ci
npx playwright install --with-deps   # all three browsers
npm run test:e2e                     # both suites: test:e2e:ui then test:e2e:integration
```

Run one suite alone with `npm run test:e2e:ui` (mocked API,
`tests/ui/`) or `npm run test:e2e:integration` (real server,
`tests/e2e/`) while debugging.

Targets Chromium, Firefox, and WebKit via Playwright's bundled browsers.

### Playwright WebKit on Fedora 43+

Playwright's bundled WebKit runtime (separate from the WebKitGTK packages
used by the Tauri desktop shell) needs older shared-library SONAMEs no
longer in Fedora 43's default repos:

| Library | Playwright WebKit needs | Fedora 43 ships |
| --- | --- | --- |
| ICU | `libicu*.so.74` | `libicu*.so.75` |
| libjpeg | `libjpeg.so.8` | `libjpeg.so.9` (libjpeg-turbo) |
| libjxl | `libjxl.so.0.8` | `libjxl.so.0.10` |

`playwright.config.ts` (via the shared `playwright.shared.ts` factory)
detects Fedora via `/etc/os-release` and excludes the `webkit` project there
automatically — Chromium and Firefox are unaffected and run normally. To
force WebKit anyway (e.g. with compat libs via Nix or a container):

```bash
PLAYWRIGHT_INCLUDE_WEBKIT=1 npx playwright test --project=webkit
```
