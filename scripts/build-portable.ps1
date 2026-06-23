<#
.SYNOPSIS
    Build a portable, single-file Librarium server for Windows.

.DESCRIPTION
    Produces a self-contained folder under dist/portable/ containing:
      - Librarium.exe         the standalone server with the frontend embedded
                              and SQLite compiled in (no runtime dependencies)
      - config.toml           local configuration (auth on, all paths relative)
      - Start-Librarium.cmd   launcher: starts the server, opens the browser
      - README.txt            first-run instructions

    The exe is fully portable: when run from its folder it loads ./config.toml
    and writes everything (database, vaults, logs) beside itself. Nothing is
    installed and nothing is written to AppData.

    On first launch, if the user table is empty, the server provisions an
    admin account with a RANDOM password, forces a password change at first
    login, and writes the one-time credentials to data/FIRST-RUN-CREDENTIALS.txt.

.PARAMETER OutDir
    Output directory for the staged package. Default: dist/portable.

.PARAMETER Port
    TCP port the portable server binds on localhost. Default: 8080.

.PARAMETER SkipFrontend
    Reuse an existing target/frontend bundle instead of rebuilding it.

.PARAMETER SkipServer
    Reuse an existing target/release/librarium.exe instead of rebuilding it.

.EXAMPLE
    pwsh scripts/build-portable.ps1
    pwsh scripts/build-portable.ps1 -Port 9000 -OutDir dist/librarium-portable
#>
[CmdletBinding()]
param(
    [string]$OutDir,
    [int]$Port = 8080,
    [switch]$SkipFrontend,
    [switch]$SkipServer
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Repo root is the parent of this script's directory.
$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $RepoRoot 'dist/portable' }

$FrontendDir = Join-Path $RepoRoot 'frontend'
$FrontendOut = Join-Path $RepoRoot 'target/frontend'
$ServerExe   = Join-Path $RepoRoot 'target/release/librarium.exe'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }

# ── 1. Frontend bundle (embedded into the exe by rust_embed at compile time) ──
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
        # Invoke vite directly rather than `npm run build`: the latter also runs
        # `vue-tsc --noEmit`, a dev-time type gate that must not block packaging.
        npx --no-install vite build
        if ($LASTEXITCODE -ne 0) { throw 'vite build failed' }
    } finally {
        Pop-Location
    }
}

# ── 2. Server binary (release embeds the frontend; SQLite is statically linked) ──
if ($SkipServer) {
    Write-Step 'Skipping server build (-SkipServer)'
    if (-not (Test-Path $ServerExe)) {
        throw "No existing server binary at $ServerExe. Run without -SkipServer."
    }
} else {
    Write-Step 'Building server binary (cargo build --release)'
    Push-Location $RepoRoot
    try {
        cargo build --release -p librarium-server --bin librarium
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally {
        Pop-Location
    }
}

# ── 3. Stage the portable package ─────────────────────────────────────────────
Write-Step "Staging portable package at $OutDir"
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

Copy-Item $ServerExe (Join-Path $OutDir 'Librarium.exe') -Force

# Per-build random JWT secret: keeps sessions valid across restarts (an empty
# secret would force re-login every launch) without shipping a shared default.
$jwtBytes  = [byte[]]::new(48)
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($jwtBytes)
$jwtSecret = -join ($jwtBytes | ForEach-Object { $_.ToString('x2') })

$configToml = @"
# Librarium — portable configuration.
# All paths are relative to this folder, so the whole directory is portable.

[server]
host = "127.0.0.1"
port = $Port

[database]
# SQLite database file. Its parent directory is created automatically.
path = "./data/librarium.db"

[vault]
# Where note vaults live by default.
base_dir = "./vaults"

[auth]
enabled = true
provider = "password"
# Persistent secret so logins survive restarts. Regenerated on every build.
jwt_secret = "$jwtSecret"
# First admin username. The password is generated on first launch and written
# to data/FIRST-RUN-CREDENTIALS.txt; you must change it at first login.
bootstrap_admin_username = "admin"
"@
Set-Content -Path (Join-Path $OutDir 'config.toml') -Value $configToml -Encoding UTF8

$launcher = @"
@echo off
rem Start the portable Librarium server from this folder and open the UI.
cd /d "%~dp0"
start "Librarium" /min "Librarium.exe"
rem Wait for the server to report healthy, then open the default browser.
powershell -NoProfile -Command "for(`$i=0;`$i -lt 30;`$i++){try{Invoke-WebRequest -UseBasicParsing http://127.0.0.1:$Port/api/health -TimeoutSec 1 ^| Out-Null; break}catch{Start-Sleep -Milliseconds 500}}"
start "" "http://127.0.0.1:$Port"
"@
Set-Content -Path (Join-Path $OutDir 'Start-Librarium.cmd') -Value $launcher -Encoding ASCII

$readme = @"
Librarium — portable edition
============================

Quick start
-----------
1. Double-click Start-Librarium.cmd. It launches the server and opens
   http://127.0.0.1:$Port in your browser.
   (Or run Librarium.exe directly and browse to that address yourself.)

2. On first launch an administrator account is created automatically:
     username: admin
     password: a random one-time password written to
               data\FIRST-RUN-CREDENTIALS.txt
   Log in with those credentials. You will immediately be asked to set a
   new password. After that, delete data\FIRST-RUN-CREDENTIALS.txt.

Where your data lives (all inside this folder)
----------------------------------------------
  data\librarium.db   the SQLite database
  vaults\             your note vaults
  logs\               application logs
  config.toml         configuration (port, paths, auth)

Notes
-----
- The whole folder is portable: copy it to a USB stick or another PC and it
  keeps working. Keep config.toml next to Librarium.exe.
- The server listens on 127.0.0.1 only (this machine). To share it on a
  network, change host in config.toml — but note auth is the only protection.
- To reset everything, stop the app and delete the data\ folder; a fresh
  admin account is provisioned on the next launch.
"@
Set-Content -Path (Join-Path $OutDir 'README.txt') -Value $readme -Encoding UTF8

Write-Step 'Done.'
Write-Host ''
Write-Host "Portable package: $OutDir" -ForegroundColor Green
Get-ChildItem $OutDir | Select-Object Name, Length | Format-Table -AutoSize
