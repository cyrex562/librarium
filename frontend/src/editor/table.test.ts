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
        const t = deleteRow(base(), 0)!;
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
