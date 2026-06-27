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
- [x] **LIB-010** Review search index consistency under watcher-driven rename/delete bursts and cross-process file changes.
- [x] **LIB-011** Confirm the reindex flow is now the single source of truth for both search and entity state, then remove any remaining duplicate client helpers or stale assumptions.

- [x] **LIB-044** Refactor `import-archive` to stream archive entries rather than buffering the entire request body into memory (`web::Bytes`). Large archives currently risk OOMing the server. This requires switching to a streaming multipart or chunked body reader and feeding the decompressor incrementally.
- [x] **LIB-041** Decide whether recent-file tracking (`/api/vaults/{vault_id}/recent`) should be scoped per-user rather than per-vault; currently all members of a vault share one recent-files list.
- [x] **LIB-042** Document that `/api/render` (stateless markdown rendering) is intentionally unauthenticated, or add auth if that was an oversight.

- [x] **LIB-043** Audit the auth stack for offline-first / air-gapped compatibility. The app must function fully on isolated lab networks where no external endpoints are reachable. Specific concerns: (1) OIDC discovery and token exchange make outbound HTTP calls at login time — these must be gracefully disabled, not just unconfigured; (2) any other runtime HTTP calls (plugin asset CDNs, avatar URLs, link-preview fetches, etc.) must be identified and made optional; (3) the auth provider selection should be structured so local username/password always works as a self-contained fallback, with OIDC/LDAP as additive layers, and the design should accommodate future providers (SAML, client-cert, etc.) without requiring core changes.

## Config And Deployment

- [x] **LIB-012** Write explicit production guidance for auth bootstrapping now that committed defaults no longer create an admin user automatically.
- [x] **LIB-013** Add a safe example config for local authenticated development that does not rely on shipping default credentials in tracked files.
- [x] **LIB-014** Audit committed docs for stale statements about the app architecture, frontend stack, auth providers, and configuration defaults.
- [x] **LIB-015** Document the system dependencies required for the Tauri target on Linux (`webkit2gtk`, `libsoup`, `javascriptcoregtk`) so workspace validation is reproducible.

## TLS And Transport Security

- [ ] **LIB-046** Document that the portable/localhost deployment is intentionally plain HTTP and that this is safe: loopback traffic never hits a network interface, and browsers already treat `http://localhost` / `http://127.0.0.1` as a secure context. Goal: prevent anyone from adding self-signed TLS to the localhost path, which only adds browser warnings for zero security benefit.
- [ ] **LIB-047** Add an opt-in self-signed certificate generator CLI subcommand (e.g. `librarium gen-cert --host <addr>`) using `rcgen` that writes `cert.pem` / `key.pem` into the data directory and prints the `[tls].cert_file` / `key_file` lines to paste into `config.toml`. Intended only for quick LAN testing; must clearly warn about browser trust prompts and must never be wired into the localhost default.
- [ ] **LIB-048** Add an admin "TLS certificate" screen (and supporting API) to upload a PEM certificate chain + private key, persist them next to the database, and point `[tls].cert_file` / `key_file` at them without hand-editing `config.toml`. Validate that the key matches the certificate, surface clear parse/mismatch errors, and document that a restart is required to rebind. This is the highest-value piece — it covers internal-CA, Let's Encrypt, and corp-issued certs.
- [ ] **LIB-049** Document the recommended production HTTPS path in `docs/DEPLOYMENT.md`: a reverse proxy (e.g. Caddy with automatic HTTPS) terminating TLS in front of Librarium bound on loopback. This matches the existing "mTLS requires a reverse proxy" stance and is the most practical way to get warning-free, auto-renewing certificates for sustained network exposure.
- [ ] **LIB-050** Decide whether to support in-app CSR generation (generate a keypair + CSR for an internal/enterprise CA, then import the signed chain). Currently deferred as low value because external key generation plus the upload flow (LIB-048) covers the same need; revisit only on real demand.
- [ ] **LIB-051** When TLS is enabled, decide the scope of hardening additions: (a) an optional HTTP→HTTPS redirect listener, (b) an HSTS response header, and (c) hot-reloading certificates on renewal without a full restart. Document which are in scope versus intentionally deferred.

## Frontend And API Contract

- [x] **LIB-016** Clean up duplicate or stale API helpers in `frontend/src/api/client.ts`, especially around reindex and auth lifecycle responses.
- [x] **LIB-017** Add explicit frontend handling for `TOTP_VERIFICATION_REQUIRED` and related auth errors so redirects and logout behavior stay predictable.
- [x] **LIB-018** Reconcile any remaining drift between frontend TypeScript types and live backend payloads.
- [x] **LIB-019** Review WebSocket message handling for future message types so auth filtering stays centralized instead of per-message ad hoc.

## Tests And Tooling

- [x] **LIB-020** Expand backend integration coverage for:
  - logout revocation behavior
  - refresh token rotation after TOTP completion
  - cross-vault entity ID lookups
  - archive extraction safety
- [x] **LIB-021** Add CI coverage that runs the targeted backend integration suite added during this review.
- [x] **LIB-022** Ensure Playwright browser provisioning is reproducible for the full matrix (`chromium`, `firefox`, `webkit`) instead of assuming local browser caches exist.
- [x] **LIB-023** Decide how to handle Playwright WebKit on Fedora 43+: the bundled WebKit runtime currently requires older SONAMEs (`libicu*.so.74`, `libjpeg.so.8`, `libjxl.so.0.8`) that are not all available from stock Fedora repos.
- [x] **LIB-024** Add a frontend test that covers the two-step login flow with pending TOTP state stored in Pinia/localStorage.
- [x] **LIB-025** Decide whether to address the existing compile warnings in `request_id.rs`, `search_service.rs`, `ws.rs` (unused imports: `FileChangeEvent`, `broadcast`), `file_service.rs` (unused import: `WalkDir`), `markdown_service.rs` (unused variable: `match_start`), and test helpers, or intentionally defer them. (`watcher/mod.rs` warning resolved — `warn` is now used.)

## Plugin Follow-Up

- [x] **LIB-026** Fix bundled plugin manifests that still declare `modify_ui`; the server currently only accepts `modify_u_i`, so `backlinks`, `daily-notes`, and `word-count` fail to load during startup.
- [x] **LIB-027** Implement custom date-format support in `plugins/daily-notes/main.js` (line 150 TODO) so users can configure the note filename format beyond the hardcoded default.
- [x] **LIB-028** Implement the backlinks UI panel update in `plugins/backlinks/main.js` (line 175 TODO) so detected wiki-link backlinks are actually rendered in the sidebar panel.

## Roadmap Buckets

- [x] **LIB-029** Desktop parity and Tauri stabilization
  - keep working through `docs/DESKTOP_FEATURE_PARITY_PLAN.md`
  - verify Linux packaging and runtime dependencies
- [x] **LIB-030** Canvas feature completion
  - continue implementation from `docs/CANVAS_INTEGRATION_PLAN.md` and `docs/CANVAS_EDITING_PLAN.md`
- [x] **LIB-031** Plugin parity and plugin security hardening
  - continue `docs/PLUGIN_PARITY_ROADMAP.md`
  - review plugin enable/disable/config authorization expectations
- [x] **LIB-032** Vue/frontend migration completion and cleanup
  - continue `docs/VUE_PORT_AND_API_PLAN.md`
  - remove stale HTMX-era documentation and assumptions
- [x] **LIB-033** Multi-user, sharing, and sync hardening
  - continue `docs/PLAN-desktop-sync-multiuser.md`
  - verify group-sharing and invitation edge cases under auth enforcement


## Packaging And Distribution

- [x] **LIB-045** Create single-file installers for both the desktop (Tauri) and server variants, with platform-specific builds for Windows and Linux.
  - [x] **Desktop / Windows** – NSIS `.exe` via `cargo tauri build --bundles nsis` in `.github/workflows/release.yml`.
  - [x] **Desktop / Linux** – AppImage + `.deb` via `cargo tauri build --bundles appimage,deb` in release workflow.
  - [x] **Server / Windows** – `librarium.exe` built on `windows-latest` runner; `scripts/install-service-windows.ps1` registers it as a native Windows Service with auto-restart and failure recovery.
  - [x] **Server / Linux** – `.deb` built by `scripts/package-server-deb.sh`: installs binary, systemd unit, hardened service config, `librarium` system user, and `/etc/librarium/config.toml` as a managed conffile.
  - [x] Wire all four targets into CI: `.github/workflows/release.yml` triggers on `v*` tags and publishes all artifacts to a GitHub Release.

## Known Review Findings Already Addressed

- [x] **LIB-034** Enforce TOTP after password login instead of issuing fully trusted tokens immediately.
- [x] **LIB-035** Validate OIDC `state`.
- [x] **LIB-036** Revoke sessions on logout.
- [x] **LIB-037** Prevent cross-vault entity lookup by global entity ID.
- [x] **LIB-038** Filter vault-scoped reindex WebSocket messages by authorization.
- [x] **LIB-039** Remove the duplicate `/api/vaults/{vault_id}/reindex` route behavior.
- [x] **LIB-040** Stop shipping `admin` / `admin` in tracked config files.

## Not Sorted

- [ ] **LIB-052** Design an alternate set of views for viewing the server on an Android device. Add tasks for design additions and other needed feature improvements to support mobile.
- [ ] **LIB-053** Organization — auto parse, tag, rename, and organize Markdown docs using local compute only (no *online* LLMs). Full design in `docs/ORGANIZATION_ML_PLAN.md`. Evolves the existing rule-based AI Insights surface (`MlService`, `routes/ml.rs`, `MlInsightsPanel.vue`) into a tiered local-ML pipeline (heuristic → classical NLP → local embeddings) and adds the missing rename verb plus a vault-wide batch mode. Broken into LIB-054 … LIB-066 below.

## Organization Feature (LIB-053)

Tiered, local-only document organization. Tier 0 (heuristics) exists today; Tier 1
(classical NLP, no model download) is the default; Tier 2 (local ONNX embeddings) is
opt-in and air-gap-safe. See `docs/ORGANIZATION_ML_PLAN.md` for the full design,
research citations, and rationale.

### Phase 1 — Foundations

- [x] **LIB-054** Refactor `MlService` from standalone static functions into a parse-first
  pipeline that builds a shared `NoteAnalysis` (title, outline, inline tags, frontmatter tags,
  wiki-links, tasks, word count) once per note, then runs tag/rename/organize suggesters over
  it. Preserves current outline/suggestion behavior as the Tier 0 path. Exposed via
  `POST /ml/analyze`; keyphrases/embeddings fields are reserved for later tiers. Unit tests
  cover structure extraction, code-fence skipping, title fallback, and tag dedup.
- [x] **LIB-055** Add an `[ml]` config section in `crates/librarium-server/src/config/mod.rs`
  (`enabled`, `tier`, `model`, `cache_dir`, `allow_model_download`, `auto_suggest_on_open`,
  `naming_scheme`, `min_confidence`, `max_suggestions`) with air-gap-safe defaults
  (`tier = "classical"`, `allow_model_download = false`). Documented in
  `docs/CONFIGURATION.md` and `config.example.toml`. Tier is plumbed into `/ml/analyze`.

### Phase 2 — Tag upgrade (Tier 1, no model download)

- [x] **LIB-056** Add classical keyphrase extraction (YAKE! via the MIT-licensed
  `yake-rust` crate — the LGPL `keyword_extraction` crate was rejected as incompatible
  with the project's MIT license and static binaries) and surface keyphrase-derived tag
  candidates in the `/ml/suggestions` response. Keyphrases populate `NoteAnalysis` under
  the `classical`/`embeddings` tiers (empty under `heuristic`); tag candidates are
  normalized per `docs/TAG_SYSTEM_SPEC.md`, deduped against existing and rule tags, and
  scored in a 0.50–0.74 confidence band below the curated rule tags. Each suggestion now
  carries a `source` (`rule` | `keyphrase` | `semantic`). Reuses the existing
  frontmatter-tag apply/undo path. Unit tests cover tag normalization, tier gating, and
  dedup.

### Phase 3 — Rename (new verb)

- [x] **LIB-057** Add rename suggestions: derive a canonical filename from frontmatter
  `title` → first H1 → top keyphrases, formatted by the configured `naming_scheme`
  (`kebab-case` | `title-case` | `date-prefixed` | `category-slug`). New
  `POST /ml/rename-suggestion` endpoint (`MlService::suggest_rename`) and a `rename`
  suggestion kind in apply (carrying `new_name`). Renames keep the note in its folder.
  Unit tests cover each naming scheme, slugification, no-op detection, and keyphrase
  fallback.
- [x] **LIB-058** Make rename link-safe: on apply, rename the file then find inbound
  `[[wiki-links]]`/`![[embeds]]` across the vault and rewrite them (new
  `wiki_link_service::rewrite_wiki_links` — basename match, alias/heading/`.md` preserved,
  path-qualified links gated on directory). The affected-link count is reported in dry-run
  and apply via `updated_links`. `ReverseAction::RenameWithLinks` restores both the filename
  and the rewritten links on undo. Unit tests for the rewriter (reversibility, case, dir
  gating) plus an integration test for apply→links-rewritten→undo→restored.

### Phase 4 — Local embeddings (Tier 2, opt-in)

- [x] **LIB-059** Integrate `fastembed-rs` for local ONNX sentence embeddings (default
  `bge-small-en-v1.5`), behind an off-by-default `embeddings` Cargo feature so the native
  `onnxruntime` dependency never burdens the default build. An `Embedder` trait + a
  process-wide `OnceLock` provider lazily load the model once (primed at startup), honoring
  `cache_dir` and `allow_model_download` (refuses to construct — no network — when downloads
  are disabled and the cache is empty). When `tier = "embeddings"` but the backend/model is
  unavailable, it logs once and the provider returns `None` so everything falls back to
  Tier 1 instead of erroring.
- [x] **LIB-060** Added a `note_embeddings` SQLite table (vault_id, file_path, model, dim,
  vector BLOB, content_hash, tags, updated_at) in `db::run_migrations` with accessors.
  Embeddings are computed off the request path in `reindex_service::index_file` (single
  notes) and a batched vault-wide `backfill_vault` (run at the end of `reindex_vault`);
  recompute is skipped when `content_hash` is unchanged, and deletions clean up the row. The
  synchronous embedder runs on the blocking pool.
- [x] **LIB-061** Added controlled-vocabulary semantic tagging: `build_tag_prototypes` builds
  a prototype vector per existing tag (L2-normalized mean of notes carrying it) and
  `suggest_semantic_tags` returns the nearest tags above `min_confidence`, excluding tags
  already present/suggested. Surfaced alongside Tier 1 keyphrase tags in
  `generate_suggestions` with a `semantic` source label, then re-sorted by confidence and
  capped. (Verified end-to-end with a deterministic mock embedder; the native backend is
  unbuildable in this sandbox because `ort-sys` downloads a binary from a blocked host.)

### Phase 5 — Organize (single-note + vault-wide)

- [x] **LIB-062** Replaced the string-match folder inference with layered semantic folder
  placement: Tier 2 embedding kNN (`embedding_service::suggest_folder`, vote the nearest
  notes' folder) and Tier 1 TF-IDF nearest folder (`MlService::nearest_folder_tfidf`), with
  the heuristic `infer_category` as the final fallback. Surfaced via the existing
  `move_to_folder` apply/undo path (source `semantic`/`tfidf`).
- [x] **LIB-063** Added the vault-wide organization plan (`services/organize_service.rs`):
  notes with cached embeddings are grouped by a pure cosine-threshold union-find clusterer
  (variable cluster count) and each cluster labelled by a class-based TF-IDF (c-TF-IDF) of
  its top terms (chose an in-house clusterer over `hdbscan`/`linfa-clustering`, which are
  only meaningful with the unbuildable-here embeddings backend). `build_plan` produces a
  reviewable `{file, suggested_tags, suggested_name, target_folder, cluster, confidence}`
  plan; Tier 1 falls back to TF-IDF placement. Nothing mutates until applied.
- [x] **LIB-064** Added the batch organize API: `POST /ml/organize-vault` (computes + returns
  `plan_id` + plan, emits an `OrganizeComplete` event on the authorized WS channel) and
  `POST /ml/apply-plan` (applies selected per-row tag/rename/folder actions as one batch
  under a single `group_id`, reusing the extracted `apply_suggestion_core`). `/ml/undo` now
  accepts a `group_id` to consume the whole receipt group (newest-first) for bulk undo.
  Integration-tested: organize → apply-plan batch → bulk undo restores everything and is
  single-use.

### Phase 6 — Frontend, provisioning, tests

- [x] **LIB-065** Upgraded the UI: `MlInsightsPanel.vue` shows extracted key phrases, tag
  suggestions with confidence + source chip (rule/keyphrase/semantic), and a rename card
  ("N inbound link(s) will be updated" from a dry run). New `OrganizeVaultModal.vue` shows
  the batch plan in a checkbox table (tag / rename / move) with "Apply selected" (one batch)
  and "Undo last organize" (group undo), behind the existing suggest-only framing.
- [x] **LIB-066** Air-gap model provisioning + coverage: documented pre-seeding `cache_dir`
  with the ONNX model for isolated hosts (step-by-step in `docs/ORGANIZATION_ML_PLAN.md`);
  added an offline-mode integration test (`tests/ml_offline.rs`) proving the embedder is
  never constructed and ML endpoints fall back to Tier 1 with no network when
  `allow_model_download = false` (mirrors `LIB-043`); extended `benches/markdown_benchmarks.rs`
  with clustering, c-TF-IDF labelling, TF-IDF folder placement, keyphrase, and a
  (feature-gated, degrade-to-noop) embedding-throughput benchmark on a synthetic large vault.

### Phase 7 — Local verification & follow-ups (test on Windows)

> These are the carry-overs from Phases 1–6 that could not be fully verified in the CI/agent
> sandbox (no native ONNX build, no GUI). Each is a concrete thing to run on a local Windows
> machine. Check the box once verified; file a bug if it fails.

- [ ] **LIB-067** Verify the Tier-2 `embeddings` feature builds and runs on Windows. The agent
  sandbox can't compile it (the `ort-sys` build script downloads an onnxruntime binary from a
  proxy-blocked host), so the entire neural path is mock-tested only. Steps:
  1. `cargo build -p librarium-server --features embeddings` (needs network for the ONNX
     runtime + model download the first time).
  2. Run with `[ml] tier="embeddings", allow_model_download=true, cache_dir="./ml-models"`;
     confirm the model downloads once and the startup log shows "ML embeddings backend ready".
  3. Open a few notes, run **Suggest organization**, and confirm `semantic`-sourced tag and
     folder suggestions appear; run **Organize vault…** and confirm clusters are formed
     (`cluster_count > 0`) and cluster-labelled target folders are proposed.
  4. Confirm embeddings populate the `note_embeddings` table after a reindex and that editing
     a note refreshes only that row (content-hash skip).
- [ ] **LIB-068** Verify air-gapped provisioning end-to-end on Windows (the LIB-066 flow):
  copy a pre-seeded `ml-models` dir to a host with no network, set
  `allow_model_download=false`, and confirm the server loads the model from disk with zero
  outbound calls — and that a *missing/empty* cache logs one warning and falls back to Tier 1
  without erroring. Validate the Windows `cache_dir` path handling (drive letters, backslashes).
- [ ] **LIB-069** Verify ML file mutations on Windows paths (rename / move / organize-plan).
  The server normalizes vault-relative paths to forward slashes in several places
  (`parent_dir`, `rewrite_wiki_links`, `compute_rename_link_changes`, the TF-IDF corpus),
  while `FileService::list_markdown_files` yields native (`\`) separators via
  `to_string_lossy`. Confirm on Windows that: (a) a link-safe **rename** rewrites inbound
  `[[wiki-links]]` in notes located in subfolders; (b) **move-to-folder** and the batch
  **apply-plan** move files correctly and **bulk undo** restores them; (c) the renamed file
  itself is correctly excluded from the inbound-link scan (no path-separator mismatch). If any
  fail, normalize `list_markdown_files` output to forward slashes at the boundary.
- [ ] **LIB-070** Verify the AI Insights UI manually in the desktop/web app: key-phrase chips,
  source chips, the rename card's "N inbound links will be updated" dry-run count, and the
  **Organize Vault** modal (checkbox table, Apply selected, Undo last organize, tree refresh
  after moves). Confirm the `OrganizeComplete` WebSocket event is received by clients scoped
  to the vault.
- [ ] **LIB-071** (Pre-existing, not from this feature) Fix the 3 failing
  `entity_api_tests` (`test_list_entity_types_*`, `test_list_relation_types_*`) — they return
  non-success and fail on `main` independently of the ML work; likely an endpoint/registry
  setup issue surfaced only in the test harness.
- [ ] **LIB-072** (Pre-existing) Fix the 2 frontend type errors blocking `npm run build`:
  `CanvasView.vue` (`CSSProperties.position` typed as `string`) and `OidcCallbackPage.vue`
  (`loginWithOidc` missing on the auth store). Unrelated to the ML feature but they break the
  production build.
- [ ] **LIB-073** Share the link-safe rename logic with the plain `/api/vaults/{id}/rename`
  route. Today only the ML rename path rewrites inbound `[[wiki-links]]`/`![[embeds]]`; a
  manual rename via the file tree still orphans links. Extract the rewrite + receipt logic so
  both paths stay link-safe.
- [x] LIB-074: looked a creating or adapting an existing VSLLM/MoE to help organize — added a
  `local_lm` ML tier (option B). `local_lm_service` exposes a `LabelScorer` whose default backend
  is a zero-shot scorer reusing the side-loaded ONNX embedding model (embeds note + humanized
  candidate labels, cosine→[0,1]); `build_plan` blends those scores into folder-candidate ranking
  via `blend_label_scores`. The provider returns `None` (graceful fallback to embeddings/classical)
  when the tier is off or no local model is present, so the air-gap default holds (no download).
  Tier serde fixed to `snake_case` so `tier = "local_lm"` round-trips. Unit-tested.
- [x] LIB-075: add reinforcement to the organization process — per-vault `org_feedback` SQLite table
  of `(kind, target) -> (accepts, rejects)`. Signals captured in the apply/undo routes: applying a
  folder/tag = accept, an applied group undone = reject, and offered-but-unchosen candidates (new
  `reject_folders`/`reject_tags` on `ApplyPlanRow`, sent by OrganizeVaultModal) = reject. `build_plan`
  reweights folder + tag candidates by a Laplace-smoothed acceptance-rate multiplier (neutral 1.0,
  bounded 0–2) and drops candidates rejected ≥3× with low acceptance. Simple, explainable, unit-tested.
- [x] LIB-076: multi-level folder structures — `nested_cluster_labels` sub-clusters each large
  top-level cluster at a tighter cosine threshold (0.78) and labels each cohesive subgroup via
  c-TF-IDF, emitting nested `parent/child` `target_folder`/`folder_candidates`. Path-aware
  `slugify_folder_path` preserves `/`. Unit-tested; data model + apply path already handle nesting.
- [x] LIB-077: common terms for folder structure (like dewey decimal categories?) — a controlled
  vocabulary layered over cluster labels. `build_taxonomy` sources categories from the vault's
  `librarium_type` entity types + tags (always) plus optional `[ml] folder_taxonomy` config;
  `canonicalize_label`/`canonicalize_label_path` map raw cluster term-labels to the closest category
  by deterministic keyword/prefix overlap (air-gap safe, no model). Per-segment, nesting-aware,
  dedups `parent/parent`. Unit-tested.
- [x] LIB-078: colored folders — per-path `color_map` in preferences (mirrors `icon_map`,
  persisted server-side for both anon + per-user prefs), a "Set/Clear folder color" context-menu
  item + v-color-picker dialog in FileTreeNode, folder icon tinted from the map, and colors
  remap/clear alongside icons on rename/move/delete.
- [x] LIB-079: show full path to note somewhere
- [x] LIB-080: the desktop version of the app should remain logged in essentially indefinitely.
- [x] LIB-081: right click on tag and select delete
- [x] LIB-082: right click on note to delete in multiple location
- [x] LIB-083: select and delete multiple tags
- [ ] LIB-084: mark low quality tags

### Code review findings (2026-06) — severity-tagged

- [x] **LIB-085 · CRITICAL** (C2) Frontmatter is rewritten via `serde_json`→YAML without `preserve_order`, so keys come out alphabetically sorted/reformatted on every write (delete-tag, rename link-rewrite, ML apply, normal saves). Fix: enable `serde_json` `preserve_order` feature. — Already enabled in `librarium-server/Cargo.toml`; `Value` is `serde_json::Value` so YAML round-trips in document order. Added a `frontmatter_preserves_key_order_on_roundtrip` regression test (non-alphabetical keys).
- [x] **LIB-086 · CRITICAL** (C1) WAL switch undermines the startup DB backup — recent commits can live in an un-checkpointed `-wal` sidecar, so `.bak` may be stale/torn. Fix: `PRAGMA wal_checkpoint(TRUNCATE)` before copying (or copy `-wal`/`-shm`). — Already fixed: `checkpoint_wal()` opens a short-lived WAL connection and runs `PRAGMA wal_checkpoint(TRUNCATE)` before `backup_database_if_needed()` in `Database::new`.
- [x] **LIB-087 · CRITICAL** (C3) Delete-tag has no dry-run/preview and no undo for a vault-wide destructive rewrite. Fix: preview affected-file count + reuse the ML undo-receipt mechanism. — Already implemented: `?dry_run=true` returns the affected file list/count with no writes; real deletes snapshot each rewritten file as an `MlUndoReceipt` (`RestoreContent`) under a shared `group_id` for one-shot bulk undo.
- [x] **LIB-088 · CRITICAL** (C4) Inline `#tag` deletion is content-blind: strips tags inside code blocks/URLs and leaves dangling whitespace. Fix: word-boundary match, skip fenced code, swallow one adjacent space. — Already implemented: `(^|\s)#([A-Za-z0-9_-]+)` boundary regex (whole match consumed to collapse the leading space), fenced (```` ``` ````/`~~~`) and inline-backtick spans skipped. Covered by unit tests.
- [x] **LIB-089 · CRITICAL** (C5) LIB-080 stores a 10-year refresh token in `localStorage` and bumps the TTL on the server-global config (affects any client on that server). Fix: scope long lifetime to the embedded desktop session; move token to OS-secure storage. (Needs decision.) — Resolved: refresh token moved out of `localStorage` into an HttpOnly + SameSite=Strict cookie (`librarium_refresh`, `build_refresh_cookie`) unreadable by page JS/XSS; the 10-year TTL floor is applied only in the Tauri desktop binary (`librarium-tauri/src/main.rs`), not the shared server config. Decision: HttpOnly cookie (persists across desktop restarts via Max-Age) chosen over OS-keyring as the simpler, XSS-safe fit for the localhost/embedded context.
- [x] **LIB-090 · HIGH** (H1) Incremental `index_vault` dropped `delete_all_documents`, so on first run after upgrade old backslash-keyed docs linger → duplicate search results. Fix: `delete_all` when manifest is missing on a non-empty index. — `index_vault` now computes `stale_index = manifest_missing && searcher.num_docs() > 0` and calls `writer.delete_all_documents()` before re-adding (also opens the writer in that case even if nothing else changed). Regression test `missing_manifest_over_populated_index_does_not_duplicate`.
- [x] **LIB-091 · HIGH** (H2) Incremental indexing detects changes by mtime only; misses edits that preserve mtime (git checkout, restore, rsync). Fix: also compare file size (or hash). — Change detection now uses a `FileSig { mtime_ms, size }` (one `metadata()` stat) stored in the manifest; a size change is detected even when mtime is unchanged. Test `file_sig_tracks_size_change`.
- [x] **LIB-092 · HIGH** (H3) StatusBar active-note path uses global `activeTab`; may not track the focused pane in split view. Verify/fix. — Verified correct: `tabsStore.activeTab` is derived from `activePaneId`, and `PaneContainer` sets `activePaneId` via `@click.capture` on each pane wrapper, so focusing a pane updates the StatusBar path. No change needed.
- [x] **LIB-093 · HIGH** (H4) OrganizeVaultModal can leave the folder checkbox checked while `folderChoice` is empty, silently dropping the move. Fix: guard empty selection. — Default `folderChoice` now always resolves to a listed option (recommended target only if it's among candidates, else the first), and `applySelected` aborts with a clear error if any row has the folder box checked but no destination.
- [x] **LIB-094 · MEDIUM** (M1) IndexingStatus counter can stick `>0` (indicator stuck) if a `false` WS event is dropped or the client connects mid-operation. Fix: level-based state or reset on reconnect. — Indexing store now (a) resets on WS reconnect and (b) arms a 45s per-vault watchdog (re-armed on each level change) that force-clears a stuck counter, so a dropped `active:false` self-heals.
- [x] **LIB-095 · MEDIUM** (M2) WS reconnect doesn't resync indexing state or pending tree reloads. Fix: reconcile on reconnect. — The WS `open` handler detects a reconnect (`reconnectAttempts > 0`) and reconciles: `indexingStore.reset()` plus a debounced tree reload of the active vault, recovering from events missed during the outage.
- [x] **LIB-096 · MEDIUM** (M3) WAL + `max_connections=5` under the background indexer + watcher + reindex could surface 5s `busy_timeout` waits/500s. Verify no user-visible double-writer stall. — Verified acceptable: no code holds a multi-statement write transaction (all DB writes are single-statement autocommit via sqlx), and the search index is Tantivy (own per-vault writer mutex), so it never contends on SQLite. Under WAL, readers don't block the writer; concurrent writers serialize but each commit is sub-ms, well within the 5s `busy_timeout`. No change.
- [x] **LIB-097 · MEDIUM** (M4) The 25ms throttle sleep in the drain loop delays `FileChanged` WS broadcasts and entity writes for later events in a large batch. Acceptable; note the lag. — Documented the accepted tradeoff in the drain loop: up to ~`(len/40)*25ms` added latency for the last event of a very large bulk batch; correctness is unaffected (search index already committed) and interactive single edits are not throttled.
- [x] **LIB-098 · MEDIUM** (M5) Delete-tag walks + rewrites the whole vault synchronously on the actix worker thread. Fix: offload to `spawn_blocking`/background with a WS-completion message. — Extracted the FS walk+rewrite into `scan_and_rewrite_tag` run via `web::block` (actix blocking threadpool); the async handler only persists undo receipts and replies. The synchronous response is the completion signal and the watcher re-indexes rewritten files.
- [x] **LIB-099 · MEDIUM** (M6) Delete-tag leaves `tags: []` when the last array entry is removed; verify `%2F` tag path-segment decoding. — `remove_tag_from_frontmatter` now drops the `tags` key when the array empties (matching the scalar case), with a regression test. `%2F`: actix `web::Path` percent-decodes the captured segment, so a slash-containing tag arrives decoded and is handled by the existing trim/strip — acceptable.
- [x] **LIB-100 · LOW** (L1) Drain loop `try_recv` batch is uncapped; under sustained high event rate `events` can grow large. Consider a max-batch cap. — Drain now stops at `MAX_BATCH = 512`; the overflow stays queued and is handled by the next outer-loop iteration, bounding memory and per-commit size.
- [x] **LIB-101 · LOW** (L3/L4) Delete-note error handling swallows failures and leaves partial side effects (bookmark removed for a still-existing file). Fix: try/catch + surface errors. — `useDeleteNote` now deletes on the server first inside try/catch (alerts + returns false on failure) and only then closes tabs / clears icon prefs / prunes recents, so a failed delete leaves no orphaned side effects.
- [x] **LIB-102 · LOW** (L7) Manifest write is non-atomic (`fs::write` truncate+write); a crash mid-write corrupts it (safe-ish: triggers full re-index). Fix: write-temp-then-rename. — Manifest now written to `.index_manifest.json.tmp` then `fs::rename`d into place (atomic on one volume); the temp file is cleaned up if the rename fails.
- [x] **LIB-103 · LOW** (L6) Verify MlInsightsPanel default-expanded doesn't auto-fire an ML/embedding request on every note open. — Verified: the `[vaultId, filePath]` watcher only RESETS panel state on note change; no generate/embedding call. Outline/suggestion/rename requests fire only from explicit button clicks. No auto-fire.

- [x] LIB-104: For large note collections we need a way to right click an element in the file listing, click "move", have a separate panel/pane apepar on screen to select the folder/sub-folder to move the element into.
- [ ] LIB-105: display the folder choosing window for moving folder list as a tree rather than flat list
- [ ] LIB-106: when opening the folder choosing window, automatically put cursor in filter box so that typing begins typing a filter pattern
- [ ] LIB-107: when in the folder choosing window, enable arrow key navigation and tab navigation between elements, such that after typing a fitler, the user can arrow down or tab to the list. Once a folder is selected, typing enter should confirm it and then close the window.
- [ ] LIB-108 look at VSLLM for categorization folder organization for articles. It needs to suggest categories like DIY and Making for an article about choosing an anvil for smithing, and things like that. The sub-system needs completion/next token suggestion capabailities like an LLM rather than just pulling a word from the article itself. If hosting a VSLLM is more generally useful, I want it to support turning articles like listicles into just the list of pieces of advice.
