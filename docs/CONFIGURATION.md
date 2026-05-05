# Configuration Guide

Librarium can be configured via a `config.toml` file or environment variables.

## File Configuration (`config.toml`)

Place a file named `config.toml` in the running directory.

```toml
[server]
host = "0.0.0.0"  # Listen address
port = 8080       # Listen port

[database]
path = "./librarium.db" # Path to SQLite database

[vault]
index_exclusions = [".git", ".obsidian", ".trash", "node_modules"] # Folders to ignore

[auth]
enabled = false
provider = "password" # password (implemented), oidc (planned), mtls (planned)
jwt_secret = ""
access_token_ttl = 3600
refresh_token_ttl = 604800

[storage]
backend = "local" # local (implemented), s3 (scaffolded)

[storage.s3]
endpoint = ""
bucket = ""
region = ""
access_key = ""
secret_key = ""
path_style = true
```

## Environment Variables

Environment variables override file settings. Use double underscores `__` for nesting.

| Variable | Config Option | Default | Description |
|----------|---------------|---------|-------------|
| `LIBRARIUM__SERVER__HOST` | `server.host` | `127.0.0.1` | Binding address |
| `LIBRARIUM__SERVER__PORT` | `server.port` | `8080` | Binding port |
| `LIBRARIUM__DATABASE__PATH` | `database.path` | `./librarium.db` | Database file location |
| `LIBRARIUM__AUTH__ENABLED` | `auth.enabled` | `false` | Enable authentication |
| `LIBRARIUM__AUTH__PROVIDER` | `auth.provider` | `password` | Auth provider selection |
| `LIBRARIUM__AUTH__JWT_SECRET` | `auth.jwt_secret` | `""` | JWT signing secret |
| `LIBRARIUM__STORAGE__BACKEND` | `storage.backend` | `local` | Storage backend selection |
| `LIBRARIUM__STORAGE__S3__ENDPOINT` | `storage.s3.endpoint` | `""` | S3/MinIO endpoint URL |
| `LIBRARIUM__STORAGE__S3__BUCKET` | `storage.s3.bucket` | `""` | S3 bucket name |
| `LIBRARIUM__STORAGE__S3__REGION` | `storage.s3.region` | `""` | S3 region |
| `LIBRARIUM__STORAGE__S3__ACCESS_KEY` | `storage.s3.access_key` | `""` | S3 access key |
| `LIBRARIUM__STORAGE__S3__SECRET_KEY` | `storage.s3.secret_key` | `""` | S3 secret key |
| `LIBRARIUM__STORAGE__S3__PATH_STYLE` | `storage.s3.path_style` | `true` | Use path-style URLs (MinIO-friendly) |
| `RUST_LOG` | N/A | `warn` | Logging verbosity (error, warn, info, debug, trace) |

## Logging

Logging is configured via `RUST_LOG`.

- **JSON Format**: Set `LOG_FORMAT=json` for structured logging (useful for clouds).
- **File Logging**: Logs are automatically rotated in `./logs/`.
