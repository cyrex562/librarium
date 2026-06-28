# Unified Auth implementation Plan

Because the two branches introduced entirely divergent, fully featured auth schemas (Password/JWT vs Session/OIDC), we will restructure the project to cleanly support both providers under a unified `auth` umbrella.

## 1. Core Module Reorganization
Instead of simple `auth.rs` files, we will create explicit modules for authentication to encapsulate both strategies.
* `crates/librarium-server/src/auth/mod.rs` (Unified auth models, User/Session representations)
* `crates/librarium-server/src/auth/jwt.rs` (Password / JWT logic ported from `main`)
* `crates/librarium-server/src/auth/oidc.rs` (OAuth / Session logic imported from OIDC branch)

## 2. Middleware & Extractors
We will modify the middleware layer to accept cookies (OIDC) *or* authorization headers (JWT):
* The unified `AuthMiddleware` / `AuthenticatedUser` extractor will check for a valid Session Cookie. If absent, it will fall back to checking the Bearer Token.

## 3. Database Layer Migration
The incoming OIDC branch includes several new tables and methods. The `main` branch uses password properties.
* We will merge the SQLite logic.

## 4. Routes
* All routes mapped under `crates/librarium-server/src/routes/auth/`.
