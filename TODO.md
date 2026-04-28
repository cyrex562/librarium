# TODO

This file is the top-level backlog for unfinished tasks, near-term follow-up work, and larger roadmap items that are still active in the repository.

## Immediate Follow-Up

- [ ] Complete TOTP login challenge coverage across the frontend stack.
  - [x] Add a Vitest store test for pending-TOTP login state and localStorage persistence.
  - [x] Add a Vitest store test for completing TOTP login and loading the authenticated profile.
  - [x] Add a Vitest store test for logout clearing pending-TOTP state and passing the refresh token to the backend.
  - [x] Add a mocked Playwright UI flow that shows the verification step after password login and completes sign-in.
  - [x] Add a mocked Playwright negative-path test for invalid TOTP verification codes.
  - [ ] Deploy the auth changes to the test server and run a manual smoke test of the two-step login flow.
- [x] Add automated coverage for OIDC `state` validation, including missing-cookie and mismatched-state cases.
- [x] Add an integration test covering WebSocket authorization for `ReindexComplete` events so vault metadata cannot leak across users.
- [x] Add tests for archive import conflict modes (`fail`, `overwrite`, `rename_with_timestamp`) and non-file tar entries.
- [ ] Decide whether `/api/auth/logout` should revoke only the supplied session or all sessions when no refresh token is provided, then document that contract clearly.
- [ ] Decide whether API-key-authenticated requests should be allowed to complete TOTP-gated login flows or remain excluded by design.

## Security And Correctness

- [ ] Review all non-`/api/vaults/...` routes for missing resource-scoped authorization checks, especially plugin, label, relation-type, and admin-adjacent endpoints.
- [ ] Review remaining auth edge cases:
  - OIDC local user provisioning and username collision policy
  - API key scope and whether keys should be session-independent
  - public-vault read paths versus authenticated read paths
- [ ] Review filesystem mutation paths for race conditions and overwrite behavior:
  - upload session finalization
  - rename overwrite semantics
  - trash restore versus existing file conflicts
  - archive import behavior on large archives and nested conflicts
- [ ] Review search index consistency under watcher-driven rename/delete bursts and cross-process file changes.
- [ ] Confirm the reindex flow is now the single source of truth for both search and entity state, then remove any remaining duplicate client helpers or stale assumptions.

## Config And Deployment

- [ ] Write explicit production guidance for auth bootstrapping now that committed defaults no longer create an admin user automatically.
- [ ] Add a safe example config for local authenticated development that does not rely on shipping default credentials in tracked files.
- [ ] Audit committed docs for stale statements about the app architecture, frontend stack, auth providers, and configuration defaults.
- [ ] Document the system dependencies required for the Tauri target on Linux (`webkit2gtk`, `libsoup`, `javascriptcoregtk`) so workspace validation is reproducible.

## Frontend And API Contract

- [ ] Clean up duplicate or stale API helpers in `frontend/src/api/client.ts`, especially around reindex and auth lifecycle responses.
- [ ] Add explicit frontend handling for `TOTP_VERIFICATION_REQUIRED` and related auth errors so redirects and logout behavior stay predictable.
- [ ] Reconcile any remaining drift between frontend TypeScript types and live backend payloads.
- [ ] Review WebSocket message handling for future message types so auth filtering stays centralized instead of per-message ad hoc.

## Tests And Tooling

- [ ] Expand backend integration coverage for:
  - logout revocation behavior
  - refresh token rotation after TOTP completion
  - cross-vault entity ID lookups
  - archive extraction safety
- [ ] Add CI coverage that runs the targeted backend integration suite added during this review.
- [ ] Ensure Playwright browser provisioning is reproducible for the full matrix (`chromium`, `firefox`, `webkit`) instead of assuming local browser caches exist.
- [ ] Decide how to handle Playwright WebKit on Fedora 43+: the bundled WebKit runtime currently requires older SONAMEs (`libicu*.so.74`, `libjpeg.so.8`, `libjxl.so.0.8`) that are not all available from stock Fedora repos.
- [x] Add a frontend test that covers the two-step login flow with pending TOTP state stored in Pinia/localStorage.
- [ ] Decide whether to address the existing compile warnings in `request_id.rs`, `search_service.rs`, and test helpers, or intentionally defer them.

## Plugin Follow-Up

- [ ] Fix bundled plugin manifests that still declare `modify_ui`; the server currently only accepts `modify_u_i`, so `backlinks`, `daily-notes`, and `word-count` fail to load during startup.

## Roadmap Buckets

- [ ] Desktop parity and Tauri stabilization
  - keep working through `docs/DESKTOP_FEATURE_PARITY_PLAN.md`
  - verify Linux packaging and runtime dependencies
- [ ] Canvas feature completion
  - continue implementation from `docs/CANVAS_INTEGRATION_PLAN.md` and `docs/CANVAS_EDITING_PLAN.md`
- [ ] Plugin parity and plugin security hardening
  - continue `docs/PLUGIN_PARITY_ROADMAP.md`
  - review plugin enable/disable/config authorization expectations
- [ ] Vue/frontend migration completion and cleanup
  - continue `docs/VUE_PORT_AND_API_PLAN.md`
  - remove stale HTMX-era documentation and assumptions
- [ ] Multi-user, sharing, and sync hardening
  - continue `docs/PLAN-desktop-sync-multiuser.md`
  - verify group-sharing and invitation edge cases under auth enforcement

## Known Review Findings Already Addressed

- [x] Enforce TOTP after password login instead of issuing fully trusted tokens immediately.
- [x] Validate OIDC `state`.
- [x] Revoke sessions on logout.
- [x] Prevent cross-vault entity lookup by global entity ID.
- [x] Filter vault-scoped reindex WebSocket messages by authorization.
- [x] Remove the duplicate `/api/vaults/{vault_id}/reindex` route behavior.
- [x] Stop shipping `admin` / `admin` in tracked config files.
