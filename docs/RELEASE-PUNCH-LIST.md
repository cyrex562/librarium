# Release punch list

**Date:** 2026-09-13 · **Version at audit:** 0.102.3 · **Repo:** already public, MIT, 0 stars, 0 releases

What stands between today and handing Librarium to friends or a GitHub audience.
Every item below is evidence-backed — file and line references, or commands you
can re-run.

**Verification note:** this repo has no hosted CI by choice. `cargo xtask ci` is
the gate; run it before every push.

---

## Already fixed during this audit

| # | Item | Evidence |
| --- | --- | --- |
| ✅ | **Hosted CI removed entirely.** GitHub Actions was judged not to work well for this project, so `.github/workflows/` is gone and `cargo xtask ci` is now the verification gate. It runs every check, prints one summary, exits non-zero on failure, and reports a missing tool as SKIPPED rather than PASSED. | `cargo xtask ci` → 5 passed, 0 failed. The deleted workflows remain in git history at `83626fc` if a build recipe is ever needed. |
| ✅ | **`release.yml` was an invalid workflow file** — the `android` job used `if: ${{ secrets.… }}` at *job* level, which GitHub Actions rejects, so the file never ran and **tagging would have produced no binaries**. Fixed in `6f2617c`, then removed with the rest of CI. | Retained here because it explains why no release ever built, and because the same recipe is the starting point for a local release script. |
| ✅ | **`cargo fmt --check` failed**, blocking every other job. Four files, two pre-existing. | Fixed in `2fe5494`; `cargo xtask ci` now covers it. |
| ✅ | **The two Rust 1.98 clippy lints** (item 1) are fixed: `ClientError::WebSocket` is boxed, and `blob_to_vector` uses `as_chunks::<4>()`. The Dockerfile moved 1.88 → 1.90, since `as_chunks` stabilised in exactly 1.88 and sitting on that boundary was too tight. **The Docker image was not rebuilt to confirm.** | `cargo clippy --all-targets --all-features -- -D warnings` clean. |
| ✅ | **The exposure warning** (item 6): binding a routable address now warns when auth is disabled, and always warns about cleartext HTTP. Loopback stays silent. | Verified both ways against a running server. |
| ✅ | **The E2E fixture and stale specs** (item 2, partial): 9 stale `ui/` specs fixed via PR #124 — an unmocked-endpoint 401 cascade, two default-state assumptions, and three assertions on UI that changed underneath them. | `ui/` alone: 165/12 → 173/4. Cross-suite contamination (`e2e/` polluting `ui/`) is the remaining, larger piece — still open. |
| ✅ | **E2E suite isolation** (item 2, the rest): `e2e/` and `ui/` now run against separate servers (PR #125), eliminating the 35-failure cross-contamination. | Verified running both back to back: `ui/` 174/3 (matches its 173/4 standalone baseline), `e2e/` 8/6 (unchanged). Remaining failures in each are real, independent, individually diagnosable. |
| ✅ | **Release pipeline decision** (item 4b): build locally per platform, publish by hand. Linux + Android on this machine; Windows built and published from a Windows host as needed, debug or release, via the existing `cargo xtask build-installer` / `build-desktop [--debug]`. macOS deferred — no Mac available. | User decision, 2026-09-14. No code change needed — the xtask commands already support this. |
| ✅ | **First release published** (item 1): [`v0.102.4-rc1`](https://github.com/cyrex562/librarium/releases/tag/v0.102.4-rc1), 5 artifacts, Windows to follow. | Server binary and Android APK verified running, not just built; desktop bundles verified structurally (no display to launch-test here). |

---

## P0 — blocks any release at all

### 1. Nothing has ever been released ✅ *(first rc published 2026-09-14)*

[`v0.102.4-rc1`](https://github.com/cyrex562/librarium/releases/tag/v0.102.4-rc1)
is up: server binary, server `.deb`, desktop AppImage, desktop `.deb`, and an
Android debug APK, all built locally per the 4b decision. Verified for real,
not just "the build exited 0":

- Server binary: ran `--version`, got `librarium 0.102.4`.
- Android APK: installed and launched on the local emulator (from a cold
  `adb install`), reached the pairing screen — the same screen verified
  working earlier this session when built from source.
- Desktop AppImage/deb: package structurally correct (`dpkg -c`), but **not
  launch-tested** — this box has no X/Wayland session, so GTK cannot
  initialize here. Needs a real display to confirm.
- `cargo xtask ci` green before tagging.

**Nothing broke in packaging** — the predicted "first tag surfaces real
breakage" didn't happen this time, likely because `cargo xtask build-installer`
/ `build-desktop` were already being exercised manually earlier in this
session's Android work.

**Known gaps in this rc**, documented in the release notes: no Windows
artifact yet (next per 4b), Android is an unsigned debug build (no release
keystore exists — a real gap, not a workaround) and is correspondingly huge
(~700 MB, unstripped native debug symbols for both ABIs), no macOS.

**Still true:** `README.md:170`'s Android download link now resolves to a real
(prerelease) asset instead of an empty page, but the README doesn't yet say
"prerelease" or point at this rc specifically — worth a follow-up once a
non-rc release exists.

**Unrelated discovery made while staging artifacts:** `dist/` is tracked in
git and already holds ~36 MB of stale binaries from an old pre-rename
"codex" deploy (`dist/deploy/codex-*.tar.gz`, `dist/server/codex`, dated
April). Not touched — release artifacts were staged in a separate untracked
`release-out/` instead. Worth its own cleanup pass; flagging rather than
fixing since it's unrelated to this task and someone should confirm nothing
depends on it first.

### 2. The Playwright E2E suite ✅ *(contamination fixed 2026-09-16, PR #125)*

**Two separate problems. Both now understood; the bigger one is fixed.**

**a) Cross-suite contamination — 35 of the original 47 failures. FIXED.** The
`e2e/` specs drive the *real* server (creating users, vaults, files) and used to
share one server process with the `ui/` specs, which mock most routes but fall
through to that now-dirty server for anything unmocked. Fixed by giving each
suite its own server, port, and SQLite state dir
(`frontend/playwright.shared.ts`) — structural isolation, not a chase for the
specific leak. Verified by running both back to back in one invocation:

| Run | `ui/` | `e2e/` |
| --- | --- | --- |
| Shared server (before) | 144 passed / 47 failed | — |
| Isolated servers (after) | 174 passed / 3 failed | 8 passed / 6 failed |

`ui/`'s isolated number matches its own standalone baseline (173/4, the ±1 is
ordinary retry noise) and `e2e/` is unchanged from its own standalone baseline
(8/6) — proving the contamination, not the suites themselves, was the 35-failure
cost.

**b) Stale tests — the residual dozen, of which 9 are fixed.** Every one was
test-vs-code drift: the app was behaving correctly and the test had not kept up.
That is what you would expect from a suite that last ran green in April 2026.

| Spec | Why it failed |
| --- | --- |
| `editor_toolbar` (6) | `/api/vaults/:id/favorites` unmocked → 401 → session cleared → blank page |
| `ml_insights` (3) | `POST /ml/analyze` unmocked → the same 401 cascade; plus the helper clicked the panel header unconditionally, which *collapsed* it (the panel defaults to expanded) |
| `file_tree` (2) | Assumed folders start expanded; they start collapsed, so "collapse all" ran against a tree that was never expanded |
| `canvas_editor` | Asserted the "Binary file" fallback; `.canvas` renders in `CanvasView` now |
| `context_menu` | Drove rename through an inline input; rename is a dialog now |
| `theme_mode`, `interface_elements_smoke` | Both matched `button[title="Theme"]`; that title is dynamic ("Switch to light theme") and never existed |

**Still failing, each a separate small investigation** — now cleanly
attributable since isolation removed the noise:

- `ui/` (3): `import_upload` (a "Subfolder" tree node never appears),
  `vault_management` (times out creating groups), `structural_editor` (passes
  standalone in isolation but flakes in a full `ui/` run — order-dependent
  *within* `ui/` itself, unrelated to `e2e/`; `import_export_advanced` is
  intermittently in this list too).
- `e2e/` (6), never diagnosed individually before because they were buried in
  the combined 47: `01-authentication` "short password shows validation
  error", `02-vault-management` "absolute path" and "invalid path" (×2),
  `03-file-operations` "opens in editor tab", "New File option", "closes its
  tab" (×3).

**Two traps worth remembering:**

- `npx playwright test | tail -30` exits 0 even when the run fails, because a
  shell pipeline returns the *last* command's status. Redirect to a file, or
  read `frontend/test-results/.last-run.json`.
- Playwright consults route handlers in **reverse** registration order. A
  catch-all mock added to `installCommonAppMocks` was tried and reverted: it
  shadowed every route that specs register *before* calling the helper, fixing
  nothing and breaking three specs. There is no registration order that is
  reliably lowest-priority — add the specific mock instead.

**None of this is recent regression:** the same failures reproduce at `5a7cd98`,
before the table and password epics.

---

## P1 — your two stated goals

### 3. Easy-to-run binaries

**The cross-platform build pipeline is gone.** `release.yml` built 8 artifacts
(Linux and Windows server binaries, Linux AppImage + deb, Windows NSIS, two
portable zips, Android APK); removing hosted CI removed that with it. From a
Linux laptop you can build Linux artifacts and the Android APK locally; Windows
and macOS need either those machines or a cross-build toolchain. **This is now
the largest open question for shipping binaries** — see "Replacing the release
pipeline" below.

The remaining gaps:

- **The Quick Start requires building from source.** `README.md:57` opens with
  `npm install` → `npm run build` → `cargo run`. That asks a friend to install
  Node *and* the Rust toolchain before seeing anything. There is no
  "download this file and run it" path anywhere in the README.
- **No macOS build at all.** The release matrix is Linux + Windows + Android.
  If any of your testers use a Mac, they must build from source.
- **`docker-compose.yml` builds from source** (`build: .`, `image: librarium:latest`)
  — there is no published image, so `docker compose up` compiles Rust rather
  than pulling a container. Publishing to GHCR would be the single cheapest
  "works everywhere" distribution channel.
- **No release keystore exists yet.** A release APK must be signed to be
  installable. `crates/librarium-tauri/gen/android/keystore.properties` is
  gitignored and absent, so `cargo tauri android build --apk` currently
  produces an *unsigned* release APK, which Android will refuse to install.
  Generating a keystore once (AGENTS.md's "Android release signing") is a
  prerequisite for shipping the APK the README promises.
- **Windows install/upgrade is an open question** — issue #27.

### 4. Documentation people can read

Better than it first appears, and worse in a specific way.

**`docs/archive/` already contains real user documentation** — 42 files,
including `DEPLOYMENT.md` (266 lines), `BUILD.md` (232), `API.md` (150),
`CONFIGURATION.md` (107), `DOCKER.md` (76), `CONTRIBUTING.md` (50), and
`LIBRARIUM_OVERVIEW.md` (846).

The problem: `docs/DESIGN.md` states *"Treat archived files as background, not
as a description of the current system."* So the project's own docs tell readers
not to trust the only deployment guide — which `README.md:73` nonetheless links
to as the deployment guide. That contradiction is worse than having no docs,
because a reader cannot tell which parts are true.

All seven were last touched **2026-06-28**, roughly two and a half months and
several subsystems ago (mobile client, the auth changes, tables). They need
verification, not just relocation.

**The work is triage, not authorship:** read each, verify against the current
code, promote the accurate ones out of `archive/` into a real `docs/` tree, and
delete or clearly date-stamp the rest.

**Missing entirely:**

| File | Why it matters for a public repo |
| --- | --- |
| `CONTRIBUTING.md` (root) | One exists in `archive/`; GitHub only surfaces it at the root or `.github/` |
| `SECURITY.md` | Where to report a vulnerability privately, on a repo that handles passwords |
| `CHANGELOG.md` | Testers need to know what changed between builds |
| `.github/ISSUE_TEMPLATE/` | Bug reports without version/OS/logs cost a round-trip each |
| Backup/restore guidance | Nothing in the README says what to back up. The vault is Markdown on disk, but `librarium.db` holds users, API keys, and sync state |

---

### 4b. Replacing the release pipeline ✅ *(decided 2026-09-14)*

**Decision: build locally per platform, publish by hand.**

- **Linux + Android**: built on this machine.
  `cargo xtask build-desktop` / `build-frontend` for the server+desktop bundle;
  the Android APK via `cargo tauri android build` (see AGENTS.md's "Android
  build" section) or `scripts/android-deploy.sh` for a device-installed build.
- **Windows**: built by hand on a Windows host.
  `cargo xtask build-installer` for a distributable NSIS `.exe` (release-only,
  idempotent — re-running it upgrades an existing install in place);
  `cargo xtask build-desktop [--debug]` for an unpackaged debug or release
  binary. No cross-compilation setup needed on this machine.
- **macOS**: not yet decided — no Mac available. Revisit if a tester asks.

Publish artifacts to a GitHub Release by hand (`gh release create` or the web
UI) once built on each platform. The old automated matrix's build commands are
recoverable from `git show 83626fc:.github/workflows/release.yml` if this ever
needs automating again, but for the current cadence a person building on each
platform and uploading the result is the whole pipeline.

## P2 — before strangers run this

### 5. 18 vulnerable production dependencies

`npm --prefix frontend audit --omit=dev` → **5 moderate, 13 high** (24 including
dev). Two are directly reachable from untrusted note content:

- **`dompurify`** — this is the XSS sanitizer guarding rendered Markdown
  (`MarkdownPreview.vue:56`). A weakness here is a weakness in the control that
  exists to stop XSS.
- **`yaml`** — stack overflow on deeply nested collections; used to parse
  frontmatter in three places (`FrontmatterPanel.vue`, `structural-utils.ts`,
  `NewEntityDialog.vue`), i.e. on content from any note a user opens.

Also `lodash-es` (prototype pollution, code injection), `linkify-it` (quadratic
DoS on attacker text), `@tiptap/core`, `markdown-it`.

Every one reports **"fix available via `npm audit fix`"**, so this is likely an
afternoon, not a project. Worth doing before the repo gets attention — and
re-running as a release gate.

### 6. Auth is off by default ✅ *(warning added — see the table at the top)*

`default_auth_enabled()` returns `false` (`config/mod.rs:346`). The default bind
is `127.0.0.1`, which makes that defensible locally. But nothing warns when
someone sets `host = "0.0.0.0"` **and** leaves auth disabled — that combination
publishes an unauthenticated, fully writable vault to the LAN.

A startup warning when `host != 127.0.0.1 && !auth.enabled` is a few lines and
prevents the worst self-hosting mistake.

### 7. TLS works but is undocumented for the networked case

Better than the open epic (#16) suggests: rustls is wired up and
`[tls].cert_file`/`key_file` are honoured (`lib.rs:786`). What is missing is the
*guidance* — the recommended path (reverse proxy terminating TLS, e.g. Caddy)
and a plain statement that loopback-only HTTP is intentional and safe. That is
docs work, not feature work.

---

## P3 — polish

- **Raw OS errors on common failures.** Starting with the port occupied prints
  `Error: Address already in use (os error 98)` and exits — no mention of the
  port, or of `--config`/`LIBRARIUM__SERVER__PORT`. First-run friction.
- **Mobile pair + sync is blocked** by issue #88 (Android Keystore JNI
  bootstrap). The emulator is now set up locally to debug it. Until then the
  README's Android instructions cannot be completed by a reader.
- **Two open verification issues**, #106 and #107, from a previous Tauri ACL
  fix — both are "confirm this still works," cheap to close.
- **Issue #122 (tables)** can probably be closed: symptoms 1 and 3 are fixed,
  and symptom 2 turned out to be by-design. **#123** (rendered tables use `<th>`
  for body cells and drop column alignment) is filed and open.
- **Unused Tiptap dependency.** `TiptapEditor.vue` is imported by nothing; five
  `@tiptap/*` packages ship in `package.json`. Removing them cuts bundle size
  and one of the high-severity advisories above.

---

## Suggested order

1. **Build and attach the Windows artifact to `v0.102.4-rc1`** (item 4b
   follow-through) — from a Windows host: `cargo xtask build-installer` for
   the NSIS installer, `cargo xtask build-desktop --debug` if a debug build is
   also wanted, then `gh release upload v0.102.4-rc1 <files>`.
2. **`npm audit fix`** (item 5) — cheap, and best done before attention arrives.
3. **Rewrite the Quick Start around downloading a binary** (item 3) — v0.102.4-rc1
   proves the artifacts work; update the README to point at it (and say
   "prerelease") once Windows lands.
4. **Docs triage** (item 4) — promote what survives verification out of
   `archive/`, add `SECURITY.md` and a root `CONTRIBUTING.md`.
5. **The 9 individually-diagnosable E2E failures** (item 2's remainder) — no
   longer urgent now that they're not masking each other or a bigger problem,
   but worth clearing before relying on the suite as a real regression gate.
6. Everything in P3, as it suits you. The stray `dist/` cruft noted under
   item 1 is worth a look here too — since fixed: it is gone, tracked cruft was
   removed and `dist/` is gitignored.

Items 1, 2, 4b, and 6 (the exposure warning) are done, as are the clippy lints
from the old item 1 — see the table at the top.

Items 1–4 are the realistic definition of "ready to hand to a friend." Adding
item 5 (docs) gets you to "ready to post publicly."
