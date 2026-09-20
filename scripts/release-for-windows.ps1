<#
.SYNOPSIS
    Build the Windows release artifacts and publish them to a GitHub release.

.DESCRIPTION
    This repo has no hosted CI (see AGENTS.md) — Windows artifacts are built
    and published by hand from a Windows machine. This script is that manual
    step, made repeatable: it builds all three Windows deliverables and
    uploads them to a tagged GitHub release, creating the release if it
    doesn't exist yet.

    Artifacts produced (via the existing build scripts/xtask commands — this
    script doesn't duplicate their logic, just chains and publishes them):
      - Librarium-<tag>-windows-x86_64-setup.exe
          NSIS installer, via `cargo xtask build-installer`.
      - Librarium-<tag>-windows-x86_64-portable-server.zip
          Zipped output of scripts/build-portable.ps1 (server only).
      - Librarium-<tag>-windows-x86_64-portable-desktop.zip
          Zipped output of scripts/build-portable-desktop.ps1 (Tauri app).
      - SHA256SUMS.txt
          Checksums for the three files above, merged into the release's
          existing SHA256SUMS.txt if one is already there (e.g. uploaded by
          a Linux-side run for the same tag) rather than clobbering it.

    Requires: git, cargo (+ the Tauri CLI — see AGENTS.md's Windows section),
    node/npm, and the GitHub CLI (`gh`, already authenticated: `gh auth
    login`). Refuses to run with a dirty working tree, same as `cargo xtask
    update` — commit or stash first.

.PARAMETER Tag
    The release tag to publish to, e.g. v0.102.7-rc1. Defaults to
    "v<version>-rc1" using the version in crates/librarium-server/Cargo.toml.
    If the tag doesn't exist as a release yet, one is created (marked
    prerelease when the tag ends in -rc<N>, -beta<N>, or -alpha<N>).

.PARAMETER SkipPull
    Don't run `git pull --ff-only` first — use the working tree as-is.

.PARAMETER SkipInstaller
.PARAMETER SkipPortableServer
.PARAMETER SkipPortableDesktop
    Skip that artifact's build step and reuse whatever's already staged
    under target/ or dist/ from a previous run.

.PARAMETER DryRun
    Build and stage everything, print what would be uploaded, but don't
    touch the GitHub release at all.

.EXAMPLE
    pwsh scripts/release-for-windows.ps1
    pwsh scripts/release-for-windows.ps1 -Tag v0.102.7-rc1
    pwsh scripts/release-for-windows.ps1 -DryRun
    pwsh scripts/release-for-windows.ps1 -SkipPortableDesktop   # installer + server only
#>
[CmdletBinding()]
param(
    [string]$Tag,
    [switch]$SkipPull,
    [switch]$SkipInstaller,
    [switch]$SkipPortableServer,
    [switch]$SkipPortableDesktop,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$StageDir   = Join-Path $RepoRoot 'dist/release-windows'
$PortableDir        = Join-Path $RepoRoot 'dist/portable'
$PortableDesktopDir = Join-Path $RepoRoot 'dist/portable-desktop'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Require-Tool($name, $hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        throw "$name not found on PATH. $hint"
    }
}

Require-Tool 'git'  'Install Git for Windows.'
Require-Tool 'cargo' 'Install Rust: https://rustup.rs'
Require-Tool 'gh'   "Install the GitHub CLI: https://cli.github.com, then run 'gh auth login'."

gh auth status *> $null
if ($LASTEXITCODE -ne 0) {
    throw "gh is not authenticated. Run 'gh auth login' first."
}

# ── 0. Git safety ──────────────────────────────────────────────────────────
Push-Location $RepoRoot
try {
    if (-not $SkipPull) {
        $dirty = git status --porcelain
        if ($dirty) {
            throw "Working tree has uncommitted changes — commit or stash first:`n$dirty"
        }
        Write-Step 'git pull --ff-only'
        git pull --ff-only
        if ($LASTEXITCODE -ne 0) { throw 'git pull --ff-only failed (local branch has diverged?)' }
    } else {
        Write-Step 'Skipping git pull (-SkipPull)'
    }

    # ── 1. Resolve version / tag ─────────────────────────────────────────────
    $cargoToml = Get-Content (Join-Path $RepoRoot 'crates/librarium-server/Cargo.toml') -Raw
    if ($cargoToml -match 'version = "([^"]+)"') {
        $version = $Matches[1]
    } else {
        throw 'Could not find version = "..." in crates/librarium-server/Cargo.toml'
    }
    if (-not $Tag) { $Tag = "v$version-rc1" }
    Write-Step "Version $version, publishing to tag $Tag"

    # ── 2. Build ──────────────────────────────────────────────────────────────
    if ($SkipInstaller) {
        Write-Step 'Skipping installer build (-SkipInstaller)'
    } else {
        Write-Step 'Building NSIS installer (cargo xtask build-installer)'
        cargo xtask build-installer
        if ($LASTEXITCODE -ne 0) { throw 'cargo xtask build-installer failed' }
    }

    if ($SkipPortableServer) {
        Write-Step 'Skipping portable server build (-SkipPortableServer)'
    } else {
        Write-Step 'Building portable server package (scripts/build-portable.ps1)'
        & (Join-Path $PSScriptRoot 'build-portable.ps1') -OutDir $PortableDir
    }

    if ($SkipPortableDesktop) {
        Write-Step 'Skipping portable desktop build (-SkipPortableDesktop)'
    } else {
        Write-Step 'Building portable desktop package (scripts/build-portable-desktop.ps1)'
        & (Join-Path $PSScriptRoot 'build-portable-desktop.ps1') -OutDir $PortableDesktopDir
    }

    # ── 3. Stage + package ───────────────────────────────────────────────────
    Write-Step "Staging release artifacts at $StageDir"
    if (Test-Path $StageDir) { Remove-Item $StageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

    $installerSrc = Get-ChildItem (Join-Path $RepoRoot 'target/release/bundle/nsis') -Filter '*.exe' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $installerSrc) {
        throw "No NSIS installer found under target/release/bundle/nsis/. Run without -SkipInstaller."
    }
    $installerDest = Join-Path $StageDir "Librarium-$Tag-windows-x86_64-setup.exe"
    Copy-Item $installerSrc.FullName $installerDest -Force

    $portableZip = Join-Path $StageDir "Librarium-$Tag-windows-x86_64-portable-server.zip"
    if (-not (Test-Path $PortableDir)) {
        throw "No portable server package at $PortableDir. Run without -SkipPortableServer."
    }
    Compress-Archive -Path (Join-Path $PortableDir '*') -DestinationPath $portableZip -Force

    $portableDesktopZip = Join-Path $StageDir "Librarium-$Tag-windows-x86_64-portable-desktop.zip"
    if (-not (Test-Path $PortableDesktopDir)) {
        throw "No portable desktop package at $PortableDesktopDir. Run without -SkipPortableDesktop."
    }
    Compress-Archive -Path (Join-Path $PortableDesktopDir '*') -DestinationPath $portableDesktopZip -Force

    $artifacts = @($installerDest, $portableZip, $portableDesktopZip)

    # ── 4. Checksums ──────────────────────────────────────────────────────────
    Write-Step 'Computing SHA256 checksums'
    $newLines = $artifacts | ForEach-Object {
        $hash = (Get-FileHash $_ -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $(Split-Path -Leaf $_)"
    }

    Write-Host ''
    Write-Host 'Staged artifacts:' -ForegroundColor Green
    Get-ChildItem $StageDir | Select-Object Name, Length | Format-Table -AutoSize
    $newLines | ForEach-Object { Write-Host $_ }

    if ($DryRun) {
        Write-Host ''
        Write-Host "-DryRun: not touching the GitHub release for $Tag." -ForegroundColor Yellow
        return
    }

    # ── 5. Create the release if it doesn't exist yet ───────────────────────
    gh release view $Tag *> $null
    $releaseExists = ($LASTEXITCODE -eq 0)

    if (-not $releaseExists) {
        Write-Step "Creating release $Tag"
        $prereleaseFlag = @()
        if ($Tag -match '-(rc|beta|alpha)\d*$') { $prereleaseFlag = @('--prerelease') }
        gh release create $Tag --title "Librarium $Tag" --generate-notes @prereleaseFlag
        if ($LASTEXITCODE -ne 0) { throw 'gh release create failed' }
    } else {
        Write-Step "Release $Tag already exists — uploading into it"
    }

    # ── 6. Merge SHA256SUMS.txt (don't clobber other platforms' entries) ────
    $sumsFile = Join-Path $StageDir 'SHA256SUMS.txt'
    $existingSums = @()
    if ($releaseExists) {
        $tmpSums = Join-Path $StageDir '.existing-SHA256SUMS.txt'
        gh release download $Tag -p 'SHA256SUMS.txt' -O $tmpSums *> $null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $tmpSums)) {
            $newNames = $artifacts | ForEach-Object { Split-Path -Leaf $_ }
            $existingSums = Get-Content $tmpSums | Where-Object {
                $line = $_
                -not ($newNames | Where-Object { $line -like "*$_" })
            }
            Remove-Item $tmpSums -Force
        }
    }
    ($existingSums + $newLines) | Set-Content -Path $sumsFile -Encoding ASCII

    # ── 7. Upload ─────────────────────────────────────────────────────────────
    Write-Step "Uploading artifacts to $Tag"
    gh release upload $Tag $installerDest $portableZip $portableDesktopZip $sumsFile --clobber
    if ($LASTEXITCODE -ne 0) { throw 'gh release upload failed' }

    $url = gh release view $Tag --json url --jq '.url'
    Write-Host ''
    Write-Host "Done: $url" -ForegroundColor Green
} finally {
    Pop-Location
}
