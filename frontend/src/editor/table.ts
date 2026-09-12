/**
 * Markdown (GFM) table parsing, mutation, and serialization.
 *
 * Pure functions only — no Vue, no DOM. The editor components are thin
 * adapters over this module. A block only counts as a table if it has a
 * valid separator row; `findTableAt` returning non-null is the single
 * predicate for "the cursor is in a table", shared by the toolbar's
 * visibility and by the operations themselves so the two cannot disagree.
 */

// Type-only, so there is no runtime cycle with markdown-toolbar.ts (which
// imports this module's functions).
import type { MarkdownCommandResult } from './markdown-toolbar';

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
    if (!line.includes('-')) return false;
    const cells = splitRow(line);
    if (cells.length === 0) return false;
    return cells.every((c) => SEPARATOR_CELL.test(c));
}

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
        rows.push(header.map((_, c) => (c < cells.length ? cells[c] : '')));
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
        `${table.indent}| ${cells.map((c, i) => (c ?? '').padEnd(widths[i])).join(' | ')} |`;

    return [
        line(table.header),
        line(table.alignments.map((a, i) => separatorCell(a, widths[i]))),
        ...table.rows.map((r) => line(r)),
    ].join('\n');
}

/**
 * Absolute offset of the start of a cell's content in the *serialized*
 * table. Layout is `indent + "| " + cell0 + " | " + cell1 + ... + " |"`,
 * so each preceding column costs its width plus the three characters of
 * `" | "`.
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
    let cursorLine = lines.findIndex((l) => offset >= l.start && offset <= l.end);
    if (cursorLine === -1) cursorLine = headerLine;

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

    const swap = <T,>(arr: T[]): T[] => {
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
    // Land in the first header cell: past the newline prefix, then "| ".
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
        // Also consume the newline the removed block leaves behind, so
        // deleting a table does not open a stray blank line in its place.
        const after = content[table.blockEnd] === '\n' ? table.blockEnd + 1 : table.blockEnd;
        const nextContent = content.slice(0, table.blockStart) + content.slice(after);
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
