#!/usr/bin/env bash
# Build a minimal .deb package for the Librarium server binary.
#
# Usage: bash scripts/package-server-deb.sh <version>
# Example: bash scripts/package-server-deb.sh v0.2.0
#
# Expects the release binary to already exist at target/release/librarium.
# Produces dist/librarium_<version>_amd64.deb.

set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
# Strip leading 'v' for Debian version fields.
DEB_VERSION="${VERSION#v}"
ARCH="amd64"
PKG_NAME="librarium_${DEB_VERSION}_${ARCH}"
BUILD_DIR="$(mktemp -d)"
DIST_DIR="$(pwd)/dist"

echo "==> Packaging librarium ${DEB_VERSION} as ${PKG_NAME}.deb"

mkdir -p \
  "${BUILD_DIR}/DEBIAN" \
  "${BUILD_DIR}/usr/bin" \
  "${BUILD_DIR}/lib/systemd/system" \
  "${BUILD_DIR}/etc/librarium"

# ── Binary ──────────────────────────────────────────────────────────────────
cp target/release/librarium "${BUILD_DIR}/usr/bin/librarium"
chmod 755 "${BUILD_DIR}/usr/bin/librarium"

# ── Default config (non-executable, non-conffile — admin edits in /etc) ─────
cat > "${BUILD_DIR}/etc/librarium/config.toml" <<'TOMLEOF'
# /etc/librarium/config.toml — production defaults installed by the package.
# Edit this file, then restart: sudo systemctl restart librarium
#
# All values can be overridden with environment variables using double
# underscores for nesting: LIBRARIUM__AUTH__JWT_SECRET="..." etc.

[server]
host = "127.0.0.1"   # Change to 0.0.0.0 to listen on all interfaces
port = 8080

[database]
path = "/var/lib/librarium/librarium.db"

[vault]
base_dir = "/var/lib/librarium/vaults"

[auth]
enabled = true
provider = "password"   # "password" | "ldap" | "oidc"
# REQUIRED: set a strong random secret before first run.
# Generate one: openssl rand -hex 32
jwt_secret = ""

# First-run bootstrap: creates the initial admin account when no users exist.
# Remove or leave commented out after the first login.
# bootstrap_admin_username = "admin"
# bootstrap_admin_password = ""

access_token_ttl  = 3600
refresh_token_ttl = 604800

[cors]
allowed_origins = []

[sync]
change_log_retention_days = 30
TOMLEOF

# ── systemd unit ─────────────────────────────────────────────────────────────
cat > "${BUILD_DIR}/lib/systemd/system/librarium.service" <<'UNITEOF'
[Unit]
Description=Librarium Server
Documentation=https://github.com/cyrex562/librarium
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=librarium
Group=librarium
WorkingDirectory=/var/lib/librarium
ExecStart=/usr/bin/librarium --config /etc/librarium/config.toml
Restart=on-failure
RestartSec=5
# Harden the service process.
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/librarium
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
UNITEOF

# ── DEBIAN/control ──────────────────────────────────────────────────────────
cat > "${BUILD_DIR}/DEBIAN/control" <<CTRLEOF
Package: librarium
Version: ${DEB_VERSION}
Architecture: ${ARCH}
Maintainer: Librarium Project <noreply@example.com>
Description: Librarium server — offline-first markdown knowledge base
 Self-hosted server for Librarium, a local-first, air-gap-compatible
 markdown knowledge base with full-text search, entity relationships,
 multi-user support, and optional OIDC/LDAP authentication.
Homepage: https://github.com/cyrex562/librarium
Section: utils
Priority: optional
Depends: adduser
CTRLEOF

# ── DEBIAN/conffiles ─────────────────────────────────────────────────────────
# Mark /etc/librarium/config.toml as a conffile so dpkg preserves local edits
# on upgrade.
echo "/etc/librarium/config.toml" > "${BUILD_DIR}/DEBIAN/conffiles"

# ── DEBIAN/postinst ──────────────────────────────────────────────────────────
cat > "${BUILD_DIR}/DEBIAN/postinst" <<'POSTEOF'
#!/bin/sh
set -e

# Create the dedicated service user if it doesn't already exist.
if ! id -u librarium >/dev/null 2>&1; then
    adduser --system --group --no-create-home \
            --home /var/lib/librarium --shell /usr/sbin/nologin \
            librarium
fi

# Create data directory.
mkdir -p /var/lib/librarium/vaults
chown -R librarium:librarium /var/lib/librarium

# Reload systemd and enable the unit (but don't start automatically —
# the admin must set jwt_secret in /etc/librarium/config.toml first).
if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
    systemctl enable librarium.service >/dev/null 2>&1 || true
fi

echo ""
echo "Librarium installed."
echo ""
echo "Before starting the service:"
echo "  1. Edit /etc/librarium/config.toml"
echo "  2. Set jwt_secret to: \$(openssl rand -hex 32)"
echo "  3. Set bootstrap_admin_password for the first admin account"
echo "  4. sudo systemctl start librarium"
echo ""
POSTEOF
chmod 755 "${BUILD_DIR}/DEBIAN/postinst"

# ── DEBIAN/prerm ─────────────────────────────────────────────────────────────
cat > "${BUILD_DIR}/DEBIAN/prerm" <<'PREEOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    systemctl stop librarium.service >/dev/null 2>&1 || true
    systemctl disable librarium.service >/dev/null 2>&1 || true
fi
PREEOF
chmod 755 "${BUILD_DIR}/DEBIAN/prerm"

# ── Build the package ────────────────────────────────────────────────────────
mkdir -p "${DIST_DIR}"
dpkg-deb --build --root-owner-group "${BUILD_DIR}" \
  "${DIST_DIR}/${PKG_NAME}.deb"

echo "==> Created ${DIST_DIR}/${PKG_NAME}.deb"
