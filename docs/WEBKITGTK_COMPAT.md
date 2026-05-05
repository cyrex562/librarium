# WebKitGTK 2.36 Compatibility

This document records the compatibility constraints, verification approach, and
known issues for running the Librarium Vue frontend on WebKitGTK 2.36 (the version
shipped with Ubuntu 22.04 LTS).

WebKitGTK 2.36 is the **minimum Tauri desktop target**. Browser users on Ubuntu
22.04 can use Chrome or Firefox as a zero-install fallback and are not subject
to these constraints.

---

## Why WebKitGTK 2.36?

Ubuntu 22.04 LTS is an actively-supported enterprise distribution with a large
user base. It ships WebKitGTK 2.36 in its default repositories and is a known
deployment target (e.g. AWS Workspaces). Tauri 2 supports this version.

| Distro           | WebKitGTK | Status         |
|------------------|-----------|----------------|
| Ubuntu 22.04 LTS | 2.36      | Minimum target |
| Ubuntu 24.04 LTS | 2.44+     | Full support   |
| Fedora 39/40     | 2.44+     | Full support   |

---

## Known Constraints

### JavaScript engine (JavaScriptCore in WebKitGTK 2.36)

- **ES2020 is the maximum safe target.** The Vite build is configured with
  `build.target: 'es2020'` in `vite.config.ts` to prevent transpilation of
  features not supported by this engine.
- Logical assignment operators (`??=`, `||=`, `&&=`) are **not** supported and
  must not appear in transpiled output. Vite's ES2020 target handles this.
- `String.prototype.replaceAll` — supported from ES2021; use `.replace(/pattern/g, …)`
  as a fallback if Vite does not polyfill it.

### CSS

- **CSS container queries (`@container`, `container-type`) are not supported.**
  Use Vuetify responsive breakpoints or media queries instead. No container
  queries were found in the codebase as of the Phase 3 audit.
- The `<dialog>` HTML element is not supported. Vuetify modal/dialog components
  (`v-dialog`) are used throughout — this is already compliant.
- `aspect-ratio` CSS property — supported since WebKitGTK 2.34, so 2.36 is fine.

### Canvas / WebGL

- **WebGL is not supported.** The D3 knowledge graph renderer uses SVG
  exclusively (verified in `GraphView.vue`). No WebGL code paths exist.
- 2D canvas (`getContext('2d')`) is supported and is used only by the PDF viewer
  component (`PdfViewer.vue`). This is acceptable — PDF rendering requires canvas
  and PDF viewing is a non-critical feature.

### Web APIs

- `ResizeObserver` — supported; used for graph sizing and split-pane resize.
- `IntersectionObserver` — supported; used for lazy-loading in file tree.
- `CSS.escape()` — supported; used in anchor navigation.
- WebSocket — supported; used for real-time sync.

---

## Verification Approach

### CI gate

A dedicated `webkit-compat` job in `.github/workflows/ci.yml` runs on the
`ubuntu-22.04` GitHub-hosted runner (which ships WebKitGTK 2.36). It:

1. Installs `libwebkit2gtk-4.0-dev` from the system package manager.
2. Installs the Playwright `webkit` browser via `npx playwright install webkit`.
3. Starts the `librarium-server` binary against a temp config.
4. Runs the full Playwright suite with `--project=webkit`.

This job must pass before Phase 3 is considered complete and before Phase 4
(desktop client retirement) begins.

### Local verification (Ubuntu 22.04)

```bash
# On an Ubuntu 22.04 machine or inside a container:
sudo apt-get install -y libwebkit2gtk-4.0-dev

cd frontend
npm ci
npx playwright install --with-deps webkit
npm run test:e2e -- --project=webkit
```

### Playwright webkit project

The Playwright `webkit` project in `playwright.config.ts` targets `Desktop Safari`
device, which uses WebKit and is a close proxy for WebKitGTK logic testing.
The `webkit-compat` CI job runs the actual Playwright webkit binary on the
Ubuntu 22.04 runner for the definitive gate.

---

## Phase 3 Audit Results

The following checks were performed as part of the Phase 3 audit:

| Check | Result |
|---|---|
| `vite.config.ts` build target | Set to `es2020` ✓ |
| CSS container queries in Vue components | None found ✓ |
| CSS container queries in global CSS | None found ✓ |
| D3 graph renderer uses canvas or WebGL | No — SVG only ✓ |
| `<dialog>` element usage | None — Vuetify modals used throughout ✓ |
| Playwright `firefox` project added | ✓ |
| Playwright `webkit` project added | ✓ |
| CI `webkit-compat` job added | ✓ (`ubuntu-22.04` runner) |

---

## Known Issues

None identified as of the Phase 3 audit. This section will be updated as the
`webkit-compat` CI job accumulates run history.

---

## References

- [WebKitGTK release notes](https://webkitgtk.org/news.html)
- [Can I use — CSS Container Queries](https://caniuse.com/css-container-queries)
- [Tauri v2 platform support](https://v2.tauri.app/start/prerequisites/)
- `docs/tauri_desktop_plan.md` — Phase 3 spec
