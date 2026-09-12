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
