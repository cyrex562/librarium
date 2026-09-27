/**
 * Character-offset selection helpers for the CodeJar editors (Markdown and
 * Typst). Offsets count characters of the editor's textContent, which the
 * highlighters guarantee equals the source text.
 */

/**
 * The selection as [start, end) offsets into `root`'s text. Falls back to
 * `fallback` (typically the end of the document) when there is no selection
 * inside `root`.
 */
export function getSelectionOffsets(root: HTMLElement, fallback: number): { start: number; end: number } {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) {
        return { start: fallback, end: fallback };
    }

    const range = selection.getRangeAt(0);
    if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) {
        return { start: fallback, end: fallback };
    }

    const preStart = range.cloneRange();
    preStart.selectNodeContents(root);
    preStart.setEnd(range.startContainer, range.startOffset);

    const preEnd = range.cloneRange();
    preEnd.selectNodeContents(root);
    preEnd.setEnd(range.endContainer, range.endOffset);

    return {
        start: preStart.toString().length,
        end: preEnd.toString().length,
    };
}

export function setSelectionOffsets(root: HTMLElement, start: number, end: number) {
    const selection = window.getSelection();
    if (!selection) return;

    // Clamp to the actual available text length first. An out-of-range offset
    // (content shifted slightly between measuring and restoring — e.g. a
    // stale offset from before an external update) used to fall all the way
    // through to the root.focus() fallback below, which drops the caret at
    // the very start of the document — this is the "cursor randomly jumps to
    // the top" bug. Clamping means that fallback is now only ever reached for
    // a genuinely empty document, where focus-at-start is correct.
    const totalLength = root.textContent?.length ?? 0;
    const clampedStart = Math.max(0, Math.min(start, totalLength));
    const clampedEnd = Math.max(clampedStart, Math.min(end, totalLength));

    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let currentOffset = 0;
    let startNode: Text | null = null;
    let endNode: Text | null = null;
    let startNodeOffset = 0;
    let endNodeOffset = 0;

    while (walker.nextNode()) {
        const node = walker.currentNode as Text;
        const nextOffset = currentOffset + node.textContent!.length;

        if (!startNode && clampedStart <= nextOffset) {
            startNode = node;
            startNodeOffset = Math.max(0, clampedStart - currentOffset);
        }

        if (!endNode && clampedEnd <= nextOffset) {
            endNode = node;
            endNodeOffset = Math.max(0, clampedEnd - currentOffset);
            break;
        }

        currentOffset = nextOffset;
    }

    if (!startNode || !endNode) {
        root.focus();
        return;
    }

    const range = document.createRange();
    range.setStart(startNode, Math.min(startNodeOffset, startNode.length));
    range.setEnd(endNode, Math.min(endNodeOffset, endNode.length));
    selection.removeAllRanges();
    selection.addRange(range);
}
