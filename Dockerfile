# Stage 1: Build Frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: Build Backend
FROM rust:1.88-slim-bookworm AS backend-builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev gcc && rm -rf /var/lib/apt/lists/*
COPY . .
# Copy built frontend assets from stage 1 to the location expected by rust-embed.
COPY --from=frontend-builder /app/target/frontend ./target/frontend
RUN cargo build --release --bin codex

# Stage 3: Runtime
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libsqlite3-0 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary and default config
COPY --from=backend-builder /app/target/release/codex .
COPY config.toml .

# Create directories for data
RUN mkdir -p /data/vaults /app/logs

# Environment defaults for Docker deployment
ENV CODEX__SERVER__HOST=0.0.0.0
ENV CODEX__SERVER__PORT=8080
ENV CODEX__DATABASE__PATH=/data/codex.db
ENV CODEX__VAULT__BASE_DIR=/data/vaults
ENV RUST_LOG=info,codex=info,actix_web=info

EXPOSE 8080

VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/api/health || exit 1

CMD ["./codex", "--config", "/app/config.toml"]
