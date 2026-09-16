# WebKitGTK 2.36 compatibility

Constraints for running the Vue frontend on WebKitGTK 2.36 — the version
Ubuntu 22.04 LTS ships, and the **minimum Tauri desktop target**. Browser
users on any OS can use Chrome or Firefox and aren't subject to these limits.

| Distro | WebKitGTK | Status |
| --- | --- | --- |
| Ubuntu 22.04 LTS | 2.36 | Minimum target |
| Ubuntu 24.04 LTS | 2.44+ | Full support |
| Fedora 39+ | 2.44+ | Full support |

## Constraints

### JavaScript

- **ES2020 is the ceiling.** `vite.config.ts` sets `build.target: 'es2020'`
  (verified current) so the build never emits anything JavaScriptCore 2.36
  can't run.
- Logical assignment (`??=`, `||=`, `&&=`) isn't supported — the ES2020
  target handles this at build time, nothing to do manually.
- `String.prototype.replaceAll` (ES2021) — fall back to
  `.replace(/pattern/g, …)` if it's ever needed somewhere Vite doesn't
  polyfill it.

### CSS

- **Container queries (`@container`, `container-type`) are not supported.**
  Verified none are used anywhere in the frontend today (component styles or
  global CSS) — use Vuetify breakpoints or media queries if this ever comes
  up.
- The `<dialog>` element isn't supported; Vuetify's `v-dialog` is used
  throughout instead, which is already compliant.
- `aspect-ratio` is fine — supported since WebKitGTK 2.34.

### Canvas / WebGL

- **WebGL is not used anywhere in the frontend** (verified) — the D3
  knowledge graph renders as SVG exclusively, so this constraint doesn't
  currently bite.
- 2D canvas (`getContext('2d')`) is used by the PDF viewer only, which is
  fine — WebKitGTK 2.36 supports it.

### Web APIs in use

`ResizeObserver`, `IntersectionObserver`, `CSS.escape()`, and WebSocket are
all supported and all used (graph/split-pane sizing, file-tree lazy loading,
anchor navigation, and real-time sync respectively).

## Verifying WebKit behaviour locally

This repo has no hosted CI — `cargo xtask ci --full` (or Playwright directly)
is how you check this yourself:

```bash
cd frontend
npm ci
npx playwright install --with-deps webkit
npm run test:e2e -- --project=webkit    # both suites, webkit only
```

The `webkit` Playwright project targets `Desktop Safari`, which uses the
same WebKit engine and is a reasonable proxy for logic testing. It does
**not** run against WebKitGTK 2.36 specifically — for that, run the above on
an actual Ubuntu 22.04 machine or container with `libwebkit2gtk-4.0-dev`
installed, since Playwright's bundled WebKit and the system WebKitGTK the
Tauri shell links against are different runtimes that happen to share an
engine lineage.

On Fedora 43+, Playwright's bundled WebKit needs older SONAMEs the distro no
longer ships — see [BUILD.md](BUILD.md#playwright-webkit-on-fedora-43) for
the specifics and the `PLAYWRIGHT_INCLUDE_WEBKIT` escape hatch;
`playwright.config.ts` skips the `webkit` project on Fedora automatically.

## References

- [WebKitGTK release notes](https://webkitgtk.org/news.html)
- [Tauri v2 platform support](https://v2.tauri.app/start/prerequisites/)
