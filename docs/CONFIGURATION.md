# Configuration

Librarium reads a `config.toml` file, with environment variables layered on
top of it. There is no required configuration — every value has a sane
default (auth is **off** by default; see [Authentication](#authentication)
below before exposing the server beyond loopback).

## Which file gets loaded

```
--config <PATH> flag  >  LIBRARIUM_CONFIG env var  >  config.toml next to the executable  >  ./config.toml
```

The full, annotated reference for every key is
[`config.example.toml`](../config.example.toml) at the repo root — copy it
rather than duplicating it here, since a second copy of the same TOML is
exactly how documentation goes stale. This file covers what
`config.example.toml`'s comments don't: environment-variable overrides and
where things end up at runtime.

## Environment variable overrides

Environment variables override the file. Nest with a double underscore:

```bash
LIBRARIUM__AUTH__JWT_SECRET="$(openssl rand -hex 32)" ./librarium
```

maps to

```toml
[auth]
jwt_secret = "..."
```

The sections are `server`, `database`, `vault`, `auth`, `sync`, `cors`,
`tls`, and `ml` — matching `config.example.toml` exactly (there is no
`[storage]`/S3 section; an earlier draft of this document described one that
was never implemented).

## Authentication

**Off by default.** With `auth.enabled = false`, anyone who can reach the
server has full read/write access to every vault — fine on loopback
(`127.0.0.1`, the default `server.host`), a real risk on a routable address.
The server warns at startup if you bind a non-loopback address without auth
enabled.

Three providers, set via `auth.provider`: `password` (default, fully
offline), `ldap`, `oidc`. See `config.example.toml`'s `[auth]` section for
every field for each.

**First admin account:** with `auth.enabled = true` and no
`bootstrap_admin_password` set, Librarium generates one on first run, writes
the credentials to a file beside the database, and forces a password change
at first login — no plaintext credential ever needs to live in your config.
Setting `bootstrap_admin_password` explicitly works too but is discouraged
for that reason. See `config.example.toml`'s Bootstrap section.

**Forgot a password?** `librarium admin set-password <username>` — works
with the server stopped, revokes every session for that user. See
`librarium admin --help` for `create-user` and `list-users` too.

## TLS

Two options, not mutually exclusive with a reverse proxy:

- **Built in**: set `[tls].cert_file` and `.key_file` (PEM) and the server
  terminates TLS itself via rustls.
- **Reverse proxy** (e.g. Caddy, for automatic Let's Encrypt renewal):
  terminate TLS there, proxy plain HTTP to the server on loopback.

Loopback-only plain HTTP (the default) is intentional and safe — browsers
treat `http://localhost`/`http://127.0.0.1` as a secure context, and the
traffic never reaches a network interface.

## Rate limiting

Set via environment variables (no `[rate_limit]` TOML section exists yet):

| Variable | Default | Applies to |
| --- | --- | --- |
| `RATE_LIMIT_REQUESTS` | 120 | requests per window, per IP |
| `RATE_LIMIT_USER_REQUESTS` | 300 | requests per window, per authenticated user |
| `RATE_LIMIT_UPLOAD_REQUESTS` | 2000 | upload requests per window |
| `RATE_LIMIT_WINDOW_SECS` | 60 | the shared window size, in seconds |

## Logging

- `RUST_LOG` sets verbosity (standard `tracing` filter syntax, e.g.
  `librarium=debug`). Default: `warn,librarium=info,actix_web=info,actix_server=info`.
- `LOG_FORMAT=json` switches to structured JSON output (stdout stays
  human-readable either way; only the file log changes format).
- **Log files are written next to the database**, not a fixed `./logs/`:
  `{parent of database.path}/logs/librarium.log`, rotated daily. If
  `database.path` has no parent directory, it falls back to `./logs`. In
  Docker with `LIBRARIUM__DATABASE__PATH=/data/librarium.db`, logs land in
  `/data/logs/`.
