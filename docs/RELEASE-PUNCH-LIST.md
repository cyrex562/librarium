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

---

## P0 — blocks any release at all

### 1. Two clippy lints will bite on the next Rust upgrade

No longer a CI problem — there is no CI — but still a real one. `cargo xtask ci`
runs `clippy -D warnings` against **your local toolchain**, currently
`rustc 1.96.0`, and passes. Rust 1.98 adds two lints this codebase trips:

| Lint | Where | Fix |
| --- | --- | --- |
| `result_large_err` (55 sites) | `librarium-client` — one root cause: `ClientError::WebSocket` holds ≥136 bytes | Box that one variant |
| `chunks_exact_to_as_chunks` (1 site) | `crates/librarium-server/src/services/embedding_service.rs:133` | `as_chunks::<4>().0.iter()` |

Nothing breaks today. The day you `rustup update`, `cargo xtask ci` goes red
with 56 errors from two root causes. Worth fixing on your own schedule rather
than discovering mid-task.

### 2. Nothing has ever been released, and the README already points at the empty page

`git tag` → 0. `gh release list` → empty. Meanwhile `README.md:170` tells Android
users to *"download the latest `Librarium-*-android-universal.apk` from the
Releases page."* **Anyone who reads the README today follows a dead link.**

Fixing this now means building artifacts by hand (see 4b) and attaching them
with `gh release create`. Do a throwaway `v0.102.4-rc1` first: the point is to
find out what breaks in packaging before anyone is watching. Until a release
exists, either publish one or soften the README's download instructions.

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

### 4b. Replacing the release pipeline

Options, roughly in increasing order of effort:

1. **`cargo xtask release` building Linux + Android locally**, published as a
   GitHub Release by hand or via `gh release create`. Covers you, most Linux
   testers, and Android. Windows and macOS users build from source.
2. **Add cross-compilation** — `cargo-xwin` (already a dependency you have
   installed) can produce Windows binaries from Linux; `cargo-zigbuild`
   (likewise installed) helps for glibc targets. Neither produces a macOS
   `.app` without a Mac.
3. **A borrowed machine per platform** — build on a Windows box and a Mac when
   you cut a release. Reliable, manual, and fine at this cadence.

The build commands themselves are all recoverable from
`git show 83626fc:.github/workflows/release.yml`, so whichever route you pick,
the recipes are not lost.

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

### 6. Auth is off by default, with no warning when you expose the port

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

1. **Decide how binaries get built** (item 4b) — everything else about
   distribution waits on this. A `cargo xtask release` covering Linux + Android
   is the smallest thing that works.
2. **Cut `v0.102.4-rc1`** (item 2) — packaging always breaks the first time;
   better to find out on a throwaway tag.
3. **`npm audit fix`** (item 5) — cheap, and best done before attention arrives.
4. **Rewrite the Quick Start around downloading a binary** (item 3), once step 2
   proves artifacts actually build.
5. **Docs triage** (item 4) — promote what survives verification out of
   `archive/`, add `SECURITY.md` and a root `CONTRIBUTING.md`.
6. **The 0.0.0.0 warning** (item 6) — small, prevents the worst mistake.
7. Everything in P3, as it suits you.

Items 1–4 are the realistic definition of "ready to hand to a friend." Items 5–6
are "ready to post publicly."
