# Markdown Table Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Markdown tables reliable to create and edit in Librarium — fix the separator-row bug that makes hand-typed tables invalid, and add a contextual toolbar with Confluence-style row/column controls plus a grid-based table creator.

**Architecture:** All table logic lives in one new pure-function module, `frontend/src/editor/table.ts`, which parses a table block into a `ParsedTable` structure, mutates the structure, and serializes it back with auto-aligned columns. Vue components stay thin: `MarkdownEditor.vue` becomes an adapter that calls these functions, and `EditorToolbar.vue` renders a contextual button group driven by a `table-context` emit. No rich-text document model is introduced — the Markdown source remains the editing surface.

**Tech Stack:** TypeScript, Vue 3 `<script setup>`, Vuetify 3, Vitest, CodeJar.

**Spec:** `docs/superpowers/specs/2026-09-12-markdown-table-editing-design.md`

## Global Constraints

- Markdown files on disk are the source of truth. No transformation may alter content outside the table block being edited.
- All logic in `frontend/src/editor/*.ts` is pure: no Vue imports, no DOM access, no I/O.
- Every operation is total. If the cursor is not in a valid table, the command is a no-op returning content unchanged.
- The parser splits on **unescaped** pipes only; `\|` inside a cell is content.
- A block is only a table if a valid separator row is present. This is the single predicate for "is the cursor in a table?" and is shared by the toolbar's visibility and by the operations.
- The serializer always normalizes to leading and trailing pipes, and pads every column to its widest cell (minimum width 3, so `:-:` fits).
- Padding counts UTF-16 code units. CJK and emoji will still appear ragged; this is an accepted limitation, not a bug to fix here.
- Existing test convention: Vitest, 4-space indent, `describe`/`it`, imports from the module under test by relative path.
- Run `npm --prefix frontend test` for tests and `npm --prefix frontend run build` for the `vue-tsc` typecheck.

## Deviation from the spec (deliberate)

The spec calls for the table group to collapse into a menu **only on narrow viewports**. While planning I found `EditorToolbar.vue` already solves narrow screens by making the whole toolbar a single horizontally-scrollable row (`@media (max-width: 959px)`, lines 137-150). A viewport-conditional collapse would fight that existing pattern.

Instead: **on all viewports**, the six primary operations render as buttons and the remaining nine (moves, alignment, delete-table) live in one overflow menu inside the table group. Same commands, no viewport branching, consistent with the toolbar's existing overflow-menu pattern. This is a presentation-only change; the command set and every pure function are exactly as specced.

## File Structure

| File | Responsibility |
| --- | --- |
| `frontend/src/editor/table.ts` | **New.** Types, parser, serializer, cursor location, 7 mutators, `createTable`, command driver, Enter/Tab handlers. |
| `frontend/src/editor/table.test.ts` | **New.** Primary coverage for all of the above. |
| `frontend/src/editor/markdown-toolbar.ts` | Extend command union; delegate table commands to `table.ts`; replace `insertTable`. |
| `frontend/src/editor/markdown-toolbar.test.ts` | Driver dispatch and `table_create` payload cases. |
| `frontend/src/editor/index.ts` | Export the new module. |
| `frontend/src/components/editor/MarkdownEditor.vue` | Delete ~100 lines of helpers; keydown handlers become adapters; emit `table-context`; `applyCommand` gains optional payload. |
| `frontend/src/components/editor/EditorPane.vue` | Hold table context in a ref; pass to toolbar; forward command payload. |
| `frontend/src/components/editor/EditorToolbar.vue` | Contextual table group, overflow menu, grid picker. |

---

### Task 1: Table types, parser, and serializer

The foundation. Everything else operates on `ParsedTable`.

**Files:**
- Create: `frontend/src/editor/table.ts`
- Test: `frontend/src/editor/table.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: `ColumnAlignment`, `ParsedTable`, `TableCursor`, `splitRow(line: string): string[]`, `isSeparatorRow(line: string): boolean`, `findTableAt(content: string, offset: number): ParsedTable | null`, `serializeTable(table: ParsedTable): string`, `locateCursor(table: ParsedTable, content: string, offset: number): TableCursor`.

- [ ] **Step 1: Write the failing test for row splitting**

Create `frontend/src/editor/table.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { splitRow, isSeparatorRow } from './table';

describe('splitRow', () => {
    it('splits a fully delimited row and trims cells', () => {
        expect(splitRow('| a | b | c |')).toEqual(['a', 'b', 'c']);
    });

    it('splits a row with no outer pipes', () => {
        expect(splitRow('a | b')).toEqual(['a', 'b']);
    });

    it('keeps genuinely empty trailing cells', () => {
        expect(splitRow('| a | |')).toEqual(['a', '']);
    });

    it('does not split on an escaped pipe', () => {
        expect(splitRow('| a \\| b | c |')).toEqual(['a \\| b', 'c']);
    });
});

describe('isSeparatorRow', () => {
    it('accepts a plain separator', () => {
        expect(isSeparatorRow('| --- | --- |')).toBe(true);
    });

    it('accepts alignment markers', () => {
        expect(isSeparatorRow('|:---|:---:|---:|')).toBe(true);
    });

    it('rejects a content row', () => {
        expect(isSeparatorRow('| a | b |')).toBe(false);
    });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — cannot resolve `./table`.

- [ ] **Step 3: Implement types, `splitRow`, and `isSeparatorRow`**

Create `frontend/src/editor/table.ts`:

```ts
/**
 * Markdown (GFM) table parsing, mutation, and serialization.
 *
 * Pure functions only — no Vue, no DOM. The editor components are thin
 * adapters over this module. A block only counts as a table if it has a
 * valid separator row; `findTableAt` returning non-null is the single
 * predicate for "the cursor is in a table".
 */

export type ColumnAlignment = 'none' | 'left' | 'center' | 'right';

export interface ParsedTable {
    /** Leading whitespace of the header line, reapplied on serialize. */
    indent: string;
    /** Trimmed header cell texts. Length defines the column count. */
    header: string[];
    /** Parallel to `header`. */
    alignments: ColumnAlignment[];
    /** Body rows, each normalized to `header.length` cells. */
    rows: string[][];
    /** Absolute offset of the first character of the header line. */
    blockStart: number;
    /** Absolute offset just past the last character of the last row. */
    blockEnd: number;
}

export interface TableCursor {
    /** -1 addresses the header row; 0..n-1 address body rows. */
    rowIndex: number;
    colIndex: number;
}

/** Minimum separator width, so `:-:` always fits. */
const MIN_COL_WIDTH = 3;

const SEPARATOR_CELL = /^:?-+:?$/;

/**
 * Split one table row into trimmed cells, honouring `\|` escapes and
 * optional outer pipes.
 */
export function splitRow(line: string): string[] {
    const cells: string[] = [];
    let current = '';

    for (let i = 0; i < line.length; i += 1) {
        const ch = line[i];
        if (ch === '\\' && line[i + 1] === '|') {
            current += '\\|';
            i += 1;
            continue;
        }
        if (ch === '|') {
            cells.push(current);
            current = '';
            continue;
        }
        current += ch;
    }
    cells.push(current);

    // A leading pipe produces an empty first element, a trailing pipe an
    // empty last one. Drop those, but never a genuinely empty middle cell.
    if (cells.length > 1 && cells[0].trim() === '') cells.shift();
    if (cells.length > 1 && cells[cells.length - 1].trim() === '') cells.pop();

    return cells.map((c) => c.trim());
}

/** True when every cell of the line is a `---` / `:--` / `:-:` / `--:` marker. */
export function isSeparatorRow(line: string): boolean {
    if (!line.includes('|') && !line.includes('-')) return false;
    const cells = splitRow(line);
    if (cells.length === 0) return false;
    return cells.every((c) => SEPARATOR_CELL.test(c));
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS, 7 tests.

- [ ] **Step 5: Write the failing test for `findTableAt` and `serializeTable`**

Append to `frontend/src/editor/table.test.ts`:

```ts
import { findTableAt, serializeTable, locateCursor } from './table';

const TABLE = [
    '| Name | Role |',
    '| --- | ---: |',
    '| Ada | Eng |',
    '| Grace | Eng |',
].join('\n');

describe('findTableAt', () => {
    it('parses header, alignments, and rows', () => {
        const t = findTableAt(TABLE, 0)!;
        expect(t.header).toEqual(['Name', 'Role']);
        expect(t.alignments).toEqual(['none', 'right']);
        expect(t.rows).toEqual([['Ada', 'Eng'], ['Grace', 'Eng']]);
    });

    it('returns null for a header with no separator row', () => {
        const noSep = '| Name | Role |\n| Ada | Eng |';
        expect(findTableAt(noSep, 0)).toBeNull();
    });

    it('returns null when the cursor is outside any table', () => {
        expect(findTableAt('just prose', 2)).toBeNull();
    });

    it('pads short rows and drops overflow cells', () => {
        const ragged = '| a | b |\n| --- | --- |\n| 1 |\n| 1 | 2 | 3 |';
        const t = findTableAt(ragged, 0)!;
        expect(t.rows).toEqual([['1', ''], ['1', '2']]);
    });

    it('finds the table when the cursor is on a body row', () => {
        const offset = TABLE.indexOf('Grace');
        const t = findTableAt(TABLE, offset)!;
        expect(t.rows.length).toBe(2);
    });

    it('reports block bounds covering exactly the table', () => {
        const doc = `intro\n\n${TABLE}\n\noutro`;
        const t = findTableAt(doc, doc.indexOf('Ada'))!;
        expect(doc.slice(t.blockStart, t.blockEnd)).toBe(TABLE);
    });
});

describe('serializeTable', () => {
    it('pads columns to the widest cell and emits alignment markers', () => {
        const t = findTableAt(TABLE, 0)!;
        expect(serializeTable(t)).toBe([
            '| Name  | Role |',
            '| ----- | ---: |',
            '| Ada   | Eng  |',
            '| Grace | Eng  |',
        ].join('\n'));
    });

    it('round-trips: parsing serialized output yields an equal structure', () => {
        const once = findTableAt(TABLE, 0)!;
        const text = serializeTable(once);
        const twice = findTableAt(text, 0)!;
        expect(twice.header).toEqual(once.header);
        expect(twice.alignments).toEqual(once.alignments);
        expect(twice.rows).toEqual(once.rows);
    });
});

describe('locateCursor', () => {
    it('reports the header row as -1', () => {
        const t = findTableAt(TABLE, 0)!;
        expect(locateCursor(t, TABLE, TABLE.indexOf('Role'))).toEqual({ rowIndex: -1, colIndex: 1 });
    });

    it('reports a body cell', () => {
        const t = findTableAt(TABLE, 0)!;
        expect(locateCursor(t, TABLE, TABLE.indexOf('Grace'))).toEqual({ rowIndex: 1, colIndex: 0 });
    });
});
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — `findTableAt is not a function`.

- [ ] **Step 7: Implement `findTableAt`, `serializeTable`, and `locateCursor`**

Append to `frontend/src/editor/table.ts`:

```ts
function parseAlignment(cell: string): ColumnAlignment {
    const left = cell.startsWith(':');
    const right = cell.endsWith(':');
    if (left && right) return 'center';
    if (left) return 'left';
    if (right) return 'right';
    return 'none';
}

/** Line boundaries of `content`, as [start, end) offsets excluding the newline. */
function lineBounds(content: string): Array<{ start: number; end: number; text: string }> {
    const out: Array<{ start: number; end: number; text: string }> = [];
    let start = 0;
    for (let i = 0; i <= content.length; i += 1) {
        if (i === content.length || content[i] === '\n') {
            out.push({ start, end: i, text: content.slice(start, i) });
            start = i + 1;
        }
    }
    return out;
}

/** A line that could belong to a table block: non-blank and containing a pipe. */
function isTableCandidate(text: string): boolean {
    return text.trim() !== '' && text.includes('|');
}

export function findTableAt(content: string, offset: number): ParsedTable | null {
    const lines = lineBounds(content);
    const clamped = Math.max(0, Math.min(offset, content.length));
    let cursorLine = lines.findIndex((l) => clamped >= l.start && clamped <= l.end);
    if (cursorLine === -1) cursorLine = lines.length - 1;
    if (!isTableCandidate(lines[cursorLine].text)) return null;

    let first = cursorLine;
    while (first > 0 && isTableCandidate(lines[first - 1].text)) first -= 1;
    let last = cursorLine;
    while (last < lines.length - 1 && isTableCandidate(lines[last + 1].text)) last += 1;

    // The separator is the anchor: the header is the line directly above it.
    let sep = -1;
    for (let i = first + 1; i <= last; i += 1) {
        if (isSeparatorRow(lines[i].text)) {
            sep = i;
            break;
        }
    }
    if (sep === -1) return null;

    const headerLine = sep - 1;
    if (headerLine < first) return null;
    // A cursor sitting on prose above the header is not in the table.
    if (cursorLine < headerLine) return null;

    const header = splitRow(lines[headerLine].text);
    if (header.length === 0) return null;

    const sepCells = splitRow(lines[sep].text);
    const alignments: ColumnAlignment[] = header.map((_, i) =>
        i < sepCells.length ? parseAlignment(sepCells[i]) : 'none',
    );

    const rows: string[][] = [];
    for (let i = sep + 1; i <= last; i += 1) {
        if (isSeparatorRow(lines[i].text)) break;
        const cells = splitRow(lines[i].text);
        const normalized = header.map((_, c) => (c < cells.length ? cells[c] : ''));
        rows.push(normalized);
    }

    const indent = lines[headerLine].text.match(/^(\s*)/)?.[1] ?? '';
    const lastLine = sep + rows.length;

    return {
        indent,
        header,
        alignments,
        rows,
        blockStart: lines[headerLine].start,
        blockEnd: lines[lastLine].end,
    };
}

/** Column widths used by both the serializer and cursor arithmetic. */
function columnWidths(table: ParsedTable): number[] {
    return table.header.map((h, c) => {
        let w = Math.max(MIN_COL_WIDTH, h.length);
        for (const row of table.rows) w = Math.max(w, row[c]?.length ?? 0);
        return w;
    });
}

function separatorCell(alignment: ColumnAlignment, width: number): string {
    switch (alignment) {
        case 'left':
            return `:${'-'.repeat(width - 1)}`;
        case 'right':
            return `${'-'.repeat(width - 1)}:`;
        case 'center':
            return `:${'-'.repeat(width - 2)}:`;
        default:
            return '-'.repeat(width);
    }
}

export function serializeTable(table: ParsedTable): string {
    const widths = columnWidths(table);
    const line = (cells: string[]) =>
        `${table.indent}| ${cells.map((c, i) => c.padEnd(widths[i])).join(' | ')} |`;

    return [
        line(table.header),
        line(table.alignments.map((a, i) => separatorCell(a, widths[i]))),
        ...table.rows.map((r) => line(r)),
    ].join('\n');
}

/**
 * Absolute offset of the start of a cell's content in the *serialized*
 * table, given the table's `blockStart`. Layout is
 * `indent + "| " + cell0 + " | " + cell1 + ... + " |"`, so each preceding
 * column costs its width plus the three characters of " | ".
 */
export function offsetOfCell(table: ParsedTable, cursor: TableCursor): number {
    const widths = columnWidths(table);
    const lineIndex = cursor.rowIndex === -1 ? 0 : cursor.rowIndex + 2;
    const serialized = serializeTable(table).split('\n');

    let offset = table.blockStart;
    for (let i = 0; i < lineIndex && i < serialized.length; i += 1) {
        offset += serialized[i].length + 1;
    }
    offset += table.indent.length + 2;
    for (let c = 0; c < cursor.colIndex; c += 1) offset += widths[c] + 3;
    return offset;
}

export function locateCursor(table: ParsedTable, content: string, offset: number): TableCursor {
    const lines = lineBounds(content);
    const headerLine = lines.findIndex((l) => l.start === table.blockStart);
    const cursorLine = lines.findIndex((l) => offset >= l.start && offset <= l.end);

    const relative = cursorLine - headerLine;
    // 0 = header, 1 = separator (treated as the header row), 2+ = body.
    const rowIndex = relative <= 1 ? -1 : relative - 2;

    const text = lines[cursorLine]?.text ?? '';
    const within = offset - (lines[cursorLine]?.start ?? 0);
    let colIndex = 0;
    for (let i = 0; i < within && i < text.length; i += 1) {
        if (text[i] === '|' && !(i > 0 && text[i - 1] === '\\')) colIndex += 1;
    }
    // A leading pipe means the first cell is index 0, not 1.
    if (text.trimStart().startsWith('|') && colIndex > 0) colIndex -= 1;

    return {
        rowIndex: Math.min(rowIndex, table.rows.length - 1),
        colIndex: Math.max(0, Math.min(colIndex, table.header.length - 1)),
    };
}
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS, all tests.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/editor/table.ts frontend/src/editor/table.test.ts
git commit -m "feat(editor): add Markdown table parser and serializer

Pure-function module for GFM tables: splits on unescaped pipes only,
requires a separator row to recognise a table at all, and serializes
with auto-padded columns and alignment markers.

Refs #122"
```

---

### Task 2: Row operations

**Files:**
- Modify: `frontend/src/editor/table.ts`
- Test: `frontend/src/editor/table.test.ts`

**Interfaces:**
- Consumes: `ParsedTable` from Task 1.
- Produces: `insertRow(t: ParsedTable, at: number, side: 'above' | 'below'): ParsedTable`, `deleteRow(t: ParsedTable, at: number): ParsedTable | null`, `moveRow(t: ParsedTable, at: number, dir: 'up' | 'down'): ParsedTable`. `deleteRow` returns `null` when the last body row is removed, meaning "delete the whole table".

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/editor/table.test.ts`:

```ts
import { insertRow, deleteRow, moveRow } from './table';

describe('row operations', () => {
    const base = () => findTableAt(TABLE, 0)!;

    it('inserts a blank row above the given index', () => {
        const t = insertRow(base(), 1, 'above');
        expect(t.rows).toEqual([['Ada', 'Eng'], ['', ''], ['Grace', 'Eng']]);
    });

    it('inserts a blank row below the given index', () => {
        const t = insertRow(base(), 0, 'below');
        expect(t.rows).toEqual([['Ada', 'Eng'], ['', ''], ['Grace', 'Eng']]);
    });

    it('deletes the given row', () => {
        const t = deleteRow(base(), 0)!;
        expect(t.rows).toEqual([['Grace', 'Eng']]);
    });

    it('returns null when the last body row is deleted', () => {
        let t = deleteRow(base(), 0)!;
        expect(deleteRow(t, 0)).toBeNull();
    });

    it('moves a row down', () => {
        const t = moveRow(base(), 0, 'down');
        expect(t.rows).toEqual([['Grace', 'Eng'], ['Ada', 'Eng']]);
    });

    it('moving the first row up is a no-op', () => {
        const t = moveRow(base(), 0, 'up');
        expect(t.rows).toEqual([['Ada', 'Eng'], ['Grace', 'Eng']]);
    });

    it('moving the last row down is a no-op', () => {
        const t = moveRow(base(), 1, 'down');
        expect(t.rows).toEqual([['Ada', 'Eng'], ['Grace', 'Eng']]);
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — `insertRow is not a function`.

- [ ] **Step 3: Implement the row operations**

Append to `frontend/src/editor/table.ts`:

```ts
function blankRow(table: ParsedTable): string[] {
    return table.header.map(() => '');
}

export function insertRow(table: ParsedTable, at: number, side: 'above' | 'below'): ParsedTable {
    const index = side === 'above' ? at : at + 1;
    const rows = [...table.rows];
    rows.splice(Math.max(0, Math.min(index, rows.length)), 0, blankRow(table));
    return { ...table, rows };
}

/** Returns null when the table would be left with no body rows. */
export function deleteRow(table: ParsedTable, at: number): ParsedTable | null {
    if (table.rows.length <= 1) return null;
    const rows = [...table.rows];
    rows.splice(at, 1);
    return { ...table, rows };
}

export function moveRow(table: ParsedTable, at: number, dir: 'up' | 'down'): ParsedTable {
    const target = dir === 'up' ? at - 1 : at + 1;
    if (target < 0 || target >= table.rows.length) return table;
    const rows = [...table.rows];
    [rows[at], rows[target]] = [rows[target], rows[at]];
    return { ...table, rows };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/editor/table.ts frontend/src/editor/table.test.ts
git commit -m "feat(editor): add table row insert/delete/move operations

Refs #122"
```

---

### Task 3: Column operations

**Files:**
- Modify: `frontend/src/editor/table.ts`
- Test: `frontend/src/editor/table.test.ts`

**Interfaces:**
- Consumes: `ParsedTable` from Task 1.
- Produces: `insertColumn(t, at, side: 'before' | 'after'): ParsedTable`, `deleteColumn(t, at): ParsedTable | null`, `moveColumn(t, at, dir: 'left' | 'right'): ParsedTable`, `setColumnAlignment(t, at, alignment: ColumnAlignment): ParsedTable`. `deleteColumn` returns `null` when the last column is removed.

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/editor/table.test.ts`:

```ts
import { insertColumn, deleteColumn, moveColumn, setColumnAlignment } from './table';

describe('column operations', () => {
    const base = () => findTableAt(TABLE, 0)!;

    it('inserts a column before the given index', () => {
        const t = insertColumn(base(), 1, 'before');
        expect(t.header).toEqual(['Name', '', 'Role']);
        expect(t.rows[0]).toEqual(['Ada', '', 'Eng']);
        expect(t.alignments).toEqual(['none', 'none', 'right']);
    });

    it('inserts a column after the given index', () => {
        const t = insertColumn(base(), 0, 'after');
        expect(t.header).toEqual(['Name', '', 'Role']);
    });

    it('deletes a column and its alignment', () => {
        const t = deleteColumn(base(), 0)!;
        expect(t.header).toEqual(['Role']);
        expect(t.alignments).toEqual(['right']);
        expect(t.rows).toEqual([['Eng'], ['Eng']]);
    });

    it('returns null when the last column is deleted', () => {
        const t = deleteColumn(base(), 0)!;
        expect(deleteColumn(t, 0)).toBeNull();
    });

    it('moves a column right, carrying its alignment', () => {
        const t = moveColumn(base(), 0, 'right');
        expect(t.header).toEqual(['Role', 'Name']);
        expect(t.alignments).toEqual(['right', 'none']);
        expect(t.rows[1]).toEqual(['Eng', 'Grace']);
    });

    it('moving the first column left is a no-op', () => {
        expect(moveColumn(base(), 0, 'left').header).toEqual(['Name', 'Role']);
    });

    it('sets a column alignment', () => {
        const t = setColumnAlignment(base(), 0, 'center');
        expect(t.alignments).toEqual(['center', 'right']);
    });

    it('serializes centre alignment with the minimum width', () => {
        const t = setColumnAlignment(base(), 1, 'center');
        expect(serializeTable(t).split('\n')[1]).toBe('| ----- | :--: |');
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — `insertColumn is not a function`.

- [ ] **Step 3: Implement the column operations**

Append to `frontend/src/editor/table.ts`:

```ts
export function insertColumn(table: ParsedTable, at: number, side: 'before' | 'after'): ParsedTable {
    const index = Math.max(0, Math.min(side === 'before' ? at : at + 1, table.header.length));
    const header = [...table.header];
    const alignments = [...table.alignments];
    header.splice(index, 0, '');
    alignments.splice(index, 0, 'none');
    const rows = table.rows.map((r) => {
        const next = [...r];
        next.splice(index, 0, '');
        return next;
    });
    return { ...table, header, alignments, rows };
}

/** Returns null when the table would be left with no columns. */
export function deleteColumn(table: ParsedTable, at: number): ParsedTable | null {
    if (table.header.length <= 1) return null;
    const header = [...table.header];
    const alignments = [...table.alignments];
    header.splice(at, 1);
    alignments.splice(at, 1);
    const rows = table.rows.map((r) => {
        const next = [...r];
        next.splice(at, 1);
        return next;
    });
    return { ...table, header, alignments, rows };
}

export function moveColumn(table: ParsedTable, at: number, dir: 'left' | 'right'): ParsedTable {
    const target = dir === 'left' ? at - 1 : at + 1;
    if (target < 0 || target >= table.header.length) return table;

    const swap = <T>(arr: T[]): T[] => {
        const next = [...arr];
        [next[at], next[target]] = [next[target], next[at]];
        return next;
    };

    return {
        ...table,
        header: swap(table.header),
        alignments: swap(table.alignments),
        rows: table.rows.map((r) => swap(r)),
    };
}

export function setColumnAlignment(
    table: ParsedTable,
    at: number,
    alignment: ColumnAlignment,
): ParsedTable {
    const alignments = [...table.alignments];
    alignments[at] = alignment;
    return { ...table, alignments };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/editor/table.ts frontend/src/editor/table.test.ts
git commit -m "feat(editor): add table column operations and alignment

Insert/delete/move columns carry their alignment with them.

Refs #122"
```

---

### Task 4: `createTable` and the command driver

**Files:**
- Modify: `frontend/src/editor/table.ts`
- Test: `frontend/src/editor/table.test.ts`

**Interfaces:**
- Consumes: everything from Tasks 1-3, plus `MarkdownCommandResult` from `./markdown-toolbar`.
- Produces: `TableCommand` (string-literal union of the 15 parameterless commands), `createTable(rows: number, cols: number): string`, `applyTableCommand(content: string, offset: number, command: TableCommand): MarkdownCommandResult | null`, `insertTableAt(content: string, start: number, end: number, rows: number, cols: number): MarkdownCommandResult`.

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/editor/table.test.ts`:

```ts
import { createTable, applyTableCommand, insertTableAt } from './table';

describe('createTable', () => {
    it('creates a header plus the requested body rows', () => {
        expect(createTable(2, 2)).toBe([
            '|     |     |',
            '| --- | --- |',
            '|     |     |',
            '|     |     |',
        ].join('\n'));
    });

    it('clamps to at least one row and one column', () => {
        expect(createTable(0, 0)).toBe('|     |\n| --- |\n|     |');
    });
});

describe('applyTableCommand', () => {
    it('returns null when the cursor is not in a table', () => {
        expect(applyTableCommand('prose', 2, 'table_row_insert_below')).toBeNull();
    });

    it('adds a row below and re-aligns the whole table', () => {
        const res = applyTableCommand(TABLE, TABLE.indexOf('Ada'), 'table_row_insert_below')!;
        expect(res.content).toBe([
            '| Name  | Role |',
            '| ----- | ---: |',
            '| Ada   | Eng  |',
            '|       |      |',
            '| Grace | Eng  |',
        ].join('\n'));
    });

    it('places the cursor in the newly created row', () => {
        const res = applyTableCommand(TABLE, TABLE.indexOf('Ada'), 'table_row_insert_below')!;
        const line = res.content.slice(0, res.selectionStart).split('\n').length;
        expect(line).toBe(4);
    });

    it('preserves surrounding document text', () => {
        const doc = `intro\n\n${TABLE}\n\noutro`;
        const res = applyTableCommand(doc, doc.indexOf('Ada'), 'table_row_insert_below')!;
        expect(res.content.startsWith('intro\n\n')).toBe(true);
        expect(res.content.endsWith('\n\noutro')).toBe(true);
    });

    it('deletes the whole table when the last row is removed', () => {
        const single = '| a |\n| --- |\n| 1 |';
        const res = applyTableCommand(single, single.indexOf('1'), 'table_row_delete')!;
        expect(res.content).toBe('');
    });

    it('deletes the whole table on table_delete', () => {
        const doc = `intro\n\n${TABLE}\n\noutro`;
        const res = applyTableCommand(doc, doc.indexOf('Ada'), 'table_delete')!;
        expect(res.content).toBe('intro\n\n\noutro');
    });

    it('sets alignment on the cursor column', () => {
        const res = applyTableCommand(TABLE, TABLE.indexOf('Name'), 'table_align_center')!;
        expect(res.content.split('\n')[1]).toBe('| :---: | ---: |');
    });
});

describe('insertTableAt', () => {
    it('inserts a table at the cursor with surrounding newlines', () => {
        const res = insertTableAt('abc', 3, 3, 1, 2);
        expect(res.content).toBe('abc\n|     |     |\n| --- | --- |\n|     |     |');
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — `createTable is not a function`.

- [ ] **Step 3: Implement `createTable`, `applyTableCommand`, `insertTableAt`**

Append to `frontend/src/editor/table.ts`:

```ts
import type { MarkdownCommandResult } from './markdown-toolbar';

export type TableCommand =
    | 'table_col_insert_before'
    | 'table_col_insert_after'
    | 'table_col_delete'
    | 'table_col_move_left'
    | 'table_col_move_right'
    | 'table_row_insert_above'
    | 'table_row_insert_below'
    | 'table_row_delete'
    | 'table_row_move_up'
    | 'table_row_move_down'
    | 'table_delete'
    | 'table_align_none'
    | 'table_align_left'
    | 'table_align_center'
    | 'table_align_right';

/** `rows` counts body rows; a header row is always added on top. */
export function createTable(rows: number, cols: number): string {
    const nCols = Math.max(1, Math.floor(cols));
    const nRows = Math.max(1, Math.floor(rows));
    const table: ParsedTable = {
        indent: '',
        header: Array.from({ length: nCols }, () => ''),
        alignments: Array.from({ length: nCols }, () => 'none' as ColumnAlignment),
        rows: Array.from({ length: nRows }, () => Array.from({ length: nCols }, () => '')),
        blockStart: 0,
        blockEnd: 0,
    };
    return serializeTable(table);
}

export function insertTableAt(
    content: string,
    start: number,
    end: number,
    rows: number,
    cols: number,
): MarkdownCommandResult {
    const table = createTable(rows, cols);
    const prefix = start > 0 && content[start - 1] !== '\n' ? '\n' : '';
    const suffix = end < content.length && content[end] !== '\n' ? '\n' : '';
    const insertion = `${prefix}${table}${suffix}`;
    const nextContent = `${content.slice(0, start)}${insertion}${content.slice(end)}`;
    // Land in the first header cell: past the newline prefix, "| ".
    const caret = start + prefix.length + 2;
    return { content: nextContent, selectionStart: caret, selectionEnd: caret };
}

function replaceBlock(
    content: string,
    table: ParsedTable,
    next: ParsedTable | null,
    cursor: TableCursor,
): MarkdownCommandResult {
    if (next === null) {
        const nextContent = content.slice(0, table.blockStart) + content.slice(table.blockEnd);
        return {
            content: nextContent,
            selectionStart: table.blockStart,
            selectionEnd: table.blockStart,
        };
    }

    const serialized = serializeTable(next);
    const nextContent =
        content.slice(0, table.blockStart) + serialized + content.slice(table.blockEnd);

    const safeCursor: TableCursor = {
        rowIndex: Math.min(cursor.rowIndex, next.rows.length - 1),
        colIndex: Math.max(0, Math.min(cursor.colIndex, next.header.length - 1)),
    };
    const caret = offsetOfCell({ ...next, blockStart: table.blockStart }, safeCursor);
    return { content: nextContent, selectionStart: caret, selectionEnd: caret };
}

export function applyTableCommand(
    content: string,
    offset: number,
    command: TableCommand,
): MarkdownCommandResult | null {
    const table = findTableAt(content, offset);
    if (!table) return null;

    const cursor = locateCursor(table, content, offset);
    const row = Math.max(0, cursor.rowIndex);
    const col = cursor.colIndex;

    switch (command) {
        case 'table_col_insert_before':
            return replaceBlock(content, table, insertColumn(table, col, 'before'), cursor);
        case 'table_col_insert_after':
            return replaceBlock(content, table, insertColumn(table, col, 'after'), {
                ...cursor,
                colIndex: col + 1,
            });
        case 'table_col_delete':
            return replaceBlock(content, table, deleteColumn(table, col), cursor);
        case 'table_col_move_left':
            return replaceBlock(content, table, moveColumn(table, col, 'left'), {
                ...cursor,
                colIndex: Math.max(0, col - 1),
            });
        case 'table_col_move_right':
            return replaceBlock(content, table, moveColumn(table, col, 'right'), {
                ...cursor,
                colIndex: Math.min(table.header.length - 1, col + 1),
            });
        case 'table_row_insert_above':
            return replaceBlock(content, table, insertRow(table, row, 'above'), {
                ...cursor,
                rowIndex: row,
            });
        case 'table_row_insert_below':
            return replaceBlock(content, table, insertRow(table, row, 'below'), {
                ...cursor,
                rowIndex: row + 1,
            });
        case 'table_row_delete':
            return replaceBlock(content, table, deleteRow(table, row), {
                ...cursor,
                rowIndex: Math.max(0, row - 1),
            });
        case 'table_row_move_up':
            return replaceBlock(content, table, moveRow(table, row, 'up'), {
                ...cursor,
                rowIndex: Math.max(0, row - 1),
            });
        case 'table_row_move_down':
            return replaceBlock(content, table, moveRow(table, row, 'down'), {
                ...cursor,
                rowIndex: Math.min(table.rows.length - 1, row + 1),
            });
        case 'table_delete':
            return replaceBlock(content, table, null, cursor);
        case 'table_align_none':
            return replaceBlock(content, table, setColumnAlignment(table, col, 'none'), cursor);
        case 'table_align_left':
            return replaceBlock(content, table, setColumnAlignment(table, col, 'left'), cursor);
        case 'table_align_center':
            return replaceBlock(content, table, setColumnAlignment(table, col, 'center'), cursor);
        case 'table_align_right':
            return replaceBlock(content, table, setColumnAlignment(table, col, 'right'), cursor);
        default:
            return null;
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/editor/table.ts frontend/src/editor/table.test.ts
git commit -m "feat(editor): add table creation and the command driver

applyTableCommand maps the fifteen parameterless table commands onto the
pure operations, re-aligning the block and repositioning the caret.

Refs #122"
```

---

### Task 5: Enter and Tab handlers — the #122 separator fix

This is the fix for the headline bug: typing a header and pressing Enter must produce the separator row.

**Files:**
- Modify: `frontend/src/editor/table.ts`
- Test: `frontend/src/editor/table.test.ts`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: `handleTableEnterAt(content: string, offset: number): MarkdownCommandResult | null`, `handleTableTabAt(content: string, offset: number, reverse: boolean): MarkdownCommandResult | null`. Both return `null` for "not my case — let the default happen".

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/editor/table.test.ts`:

```ts
import { handleTableEnterAt, handleTableTabAt } from './table';

describe('handleTableEnterAt — #122 separator bug', () => {
    it('inserts a separator row and a blank body row after a bare header', () => {
        const src = '| Name | Role |';
        const res = handleTableEnterAt(src, src.length)!;
        expect(res.content).toBe([
            '| Name | Role |',
            '| ---- | ---- |',
            '|      |      |',
        ].join('\n'));
    });

    it('puts the caret in the first cell of the new body row', () => {
        const src = '| Name | Role |';
        const res = handleTableEnterAt(src, src.length)!;
        const before = res.content.slice(0, res.selectionStart);
        expect(before.split('\n').length).toBe(3);
    });

    it('adds a plain blank row when a separator already exists', () => {
        const res = handleTableEnterAt(TABLE, TABLE.indexOf('Eng'))!;
        expect(res.content.split('\n').length).toBe(5);
    });

    it('returns null outside a table', () => {
        expect(handleTableEnterAt('prose', 3)).toBeNull();
    });

    it('returns null on a blank line', () => {
        expect(handleTableEnterAt('| a |\n| --- |\n| 1 |\n\n', 20)).toBeNull();
    });
});

describe('handleTableTabAt', () => {
    it('moves to the next cell in the same row', () => {
        const res = handleTableTabAt(TABLE, TABLE.indexOf('Ada'), false)!;
        const after = res.content.slice(res.selectionStart);
        expect(after.startsWith('Eng')).toBe(true);
    });

    it('wraps from the last cell of the last row into a new row', () => {
        const offset = TABLE.lastIndexOf('Eng');
        const res = handleTableTabAt(TABLE, offset, false)!;
        expect(res.content.split('\n').length).toBe(5);
    });

    it('never lands on the separator row going backwards', () => {
        const offset = TABLE.indexOf('Ada');
        const res = handleTableTabAt(TABLE, offset, true)!;
        const line = res.content.slice(0, res.selectionStart).split('\n').length;
        expect(line).toBe(1);
    });

    it('returns null outside a table', () => {
        expect(handleTableTabAt('prose', 3, false)).toBeNull();
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: FAIL — `handleTableEnterAt is not a function`.

- [ ] **Step 3: Implement the handlers**

Append to `frontend/src/editor/table.ts`:

```ts
/**
 * Enter inside a table.
 *
 * The bug this fixes (#122): a freshly typed header row has no separator
 * beneath it, so the block is not a table yet and `findTableAt` returns
 * null. Previously the editor inserted another content row, so the
 * separator was never written and the table stayed invalid. Now that case
 * is detected explicitly and produces separator + blank row.
 */
export function handleTableEnterAt(content: string, offset: number): MarkdownCommandResult | null {
    const lines = lineBounds(content);
    const idx = lines.findIndex((l) => offset >= l.start && offset <= l.end);
    if (idx === -1) return null;

    const line = lines[idx];
    if (!isTableCandidate(line.text) || isSeparatorRow(line.text)) return null;

    const existing = findTableAt(content, offset);

    if (!existing) {
        // Bare header with no separator beneath it.
        const header = splitRow(line.text);
        if (header.length === 0) return null;

        const indent = line.text.match(/^(\s*)/)?.[1] ?? '';
        const table: ParsedTable = {
            indent,
            header,
            alignments: header.map(() => 'none' as ColumnAlignment),
            rows: [header.map(() => '')],
            blockStart: line.start,
            blockEnd: line.end,
        };
        const serialized = serializeTable(table);
        const nextContent =
            content.slice(0, line.start) + serialized + content.slice(line.end);
        const caret = offsetOfCell({ ...table, blockStart: line.start }, {
            rowIndex: 0,
            colIndex: 0,
        });
        return { content: nextContent, selectionStart: caret, selectionEnd: caret };
    }

    // Already a valid table: add a blank row below the cursor's row.
    return applyTableCommand(content, offset, 'table_row_insert_below');
}

/** Tab / Shift-Tab cell navigation that never lands on the separator row. */
export function handleTableTabAt(
    content: string,
    offset: number,
    reverse: boolean,
): MarkdownCommandResult | null {
    const table = findTableAt(content, offset);
    if (!table) return null;

    const cursor = locateCursor(table, content, offset);
    const lastCol = table.header.length - 1;
    const lastRow = table.rows.length - 1;

    let { rowIndex, colIndex } = cursor;

    if (reverse) {
        if (colIndex > 0) {
            colIndex -= 1;
        } else if (rowIndex > -1) {
            rowIndex -= 1;
            colIndex = lastCol;
        } else {
            return null; // At the very first cell; let the default happen.
        }
    } else if (colIndex < lastCol) {
        colIndex += 1;
    } else if (rowIndex < lastRow) {
        rowIndex += 1;
        colIndex = 0;
    } else {
        // Past the last cell: grow the table by one row.
        const grown = insertRow(table, Math.max(0, lastRow), 'below');
        return replaceBlock(content, table, grown, { rowIndex: lastRow + 1, colIndex: 0 });
    }

    const caret = offsetOfCell(table, { rowIndex, colIndex });
    const serialized = serializeTable(table);
    const nextContent =
        content.slice(0, table.blockStart) + serialized + content.slice(table.blockEnd);
    return { content: nextContent, selectionStart: caret, selectionEnd: caret };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `npm --prefix frontend test -- table.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/editor/table.ts frontend/src/editor/table.test.ts
git commit -m "fix(editor): Enter after a table header inserts the separator row

Pressing Enter on a freshly typed header produced another content row, so
the separator line was never written and the block was never a valid GFM
table. Detect the bare-header case explicitly and emit separator + blank
row, landing the caret in the first body cell.

Also reworks Tab/Shift-Tab navigation onto the parsed model so it cannot
strand the caret on the separator row.

Fixes the first symptom of #122"
```

---

### Task 6: Wire into `markdown-toolbar.ts`

**Files:**
- Modify: `frontend/src/editor/markdown-toolbar.ts`
- Modify: `frontend/src/editor/index.ts`
- Test: `frontend/src/editor/markdown-toolbar.test.ts`

**Interfaces:**
- Consumes: `TableCommand`, `applyTableCommand`, `insertTableAt` from Task 4.
- Produces: extended `MarkdownToolbarCommand` union including every `TableCommand` plus `'table_create'`; `applyMarkdownToolbarCommand(content, start, end, command, payload?)` where `payload` is `{ rows: number; cols: number } | undefined`.

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/editor/markdown-toolbar.test.ts`:

```ts
describe('table commands', () => {
    const TABLE = '| a | b |\n| --- | --- |\n| 1 | 2 |';

    it('dispatches a row insert through the driver', () => {
        const res = applyMarkdownToolbarCommand(TABLE, TABLE.indexOf('1'), TABLE.indexOf('1'), 'table_row_insert_below');
        expect(res.content.split('\n').length).toBe(4);
    });

    it('is a no-op when the cursor is not in a table', () => {
        const res = applyMarkdownToolbarCommand('prose', 2, 2, 'table_col_delete');
        expect(res.content).toBe('prose');
    });

    it('creates a table from a payload', () => {
        const res = applyMarkdownToolbarCommand('', 0, 0, 'table_create', { rows: 1, cols: 2 });
        expect(res.content).toBe('|     |     |\n| --- | --- |\n|     |     |');
    });

    it('defaults to 3x3 when table_create has no payload', () => {
        const res = applyMarkdownToolbarCommand('', 0, 0, 'table_create');
        expect(res.content.split('\n').length).toBe(5);
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm --prefix frontend test -- markdown-toolbar.test.ts`
Expected: FAIL — type error / unhandled command falls through to `default`.

- [ ] **Step 3: Extend the union and dispatch**

In `frontend/src/editor/markdown-toolbar.ts`, add the import at the top:

```ts
import { applyTableCommand, insertTableAt, type TableCommand } from './table';
```

Replace `| 'table'` in the `MarkdownToolbarCommand` union with:

```ts
    | 'table_create'
    | TableCommand
```

Add the payload type and extend the signature:

```ts
export interface TableCreatePayload {
    rows: number;
    cols: number;
}
```

Change the `applyMarkdownToolbarCommand` signature to accept the optional payload:

```ts
export function applyMarkdownToolbarCommand(
    content: string,
    start: number,
    end: number,
    command: MarkdownToolbarCommand,
    payload?: TableCreatePayload,
): MarkdownCommandResult {
```

Replace the existing `case 'table':` branch with:

```ts
        case 'table_create':
            return insertTableAt(content, start, end, payload?.rows ?? 3, payload?.cols ?? 3);
```

And immediately before `default:`, add the delegation for the fifteen parameterless commands:

```ts
        case 'table_col_insert_before':
        case 'table_col_insert_after':
        case 'table_col_delete':
        case 'table_col_move_left':
        case 'table_col_move_right':
        case 'table_row_insert_above':
        case 'table_row_insert_below':
        case 'table_row_delete':
        case 'table_row_move_up':
        case 'table_row_move_down':
        case 'table_delete':
        case 'table_align_none':
        case 'table_align_left':
        case 'table_align_center':
        case 'table_align_right':
            return (
                applyTableCommand(content, start, command) ?? {
                    content,
                    selectionStart: start,
                    selectionEnd: end,
                }
            );
```

Delete the now-unused `insertTable` function.

- [ ] **Step 4: Export the module**

In `frontend/src/editor/index.ts`:

```ts
// Editor module exports
export * from './table';
export * from './undo-redo';
export * from './utils';
```

- [ ] **Step 5: Run tests and typecheck**

Run: `npm --prefix frontend test`
Expected: PASS, all suites.

Run: `npm --prefix frontend run build`
Expected: `vue-tsc` clean. If it reports an unhandled `'table'` command anywhere, replace that reference with `'table_create'`.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/editor/markdown-toolbar.ts frontend/src/editor/markdown-toolbar.test.ts frontend/src/editor/index.ts
git commit -m "feat(editor): route table commands through the toolbar dispatcher

Replaces the fixed 3x3 'table' insert with table_create (which takes
rows/cols) and delegates the fifteen parameterless table commands to
applyTableCommand.

Refs #122"
```

---

### Task 7: `MarkdownEditor.vue` — adapters and table context

**Files:**
- Modify: `frontend/src/components/editor/MarkdownEditor.vue` (delete lines 257-345 and 347-400 region helpers; modify `applyCommand` at 764)

**Interfaces:**
- Consumes: `findTableAt`, `locateCursor`, `handleTableEnterAt`, `handleTableTabAt` from `@/editor/table`; `TableCreatePayload` from `@/editor/markdown-toolbar`.
- Produces: a `table-context` emit carrying `TableContext | null`, and `applyCommand(command, payload?)` exposed via `defineExpose`.

- [ ] **Step 1: Delete the superseded helpers**

Remove these functions from `MarkdownEditor.vue` — every one is now in `table.ts`:
`isTableLine`, `isTableDividerLine`, `tableColumnCount`, `getPipeIndices`, `getCellBounds`, `currentTableColumnIndex`, `makeBlankTableRowFrom`, `rowCellAbsoluteSelection`, `handleTableEnter`, `handleTableTab`.

- [ ] **Step 2: Add the import and the context type**

```ts
import {
  findTableAt,
  locateCursor,
  handleTableEnterAt,
  handleTableTabAt,
} from '@/editor/table';
import type { TableCreatePayload } from '@/editor/markdown-toolbar';

export interface TableContext {
  rowIndex: number;
  colIndex: number;
  rowCount: number;
  colCount: number;
  alignment: 'none' | 'left' | 'center' | 'right';
}
```

Add to the existing `defineEmits`:

```ts
  'table-context': [ctx: TableContext | null];
```

- [ ] **Step 3: Replace the keydown handlers with adapters**

At the two call sites (previously line 239 and 249), substitute:

```ts
    if (applyHandlerResult(handleTableEnterAt(jar.toString() as string, caretOffset()), e)) return;
```

and

```ts
    if (applyHandlerResult(handleTableTabAt(jar.toString() as string, caretOffset(), e.shiftKey), e)) return;
```

with these two helpers:

```ts
function caretOffset(): number {
  if (!editorEl.value) return 0;
  return getSelectionOffsets(editorEl.value).start;
}

/** Apply a pure handler's result, if it produced one. Returns true if handled. */
function applyHandlerResult(
  result: { content: string; selectionStart: number; selectionEnd: number } | null,
  e: KeyboardEvent,
): boolean {
  if (!result || !jar) return false;
  e.preventDefault();
  e.stopImmediatePropagation();
  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.selectionStart, result.selectionEnd);
  emitTableContext();
  return true;
}
```

- [ ] **Step 4: Emit table context on selection change**

```ts
function emitTableContext() {
  if (!jar || !editorEl.value) {
    emit('table-context', null);
    return;
  }
  const content = jar.toString() as string;
  const offset = caretOffset();
  const table = findTableAt(content, offset);
  if (!table) {
    emit('table-context', null);
    return;
  }
  const cursor = locateCursor(table, content, offset);
  emit('table-context', {
    rowIndex: cursor.rowIndex,
    colIndex: cursor.colIndex,
    rowCount: table.rows.length,
    colCount: table.header.length,
    alignment: table.alignments[cursor.colIndex] ?? 'none',
  });
}
```

Call `emitTableContext()` from the existing keyup, click, and `selectionchange` handling, and once after `applyCommand` completes.

- [ ] **Step 5: Thread the payload through `applyCommand`**

Change the signature at line 764 and forward the payload:

```ts
async function applyCommand(command: MarkdownToolbarCommand, payload?: TableCreatePayload) {
  if (!jar || !editorEl.value) return;

  if (command === 'extract_to_note') {
    await extractSelectionToNote();
    return;
  }

  const source = jar.toString() as string;
  const { start, end } = getSelectionOffsets(editorEl.value);
  const result = applyMarkdownToolbarCommand(source, start, end, command, payload);

  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.selectionStart, result.selectionEnd);
  emitTableContext();
}
```

- [ ] **Step 6: Verify tests and typecheck**

Run: `npm --prefix frontend test`
Expected: PASS.

Run: `npm --prefix frontend run build`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/editor/MarkdownEditor.vue
git commit -m "refactor(editor): move table helpers out of MarkdownEditor.vue

The table helpers lived inside a 1144-line component and could not be
unit-tested without mounting Vuetify, which is why the separator bug
shipped unnoticed. They now live in editor/table.ts; the component keeps
thin adapters and emits table-context for the toolbar.

Refs #122"
```

---

### Task 8: Contextual toolbar group and grid picker

**Files:**
- Modify: `frontend/src/components/editor/EditorPane.vue` (template line 31, handler line 236)
- Modify: `frontend/src/components/editor/EditorToolbar.vue`

**Interfaces:**
- Consumes: `TableContext` from `MarkdownEditor.vue`; the command union from Task 6.
- Produces: user-visible controls. No new exports.

- [ ] **Step 1: Hold and forward table context in `EditorPane.vue`**

```ts
import type { TableContext } from './MarkdownEditor.vue';

const tableContext = ref<TableContext | null>(null);
```

On the `<MarkdownEditor>` tag add `@table-context="tableContext = $event"`, and on `<EditorToolbar>` add `:table-context="tableContext"`.

Widen the command handler:

```ts
function onToolbarCommand(cmd: string, payload?: { rows: number; cols: number }) {
  if (cmd === 'undo') { markdownEditorRef.value?.callUndo(); return; }
  if (cmd === 'redo') { markdownEditorRef.value?.callRedo(); return; }
  if (cmd === 'collapse_all_folds') { markdownEditorRef.value?.collapseAllFolds(); return; }
  if (cmd === 'expand_all_folds') { markdownEditorRef.value?.expandAllFolds(); return; }
  markdownEditorRef.value?.applyCommand(cmd as any, payload);
}
```

Update the template binding to `@command="onToolbarCommand"`.

- [ ] **Step 2: Add the prop and widen the emit in `EditorToolbar.vue`**

```ts
import type { TableContext } from './MarkdownEditor.vue';

const props = defineProps<{
  mode: EditorMode;
  tableContext?: TableContext | null;
}>();

const emit = defineEmits<{
  command: [cmd: ToolbarCommand, payload?: { rows: number; cols: number }];
  'mode-change': [mode: EditorMode];
}>();

const inTable = computed(() => !!props.tableContext);
const activeAlignment = computed(() => props.tableContext?.alignment ?? 'none');
```

- [ ] **Step 3: Replace the fixed insert button with the grid picker**

Replace the existing `mdi-table-plus` button (line 58) with:

```vue
<v-menu :close-on-content-click="false" location="bottom start">
  <template #activator="{ props: menuProps }">
    <v-btn v-bind="{ ...btn, ...menuProps }" icon="mdi-table-plus" title="Insert table" />
  </template>
  <v-card class="pa-3">
    <div class="text-caption mb-2">{{ gridRows }} × {{ gridCols }}</div>
    <div
      v-for="r in 8"
      :key="r"
      class="d-flex"
    >
      <div
        v-for="c in 10"
        :key="c"
        class="grid-cell"
        :class="{ 'is-active': r <= gridRows && c <= gridCols }"
        @mouseenter="gridRows = r; gridCols = c"
        @click="emitCreate(r, c)"
      />
    </div>
    <div class="d-flex ga-2 mt-3">
      <v-text-field v-model.number="gridRows" label="Rows" type="number" density="compact" hide-details min="1" />
      <v-text-field v-model.number="gridCols" label="Columns" type="number" density="compact" hide-details min="1" />
      <v-btn size="small" @click="emitCreate(gridRows, gridCols)">Insert</v-btn>
    </div>
  </v-card>
</v-menu>
```

with the supporting script and style:

```ts
const gridRows = ref(3);
const gridCols = ref(3);

function emitCreate(rows: number, cols: number) {
  emit('command', 'table_create', { rows, cols });
}
```

```css
.grid-cell {
  width: 16px;
  height: 16px;
  margin: 1px;
  border: 1px solid rgb(var(--v-theme-border));
  cursor: pointer;
}
.grid-cell.is-active {
  background: rgb(var(--v-theme-primary));
}
```

- [ ] **Step 4: Add the contextual table group**

Insert immediately after the "Inserts" separator, rendered only when the caret is in a table:

```vue
<template v-if="inTable">
  <div class="toolbar-sep" />
  <v-btn v-bind="btn" icon="mdi-table-column-plus-before" title="Add column left" @mousedown.prevent="emit('command', 'table_col_insert_before')" />
  <v-btn v-bind="btn" icon="mdi-table-column-plus-after" title="Add column right" @mousedown.prevent="emit('command', 'table_col_insert_after')" />
  <v-btn v-bind="btn" icon="mdi-table-column-remove" title="Delete column" @mousedown.prevent="emit('command', 'table_col_delete')" />
  <v-btn v-bind="btn" icon="mdi-table-row-plus-before" title="Add row above" @mousedown.prevent="emit('command', 'table_row_insert_above')" />
  <v-btn v-bind="btn" icon="mdi-table-row-plus-after" title="Add row below" @mousedown.prevent="emit('command', 'table_row_insert_below')" />
  <v-btn v-bind="btn" icon="mdi-table-row-remove" title="Delete row" @mousedown.prevent="emit('command', 'table_row_delete')" />

  <v-menu location="bottom start">
    <template #activator="{ props: menuProps }">
      <v-btn v-bind="{ ...btn, ...menuProps }" icon="mdi-table-cog" title="More table options" />
    </template>
    <v-list density="compact" min-width="220">
      <v-list-subheader>Column alignment</v-list-subheader>
      <v-list-item prepend-icon="mdi-format-align-left" title="Left" :active="activeAlignment === 'left'" @click="emit('command', 'table_align_left')" />
      <v-list-item prepend-icon="mdi-format-align-center" title="Center" :active="activeAlignment === 'center'" @click="emit('command', 'table_align_center')" />
      <v-list-item prepend-icon="mdi-format-align-right" title="Right" :active="activeAlignment === 'right'" @click="emit('command', 'table_align_right')" />
      <v-list-item prepend-icon="mdi-format-align-justify" title="Default" :active="activeAlignment === 'none'" @click="emit('command', 'table_align_none')" />
      <v-divider class="my-1" />
      <v-list-subheader>Reorder</v-list-subheader>
      <v-list-item prepend-icon="mdi-arrow-up" title="Move row up" @click="emit('command', 'table_row_move_up')" />
      <v-list-item prepend-icon="mdi-arrow-down" title="Move row down" @click="emit('command', 'table_row_move_down')" />
      <v-list-item prepend-icon="mdi-arrow-left" title="Move column left" @click="emit('command', 'table_col_move_left')" />
      <v-list-item prepend-icon="mdi-arrow-right" title="Move column right" @click="emit('command', 'table_col_move_right')" />
      <v-divider class="my-1" />
      <v-list-item prepend-icon="mdi-table-remove" title="Delete table" @click="emit('command', 'table_delete')" />
    </v-list>
  </v-menu>
</template>
```

- [ ] **Step 5: Verify tests and typecheck**

Run: `npm --prefix frontend test`
Expected: PASS.

Run: `npm --prefix frontend run build`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/editor/EditorPane.vue frontend/src/components/editor/EditorToolbar.vue
git commit -m "feat(editor): contextual table toolbar and grid-based table insert

Six primary row/column controls appear while the caret is inside a table,
with alignment, reordering and delete-table in an overflow menu. The
insert button becomes a drag-or-type grid picker replacing the old fixed
3x3 insert.

Refs #122"
```

---

### Task 9: Manual verification, docs, and version bump

**Files:**
- Modify: `docs/DESIGN.md`
- Modify: `README.md` (only if the editor overview mentions table behaviour)
- Modify: version files via `cargo xtask bump-version`

- [ ] **Step 1: Verify #122 symptom 2 empirically**

Start the server and open a vault in a browser. Create a note containing a table. Switch to **Preview** (`fully_rendered`) mode and confirm the table renders as a real HTML table.

```bash
./target/debug/librarium --config config.toml
```

Record the outcome. If it renders correctly, symptom 2 is resolved by the auto-alignment work in `formatted_raw` plus already-correct behaviour in `fully_rendered`. **If it does not render, stop and open a separate issue** — that is a distinct defect from this epic.

- [ ] **Step 2: Verify the interactive behaviour that unit tests do not cover**

Walk through each and confirm:
- Typing `| Name | Role |` then Enter produces the separator row and a blank body row.
- The table group appears when the caret enters a table and disappears when it leaves.
- Each of the six buttons and nine menu items produces the expected change.
- The grid picker inserts the size shown by hover, and by typed values.
- On a narrow window the toolbar scrolls horizontally rather than wrapping.

- [ ] **Step 3: Update `docs/DESIGN.md`**

Add a short paragraph to the editor section recording that table parsing, mutation and serialization live in `frontend/src/editor/table.ts` as pure functions, that tables are auto-aligned on every operation, and that `formatted_raw` shows aligned Markdown source rather than a rendered grid — with the reason (files on disk are the source of truth; no rich-text round-trip).

- [ ] **Step 4: Bump the version**

```bash
cargo xtask bump-version
git diff
```

Review the diff, then include it in the final commit.

- [ ] **Step 5: Commit and open the PR**

```bash
git add -A
git commit -m "docs: record table editing design; bump version

Refs #122"
git push -u origin feat/markdown-table-editing
gh pr create --title "Markdown table editing: contextual toolbar, grid insert, separator fix" --body "Closes #122"
```

---

## Self-Review

**Spec coverage.** Every spec section maps to a task: data model and parsing rules → Task 1; the seven mutators → Tasks 2-3; `createTable`, the driver and error handling → Task 4; Enter/Tab and the separator fix → Task 5; command union and payload → Task 6; component adapters and `table-context` → Task 7; contextual group, grid picker and mobile → Task 8; the required empirical check of symptom 2, docs and version bump → Task 9.

**Placeholders.** None. Every code step contains runnable code; every test step contains real assertions.

**Type consistency.** `ParsedTable`, `TableCursor`, `ColumnAlignment` and `MarkdownCommandResult` are used with identical shapes throughout. `deleteRow` and `deleteColumn` both return `ParsedTable | null` and both are consumed through `replaceBlock`, which handles `null` as "delete the block". `applyTableCommand` returns `MarkdownCommandResult | null` and Task 6 supplies the no-op fallback. `TableCommand` (15 members) plus `'table_create'` gives the 16 commands the spec specifies.

**One knowing deviation** from the spec, recorded above: the overflow menu applies on all viewports rather than only narrow ones, because the toolbar already handles narrow screens by horizontal scrolling.
