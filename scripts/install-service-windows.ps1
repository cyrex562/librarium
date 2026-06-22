#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Install or remove the Librarium server as a Windows Service.

.DESCRIPTION
    Registers librarium.exe as a native Windows Service using sc.exe so it
    starts automatically at boot and can be managed through the Services MMC,
    sc.exe, or PowerShell's service cmdlets.

    Run from the directory that contains librarium.exe, or supply -BinaryPath.

.PARAMETER Action
    "install"   – Register and start the service (default).
    "remove"    – Stop and remove the service.
    "start"     – Start an already-registered service.
    "stop"      – Stop a running service.
    "status"    – Print the current service state.

.PARAMETER BinaryPath
    Full path to librarium.exe.  Defaults to .\librarium.exe.

.PARAMETER ConfigPath
    Full path to the config file passed to --config.
    Defaults to %ProgramData%\Librarium\config.toml.

.PARAMETER ServiceName
    Windows service name.  Defaults to "librarium".

.PARAMETER DisplayName
    Human-readable name shown in the Services console.

.EXAMPLE
    # Install with defaults (run from the folder containing librarium.exe):
    powershell -ExecutionPolicy Bypass -File install-service-windows.ps1

.EXAMPLE
    # Install with explicit paths:
    powershell -ExecutionPolicy Bypass -File install-service-windows.ps1 `
        -BinaryPath "C:\Librarium\librarium.exe" `
        -ConfigPath "C:\Librarium\config.toml"

.EXAMPLE
    # Remove the service:
    powershell -ExecutionPolicy Bypass -File install-service-windows.ps1 -Action remove
#>
param(
    [ValidateSet("install", "remove", "start", "stop", "status")]
    [string]$Action = "install",

    [string]$BinaryPath = "",

    [string]$ConfigPath = "",

    [string]$ServiceName = "librarium",

    [string]$DisplayName = "Librarium Server"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Resolve defaults ──────────────────────────────────────────────────────────
if (-not $BinaryPath) {
    $BinaryPath = Join-Path $PSScriptRoot "librarium.exe"
    if (-not (Test-Path $BinaryPath)) {
        $BinaryPath = Join-Path (Get-Location) "librarium.exe"
    }
}

if (-not $ConfigPath) {
    $ConfigPath = Join-Path $env:ProgramData "Librarium\config.toml"
}

$DataDir = Split-Path $ConfigPath -Parent

# ── Helper functions ──────────────────────────────────────────────────────────
function Get-ServiceStatus {
    $svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if ($null -eq $svc) { return $null }
    return $svc.Status
}

function Write-Step ([string]$msg) {
    Write-Host "  ==> $msg" -ForegroundColor Cyan
}

function Write-Ok ([string]$msg) {
    Write-Host "  [OK] $msg" -ForegroundColor Green
}

function Write-Warn ([string]$msg) {
    Write-Host "  [WARN] $msg" -ForegroundColor Yellow
}

# ── Action: status ────────────────────────────────────────────────────────────
if ($Action -eq "status") {
    $status = Get-ServiceStatus
    if ($null -eq $status) {
        Write-Host "Service '$ServiceName' is NOT installed."
        exit 0
    }
    Write-Host "Service '$ServiceName' status: $status"
    exit 0
}

# ── Action: start ─────────────────────────────────────────────────────────────
if ($Action -eq "start") {
    Write-Step "Starting service '$ServiceName'..."
    Start-Service -Name $ServiceName
    Write-Ok "Service started."
    exit 0
}

# ── Action: stop ──────────────────────────────────────────────────────────────
if ($Action -eq "stop") {
    $status = Get-ServiceStatus
    if ($null -ne $status -and $status -eq "Running") {
        Write-Step "Stopping service '$ServiceName'..."
        Stop-Service -Name $ServiceName -Force
        Write-Ok "Service stopped."
    } else {
        Write-Warn "Service is not running (status: $status)."
    }
    exit 0
}

# ── Action: remove ────────────────────────────────────────────────────────────
if ($Action -eq "remove") {
    $status = Get-ServiceStatus
    if ($null -eq $status) {
        Write-Warn "Service '$ServiceName' is not installed — nothing to remove."
        exit 0
    }
    if ($status -eq "Running") {
        Write-Step "Stopping service..."
        Stop-Service -Name $ServiceName -Force
    }
    Write-Step "Removing service '$ServiceName'..."
    sc.exe delete $ServiceName | Out-Null
    Write-Ok "Service removed."
    Write-Warn "Data directory '$DataDir' was NOT deleted. Remove it manually if no longer needed."
    exit 0
}

# ── Action: install ───────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Librarium Windows Service Installer" -ForegroundColor White
Write-Host "====================================" -ForegroundColor White
Write-Host ""

# Validate binary
if (-not (Test-Path $BinaryPath)) {
    Write-Error "Binary not found: $BinaryPath`nPlace librarium.exe in the same folder as this script, or pass -BinaryPath."
    exit 1
}
$BinaryPath = Resolve-Path $BinaryPath

Write-Host "  Binary  : $BinaryPath"
Write-Host "  Config  : $ConfigPath"
Write-Host "  Service : $ServiceName"
Write-Host ""

# Stop + remove existing registration so we can re-register cleanly.
$existing = Get-ServiceStatus
if ($null -ne $existing) {
    Write-Step "Removing existing service registration..."
    if ($existing -eq "Running") {
        Stop-Service -Name $ServiceName -Force
    }
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}

# Create data directory and default config if missing.
if (-not (Test-Path $DataDir)) {
    Write-Step "Creating data directory: $DataDir"
    New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

    # Create a sub-directory for vaults.
    New-Item -ItemType Directory -Force -Path (Join-Path $DataDir "vaults") | Out-Null
}

if (-not (Test-Path $ConfigPath)) {
    Write-Step "Writing default config: $ConfigPath"
    $vaultsDir = Join-Path $DataDir "vaults"
    $dbPath    = Join-Path $DataDir "librarium.db"
    # Escape backslashes for TOML strings.
    $vaultsDirToml = $vaultsDir -replace '\\', '\\'
    $dbPathToml    = $dbPath    -replace '\\', '\\'
    @"
# Librarium server configuration
# Edit this file, then restart the service:
#   Restart-Service librarium
#
# All values can be overridden with environment variables using double
# underscores for nesting:  LIBRARIUM__AUTH__JWT_SECRET="..."

[server]
host = "127.0.0.1"   # Change to 0.0.0.0 to listen on all interfaces
port = 8080

[database]
path = "$dbPathToml"

[vault]
base_dir = "$vaultsDirToml"

[auth]
enabled = true
provider = "password"   # "password" | "ldap" | "oidc"
# REQUIRED: set a strong random secret before first start.
# Generate one (PowerShell): -join ((0..31) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
jwt_secret = ""

# First-run bootstrap — creates the initial admin account when no users exist.
# Remove or comment out after the first login.
# bootstrap_admin_username = "admin"
# bootstrap_admin_password = ""

access_token_ttl  = 3600
refresh_token_ttl = 604800

[cors]
allowed_origins = []

[sync]
change_log_retention_days = 30
"@ | Set-Content -Encoding UTF8 $ConfigPath
}

# Register the service.
Write-Step "Registering Windows Service..."
$binPathArg = "`"$BinaryPath`" --config `"$ConfigPath`""
sc.exe create $ServiceName `
    binPath= $binPathArg `
    DisplayName= $DisplayName `
    start= auto | Out-Null

sc.exe description $ServiceName `
    "Librarium offline-first markdown knowledge base server" | Out-Null

sc.exe failure $ServiceName reset= 60 actions= restart/5000/restart/10000/restart/30000 | Out-Null

Write-Ok "Service registered."
Write-Host ""
Write-Host "  IMPORTANT: before starting the service:" -ForegroundColor Yellow
Write-Host "    1. Edit: $ConfigPath"                  -ForegroundColor Yellow
Write-Host "    2. Set jwt_secret to a strong random value"  -ForegroundColor Yellow
Write-Host "    3. Set bootstrap_admin_password for the first admin account" -ForegroundColor Yellow
Write-Host "    4. Run:  Start-Service $ServiceName"   -ForegroundColor Yellow
Write-Host "       Or:   powershell -File $($MyInvocation.MyCommand.Path) -Action start" -ForegroundColor Yellow
Write-Host ""
Write-Host "  Logs:   Event Viewer > Windows Logs > Application (Source: librarium)" -ForegroundColor Cyan
Write-Host "  Status: Get-Service $ServiceName" -ForegroundColor Cyan
Write-Host ""
