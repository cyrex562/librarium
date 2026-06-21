# Build and Deployment Guide

## Prerequisites

- **Rust**: Latest stable toolchain (install via `rustup`)
- **Node.js**: LTS version (18 or 20) for the Vue frontend
- **npm**: Ships with Node.js

### Linux system dependencies (server binary)

The server binary has no native UI dependencies beyond a standard C toolchain.
On most Linux distributions the only package you need beyond Rust/Node is:

```bash
# Debian / Ubuntu
sudo apt-get install -y build-essential pkg-config libssl-dev

# Fedora / RHEL
sudo dnf install -y gcc pkg-config openssl-devel

# Arch
sudo pacman -S base-devel pkg-config openssl
```

### Linux system dependencies (Tauri desktop binary)

The Tauri desktop shell (`librarium-tauri`) embeds the Vue UI in a native WebView
and requires **WebKitGTK** plus a handful of supporting libraries.  These must be
present both at build time and at runtime on the end-user's machine.

#### Ubuntu / Debian

```bash
# Ubuntu 22.04 LTS (WebKitGTK 4.0 — Tauri 2 minimum target)
sudo apt-get update
sudo apt-get install -y \
    build-essential pkg-config \
    libssl-dev \
    libwebkit2gtk-4.0-dev \
    libsoup2.4-dev \
    libjavascriptcoregtk-4.0-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev

# Ubuntu 24.04 LTS / Debian 12+ (WebKitGTK 4.1)
sudo apt-get update
sudo apt-get install -y \
    build-essential pkg-config \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev
```

#### Fedora / RHEL

```bash
# Fedora 39 / 40 (WebKitGTK 4.1)
sudo dnf install -y \
    gcc pkg-config openssl-devel \
    webkit2gtk4.1-devel \
    libsoup3-devel \
    javascriptcoregtk4.1-devel \
    gtk3-devel \
    libappindicator-gtk3-devel \
    librsvg2-devel

# Fedora 41+ / RHEL 9 use the same package names above.
# Note: Fedora 43+ ships newer SONAMEs (libicu*.so.75, libjpeg.so.8+).
# See docs/WEBKITGTK_COMPAT.md and TODO.md LIB-023 for the bundled Playwright
# webkit runtime compatibility issue on Fedora 43+.
```

#### Arch Linux

```bash
sudo pacman -S base-devel pkg-config openssl \
    webkit2gtk-4.1 libsoup3 gtk3 libappindicator-gtk3 librsvg
```

#### Runtime requirements (end-user machines)

A machine running the `librarium-tauri` binary needs the same WebKitGTK and
GTK3 libraries at runtime. On Ubuntu 22.04+ and Fedora 39+ these are installed
by default. If shipping a standalone binary, document the runtime deps or package
via `.deb` / `.rpm` / AppImage (Tauri's bundler handles this automatically).

#### Key package mapping

| Capability | Ubuntu 22.04 pkg | Ubuntu 24.04 / Fedora pkg |
|---|---|---|
| WebKitGTK rendering engine | `libwebkit2gtk-4.0-dev` | `libwebkit2gtk-4.1-dev` / `webkit2gtk4.1-devel` |
| HTTP stack (libsoup) | `libsoup2.4-dev` | `libsoup-3.0-dev` / `libsoup3-devel` |
| JavaScript engine | `libjavascriptcoregtk-4.0-dev` | `libjavascriptcoregtk-4.1-dev` |
| GTK3 window toolkit | `libgtk-3-dev` | same |
| System tray icon | `libayatana-appindicator3-dev` | `libappindicator-gtk3-devel` |

WebKitGTK 4.0 (`gtk-webkit2-4.0`) is the Tauri 2 minimum.  WebKitGTK 4.1
(`gtk-webkit2-4.1`) is preferred where available.  Both SONAME families can
coexist on the same machine but the binary must be linked against one or the
other — the Tauri build system selects based on which `-dev` packages are found
by `pkg-config` at compile time.

See `docs/WEBKITGTK_COMPAT.md` for JavaScript / CSS / Web API constraints that
apply when running on WebKitGTK 2.36 (Ubuntu 22.04 minimum).

## Building for Release

### Server binary (Linux / macOS / Windows)

```bash
# 1. Build the Vue frontend (output lands in target/frontend/)
cd frontend && npm ci && npm run build && cd ..

# 2. Build the server binary (frontend is embedded at compile time)
cargo build --release -p librarium-server

# The binary is at target/release/librarium
# Copy it alongside config.toml (or config.example.toml) to deploy.
```

### Windows (PowerShell script)

A convenience script is provided:

```powershell
.\scripts\build_release.ps1
```

This installs frontend dependencies, builds the Vue SPA, then builds the server
binary with LTO and symbol stripping into a `dist/` directory.

### Desktop app (Tauri)

Install Linux system dependencies first (see above), then:

```bash
# Build frontend (required — Tauri embeds it)
cd frontend && npm ci && npm run build && cd ..

# Build the desktop binary
cargo build --release -p librarium-tauri
# Binary: target/release/librarium-tauri

# Or use Tauri CLI for a bundled installer (.deb / .rpm / AppImage):
cargo install tauri-cli
cargo tauri build
```

## Binary Optimization

The `Cargo.toml` is configured with a custom release profile to minimize binary size:

- `opt-level = "z"`: Optimize for size.
- `lto = true`: Link Time Optimization enabled.
- `codegen-units = 1`:  Maximize optimization quality at cost of compile time.
- `panic = "abort"`: Removes stack unwinding code.
- `strip = true`: Removes debugging symbols.

## Cross-Compilation

To cross-compile for other platforms (e.g., Linux, macOS), we recommend using [`cross`](https://github.com/cross-rs/cross).

1. **Install cross**:

    ```bash
    cargo install cross
    ```

2. **Build for Linux (x86_64)**:

    ```bash
    cross build --target x86_64-unknown-linux-gnu --release
    ```

    *Note: This requires Docker to be running.*

3. **Build for Windows (from Linux/macOS)**:

    ```bash
    cross build --target x86_64-pc-windows-gnu --release
    ```

After cross-compiling, follow the "Assemble" steps above, using the binary from `target/<target-triple>/release/`.
