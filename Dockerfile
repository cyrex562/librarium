# syntax=docker/dockerfile:1.7
# Stage 1: Build Frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: Build Backend
FROM rust:1.88-slim-bookworm AS backend-builder
WORKDIR /app
RUN apt-get update && apt-get install -y \
        pkg-config libssl-dev gcc clang mold \
    && rm -rf /var/lib/apt/lists/*
# Use mold for the link step — much faster than ld when LTO is off.
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"
COPY . .
# Copy built frontend assets from stage 1 to the location expected by rust-embed.
COPY --from=frontend-builder /app/target/frontend ./target/frontend
# Cache mounts persist the cargo registry, git checkouts, and target/ across
# builds. The final binary lives in target/release-fast/, which is inside the
# cache mount, so we copy it out before the RUN ends (cache contents are not
# part of the resulting image layer).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --profile release-fast --bin codex \
    && cp target/release-fast/codex /codex

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
COPY --from=backend-builder /codex ./codex
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
