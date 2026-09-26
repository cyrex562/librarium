/**
 * Merged table cells, shared by Formatted mode (utils/highlight.ts) and
 * Preview (MarkdownPreview.vue) so both apply the same rules.
 *
 * A cell whose entire (trimmed) content is `^^` merges into the cell above it
 * — body rows only, so never into the header. `<<` merges into the cell to its
 * left, in any row. A marker with nothing to merge into is ordinary text.
 * The markers are plain cell content, so they survive editor/table.ts
 * re-serializing a table on every edit.
 */

export const MERGE_UP = '^^';
export const MERGE_LEFT = '<<';

export interface CellLayout {
    /** Hidden: this cell has been merged into another. */
    merged: boolean;
    colSpan: number;
    rowSpan: number;
}

/**
 * Layout for a table given its cell texts, row by row. Row 0 is the header;
 * the markdown divider row must NOT be included. Returns a matrix of the same
 * shape as `rows`.
 */
export function computeMergeLayout(rows: string[][]): CellLayout[][] {
    const layout = rows.map((cells) => cells.map(() => ({ merged: false, colSpan: 1, rowSpan: 1 })));

    // Column spans, left to right within each row.
    rows.forEach((cells, r) => {
        let anchor: number | null = null;
        cells.forEach((text, c) => {
            if (text.trim() === MERGE_LEFT && anchor !== null) {
                layout[r][c].merged = true;
                layout[r][anchor].colSpan += 1;
            } else {
                anchor = c;
            }
        });
    });

    // Row spans, top to bottom within each column, body rows (r >= 1) only.
    const colCount = Math.max(0, ...rows.map((cells) => cells.length));
    for (let c = 0; c < colCount; c += 1) {
        let anchor: number | null = null;
        for (let r = 1; r < rows.length; r += 1) {
            const text = rows[r][c];
            if (text === undefined) { anchor = null; continue; }
            if (text.trim() === MERGE_UP && anchor !== null) {
                layout[r][c].merged = true;
                layout[anchor][c].rowSpan += 1;
            } else {
                anchor = r;
            }
        }
    }
    return layout;
}

/**
 * Apply merges to rendered HTML tables (Preview mode): set rowSpan/colSpan on
 * anchor cells and remove merged cells. The server renderer (pulldown-cmark)
 * has no notion of spans, so it emits the markers as ordinary cell text.
 */
export function applyTableMerges(root: ParentNode) {
    root.querySelectorAll('table').forEach((table) => {
        const trs = [...table.querySelectorAll(':scope > thead > tr, :scope > tbody > tr, :scope > tr')];
        const cells = trs.map((tr) => [...tr.querySelectorAll(':scope > th, :scope > td')] as HTMLTableCellElement[]);
        // Already processed (or authored with spans): leave it alone.
        if (cells.some((row) => row.some((cell) => cell.colSpan > 1 || cell.rowSpan > 1))) return;

        const layout = computeMergeLayout(cells.map((row) => row.map((cell) => cell.textContent ?? '')));
        cells.forEach((row, r) => row.forEach((cell, c) => {
            const l = layout[r][c];
            if (l.colSpan > 1) cell.colSpan = l.colSpan;
            if (l.rowSpan > 1) cell.rowSpan = l.rowSpan;
        }));
        cells.forEach((row, r) => row.forEach((cell, c) => {
            if (layout[r][c].merged) cell.remove();
        }));
    });
}
