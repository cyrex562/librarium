# Markdown table editing — design

**Date:** 2026-09-12
**Status:** Approved, ready for implementation planning
**Issue:** [#122 — fix issues with tables](https://github.com/cyrex562/librarium/issues/122)
**Scope:** Frontend only (`frontend/src/editor/`, `frontend/src/components/editor/`)

## Problem

Creating and editing Markdown tables in Librarium is unreliable.

Issue #122 records three symptoms:

1. Typing a table by hand does not work. After the header row, the editor
   inserts another table row instead of the required separator row.
2. A table inserted from the toolbar shows as raw Markdown in the formatted
   editor rather than as a table.
3. There are no controls for modifying an existing table (add/remove rows and
   columns), of the kind Confluence provides.

### Root causes

**Symptom 1** is a real bug. `handleTableEnter`
(`frontend/src/components/editor/MarkdownEditor.vue:319`) checks only whether
the current line is a table line and is not itself a separator. It never
checks whether a separator row already exists below. Pressing Enter on a
freshly typed header therefore produces a blank *row*, the separator is never
written, and the block is not a valid Markdown table at all. This is the cause
of the reported difficulty creating tables by hand.

**Symptom 2** is architectural, not a bug in a single function. The editor is
CodeJar (a `contenteditable`) plus a bespoke line-based highlighter,
`frontend/src/utils/highlight.ts`. Lines 74–82 of that file deliberately render
table rows as styled `<span>` elements that retain the pipe characters. There
is no table rendering in `formatted_raw` mode to repair; it would have to be
built. The server-rendered `fully_rendered` mode is a separate path and
appears correct already — `pulldown-cmark` runs with `Options::ENABLE_TABLES`
(`crates/librarium-core/src/markdown_service.rs:56`) and `<table>` is styled at
`frontend/src/App.vue:159-160`.

**Symptom 3** is missing functionality.

A contributing factor across all three: the table helpers (`isTableLine`,
`isTableDividerLine`, `tableColumnCount`, `getPipeIndices`, `getCellBounds`,
`currentTableColumnIndex`, `makeBlankTableRowFrom`) live inside
`MarkdownEditor.vue`, a 1144-line component. They cannot be unit-tested
without mounting Vuetify, and they are not covered by the existing
`frontend/src/editor/editor-interactions.test.ts`, which tests pure helpers
against a jsdom textarea. The separator bug shipped because nothing could
easily test it.

## Decisions

These were settled during design and constrain everything below.

| Decision | Choice | Rationale |
| --- | --- | --- |
| Editing model | Styled Markdown source plus a contextual toolbar | Markdown files on disk are the source of truth (`CLAUDE.md`). A rich-text document model would round-trip the whole document and risk reformatting untouched content. |
| Rendering in `formatted_raw` | Pipes stay visible; no real grid | Follows from the editing model. Auto-alignment is what makes the source legible. |
| Column alignment of source | Auto-aligned after every table operation | Requested. Tables always read cleanly. Accepted cost: an edit to one cell can produce a multi-line diff. |
| Toolbar placement | Contextual group inside the existing `EditorToolbar` | No overlay positioning to maintain against scroll, resize, fold and re-highlight. Behaves identically in every mode and on mobile. |
| Implementation site | New pure-function module `frontend/src/editor/table.ts` | Auto-alignment makes a serializer mandatory; once a serializer exists, a matching parser is the natural other half. Matches the existing `line-indent.ts` / `list-indent.ts` convention. |

Rejected: adopting Tiptap (installed at 3.20.1, but `TiptapEditor.vue` is dead
code — nothing imports it) together with `@tiptap/extension-table`. It would
give true WYSIWYG tables, but requires a Markdown↔ProseMirror serializer whose
round-trip threatens Markdown fidelity across the entire document, not just
tables. Rejected: implementing operations in Rust (`librarium-core`). Desktop
and mobile both run this same Vue frontend, so nothing would actually be
shared, and it would add an IPC round-trip per button press.

## Architecture

### `frontend/src/editor/table.ts`

The single home for table parsing, mutation and serialization. Pure functions
only; no Vue, no DOM.

```ts
export type ColumnAlignment = 'none' | 'left' | 'center' | 'right';

export interface ParsedTable {
  indent: string;                 // common leading whitespace, preserved
  header: string[];               // trimmed cell texts; length = column count
  alignments: ColumnAlignment[];  // parallel to header
  rows: string[][];               // body rows, normalized to header.length
  blockStart: number;             // absolute offset of block start in document
  blockEnd: number;               // absolute offset of block end in document
}

export interface TableCursor {
  rowIndex: number;               // -1 = header row, 0..n-1 = body row
  colIndex: number;
}
```

Three primitives:

- `findTableAt(content: string, offset: number): ParsedTable | null` — walks
  outward from the cursor's line to the block boundaries and parses. **Returns
  `null` unless a valid separator row is present.** This makes "is the cursor
  in a table?" one trustworthy predicate, shared by the toolbar's visibility
  and by the operations themselves, so the two cannot disagree.
- `locateCursor(table: ParsedTable, content: string, offset: number): TableCursor`
- `serializeTable(table: ParsedTable): string` — pads each column to its widest
  cell and emits alignment markers (`---`, `:---`, `:---:`, `---:`). The sole
  place auto-alignment happens, so every operation inherits it.

Operations, each `ParsedTable → ParsedTable`:

```
insertColumn(t, at, 'before' | 'after')   deleteColumn(t, at)
insertRow(t, at, 'above' | 'below')       deleteRow(t, at)
moveRow(t, at, 'up' | 'down')             moveColumn(t, at, 'left' | 'right')
setColumnAlignment(t, at, alignment)
```

Plus `createTable(rows: number, cols: number): string` for the grid picker, and
a driver mapping `(content, offset, op) → MarkdownCommandResult` — the contract
`applyMarkdownToolbarCommand` already returns, so the call site at
`MarkdownEditor.vue:774` keeps its shape.

Keyboard behaviours also move here as pure functions:

```ts
handleTableEnterAt(content: string, offset: number): MarkdownCommandResult | null
handleTableTabAt(content: string, offset: number, reverse: boolean): MarkdownCommandResult | null
```

Each returns `null` for "not my case, let the default happen". The component's
keydown handlers become thin adapters: call, and if non-null apply the result
and `preventDefault()`.

### Parsing rules

- **Escaped pipes.** Cells may contain `\|`. The parser splits on *unescaped*
  pipes only. The current `getPipeIndices` does not, which is a latent bug
  today.
- **Optional outer pipes.** GFM permits `a | b` with no leading or trailing
  pipe. The parser accepts this; the serializer always normalizes to leading
  and trailing pipes.
- **Ragged rows.** GFM pads short rows and discards overflow cells. The parser
  normalizes to `header.length` on read, so operations never see ragged data.
- **Unicode width.** Padding counts code units, so CJK text and emoji will
  still appear ragged in a monospace font. Accepted limitation; true display
  width requires a width table and a new dependency.

### UI wiring

Sixteen new commands join the `MarkdownToolbarCommand` union. Fifteen are
parameterless — the target row and column always come from the cursor, so they
take no arguments:

```
table_col_insert_before   table_row_insert_above   table_row_move_up      table_align_none
table_col_insert_after    table_row_insert_below   table_row_move_down    table_align_left
table_col_delete          table_row_delete         table_col_move_left    table_align_center
table_delete                                       table_col_move_right   table_align_right
```

The sixteenth, `table_create`, is the exception: it carries `{ rows, cols }`
from the grid picker.

The parameterless fifteen route through the existing `emit('command', …)` →
`applyCommand` → `applyMarkdownToolbarCommand` path unchanged. One contract
change is required to accommodate `table_create`: an optional second argument,
`applyCommand(cmd, payload?)`.

**Context detection.** `MarkdownEditor` emits `table-context` on selection
change with `{ inTable, rowIndex, colIndex, rowCount, colCount } | null`,
computed via `findTableAt`. `EditorPane` holds it in a ref and passes it to
`EditorToolbar` as a prop; the table group renders under `v-if`. Alignment
buttons reflect the active column's current alignment as pressed state.

**Grid picker.** Replaces the current fixed 3×3 `mdi-table-plus` insert. A
`v-menu` containing a hover-to-size grid offering up to 8 rows × 10 columns,
with a live `4 × 3` readout, plus two number fields for larger tables typed
directly. Both paths call `createTable(rows, cols)`. `rows` counts body rows;
the header row is always created in addition, since GFM requires one.

**Mobile.** Fifteen icon buttons do not fit a phone toolbar. On narrow
viewports (reusing the existing `useMobile`) the group collapses to a single
`mdi-table` button opening the same operations as a `v-list` menu.

### Error handling

Every operation is total.

- `findTableAt` returning `null` makes the command a no-op returning content
  unchanged, matching the existing `default:` branch.
- Deleting the last column, or the last body row, removes the table rather than
  leaving a malformed fragment behind.
- `table_delete` removes the block and any trailing blank line it introduced.

## Fixes to #122

1. **Separator bug.** `handleTableEnterAt` adds the missing check: if the
   current line is a table line and the line below is not a valid separator,
   insert the separator row (derived from the header's column count, honouring
   any alignments already set) *and* a blank body row, placing the cursor in
   the first cell of that body row.
2. **Formatted preview.** Partly by design under the chosen editing model:
   `formatted_raw` keeps pipes and auto-alignment makes them legible. The
   `fully_rendered` path appears already correct from code inspection, but this
   **must be verified empirically in the running app** before the symptom is
   closed. If it is in fact broken there, that is a separate fix.
3. **Controls.** The contextual toolbar group described above.

Additionally, `handleTableTab` is reworked onto `findTableAt` / `locateCursor`
so Tab cannot strand the cursor on a separator row or silently exit the table.

## Testing

`frontend/src/editor/table.test.ts` carries the bulk of coverage, all
string-in/string-out:

- **Parsing** — with and without outer pipes; ragged rows padded and overflow
  dropped; all four alignment markers; escaped `\|` inside cells; indented
  tables; and rejection of a header with no separator, asserted directly as the
  #122 root cause.
- **Round-trip** — `serialize(parse(x))` is idempotent. The highest-value test,
  since auto-alignment routes every operation through the serializer.
- **Each of the sixteen table commands** — resulting cell content, column
  padding, and resulting cursor cell. Insert-column and move-column carry
  alignment with the column.
- **Collapse cases** — deleting the last column, and the last body row, remove
  the table rather than leaving `| |`.
- **Enter/Tab** — header-with-no-separator yields separator plus blank row with
  the cursor in the first body cell; Tab wraps from the last cell into a new
  row; Shift-Tab from the first body cell moves into the header; neither ever
  lands on the separator row.

`frontend/src/editor/markdown-toolbar.test.ts` gains driver-dispatch cases and
`table_create` payload handling.

**Not covered by unit tests:** toolbar show/hide on cursor movement, the grid
picker's hover-drag, and the mobile menu collapse. These are Vue component
behaviours; they will be verified manually in the running app. Adding
component-mount tests for them is a larger change to the frontend test setup
than this epic warrants.

**Regression guard:** `npm --prefix frontend test` and
`npm --prefix frontend run build` (the `vue-tsc` step catches
`MarkdownToolbarCommand` union changes across all call sites).

## Out of scope

- True WYSIWYG table rendering in `formatted_raw`.
- Removing the unused Tiptap dependency (worth its own issue).
- Display-width-aware padding for CJK and emoji.
- Table features beyond GFM: merged cells, nested blocks, per-cell styling.

## Files affected

| File | Change |
| --- | --- |
| `frontend/src/editor/table.ts` | New. Parser, serializer, operations, Enter/Tab handlers. |
| `frontend/src/editor/table.test.ts` | New. Primary test coverage. |
| `frontend/src/editor/markdown-toolbar.ts` | New command variants; delegate to `table.ts`; replace `insertTable`. |
| `frontend/src/editor/markdown-toolbar.test.ts` | Driver dispatch and `table_create` payload cases. |
| `frontend/src/editor/index.ts` | Export the new module. |
| `frontend/src/components/editor/MarkdownEditor.vue` | Remove ~100 lines of helpers; keydown handlers become adapters; emit `table-context`; `applyCommand` gains optional payload. |
| `frontend/src/components/editor/EditorPane.vue` | Hold table context; pass to toolbar; forward payload. |
| `frontend/src/components/editor/EditorToolbar.vue` | Contextual table group, grid picker menu, mobile collapse. |
