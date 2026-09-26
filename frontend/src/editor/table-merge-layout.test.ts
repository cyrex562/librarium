import { describe, it, expect } from 'vitest';
import { computeMergeLayout, applyTableMerges } from './table-merge-layout';

describe('computeMergeLayout', () => {
    it('spans rows for ^^ and columns for <<', () => {
        const layout = computeMergeLayout([
            ['Region', 'Q1', 'Q2'],
            ['North', '10', '12'],
            ['^^', '11', '13'],
            ['Total', '44', '<<'],
        ]);
        expect(layout[1][0]).toEqual({ merged: false, colSpan: 1, rowSpan: 2 });
        expect(layout[2][0].merged).toBe(true);
        expect(layout[3][1]).toEqual({ merged: false, colSpan: 2, rowSpan: 1 });
        expect(layout[3][2].merged).toBe(true);
    });

    it('leaves markers with nothing to merge into as literal', () => {
        const layout = computeMergeLayout([['a', 'b'], ['^^', 'x'], ['<<', 'y']]);
        expect(layout.flat().some((l) => l.merged)).toBe(false);
    });

    it('never merges a body cell into the header', () => {
        const layout = computeMergeLayout([['Head', 'b'], ['^^', 'x']]);
        expect(layout[1][0].merged).toBe(false);
        expect(layout[0][0].rowSpan).toBe(1);
    });
});

describe('applyTableMerges (Preview)', () => {
    function table(html: string) {
        const root = document.createElement('div');
        root.innerHTML = html;
        applyTableMerges(root);
        return root;
    }

    const RENDERED = `<table><thead><tr><th>Region</th><th>Q1</th><th>Q2</th></tr></thead><tbody>
<tr><td>North</td><td>10</td><td>12</td></tr>
<tr><td>^^</td><td>11</td><td>13</td></tr>
<tr><td>Total</td><td>44</td><td>&lt;&lt;</td></tr>
</tbody></table>`;

    it('sets spans and removes merged cells', () => {
        const root = table(RENDERED);
        const rows = root.querySelectorAll('tbody tr');
        expect((rows[0].children[0] as HTMLTableCellElement).rowSpan).toBe(2);
        expect(rows[1].children).toHaveLength(2); // ^^ removed
        expect(rows[1].textContent).not.toContain('^^');
        expect((rows[2].children[1] as HTMLTableCellElement).colSpan).toBe(2);
        expect(rows[2].children).toHaveLength(2); // << removed
    });

    it('is idempotent', () => {
        const root = table(RENDERED);
        const once = root.innerHTML;
        applyTableMerges(root);
        expect(root.innerHTML).toBe(once);
    });

    it('supports << in the header row', () => {
        const root = table('<table><thead><tr><th>Name</th><th>&lt;&lt;</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody></table>');
        expect((root.querySelector('th') as HTMLTableCellElement).colSpan).toBe(2);
        expect(root.querySelectorAll('th')).toHaveLength(1);
    });
});
