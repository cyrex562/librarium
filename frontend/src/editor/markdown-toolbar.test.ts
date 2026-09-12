import { describe, it, expect } from 'vitest';
import { applyMarkdownToolbarCommand } from './markdown-toolbar';

describe('applyMarkdownToolbarCommand', () => {
    it('wraps selection with bold markers', () => {
        const res = applyMarkdownToolbarCommand('Hello world', 6, 11, 'bold');
        expect(res.content).toBe('Hello **world**');
    });

    it('inserts placeholder for italic when selection is empty', () => {
        const res = applyMarkdownToolbarCommand('Hello', 5, 5, 'italic');
        expect(res.content).toBe('Hello*italic text*');
    });

    it('applies heading and replaces existing heading markers', () => {
        const res = applyMarkdownToolbarCommand('## Old heading', 0, 0, 'heading_1');
        expect(res.content).toBe('# Old heading');
    });

    it('applying a heading strips leading indentation — headings always start at column 1', () => {
        const res = applyMarkdownToolbarCommand('  ## Old heading', 0, 0, 'heading_1');
        expect(res.content).toBe('# Old heading');
    });

    it('applying a heading strips leading indentation on a plain (non-heading) indented line', () => {
        const res = applyMarkdownToolbarCommand('   plain text', 0, 0, 'heading_2');
        expect(res.content).toBe('## plain text');
    });

    it('applies list markers to multiline selection', () => {
        const content = 'alpha\nbeta';
        const res = applyMarkdownToolbarCommand(content, 0, content.length, 'bulleted_list');
        expect(res.content).toBe('- alpha\n- beta');
    });

    it('applies numbered list to multiline selection', () => {
        const content = 'item one\nitem two';
        const res = applyMarkdownToolbarCommand(content, 0, content.length, 'numbered_list');
        expect(res.content).toBe('1. item one\n2. item two');
    });

    it('applies lower alpha ordered list style', () => {
        const content = 'item one\nitem two\nitem three';
        const res = applyMarkdownToolbarCommand(content, 0, content.length, 'numbered_list_lower_alpha');
        expect(res.content).toBe('a. item one\nb. item two\nc. item three');
    });

    it('applies upper roman ordered list style', () => {
        const content = 'first\nsecond\nthird';
        const res = applyMarkdownToolbarCommand(content, 0, content.length, 'numbered_list_upper_roman');
        expect(res.content).toBe('I. first\nII. second\nIII. third');
    });

    it('inserts markdown link wrapper', () => {
        const res = applyMarkdownToolbarCommand('Click', 0, 5, 'link');
        expect(res.content).toBe('[Click](https://)');
    });

    it('inserts code block with selected content', () => {
        const res = applyMarkdownToolbarCommand('const x = 1;', 0, 12, 'code_block');
        expect(res.content).toBe('\n```\nconst x = 1;\n```\n');
    });

    it('inserts an empty 3x3 table by default', () => {
        const res = applyMarkdownToolbarCommand('', 0, 0, 'table_create');
        expect(res.content).toBe(
            [
                '|     |     |     |',
                '| --- | --- | --- |',
                '|     |     |     |',
                '|     |     |     |',
                '|     |     |     |',
            ].join('\n'),
        );
    });

    it('inserts table on a new line when cursor is mid-text', () => {
        const res = applyMarkdownToolbarCommand('hello world', 5, 5, 'table_create');
        expect(res.content).toContain('\n|     |     |     |');
    });

    it('the indent/outdent commands delegate to applyLineIndent', () => {
        const indented = applyMarkdownToolbarCommand('hello', 0, 0, 'indent');
        expect(indented.content).toBe('  hello');

        const outdented = applyMarkdownToolbarCommand('  hello', 0, 0, 'outdent');
        expect(outdented.content).toBe('hello');
    });

    it('outdent is a harmless no-op (unchanged content) when there is nothing to dedent', () => {
        const res = applyMarkdownToolbarCommand('hello', 2, 2, 'outdent');
        expect(res.content).toBe('hello');
    });
});

describe('table commands', () => {
    const TABLE = '| a | b |\n| --- | --- |\n| 1 | 2 |';

    it('dispatches a row insert through the driver', () => {
        const at = TABLE.indexOf('1');
        const res = applyMarkdownToolbarCommand(TABLE, at, at, 'table_row_insert_below');
        expect(res.content.split('\n').length).toBe(4);
    });

    it('is a no-op when the cursor is not in a table', () => {
        const res = applyMarkdownToolbarCommand('prose', 2, 2, 'table_col_delete');
        expect(res.content).toBe('prose');
    });

    it('creates a table from a payload', () => {
        const res = applyMarkdownToolbarCommand('', 0, 0, 'table_create', { rows: 1, cols: 2 });
        expect(res.content).toBe('|     |     |\n| --- | --- |\n|     |     |');
    });

    it('defaults to 3x3 when table_create has no payload', () => {
        const res = applyMarkdownToolbarCommand('', 0, 0, 'table_create');
        expect(res.content.split('\n').length).toBe(5);
    });
});
