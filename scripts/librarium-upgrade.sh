#!/usr/bin/env bash
# Upgrade a Librarium server to a published GitHub release, or install one.
#
# Needs only bash, curl and coreutils on the server: no git checkout, Rust or
# Node. The same script is attached to every GitHub release, shipped in the
# .deb as /usr/bin/librarium-upgrade, and copied next to the server binary
# after each successful upgrade.
#
#   sudo librarium-upgrade                    # latest stable release
#   sudo librarium-upgrade --prerelease       # newest release, -rc builds included
#   sudo librarium-upgrade --tag v0.103.0     # a specific release
#   sudo librarium-upgrade --from-dir ./dl    # air-gapped: assets + SHA256SUMS.txt
#
# What it does, in order:
#   1. Finds the installed server from its systemd unit (ExecStart,
#      WorkingDirectory, --config / LIBRARIUM_CONFIG) and recognizes the
#      layout: a Debian package, a plain binary, or a releases/ + current
#      symlink install (cargo xtask deploy). Anything else: it stops.
#   2. Downloads the release asset and checks it against SHA256SUMS.txt
#      before touching the install.
#   3. Stops the service and backs up the database next to itself
#      (librarium.db.pre-<version>.<timestamp>.bak). Migrations run on
#      startup, so that copy is the recovery point.
#   4. Swaps the binary atomically (write alongside, then rename), starts the
#      service, and waits for /api/health and for /api/version to report the
#      new version.
#   5. If that check fails, puts the previous binary and the database backup
#      back and restarts the old version.
#
# It never edits config.toml or vault data.
#
# With no installed service found, it installs the Debian package (on
# systems with dpkg). Docker installs upgrade with
# `docker compose pull && docker compose up -d` instead.

set -euo pipefail

REPO="cyrex562/librarium"
GITHUB="https://github.com/${REPO}"
GITHUB_API="https://api.github.com/repos/${REPO}"

SERVICE="librarium"
TAG=""
PRERELEASE=0
FROM_DIR=""
DB_OVERRIDE=""
URL_OVERRIDE=""
SKIP_BACKUP=0
FORCE=0
DRY_RUN=0
ASSUME_YES=0
TIMEOUT=60

usage() {
    sed -n '2,/^$/{s/^# \{0,1\}//;p}' "$0"
    cat <<'EOF'
Options:
  --tag vX.Y.Z       Upgrade to this release instead of the latest.
  --prerelease       Consider pre-releases (-rc builds) when picking the latest.
  --from-dir DIR     Use release assets already downloaded into DIR (must
                     include SHA256SUMS.txt) instead of fetching from GitHub.
  --service NAME     systemd unit name (default: librarium).
  --db PATH          Database to back up, if it can't be worked out from the
                     unit and config.
  --url URL          Base URL for the health check (default: from config).
  --skip-backup      Don't back up the database (not recommended).
  --timeout SECS     How long to wait for the new version to become healthy
                     (default: 60).
  --force            Reinstall even if that version is already running.
  --dry-run          Show what would happen; change nothing.
  -y, --yes          Don't ask for confirmation.
  -h, --help         Show this help.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --tag) TAG="${2:?--tag needs a value}"; shift 2 ;;
        --tag=*) TAG="${1#*=}"; shift ;;
        --prerelease) PRERELEASE=1; shift ;;
        --from-dir) FROM_DIR="${2:?--from-dir needs a value}"; shift 2 ;;
        --from-dir=*) FROM_DIR="${1#*=}"; shift ;;
        --service) SERVICE="${2:?--service needs a value}"; shift 2 ;;
        --service=*) SERVICE="${1#*=}"; shift ;;
        --db) DB_OVERRIDE="${2:?--db needs a value}"; shift 2 ;;
        --db=*) DB_OVERRIDE="${1#*=}"; shift ;;
        --url) URL_OVERRIDE="${2:?--url needs a value}"; shift 2 ;;
        --url=*) URL_OVERRIDE="${1#*=}"; shift ;;
        --skip-backup) SKIP_BACKUP=1; shift ;;
        --timeout) TIMEOUT="${2:?--timeout needs a value}"; shift 2 ;;
        --timeout=*) TIMEOUT="${1#*=}"; shift ;;
        --force) FORCE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        -y|--yes) ASSUME_YES=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
done

# ── Output helpers ──────────────────────────────────────────────────────────

step() { printf '\n==> %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# Run a command, or only print it under --dry-run.
act() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '    [dry-run] %s\n' "$*"
    else
        "$@"
    fi
}

confirm() {
    [ "$ASSUME_YES" -eq 1 ] || [ "$DRY_RUN" -eq 1 ] && return 0
    [ -t 0 ] || die "not a terminal; pass --yes to proceed without confirmation"
    printf '%s [y/N] ' "$1"
    local reply
    read -r reply
    case "$reply" in y|Y|yes|YES) return 0 ;; *) die "cancelled" ;; esac
}

need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed"; }

need curl
need sha256sum
need systemctl

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── Small parsers ───────────────────────────────────────────────────────────

# toml_get FILE SECTION KEY: the value of KEY in [SECTION], quotes and
# trailing comments stripped. Enough for Librarium's flat config.toml.
toml_get() {
    [ -f "$1" ] || return 0
    awk -v want="$2" -v key="$3" '
        /^[[:space:]]*\[/ {
            s = $0; gsub(/^[[:space:]]*\[|\][[:space:]]*(#.*)?$/, "", s); section = s; next
        }
        section == want {
            line = $0
            if (match(line, "^[[:space:]]*" key "[[:space:]]*=")) {
                v = substr(line, RLENGTH + 1)
                sub(/^[[:space:]]*/, "", v)
                if (v ~ /^"/) { v = substr(v, 2); sub(/".*$/, "", v) }
                else if (v ~ /^'\''/) { v = substr(v, 2); sub(/'\''.*$/, "", v) }
                else { sub(/[[:space:]]*#.*$/, "", v); sub(/[[:space:]]*$/, "", v) }
                print v; exit
            }
        }' "$1"
}

# unit_value KEY: the effective value of KEY= in the unit (last one wins; an
# empty assignment resets, as systemd does for ExecStart=).
unit_value() {
    printf '%s\n' "$UNIT_TEXT" | awk -v key="$1" '
        index($0, key "=") == 1 { v = substr($0, length(key) + 2) }
        END { print v }'
}

# unit_env NAME: NAME's value from the unit's Environment= lines.
unit_env() {
    printf '%s\n' "$UNIT_TEXT" | awk -v name="$1" '
        index($0, "Environment=") == 1 {
            n = split(substr($0, 13), parts, /[[:space:]]+/)
            for (i = 1; i <= n; i++) {
                p = parts[i]; gsub(/"/, "", p)
                if (index(p, name "=") == 1) v = substr(p, length(name) + 2)
            }
        }
        END { print v }'
}

# Absolute path of $1, relative to directory $2 when it isn't absolute.
resolve_in() {
    case "$1" in
        /*) printf '%s\n' "$1" ;;
        *) printf '%s/%s\n' "${2%/}" "${1#./}" ;;
    esac
}

binary_version() {
    # clap prints "librarium 0.102.19".
    "$1" --version 2>/dev/null | awk '{ print $NF; exit }' || true
}

# ── Resolve the target release ──────────────────────────────────────────────

resolve_tag() {
    if [ -n "$TAG" ]; then
        case "$TAG" in v*) ;; *) TAG="v$TAG" ;; esac
        return
    fi
    if [ -n "$FROM_DIR" ]; then
        # The binary asset's name carries the tag.
        local asset
        asset="$(cd "$FROM_DIR" && ls librarium-v*-linux-x86_64 2>/dev/null | head -n 1 || true)"
        [ -n "$asset" ] || die "no librarium-v*-linux-x86_64 in $FROM_DIR; pass --tag"
        TAG="${asset#librarium-}"
        TAG="${TAG%-linux-x86_64}"
        return
    fi
    if [ "$PRERELEASE" -eq 1 ]; then
        # Newest first; the first tag_name is the newest release of any kind.
        TAG="$(curl -fsSL "${GITHUB_API}/releases?per_page=1" \
            | grep -m 1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/' || true)"
    else
        # /releases/latest redirects to the newest non-prerelease.
        local location
        location="$(curl -fsSI "${GITHUB}/releases/latest" | tr -d '\r' \
            | awk 'tolower($1) == "location:" { print $2 }' | tail -n 1 || true)"
        case "$location" in
            */releases/tag/*) TAG="${location##*/}" ;;
        esac
    fi
    [ -n "$TAG" ] || die "couldn't find a release$([ "$PRERELEASE" -eq 1 ] || echo ' (only pre-releases published? try --prerelease)')"
}

# fetch ASSET: download (or copy) a release asset into $WORK and check it
# against SHA256SUMS.txt. Prints the local path.
fetch() {
    local asset="$1" dest="$WORK/$1"
    if [ ! -f "$WORK/SHA256SUMS.txt" ]; then
        get_asset SHA256SUMS.txt "$WORK/SHA256SUMS.txt" \
            || die "release $TAG has no SHA256SUMS.txt; refusing to install an unverified binary"
    fi
    get_asset "$asset" "$dest" || die "release $TAG has no asset named $asset"
    local want got
    want="$(awk -v f="$asset" '{ n = $2; sub(/^\*/, "", n) } n == f { print $1; exit }' "$WORK/SHA256SUMS.txt")"
    [ -n "$want" ] || die "$asset is not listed in SHA256SUMS.txt; refusing to install it"
    got="$(sha256sum "$dest" | awk '{ print $1 }')"
    [ "$want" = "$got" ] || die "checksum mismatch for $asset (expected $want, got $got)"
    printf '%s\n' "$dest"
}

get_asset() {
    if [ -n "$FROM_DIR" ]; then
        [ -f "$FROM_DIR/$1" ] && cp "$FROM_DIR/$1" "$2"
    else
        curl -fsSL -o "$2" "${GITHUB}/releases/download/${TAG}/$1"
    fi
}

# ── Service control and health ──────────────────────────────────────────────

WAS_ACTIVE=0

stop_service() {
    if systemctl is-active --quiet "$SERVICE"; then
        WAS_ACTIVE=1
        info "stopping $SERVICE"
        act systemctl stop "$SERVICE"
    fi
}

start_service() {
    info "starting $SERVICE"
    act systemctl start "$SERVICE"
}

# wait_healthy VERSION: /api/health reports healthy and /api/version reports
# VERSION, within $TIMEOUT seconds.
wait_healthy() {
    local want="$1" deadline=$((SECONDS + TIMEOUT)) health version
    local curl_opts=(-fsS --max-time 5)
    case "$BASE_URL" in https://*) curl_opts+=(-k) ;; esac
    while [ "$SECONDS" -lt "$deadline" ]; do
        health="$(curl "${curl_opts[@]}" "$BASE_URL/api/health" 2>/dev/null || true)"
        if printf '%s' "$health" | grep -q '"status" *: *"healthy"'; then
            version="$(curl "${curl_opts[@]}" "$BASE_URL/api/version" 2>/dev/null \
                | sed -nE 's/.*"version" *: *"([^"]+)".*/\1/p' || true)"
            if [ "$version" = "$want" ]; then
                return 0
            fi
        fi
        sleep 1
    done
    info "last /api/health response: ${health:-<none>}"
    info "last /api/version: ${version:-<none>}"
    return 1
}

# ── First install (no service yet) ──────────────────────────────────────────

first_install() {
    command -v dpkg >/dev/null 2>&1 \
        || die "no '$SERVICE' service found, and first install is only automated for the Debian package (dpkg). See docs/USER_GUIDE.md for a manual install with the release binary and deploy/systemd/librarium.service.template."
    resolve_tag
    step "Installing Librarium ${TAG} from the Debian package"
    confirm "Install librarium ${TAG}?"
    local deb
    deb="$(fetch "librarium_${TAG#v}_amd64.deb")"
    act dpkg -i "$deb"
    info "Installed. Set jwt_secret in /etc/librarium/config.toml, then: systemctl start $SERVICE"
}

# ── Discover the installed server ───────────────────────────────────────────

UNIT_TEXT="$(systemctl cat "$SERVICE" 2>/dev/null || true)"
if [ -z "$UNIT_TEXT" ]; then
    first_install
    exit 0
fi

step "Inspecting the installed server ($SERVICE.service)"

EXEC_START="$(unit_value ExecStart)"
[ -n "$EXEC_START" ] || die "$SERVICE.service has no ExecStart"
# Drop systemd's optional exec prefixes (-, @, +, !).
EXEC_START="${EXEC_START#[-@+!]}"
read -r -a EXEC_ARGV <<<"$EXEC_START"
EXEC_PATH="${EXEC_ARGV[0]}"
[ -x "$EXEC_PATH" ] || die "ExecStart binary $EXEC_PATH doesn't exist or isn't executable"
BIN="$(readlink -f "$EXEC_PATH")"
case "$(basename "$BIN")" in
    librarium) ;;
    *) die "ExecStart runs $BIN, which doesn't look like the librarium server binary; refusing to guess" ;;
esac

WORKDIR="$(unit_value WorkingDirectory)"
WORKDIR="${WORKDIR#-}"
[ -n "$WORKDIR" ] || WORKDIR="/"

# Config path, mirroring the server: --config > LIBRARIUM_CONFIG >
# exe-adjacent file > working directory.
CONFIG_ARG=""
for ((i = 1; i < ${#EXEC_ARGV[@]}; i++)); do
    case "${EXEC_ARGV[$i]}" in
        --config|-c) CONFIG_ARG="${EXEC_ARGV[$((i + 1))]:-}" ;;
        --config=*) CONFIG_ARG="${EXEC_ARGV[$i]#--config=}" ;;
    esac
done
[ -n "$CONFIG_ARG" ] || CONFIG_ARG="$(unit_env LIBRARIUM_CONFIG)"
[ -n "$CONFIG_ARG" ] || CONFIG_ARG="./config.toml"
case "$CONFIG_ARG" in
    /*) CONFIG="$CONFIG_ARG" ;;
    *)
        if [ -f "$(resolve_in "$CONFIG_ARG" "$(dirname "$BIN")")" ]; then
            CONFIG="$(resolve_in "$CONFIG_ARG" "$(dirname "$BIN")")"
        else
            CONFIG="$(resolve_in "$CONFIG_ARG" "$WORKDIR")"
        fi
        ;;
esac
[ -f "$CONFIG" ] || warn "config file $CONFIG not found; assuming defaults"

# Database: env override > [database] path > ./librarium.db, relative to the
# working directory.
if [ -n "$DB_OVERRIDE" ]; then
    DB="$DB_OVERRIDE"
else
    DB="$(unit_env LIBRARIUM__DATABASE__PATH)"
    [ -n "$DB" ] || DB="$(toml_get "$CONFIG" database path)"
    [ -n "$DB" ] || DB="./librarium.db"
    DB="$(resolve_in "$DB" "$WORKDIR")"
fi

# Health-check URL from [server] and [tls].
if [ -n "$URL_OVERRIDE" ]; then
    BASE_URL="${URL_OVERRIDE%/}"
else
    HOST="$(unit_env LIBRARIUM__SERVER__HOST)"
    [ -n "$HOST" ] || HOST="$(toml_get "$CONFIG" server host)"
    PORT="$(unit_env LIBRARIUM__SERVER__PORT)"
    [ -n "$PORT" ] || PORT="$(toml_get "$CONFIG" server port)"
    case "${HOST:-}" in ""|0.0.0.0|"::"|"[::]") HOST="127.0.0.1" ;; esac
    SCHEME="http"
    [ -n "$(toml_get "$CONFIG" tls cert_file)" ] && SCHEME="https"
    BASE_URL="${SCHEME}://${HOST}:${PORT:-8080}"
fi

# Layout.
LAYOUT="binary"
if command -v dpkg >/dev/null 2>&1 && dpkg -S "$BIN" 2>/dev/null | grep -q '^librarium:'; then
    LAYOUT="deb"
else
    # cargo xtask deploy: <app>/current -> <app>/releases/<id>/librarium
    EXEC_DIR="$(dirname "$EXEC_PATH")"
    if [ -L "$EXEC_DIR" ] && [ "$(basename "$(dirname "$(readlink -f "$EXEC_DIR")")")" = "releases" ]; then
        LAYOUT="releases"
        CURRENT_LINK="$EXEC_DIR"
        OLD_RELEASE="$(readlink -f "$EXEC_DIR")"
        RELEASES_DIR="$(dirname "$OLD_RELEASE")"
    fi
fi

OLD_VERSION="$(binary_version "$BIN")"
[ -n "$OLD_VERSION" ] || die "$BIN --version printed nothing; is this the librarium server?"

info "binary:   $BIN (version $OLD_VERSION)"
info "layout:   $LAYOUT"
info "config:   $CONFIG"
info "database: $DB"
info "health:   $BASE_URL/api/health"

if [ "$SKIP_BACKUP" -eq 0 ] && [ ! -f "$DB" ]; then
    die "database $DB not found; pass --db PATH, or --skip-backup if this server has no data yet"
fi
[ "$(uname -m)" = "x86_64" ] || die "release binaries are x86_64 only; this machine is $(uname -m)"

# ── Fetch and verify the new release ────────────────────────────────────────

resolve_tag
step "Fetching Librarium ${TAG}"
if [ "$LAYOUT" = "deb" ]; then
    NEW_ASSET="$(fetch "librarium_${TAG#v}_amd64.deb")"
    # The version check below needs the binary out of the package.
    need dpkg-deb
    dpkg-deb --fsys-tarfile "$NEW_ASSET" | tar -xO ./usr/bin/librarium >"$WORK/librarium.new"
    chmod 755 "$WORK/librarium.new"
    NEW_BIN="$WORK/librarium.new"
else
    NEW_BIN="$(fetch "librarium-${TAG}-linux-x86_64")"
    chmod 755 "$NEW_BIN"
fi
info "checksum verified"

NEW_VERSION="$(binary_version "$NEW_BIN")"
[ -n "$NEW_VERSION" ] || die "the downloaded binary doesn't run on this machine"
info "new version: $NEW_VERSION"

if [ "$NEW_VERSION" = "$OLD_VERSION" ] && [ "$FORCE" -eq 0 ]; then
    info "already running $OLD_VERSION; nothing to do (pass --force to reinstall)"
    exit 0
fi

confirm "Upgrade $SERVICE from $OLD_VERSION to $NEW_VERSION?"

# ── Swap ────────────────────────────────────────────────────────────────────

TS="$(date +%Y%m%d-%H%M%S)"
BACKUP=""

step "Stopping the service"
stop_service

if [ "$SKIP_BACKUP" -eq 0 ]; then
    step "Backing up the database"
    BACKUP="${DB}.pre-${NEW_VERSION}.${TS}.bak"
    act cp -p "$DB" "$BACKUP"
    for suffix in -wal -shm; do
        [ -f "${DB}${suffix}" ] && act cp -p "${DB}${suffix}" "${BACKUP}${suffix}"
    done
    info "$BACKUP"
fi

# Keep a copy of this script next to the new binary so the next upgrade can
# run from the install directory.
install_self() {
    local dir="$1"
    [ -f "$0" ] || return 0
    [ "$(readlink -f "$0")" = "$(readlink -f "$dir/librarium-upgrade" 2>/dev/null || true)" ] && return 0
    act install -m 755 "$0" "$dir/librarium-upgrade" || warn "couldn't copy the upgrade script to $dir"
}

step "Installing $NEW_VERSION"
case "$LAYOUT" in
    deb)
        act dpkg -i "$NEW_ASSET"
        ;;
    binary)
        act cp -p "$BIN" "$BIN.prev"
        act install -m 755 "$NEW_BIN" "$BIN.new"
        if [ "$(id -u)" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
            chown --reference="$BIN" "$BIN.new"
        fi
        act mv -f "$BIN.new" "$BIN"
        install_self "$(dirname "$BIN")"
        ;;
    releases)
        NEW_RELEASE="$RELEASES_DIR/$TAG"
        [ -e "$NEW_RELEASE" ] && NEW_RELEASE="$NEW_RELEASE-$TS"
        act mkdir -p "$NEW_RELEASE"
        act install -m 755 "$NEW_BIN" "$NEW_RELEASE/librarium"
        # Plugins ship in the deploy tarball, not the release binary.
        [ -d "$OLD_RELEASE/plugins" ] && act cp -a "$OLD_RELEASE/plugins" "$NEW_RELEASE/plugins"
        if [ "$(id -u)" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
            chown -R --reference="$OLD_RELEASE" "$NEW_RELEASE"
        fi
        install_self "$NEW_RELEASE"
        act ln -sfn "$NEW_RELEASE" "$CURRENT_LINK.new"
        act mv -Tf "$CURRENT_LINK.new" "$CURRENT_LINK"
        ;;
esac

# ── Start, check, roll back ─────────────────────────────────────────────────

rollback() {
    step "Rolling back to $OLD_VERSION"
    act systemctl stop "$SERVICE" || true
    case "$LAYOUT" in
        deb)
            # Reinstall the previous package from its release, if it has one.
            local old_deb
            if old_deb="$(TAG="v$OLD_VERSION" && rm -f "$WORK/SHA256SUMS.txt" && fetch "librarium_${OLD_VERSION}_amd64.deb")"; then
                act dpkg -i "$old_deb"
            else
                warn "couldn't fetch the $OLD_VERSION package; reinstall it by hand"
            fi
            ;;
        binary)
            act mv -f "$BIN.prev" "$BIN"
            ;;
        releases)
            act ln -sfn "$OLD_RELEASE" "$CURRENT_LINK.new"
            act mv -Tf "$CURRENT_LINK.new" "$CURRENT_LINK"
            ;;
    esac
    if [ -n "$BACKUP" ]; then
        # The new version may already have migrated the schema.
        act mv -f "$DB" "${DB}.failed-${NEW_VERSION}.${TS}"
        act cp -p "$BACKUP" "$DB"
        for suffix in -wal -shm; do
            act rm -f "${DB}${suffix}"
            [ -f "${BACKUP}${suffix}" ] && act cp -p "${BACKUP}${suffix}" "${DB}${suffix}"
        done
        info "database restored from $BACKUP"
    fi
    start_service
    if [ "$DRY_RUN" -eq 0 ] && wait_healthy "$OLD_VERSION"; then
        info "$OLD_VERSION is running again"
    else
        warn "$OLD_VERSION didn't come back healthy either; check: journalctl -u $SERVICE"
    fi
}

step "Starting $NEW_VERSION"
start_service
if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] would wait for $BASE_URL to report $NEW_VERSION"
    exit 0
fi
if wait_healthy "$NEW_VERSION"; then
    [ "$LAYOUT" = "binary" ] && rm -f "$BIN.prev"
    [ "$WAS_ACTIVE" -eq 1 ] || warn "$SERVICE wasn't running before the upgrade; it is now"
    step "Upgraded $SERVICE from $OLD_VERSION to $NEW_VERSION"
    [ -n "$BACKUP" ] && info "database backup: $BACKUP"
    exit 0
fi

warn "$NEW_VERSION didn't become healthy within ${TIMEOUT}s"
rollback
die "upgrade to $NEW_VERSION failed and was rolled back; see journalctl -u $SERVICE"
