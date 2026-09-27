#!/usr/bin/env bash
# Tests for scripts/librarium-upgrade.sh.
#
# Hermetic: no root, no network, no real systemd. A stub `systemctl` on PATH
# runs the unit's ExecStart in the background, and the "server" is a small
# bash + python3 program that answers /api/health and /api/version the way
# the real one does. Run by `cargo xtask ci`.
#
#   bash scripts/tests/librarium-upgrade.test.sh

set -euo pipefail

SCRIPT="$(cd "$(dirname "$0")/.." && pwd)/librarium-upgrade.sh"
ROOT="$(mktemp -d)"
FAILURES=0
PASSES=0

cleanup() {
    # Stop any fake server still running.
    for pidfile in "$ROOT"/*/state/pid; do
        [ -f "$pidfile" ] && kill "$(cat "$pidfile")" 2>/dev/null || true
    done
    rm -rf "$ROOT"
}
trap cleanup EXIT

pass() { PASSES=$((PASSES + 1)); printf '  ok    %s\n' "$1"; }
fail() { FAILURES=$((FAILURES + 1)); printf '  FAIL  %s\n' "$1"; }
check() { if eval "$2"; then pass "$1"; else fail "$1"; fi; }

free_port() {
    python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'
}

# make_server FILE VERSION [unhealthy]: a fake librarium binary. On start it
# "migrates" the database by appending a line, like a schema migration would.
make_server() {
    local file="$1" version="$2" status="${3:-healthy}"
    cat >"$file" <<EOF
#!/usr/bin/env bash
if [ "\${1:-}" = "--version" ]; then echo "librarium $version"; exit 0; fi
config="./config.toml"
while [ \$# -gt 0 ]; do
    case "\$1" in --config) config="\$2"; shift 2 ;; *) shift ;; esac
done
port=\$(awk -F' *= *' '/^port/ { print \$2 }' "\$config")
db=\$(awk -F' *= *' '/^path/ { gsub(/"/, "", \$2); print \$2 }' "\$config")
echo "migrated by $version" >>"\$db"
exec python3 -c '
import http.server, json, sys
status, version, port = sys.argv[1], sys.argv[2], int(sys.argv[3])
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api/health":
            code, body = (200 if status == "healthy" else 503), {"status": status, "database": "connected"}
        elif self.path == "/api/version":
            code, body = 200, {"version": version, "git_hash": "test", "build_date": "today"}
        else:
            code, body = 404, {}
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *args):
        pass
http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
' "$status" "$version" "\$port"
EOF
    chmod 755 "$file"
}

# make_stubs DIR: systemctl (and dpkg) stubs driven by DIR/unit.
make_stubs() {
    local dir="$1"
    mkdir -p "$dir/stubs" "$dir/state"
    cat >"$dir/stubs/systemctl" <<EOF
#!/usr/bin/env bash
state="$dir/state"
unit="$dir/unit"
alive() { [ -f "\$state/pid" ] && kill -0 "\$(cat "\$state/pid")" 2>/dev/null; }
case "\$1" in
    cat) [ -f "\$unit" ] && { echo "# \$unit"; cat "\$unit"; } ;;
    is-active) alive ;;
    stop)
        if alive; then kill "\$(cat "\$state/pid")"; while alive; do sleep 0.1; done; fi
        rm -f "\$state/pid"
        ;;
    start)
        alive && exit 0
        exec_start=\$(awk -F= '/^ExecStart=/ { sub(/^ExecStart=/, ""); print }' "\$unit")
        workdir=\$(awk -F= '/^WorkingDirectory=/ { print \$2 }' "\$unit")
        (cd "\$workdir" && exec \$exec_start >>"\$state/log" 2>&1) &
        echo \$! >"\$state/pid"
        ;;
    daemon-reload) ;;
    *) echo "stub systemctl: unsupported \$*" >&2; exit 1 ;;
esac
EOF
    chmod 755 "$dir/stubs/systemctl"
}

# release_dir DIR TAG VERSION [unhealthy]: a --from-dir with the binary
# asset and SHA256SUMS.txt.
release_dir() {
    local dir="$1" tag="$2" version="$3" status="${4:-healthy}"
    mkdir -p "$dir"
    make_server "$dir/librarium-$tag-linux-x86_64" "$version" "$status"
    (cd "$dir" && sha256sum librarium-* >SHA256SUMS.txt)
}

# setup_binary NAME: a plain-binary install running version 1.0.0.
setup_binary() {
    local t="$ROOT/$1"
    mkdir -p "$t/opt" "$t/data"
    make_stubs "$t"
    make_server "$t/opt/librarium" 1.0.0
    PORT="$(free_port)"
    printf '[server]\nhost = "0.0.0.0"\nport = %s\n\n[database]\npath = "%s"\n' "$PORT" "$t/data/librarium.db" >"$t/opt/config.toml"
    echo "original data" >"$t/data/librarium.db"
    printf '[Service]\nExecStart=%s --config %s\nWorkingDirectory=%s\n' "$t/opt/librarium" "$t/opt/config.toml" "$t/data" >"$t/unit"
    PATH="$t/stubs:$PATH" systemctl start librarium
    wait_up "$PORT"
    T="$t"
}

wait_up() {
    for _ in $(seq 50); do
        curl -fsS "http://127.0.0.1:$1/api/version" >/dev/null 2>&1 && return 0
        sleep 0.1
    done
    echo "fake server on port $1 never came up" >&2
    return 1
}

running_version() {
    curl -fsS "http://127.0.0.1:$PORT/api/version" 2>/dev/null | sed -nE 's/.*"version": *"([^"]+)".*/\1/p'
}

upgrade() {
    PATH="$T/stubs:$PATH" bash "$SCRIPT" --yes --timeout 10 "$@" >"$T/out" 2>&1
}

echo "librarium-upgrade.sh"

# ── plain binary: happy path ────────────────────────────────────────────────
setup_binary happy
release_dir "$T/dl" v2.0.0 2.0.0
config_before="$(sha256sum "$T/opt/config.toml")"
if upgrade --from-dir "$T/dl"; then pass "upgrade exits 0"; else fail "upgrade exits 0"; cat "$T/out"; fi
check "service now reports 2.0.0" '[ "$(running_version)" = 2.0.0 ]'
check "binary on disk is 2.0.0" '"$T/opt/librarium" --version | grep -q 2.0.0'
check "database backed up before the swap" 'grep -qx "original data" "$T"/data/librarium.db.pre-2.0.0.*.bak && ! grep -q "2.0.0" "$T"/data/librarium.db.pre-2.0.0.*.bak'
check "config untouched" '[ "$(sha256sum "$T/opt/config.toml")" = "$config_before" ]'
check "previous binary cleaned up" '[ ! -e "$T/opt/librarium.prev" ]'
check "upgrade script installed next to the binary" '[ -x "$T/opt/librarium-upgrade" ]'
check "health checked via 127.0.0.1 for host 0.0.0.0" 'grep -q "http://127.0.0.1:$PORT/api/health" "$T/out"'

# ── already up to date ──────────────────────────────────────────────────────
if upgrade --from-dir "$T/dl"; then pass "rerun exits 0"; else fail "rerun exits 0"; fi
check "rerun reports nothing to do" 'grep -q "already running 2.0.0" "$T/out"'
check "rerun makes no second backup" '[ "$(ls "$T"/data/librarium.db.pre-* | wc -l)" -eq 1 ]'

# ── checksum mismatch ───────────────────────────────────────────────────────
setup_binary tampered
release_dir "$T/dl" v2.0.0 2.0.0
echo "# tampered" >>"$T/dl/librarium-v2.0.0-linux-x86_64"
if upgrade --from-dir "$T/dl"; then fail "tampered asset is rejected"; else pass "tampered asset is rejected"; fi
check "mismatch is reported" 'grep -q "checksum mismatch" "$T/out"'
check "old version still running" '[ "$(running_version)" = 1.0.0 ]'
check "no backup taken before verification" '! ls "$T"/data/librarium.db.pre-* >/dev/null 2>&1'

# ── asset missing from SHA256SUMS.txt ───────────────────────────────────────
setup_binary unlisted
release_dir "$T/dl" v2.0.0 2.0.0
: >"$T/dl/SHA256SUMS.txt"
if upgrade --from-dir "$T/dl"; then fail "unlisted asset is rejected"; else pass "unlisted asset is rejected"; fi
check "old version still running (unlisted)" '[ "$(running_version)" = 1.0.0 ]'

# ── failed health check rolls back ──────────────────────────────────────────
setup_binary rollback
release_dir "$T/dl" v2.0.0 2.0.0 unhealthy
if upgrade --from-dir "$T/dl"; then fail "failed upgrade exits non-zero"; else pass "failed upgrade exits non-zero"; fi
check "rolled back to 1.0.0" '[ "$(running_version)" = 1.0.0 ]'
check "binary on disk is 1.0.0 again" '"$T/opt/librarium" --version | grep -q 1.0.0'
check "database restored from the backup" 'grep -qx "original data" "$T/data/librarium.db" && ! grep -q "migrated by 2.0.0" "$T/data/librarium.db"'
check "failed database kept aside" 'grep -q "migrated by 2.0.0" "$T"/data/librarium.db.failed-2.0.0.*'
check "rollback is reported" 'grep -q "Rolling back to 1.0.0" "$T/out"'

# ── dry run changes nothing ─────────────────────────────────────────────────
setup_binary dryrun
release_dir "$T/dl" v2.0.0 2.0.0
bin_before="$(sha256sum "$T/opt/librarium")"
if upgrade --from-dir "$T/dl" --dry-run; then pass "dry run exits 0"; else fail "dry run exits 0"; cat "$T/out"; fi
check "dry run leaves the binary" '[ "$(sha256sum "$T/opt/librarium")" = "$bin_before" ]'
check "dry run leaves the service running" '[ "$(running_version)" = 1.0.0 ]'
check "dry run takes no backup" '! ls "$T"/data/librarium.db.pre-* >/dev/null 2>&1'

# ── releases/ + current symlink (cargo xtask deploy) ────────────────────────
t="$ROOT/releases"
mkdir -p "$t/app/releases/abc123/plugins/demo" "$t/app/shared"
make_stubs "$t"
make_server "$t/app/releases/abc123/librarium" 1.0.0
echo "plugin" >"$t/app/releases/abc123/plugins/demo/manifest.json"
ln -s "$t/app/releases/abc123" "$t/app/current"
PORT="$(free_port)"
# No --config: the server reads ./config.toml from the working directory, and
# the database path is relative to it.
printf '[server]\nport = %s\n\n[database]\npath = "./librarium.db"\n' "$PORT" >"$t/app/shared/config.toml"
echo "original data" >"$t/app/shared/librarium.db"
printf '[Service]\nExecStart=%s\nWorkingDirectory=%s\n' "$t/app/current/librarium" "$t/app/shared" >"$t/unit"
PATH="$t/stubs:$PATH" systemctl start librarium
wait_up "$PORT"
T="$t"
release_dir "$T/dl" v2.0.0 2.0.0
if upgrade --from-dir "$T/dl"; then pass "releases layout: upgrade exits 0"; else fail "releases layout: upgrade exits 0"; cat "$T/out"; fi
check "releases layout: detected" 'grep -q "layout:   releases" "$T/out"'
check "releases layout: current points at releases/v2.0.0" '[ "$(readlink "$T/app/current")" = "$T/app/releases/v2.0.0" ]'
check "releases layout: old release kept" '"$T/app/releases/abc123/librarium" --version | grep -q 1.0.0'
check "releases layout: plugins carried over" '[ -f "$T/app/releases/v2.0.0/plugins/demo/manifest.json" ]'
check "releases layout: relative database backed up" 'ls "$T"/app/shared/librarium.db.pre-2.0.0.*.bak >/dev/null 2>&1'
check "releases layout: service reports 2.0.0" '[ "$(running_version)" = 2.0.0 ]'

# ── Debian package ──────────────────────────────────────────────────────────
# A stub dpkg owns $t/usr/bin/librarium and "installs" a package by
# extracting its binary there; the package itself is built for real.
if command -v dpkg-deb >/dev/null 2>&1; then
    t="$ROOT/deb"
    mkdir -p "$t/usr/bin" "$t/etc" "$t/var" "$t/pkg/DEBIAN" "$t/pkg/usr/bin" "$t/dl"
    make_stubs "$t"
    make_server "$t/usr/bin/librarium" 1.0.0
    cat >"$t/stubs/dpkg" <<EOF
#!/usr/bin/env bash
case "\$1" in
    -S) [ "\$2" = "$t/usr/bin/librarium" ] && echo "librarium: \$2" ;;
    -i) dpkg-deb --fsys-tarfile "\$2" | tar -xO ./usr/bin/librarium >"$t/usr/bin/librarium.tmp" \
            && chmod 755 "$t/usr/bin/librarium.tmp" && mv "$t/usr/bin/librarium.tmp" "$t/usr/bin/librarium"
        echo "\$2" >>"$t/state/dpkg-installed" ;;
    *) exit 1 ;;
esac
EOF
    chmod 755 "$t/stubs/dpkg"
    make_server "$t/pkg/usr/bin/librarium" 2.0.0
    printf 'Package: librarium\nVersion: 2.0.0\nArchitecture: amd64\nMaintainer: test\nDescription: test\n' >"$t/pkg/DEBIAN/control"
    dpkg-deb --build --root-owner-group "$t/pkg" "$t/dl/librarium_2.0.0_amd64.deb" >/dev/null
    (cd "$t/dl" && sha256sum librarium_2.0.0_amd64.deb >SHA256SUMS.txt)
    PORT="$(free_port)"
    printf '[server]\nport = %s\n\n[database]\npath = "%s"\n' "$PORT" "$t/var/librarium.db" >"$t/etc/config.toml"
    echo "original data" >"$t/var/librarium.db"
    printf '[Service]\nExecStart=%s --config %s\nWorkingDirectory=%s\n' "$t/usr/bin/librarium" "$t/etc/config.toml" "$t/var" >"$t/unit"
    PATH="$t/stubs:$PATH" systemctl start librarium
    wait_up "$PORT"
    T="$t"
    if upgrade --tag v2.0.0 --from-dir "$T/dl"; then pass "deb layout: upgrade exits 0"; else fail "deb layout: upgrade exits 0"; cat "$T/out"; fi
    check "deb layout: detected" 'grep -q "layout:   deb" "$T/out"'
    check "deb layout: package installed with dpkg -i" 'grep -q "librarium_2.0.0_amd64.deb" "$T/state/dpkg-installed"'
    check "deb layout: service reports 2.0.0" '[ "$(running_version)" = 2.0.0 ]'
    check "deb layout: database backed up" 'ls "$T"/var/librarium.db.pre-2.0.0.*.bak >/dev/null 2>&1'
else
    echo "  skip  deb layout (dpkg-deb not installed)"
fi

# ── unrecognized install refuses ────────────────────────────────────────────
t="$ROOT/unknown"
mkdir -p "$t/opt"
make_stubs "$t"
printf '#!/bin/sh\necho "something 9"\n' >"$t/opt/other-server"
chmod 755 "$t/opt/other-server"
printf '[Service]\nExecStart=%s\nWorkingDirectory=%s\n' "$t/opt/other-server" "$t/opt" >"$t/unit"
T="$t"
if upgrade --from-dir "$ROOT/happy/dl"; then fail "unknown binary refused"; else pass "unknown binary refused"; fi
check "refusal names the binary" 'grep -q "refusing to guess" "$T/out"'

# ── missing database refuses unless told ────────────────────────────────────
setup_binary nodb
release_dir "$T/dl" v2.0.0 2.0.0
rm "$T/data/librarium.db"
if upgrade --from-dir "$T/dl"; then fail "missing database refused"; else pass "missing database refused"; fi
check "old version still running (no db)" '[ "$(running_version)" = 1.0.0 ]'

echo
echo "$PASSES passed, $FAILURES failed"
[ "$FAILURES" -eq 0 ]
