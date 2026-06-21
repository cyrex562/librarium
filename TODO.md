# TODO

This file is the top-level backlog for unfinished tasks, near-term follow-up work, and larger roadmap items that are still active in the repository.

## Immediate Follow-Up

- [ ] **LIB-001** Complete TOTP login challenge coverage across the frontend stack.
  - [x] Add a Vitest store test for pending-TOTP login state and localStorage persistence.
  - [x] Add a Vitest store test for completing TOTP login and loading the authenticated profile.
  - [x] Add a Vitest store test for logout clearing pending-TOTP state and passing the refresh token to the backend.
  - [x] Add a mocked Playwright UI flow that shows the verification step after password login and completes sign-in.
  - [x] Add a mocked Playwright negative-path test for invalid TOTP verification codes.
  - [ ] Deploy the auth changes to the test server and run a manual smoke test of the two-step login flow.
- [x] **LIB-002** Add automated coverage for OIDC `state` validation, including missing-cookie and mismatched-state cases.
- [x] **LIB-003** Add an integration test covering WebSocket authorization for `ReindexComplete` events so vault metadata cannot leak across users.
- [x] **LIB-004** Add tests for archive import conflict modes (`fail`, `overwrite`, `rename_with_timestamp`) and non-file tar entries.
- [x] **LIB-005** Decide whether `/api/auth/logout` should revoke only the supplied session or all sessions when no refresh token is provided, then document that contract clearly.
- [x] **LIB-006** Decide whether API-key-authenticated requests should be allowed to complete TOTP-gated login flows or remain excluded by design.

## Security And Correctness

- [x] **LIB-007** Review all non-`/api/vaults/...` routes for missing resource-scoped authorization checks, especially plugin, label, relation-type, and admin-adjacent endpoints.
- [x] **LIB-008** Review remaining auth edge cases:
  - OIDC local user provisioning and username collision policy
  - API key scope and whether keys should be session-independent
  - public-vault read paths versus authenticated read paths
- [x] **LIB-009** Review filesystem mutation paths for race conditions and overwrite behavior:
  - upload session finalization
  - rename overwrite semantics
  - trash restore versus existing file conflicts
  - archive import behavior on large archives and nested conflicts
- [ ] **LIB-010** Review search index consistency under watcher-driven rename/delete bursts and cross-process file changes.
- [ ] **LIB-011** Confirm the reindex flow is now the single source of truth for both search and entity state, then remove any remaining duplicate client helpers or stale assumptions.

- [ ] **LIB-044** Refactor `import-archive` to stream archive entries rather than buffering the entire request body into memory (`web::Bytes`). Large archives currently risk OOMing the server. This requires switching to a streaming multipart or chunked body reader and feeding the decompressor incrementally.
- [ ] **LIB-041** Decide whether recent-file tracking (`/api/vaults/{vault_id}/recent`) should be scoped per-user rather than per-vault; currently all members of a vault share one recent-files list.
- [ ] **LIB-042** Document that `/api/render` (stateless markdown rendering) is intentionally unauthenticated, or add auth if that was an oversight.

- [ ] **LIB-043** Audit the auth stack for offline-first / air-gapped compatibility. The app must function fully on isolated lab networks where no external endpoints are reachable. Specific concerns: (1) OIDC discovery and token exchange make outbound HTTP calls at login time — these must be gracefully disabled, not just unconfigured; (2) any other runtime HTTP calls (plugin asset CDNs, avatar URLs, link-preview fetches, etc.) must be identified and made optional; (3) the auth provider selection should be structured so local username/password always works as a self-contained fallback, with OIDC/LDAP as additive layers, and the design should accommodate future providers (SAML, client-cert, etc.) without requiring core changes.

## Config And Deployment

- [ ] **LIB-012** Write explicit production guidance for auth bootstrapping now that committed defaults no longer create an admin user automatically.
- [ ] **LIB-013** Add a safe example config for local authenticated development that does not rely on shipping default credentials in tracked files.
- [ ] **LIB-014** Audit committed docs for stale statements about the app architecture, frontend stack, auth providers, and configuration defaults.
- [ ] **LIB-015** Document the system dependencies required for the Tauri target on Linux (`webkit2gtk`, `libsoup`, `javascriptcoregtk`) so workspace validation is reproducible.

## Frontend And API Contract

- [ ] **LIB-016** Clean up duplicate or stale API helpers in `frontend/src/api/client.ts`, especially around reindex and auth lifecycle responses.
- [ ] **LIB-017** Add explicit frontend handling for `TOTP_VERIFICATION_REQUIRED` and related auth errors so redirects and logout behavior stay predictable.
- [ ] **LIB-018** Reconcile any remaining drift between frontend TypeScript types and live backend payloads.
- [ ] **LIB-019** Review WebSocket message handling for future message types so auth filtering stays centralized instead of per-message ad hoc.

## Tests And Tooling

- [ ] **LIB-020** Expand backend integration coverage for:
  - logout revocation behavior
  - refresh token rotation after TOTP completion
  - cross-vault entity ID lookups
  - archive extraction safety
- [ ] **LIB-021** Add CI coverage that runs the targeted backend integration suite added during this review.
- [ ] **LIB-022** Ensure Playwright browser provisioning is reproducible for the full matrix (`chromium`, `firefox`, `webkit`) instead of assuming local browser caches exist.
- [ ] **LIB-023** Decide how to handle Playwright WebKit on Fedora 43+: the bundled WebKit runtime currently requires older SONAMEs (`libicu*.so.74`, `libjpeg.so.8`, `libjxl.so.0.8`) that are not all available from stock Fedora repos.
- [x] **LIB-024** Add a frontend test that covers the two-step login flow with pending TOTP state stored in Pinia/localStorage.
- [ ] **LIB-025** Decide whether to address the existing compile warnings in `request_id.rs`, `search_service.rs`, `ws.rs` (unused imports: `FileChangeEvent`, `broadcast`), `file_service.rs` (unused import: `WalkDir`), `watcher/mod.rs` (unused import: `warn`), `markdown_service.rs` (unused variable: `match_start`), and test helpers, or intentionally defer them.

## Plugin Follow-Up

- [ ] **LIB-026** Fix bundled plugin manifests that still declare `modify_ui`; the server currently only accepts `modify_u_i`, so `backlinks`, `daily-notes`, and `word-count` fail to load during startup.
- [ ] **LIB-027** Implement custom date-format support in `plugins/daily-notes/main.js` (line 150 TODO) so users can configure the note filename format beyond the hardcoded default.
- [ ] **LIB-028** Implement the backlinks UI panel update in `plugins/backlinks/main.js` (line 175 TODO) so detected wiki-link backlinks are actually rendered in the sidebar panel.

## Roadmap Buckets

- [ ] **LIB-029** Desktop parity and Tauri stabilization
  - keep working through `docs/DESKTOP_FEATURE_PARITY_PLAN.md`
  - verify Linux packaging and runtime dependencies
- [ ] **LIB-030** Canvas feature completion
  - continue implementation from `docs/CANVAS_INTEGRATION_PLAN.md` and `docs/CANVAS_EDITING_PLAN.md`
- [ ] **LIB-031** Plugin parity and plugin security hardening
  - continue `docs/PLUGIN_PARITY_ROADMAP.md`
  - review plugin enable/disable/config authorization expectations
- [ ] **LIB-032** Vue/frontend migration completion and cleanup
  - continue `docs/VUE_PORT_AND_API_PLAN.md`
  - remove stale HTMX-era documentation and assumptions
- [ ] **LIB-033** Multi-user, sharing, and sync hardening
  - continue `docs/PLAN-desktop-sync-multiuser.md`
  - verify group-sharing and invitation edge cases under auth enforcement

## Known Review Findings Already Addressed

- [x] **LIB-034** Enforce TOTP after password login instead of issuing fully trusted tokens immediately.
- [x] **LIB-035** Validate OIDC `state`.
- [x] **LIB-036** Revoke sessions on logout.
- [x] **LIB-037** Prevent cross-vault entity lookup by global entity ID.
- [x] **LIB-038** Filter vault-scoped reindex WebSocket messages by authorization.
- [x] **LIB-039** Remove the duplicate `/api/vaults/{vault_id}/reindex` route behavior.
- [x] **LIB-040** Stop shipping `admin` / `admin` in tracked config files.
