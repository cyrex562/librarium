<#
.SYNOPSIS
    Build a portable, single-exe Librarium desktop app for Windows.

.DESCRIPTION
    Produces a self-contained folder under dist/portable-desktop/ containing:
      - LibrariumDesktop.exe   the Tauri desktop app (no console window)
      - config.toml            local configuration — its presence next to the exe
                               activates portable mode (all paths are exe-relative)
      - Start-LibrariumDesktop.cmd   convenience launcher
      - README.txt             first-run instructions

    Portable mode is detected at startup by the presence of config.toml beside
    the exe. The app then stores everything in that same folder:
      data\librarium.db   SQLite database
      vaults\             note vaults
      cache\              Tantivy search index cache
    Nothing is written to %APPDATA% or the registry.

.PARAMETER OutDir
    Output directory for the staged package. Default: dist/portable-desktop.

.PARAMETER Port
    TCP port the embedded server binds on localhost. Default: 8080.

.PARAMETER SkipFrontend
    Reuse an existing target/frontend bundle instead of rebuilding it.

.PARAMETER SkipDesktop
    Reuse an existing target/release/librarium-tauri.exe.

.EXAMPLE
    pwsh scripts/build-portable-desktop.ps1
    pwsh scripts/build-portable-desktop.ps1 -Port 9000 -OutDir dist/librarium-desktop-portable
#>
[CmdletBinding()]
param(
    [string]$OutDir,
    [int]$Port = 8080,
    [switch]$SkipFrontend,
    [switch]$SkipDesktop
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot   = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $RepoRoot 'dist/portable-desktop' }

$FrontendDir  = Join-Path $RepoRoot 'frontend'
$FrontendOut  = Join-Path $RepoRoot 'target/frontend'
$DesktopExe   = Join-Path $RepoRoot 'target/release/librarium-tauri.exe'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }

# ── 1. Frontend bundle ────────────────────────────────────────────────────────
if ($SkipFrontend) {
    Write-Step 'Skipping frontend build (-SkipFrontend)'
    if (-not (Test-Path (Join-Path $FrontendOut 'index.html'))) {
        throw "No existing frontend bundle at $FrontendOut. Run without -SkipFrontend."
    }
} else {
    Write-Step 'Building frontend bundle (vite)'
    Push-Location $FrontendDir
    try {
        if (-not (Test-Path 'node_modules')) {
            Write-Step 'Installing frontend dependencies (npm install)'
            npm install
            if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
        }
        npx --no-install vite build
        if ($LASTEXITCODE -ne 0) { throw 'vite build failed' }
    } finally {
        Pop-Location
    }
}

# ── 2. Desktop binary ─────────────────────────────────────────────────────────
if ($SkipDesktop) {
    Write-Step 'Skipping desktop build (-SkipDesktop)'
    if (-not (Test-Path $DesktopExe)) {
        throw "No existing desktop binary at $DesktopExe. Run without -SkipDesktop."
    }
} else {
    Write-Step 'Building desktop binary (cargo build --release -p librarium-tauri)'
    Push-Location $RepoRoot
    try {
        cargo build --release -p librarium-tauri
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally {
        Pop-Location
    }
}

# ── 3. Stage the portable package ─────────────────────────────────────────────
Write-Step "Staging portable desktop package at $OutDir"
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

Copy-Item $DesktopExe (Join-Path $OutDir 'LibrariumDesktop.exe') -Force

# Per-build random JWT secret for persistent sessions across restarts.
$jwtBytes  = [byte[]]::new(48)
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($jwtBytes)
$jwtSecret = -join ($jwtBytes | ForEach-Object { $_.ToString('x2') })

# config.toml beside the exe is what activates portable mode in the app.
# All paths here are relative to the exe's directory.
$configToml = @"
# Librarium Desktop — portable configuration.
# The presence of this file beside LibrariumDesktop.exe activates portable mode:
# all data stays in this folder; nothing is written to AppData or the registry.

[server]
host = "127.0.0.1"
port = $Port

[database]
# SQLite database. Parent directory is created automatically on first launch.
path = "./data/librarium.db"

[vault]
# Default location for note vaults.
base_dir = "./vaults"

[auth]
enabled = true
provider = "password"
# Persistent secret so logins survive restarts. Regenerated on every build.
jwt_secret = "$jwtSecret"
# First admin username. Password is generated on first launch and written to
# data/FIRST-RUN-CREDENTIALS.txt; you are required to change it at first login.
bootstrap_admin_username = "admin"
"@
Set-Content -Path (Join-Path $OutDir 'config.toml') -Value $configToml -Encoding UTF8

$launcher = @"
@echo off
rem Launch the portable Librarium desktop app from this folder.
cd /d "%~dp0"
start "" "LibrariumDesktop.exe"
"@
Set-Content -Path (Join-Path $OutDir 'Start-LibrariumDesktop.cmd') -Value $launcher -Encoding ASCII

$readme = @"
Librarium Desktop — portable edition
=====================================

Quick start
-----------
1. Double-click LibrariumDesktop.exe (or Start-LibrariumDesktop.cmd).
   The app opens in its own window and appears in the system tray.
   Yellow tray icon = starting, green = ready, red = error.

2. On first launch an administrator account is created automatically:
     username: admin
     password: a random one-time password written to
               data\FIRST-RUN-CREDENTIALS.txt
   Log in with those credentials. You will be prompted to change the password
   immediately. After logging in, delete data\FIRST-RUN-CREDENTIALS.txt.

Where your data lives (all inside this folder)
----------------------------------------------
  data\librarium.db     the SQLite database (backed up daily as .bak-YYYY-MM-DD)
  vaults\               your note vaults
  cache\                search index cache (safe to delete — rebuilt on startup)
  config.toml           configuration (port, paths, auth)

Portable operation
------------------
- The whole folder is portable: copy it to a USB drive or another PC and
  everything works. Keep config.toml next to LibrariumDesktop.exe.
- The app listens on 127.0.0.1 only (this machine), accessible at
  http://127.0.0.1:$Port in a browser as well as via the embedded window.
- To reset everything: stop the app, delete data\ and vaults\; fresh
  credentials are provisioned on the next launch.

Upgrading
---------
Copy the new LibrariumDesktop.exe over the old one (or extract to the same
folder). On the next launch the app will back up data\librarium.db to
data\librarium.db.bak-YYYY-MM-DD before applying any schema changes, so your
data is safe even if the database format changed between versions.
"@
Set-Content -Path (Join-Path $OutDir 'README.txt') -Value $readme -Encoding UTF8

Write-Step 'Done.'
Write-Host ''
Write-Host "Portable desktop package: $OutDir" -ForegroundColor Green
Get-ChildItem $OutDir | Select-Object Name, Length | Format-Table -AutoSize
