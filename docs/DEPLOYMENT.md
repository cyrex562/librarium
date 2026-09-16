# Deployment

For the day-to-day path — building, installing on a box you own, keeping it
updated — see the [README's Quick start](../README.md#quick-start) and
["Running the server on its own box"](../README.md#quick-start) sections;
`cargo xtask local-install` / `cargo xtask update` / `cargo xtask deploy` are
the tested, maintained mechanisms and this document does not duplicate them.

This document covers what those don't: Docker in depth, a from-scratch manual
setup for boxes not using `cargo xtask`, and production hardening.

## Docker

```bash
docker compose up -d
```

`docker-compose.yml` at the repo root builds the image locally (multi-stage:
Node compiles the frontend, Rust compiles the server, the final image is
Debian-slim with just the binary and runtime deps) and mounts a
`librarium_data` volume at `/data` for the database and vaults. Edit its
`environment:` block before first run — the inline comments cover the
bootstrap and rate-limiting knobs; the full set of overridable variables is
in [CONFIGURATION.md](CONFIGURATION.md).

To mount vaults from the host instead of the named volume:

```yaml
volumes:
  - librarium_data:/data
  - ./my-vaults:/data/vaults
```

Then register a vault in the UI using the **container** path
(`/data/vaults/...`), not the host path.

Building and running without Compose:

```bash
docker build -t librarium .
docker run -d --name librarium -p 8080:8080 -v librarium_data:/data librarium
```

There is no published image yet — `image: librarium:latest` in the compose
file builds locally rather than pulling.

## Manual setup (no `cargo xtask`)

If you're not using `cargo xtask local-install`/`update` — a box you're
configuring by hand, or a packaging system of your own:

```bash
# Build (frontend first — it's embedded into the server binary at compile time)
npm --prefix frontend ci && npm --prefix frontend run build
cargo build --release -p librarium-server
# Binary: target/release/librarium
```

Run it directly (`./librarium`), or point it at a config explicitly:

```bash
./librarium --config /etc/librarium/config.toml
# or
LIBRARIUM_CONFIG=/etc/librarium/config.toml ./librarium
```

For a system service, adapt
[`deploy/systemd/librarium.service.template`](../deploy/systemd/librarium.service.template)
— the same template `cargo xtask local-install`/`update` fill in and install
automatically — rather than writing a unit file from scratch. It takes the
service user, working directory, and binary path as placeholders.

## Production hardening

- **TLS and CORS** — see [CONFIGURATION.md](CONFIGURATION.md#tls). Set
  `[tls]` for built-in HTTPS, or terminate TLS in a reverse proxy (Caddy's
  automatic Let's Encrypt is the least-friction option) and proxy plain HTTP
  to loopback. Either way, set `[cors].allowed_origins` to your real origin —
  the default only allows `localhost`.
- **A stable `jwt_secret`** — generate one (`openssl rand -hex 32`) and set
  it explicitly. Leaving it blank works for local use (the server generates
  an ephemeral one at startup) but invalidates every session on restart.
- **Password policy and lockout** — `min_password_length`,
  `require_uppercase`/`lowercase`/`digit`/`special`, and
  `max_failed_logins`/`lockout_minutes` in `[auth]`, all off/disabled by
  default for local dev. Tighten them for a shared deployment.

### Authentication providers

**LDAP / Active Directory:**

```toml
[auth]
provider = "ldap"
ldap_url = "ldap://ldap.example.com:389"
ldap_base_dn = "ou=people,dc=example,dc=com"
ldap_bind_dn = "cn=svc,dc=example,dc=com"
ldap_bind_password = "secret"   # or LIBRARIUM__AUTH__LDAP_BIND_PASSWORD
```

**OIDC** (Google, GitHub, Auth0, etc. — additive: password login keeps
working alongside it):

```toml
[auth]
oidc_issuer_url    = "https://accounts.google.com"
oidc_client_id     = "your-client-id"
oidc_client_secret = "your-client-secret"   # or LIBRARIUM__AUTH__OIDC_CLIENT_SECRET
oidc_redirect_uri  = "https://notes.example.com/api/auth/oidc/callback"
```

Only set `provider = "oidc"` if you intend OIDC to *replace* password login;
leave `provider = "password"` to offer both.

### API keys

For scripts and automation, generate a key while logged in and use it instead
of a bearer token:

```bash
curl -X POST https://notes.example.com/api/auth/api-keys \
  -H "Authorization: Bearer <access-token>" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-script", "expires_in_days": 90}'

curl https://notes.example.com/api/vaults -H "X-API-Key: obh_<key>"
```

Revoking a key (`DELETE /api/auth/api-keys/{id}`, or Settings → API Keys in
the UI) is immediate. Changing a user's password does **not** revoke their
API keys — they're independent credentials.

### Health check

```bash
curl http://localhost:8080/api/health
# {"status":"healthy","database":"connected","auth_enabled":true}
```

## Backup

Two things to capture, both required to restore a working instance — the
Markdown files alone are not enough to recover accounts or sharing:

- **Vault files** — plain Markdown on disk under `vault.base_dir`. Back up
  with any file-level tool; nothing Librarium-specific is needed.
- **`librarium.db`** (path from `database.path`, e.g. `/data/librarium.db`
  in the default Docker layout) — a single SQLite file holding everything
  else: user accounts and password hashes, sessions, API keys, vault
  registrations and sharing/group membership, invitations, the sync
  change-log, and the audit log. Losing it without a backup means every
  account and every share has to be recreated from scratch, even though the
  notes themselves are untouched.

```bash
sqlite3 /data/librarium.db ".backup /backups/librarium-$(date +%F).db"
```

`.backup` is safe to run against a live database (it uses SQLite's own
online backup API, not a raw file copy). Restoring is copying that file back
to `database.path` with the server stopped.
