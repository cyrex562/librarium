# Plugin Parity Roadmap (Librarium)

This roadmap tracks feature parity for requested Obsidian plugin behaviors in the Vue + Rust app.

## Status Legend

- ✅ Implemented in app
- 🟡 Partial / groundwork exists
- ⏳ Planned / not implemented yet

## Requested Plugin Parity Matrix

| Plugin / Feature | Status | Notes |
|---|---:|---|
| Advanced Tables | ⏳ | No table UX/editor tools yet. |
| Automatic List Management | ⏳ | Need Enter/Tab list continuation + auto-numbering behavior in editor. |
| Copy Button for Code Blocks | ✅ | Added in `MarkdownPreview.vue` (`Copy` / `Copied!` buttons on fenced code blocks). |
| Code Syntax Highlight | ✅ | Backend markdown renderer already applies syntax highlighting (`markdown_service.rs`, syntect). |
| Folder Notes | ⏳ | No folder-note resolution convention yet. |
| Iconize | ⏳ | No user-configurable per-file/folder icon metadata yet. |
| Kanban | ⏳ | No `.kanban`/board view editor yet. |
| Mermaid Tools | ⏳ | Mermaid fences are not yet rendered as diagrams in preview. |
| Mindmap | ⏳ | No mindmap parser/view yet. |
| Neighboring Files | ⏳ | No previous/next note navigation based on tree ordering. |
| Note Refactor | ⏳ | No heading/block extraction/refactor workflows yet. |
| Ordered List Style | ⏳ | No configurable numbering styles (roman/alpha etc.). |
| Recent Files | 🟡 | Backend+store endpoints exist; needs richer dedicated UI panel/workflow. |
| Tag Wrangler | ⏳ | No global rename/merge/delete tag operations yet. |
| Tasks | ⏳ | No dedicated task query panel/filters/scheduling UI yet. |
| Backlinks | 🟡 | Core plugin scaffolding exists; parity panel behavior in Vue app still needs full integration pass. |
| Bases (Dataview-like) | 🟡 | Metadata query specs/docs exist; full UI/query execution parity pending. |
| Bookmarks | ⏳ | No bookmark manager panel yet. |
| Canvas | 🟡 | Data model/plans exist; full interactive canvas parity still in-progress. |
| Footnotes View | 🟡 | Markdown parser supports footnotes; no dedicated footnotes panel/view yet. |
| Graph View | 🟡 | Models/plans exist; dedicated graph UI parity pending. |
| Outgoing Links | ⏳ | No dedicated outgoing links panel yet. |
| Outline | ⏳ | No live heading outline pane yet. |
| Properties View | ✅ | Frontmatter properties panel exists (`FrontmatterPanel.vue`). |
| Tags View | ⏳ | No dedicated tags browser panel in current Vue UI. |
| Word Count | 🟡 | Plugin exists under `plugins/word-count`; needs first-class Vue integration in current shell. |

## Recommended Implementation Waves

## Wave 1 — Editor UX parity (quick wins)

1. Automatic list management (continue/toggle task/numbered lists on Enter)
2. Ordered list style controls
3. Advanced tables editing helpers (row/column insert/delete, align)
4. Mermaid rendering in preview + toolbar insert

## Wave 2 — Navigation & knowledge surfaces

1. Outline panel
2. Backlinks panel (full Vue integration)
3. Outgoing links panel
4. Recent files panel
5. Neighboring files navigation

## Wave 3 — Metadata & organization

1. Tags view + Tag Wrangler ops
2. Bookmarks manager
3. Folder Notes behavior
4. Iconize-like icon metadata

## Wave 4 — Advanced views

1. Tasks panel/query DSL
2. Bases (Dataview-like views)
3. Graph view
4. Canvas editor completion
5. Kanban + Mindmap

## Proposed Next Sprint (start here)

- Implement **Automatic List Management** in `MarkdownEditor` key handling.
- Add **Outline panel** component (headings from current markdown content).
- Add **Outgoing links panel** using parsed wikilinks/markdown links.
- Add **Recent files sidebar section** (using existing `recentFiles` store).
