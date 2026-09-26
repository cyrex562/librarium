import { describe, it, expect } from 'vitest';
import { renderFormattedMarkdown } from '@/utils/highlight';
import { visibleTableCaretPosition } from './table-caret';

function setup(text: string) {
    const root = document.createElement('div');
    root.innerHTML = renderFormattedMarkdown(text).html;
    document.body.appendChild(root);
    const texts: Text[] = [];
    const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let n: Node | null;
    while ((n = w.nextNode())) texts.push(n as Text);
    return { root, texts };
}

/** Document character offset of (node, offset), counting all text nodes. */
function docOffset(texts: Text[], node: Node, offset: number): number {
    let total = 0;
    for (const t of texts) {
        if (t === node) return total + offset;
        total += t.length;
    }
    throw new Error('node not found');
}

const TABLE = '| a | b |\n| --- | --- |\n| 1 | 2 |';

describe('visibleTableCaretPosition', () => {
    it('leaves a caret in visible cell text alone', () => {
        const { root, texts } = setup(TABLE);
        const cellText = texts.find((t) => t.parentElement?.classList.contains('editor-md-table-cell'))!;
        expect(visibleTableCaretPosition(root, cellText, 1)).toBeNull();
    });

    it('moves a caret at the start of a hidden pipe to the end of the preceding cell, same document offset', () => {
        const { root, texts } = setup(TABLE);
        // The pipe after " a " in the header row.
        const pipe = texts.find((t, i) => t.parentElement?.classList.contains('editor-md-table-pipe') && texts[i - 1]?.nodeValue === ' a ')!;
        const moved = visibleTableCaretPosition(root, pipe, 0)!;
        expect(moved.node.nodeValue).toBe(' a ');
        expect(moved.offset).toBe(3);
        expect(docOffset(texts, moved.node, moved.offset)).toBe(docOffset(texts, pipe, 0));
    });

    it('moves a caret in a leading pipe forward into the first cell', () => {
        const { root, texts } = setup(TABLE);
        const leading = texts.find((t) => t.parentElement?.classList.contains('editor-md-table-pipe'))!;
        const moved = visibleTableCaretPosition(root, leading, 0)!;
        expect(moved.node.nodeValue).toBe(' a ');
        expect(moved.offset).toBe(0);
    });

    it('moves a caret in a row newline back into that row', () => {
        const { root, texts } = setup(TABLE);
        const eol = texts.find((t) => t.parentElement?.classList.contains('editor-md-table-eol'))!;
        const moved = visibleTableCaretPosition(root, eol, 1)!;
        expect(moved.node.parentElement?.closest('.editor-md-table-row')).toBe(eol.parentElement?.closest('.editor-md-table-row'));
        expect(moved.node.parentElement?.classList.contains('editor-md-table-cell')).toBe(true);
    });

    it('ignores carets outside tables', () => {
        const { root, texts } = setup(`Hello\n\n${TABLE}`);
        expect(visibleTableCaretPosition(root, texts[0], 2)).toBeNull();
    });
});
