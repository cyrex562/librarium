import { describe, it, expect } from 'vitest';
import { renderFormattedMarkdown } from './highlight';

function render(text: string, collapsed: Set<number> = new Set()): HTMLElement {
    const el = document.createElement('div');
    el.innerHTML = renderFormattedMarkdown(text, collapsed).html;
    return el;
}

const TABLE = [
    '| Name | Qty | Price |',
    '| :--- | :-: | ----: |',
    '| Apple | 3 | 1.50 |',
    '| Pear | 10 | 0.75 |',
].join('\n');

describe('renderFormattedMarkdown — textContent invariant', () => {
    // CodeJar reads the editor's textContent back as the document, and caret
    // offsets are character offsets into it. Any rendering that changes
    // textContent corrupts the note.
    const docs: Record<string, string> = {
        'plain table': TABLE,
        'table between paragraphs': `Intro text\n\n${TABLE}\n\nOutro text`,
        'table without edge pipes': 'a | b\n--- | ---\n1 | 2',
        'indented table': `  | a | b |\n  | --- | --- |\n  | 1 | 2 |`,
        'escaped pipe': '| a | b |\n| --- | --- |\n| x \\| y | z |',
        'table at end with trailing newline': `${TABLE}\n`,
        'table inside a code fence': '```\n| a | b |\n| --- | --- |\n```',
        'table under a heading': `# Title\n\n${TABLE}\n\n## Next`,
        'empty cells': '|     |     |\n| --- | --- |\n|     |     |',
    };

    for (const [name, doc] of Object.entries(docs)) {
        it(`preserves textContent: ${name}`, () => {
            expect(render(doc).textContent).toBe(doc);
        });
    }

    it('preserves textContent when a heading section containing a table is collapsed', () => {
        const doc = `# Title\n\n${TABLE}\n\n## Next`;
        expect(render(doc, new Set([0])).textContent).toBe(doc);
    });
});

describe('renderFormattedMarkdown — table grid', () => {
    it('groups a table into one grid block with one row per line', () => {
        const el = render(`Intro\n\n${TABLE}\n\nOutro`);
        const tables = el.querySelectorAll('.editor-md-table');
        expect(tables).toHaveLength(1);
        expect(tables[0].querySelectorAll(':scope > .editor-md-table-row')).toHaveLength(4);
    });

    it('marks the header row and the divider row', () => {
        const rows = render(TABLE).querySelectorAll('.editor-md-table-row');
        expect(rows[0].classList.contains('is-header')).toBe(true);
        expect(rows[1].classList.contains('is-divider')).toBe(true);
        expect(rows[2].classList.contains('is-header')).toBe(false);
    });

    it('splits rows into cells and wraps the pipes separately', () => {
        const rows = render(TABLE).querySelectorAll('.editor-md-table-row');
        const cells = rows[2].querySelectorAll('.editor-md-table-cell');
        expect([...cells].map((c) => c.textContent?.trim())).toEqual(['Apple', '3', '1.50']);
        expect(rows[2].querySelectorAll('.editor-md-table-pipe')).toHaveLength(4);
    });

    it('applies column alignment from the divider row', () => {
        const rows = render(TABLE).querySelectorAll('.editor-md-table-row');
        const cells = rows[2].querySelectorAll('.editor-md-table-cell');
        expect(cells[0].classList.contains('is-align-left')).toBe(true);
        expect(cells[1].classList.contains('is-align-center')).toBe(true);
        expect(cells[2].classList.contains('is-align-right')).toBe(true);
    });

    it('handles rows without leading/trailing pipes', () => {
        const rows = render('a | b\n--- | ---\n1 | 2').querySelectorAll('.editor-md-table-row');
        const cells = rows[2].querySelectorAll('.editor-md-table-cell');
        expect([...cells].map((c) => c.textContent?.trim())).toEqual(['1', '2']);
    });

    it('keeps an escaped pipe inside its cell', () => {
        const rows = render('| a | b |\n| --- | --- |\n| x \\| y | z |').querySelectorAll('.editor-md-table-row');
        const cells = rows[2].querySelectorAll('.editor-md-table-cell');
        expect([...cells].map((c) => c.textContent?.trim())).toEqual(['x \\| y', 'z']);
    });

    it('does not treat pipe-containing lines as a table without a divider row', () => {
        expect(render('a | b\nc | d').querySelector('.editor-md-table')).toBeNull();
    });

    it('does not render a table inside a code fence as a grid', () => {
        expect(render('```\n| a | b |\n| --- | --- |\n```').querySelector('.editor-md-table')).toBeNull();
    });

    it('still renders inline formatting inside cells', () => {
        const el = render('| a | b |\n| --- | --- |\n| **bold** | [[link]] |');
        expect(el.querySelector('.editor-md-table-cell .editor-md-strong')).not.toBeNull();
        expect(el.querySelector('.editor-md-table-cell .editor-md-wikilink')).not.toBeNull();
    });
});

describe('renderFormattedMarkdown — merged cells', () => {
    // `^^` merges a cell into the one above it; `<<` into the one to its
    // left. Markers stay in the text (textContent invariant) but are hidden,
    // and the anchor cell gets a grid span.
    const MERGED = [
        '| Region | Q1 | Q2 |',
        '| --- | --- | --- |',
        '| North | 10 | 12 |',
        '| ^^ | 11 | 13 |',
        '| Total | 44 | << |',
    ].join('\n');

    function cellsOf(el: HTMLElement, rowIndex: number) {
        return [...el.querySelectorAll('.editor-md-table-row')[rowIndex].querySelectorAll('.editor-md-table-cell')];
    }

    it('preserves textContent', () => {
        expect(render(MERGED).textContent).toBe(MERGED);
    });

    it('gives the anchor cell a row span and hides the ^^ cell', () => {
        const el = render(MERGED);
        const north = cellsOf(el, 2)[0] as HTMLElement;
        expect(north.style.gridRow).toBe('span 2');
        const marker = cellsOf(el, 3)[0];
        expect(marker.classList.contains('is-merged')).toBe(true);
    });

    it('gives the anchor cell a column span and hides the << cell', () => {
        const el = render(MERGED);
        const total44 = cellsOf(el, 4)[1] as HTMLElement;
        expect(total44.style.gridColumn).toBe('span 2');
        expect(cellsOf(el, 4)[2].classList.contains('is-merged')).toBe(true);
    });

    it('extends a row span across several ^^ cells', () => {
        const el = render('| a | b |\n| --- | --- |\n| x | 1 |\n| ^^ | 2 |\n| ^^ | 3 |');
        expect((cellsOf(el, 2)[0] as HTMLElement).style.gridRow).toBe('span 3');
    });

    it('extends a column span across several << cells', () => {
        const el = render('| a | b | c |\n| --- | --- | --- |\n| wide | << | << |');
        expect((cellsOf(el, 2)[0] as HTMLElement).style.gridColumn).toBe('span 3');
    });

    it('treats a marker with nothing to merge into as literal text', () => {
        // `^^` in the first body row (nothing above within the body), `<<` in the first column.
        const el = render('| a | b |\n| --- | --- |\n| ^^ | x |\n| << | y |');
        expect(el.querySelectorAll('.editor-md-table-cell.is-merged')).toHaveLength(0);
    });

    it('allows << merges in the header row', () => {
        const el = render('| Name | << |\n| --- | --- |\n| a | b |');
        expect((cellsOf(el, 0)[0] as HTMLElement).style.gridColumn).toBe('span 2');
        expect(cellsOf(el, 0)[1].classList.contains('is-merged')).toBe(true);
    });

    it('sets the column count on the table for the grid template', () => {
        const table = render(MERGED).querySelector('.editor-md-table') as HTMLElement;
        expect(table.style.getPropertyValue('--table-cols')).toBe('3');
    });
});
