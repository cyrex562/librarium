/**
 * Keep the caret out of the hidden parts of a formatted-mode table grid.
 *
 * In formatted mode a table's pipes, row newlines and divider row are still in
 * the DOM (so the editor's textContent stays identical to the markdown) but
 * hidden with display: none. CodeJar restores the caret by character offset,
 * and at a node boundary it picks the *following* text node — so a caret at
 * the end of a cell lands at offset 0 of the hidden pipe after it. The
 * browser can't show or reliably type into a display: none node, so move such
 * a caret to the nearest visible text in the same row: the end of the
 * immediately preceding text node when that's visible (same character offset,
 * so the document position doesn't change), otherwise the nearest visible
 * text in the row.
 */

const HIDDEN_SELECTOR = '.editor-md-table-pipe, .editor-md-table-eol, .editor-md-table-row.is-divider';

function isHiddenTableText(node: Node): boolean {
    const el = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as Element);
    return !!el?.closest(HIDDEN_SELECTOR);
}

function textNodesIn(root: Node): Text[] {
    const nodes: Text[] = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let n: Node | null;
    while ((n = walker.nextNode())) nodes.push(n as Text);
    return nodes;
}

export interface CaretPosition {
    node: Node;
    offset: number;
}

/**
 * Where a caret at (node, offset) should go, or null if it's already fine.
 * `root` is the editor element.
 */
export function visibleTableCaretPosition(root: HTMLElement, node: Node, offset: number): CaretPosition | null {
    if (node.nodeType !== Node.TEXT_NODE || !root.contains(node) || !isHiddenTableText(node)) return null;
    const row = node.parentElement?.closest('.editor-md-table-row');
    if (!row) return null;

    const all = textNodesIn(root);
    const idx = all.indexOf(node as Text);

    // Same document offset: end of the immediately preceding text node.
    if (offset === 0 && idx > 0) {
        const prev = all[idx - 1];
        if (row.contains(prev) && !isHiddenTableText(prev)) {
            return { node: prev, offset: prev.length };
        }
    }

    // Otherwise the nearest visible text in this row: forward first, then back.
    const rowNodes = textNodesIn(row).filter((t) => !isHiddenTableText(t));
    const after = rowNodes.find((t) => all.indexOf(t) > idx);
    if (after) return { node: after, offset: 0 };
    const before = [...rowNodes].reverse().find((t) => all.indexOf(t) < idx);
    if (before) return { node: before, offset: before.length };
    return null;
}
