/**
 * Markdown (GFM) table parsing, mutation, and serialization.
 *
 * Pure functions only — no Vue, no DOM. The editor components are thin
 * adapters over this module. A block only counts as a table if it has a
 * valid separator row; `findTableAt` returning non-null is the single
 * predicate for "the cursor is in a table", shared by the toolbar's
 * visibility and by the operations themselves so the two cannot disagree.
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
