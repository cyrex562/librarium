import { applyLineIndent } from './line-indent';
import { applyTableCommand, insertTableAt, type TableCommand } from './table';

export type MarkdownToolbarCommand =
    | 'bold'
    | 'italic'
    | 'strikethrough'
    | 'inline_code'
    | 'highlight'
    | 'heading_1'
    | 'heading_2'
    | 'heading_3'
    | 'blockquote'
    | 'bulleted_list'
    | 'numbered_list'
    | 'numbered_list_lower_alpha'
    | 'numbered_list_upper_alpha'
    | 'numbered_list_lower_roman'
    | 'numbered_list_upper_roman'
    | 'task_list'
    | 'link'
    | 'image'
    | 'code_block'
    | 'horizontal_rule'
    | 'table_create'
    | TableCommand
    | 'indent'
    | 'outdent'
    | 'extract_to_note';

export interface MarkdownCommandResult {
    content: string;
    selectionStart: number;
    selectionEnd: number;
}

function normalizeSelection(content: string, start: number, end: number): { start: number; end: number } {
    const safeStart = Math.max(0, Math.min(start, content.length));
    const safeEnd = Math.max(0, Math.min(end, content.length));
    return safeStart <= safeEnd
        ? { start: safeStart, end: safeEnd }
        : { start: safeEnd, end: safeStart };
}

function wrapInline(
    content: string,
    start: number,
    end: number,
    prefix: string,
    suffix: string,
    placeholder: string,
): MarkdownCommandResult {
    const selected = content.slice(start, end);
    const body = selected.length > 0 ? selected : placeholder;
    const replacement = `${prefix}${body}${suffix}`;
    const nextContent = `${content.slice(0, start)}${replacement}${content.slice(end)}`;

    if (selected.length > 0) {
        return {
            content: nextContent,
            selectionStart: start + prefix.length,
            selectionEnd: start + prefix.length + body.length,
        };
    }

    return {
        content: nextContent,
        selectionStart: start + prefix.length,
        selectionEnd: start + prefix.length + placeholder.length,
    };
}

function applyLinePrefix(
    content: string,
    start: number,
    end: number,
    prefix: string,
): MarkdownCommandResult {
    const lineStart = content.lastIndexOf('\n', Math.max(0, start - 1)) + 1;
    const lineEndCandidate = content.indexOf('\n', end);
    const lineEnd = lineEndCandidate === -1 ? content.length : lineEndCandidate;

    const block = content.slice(lineStart, lineEnd);
    const lines = block.split('\n');
    const updatedLines = lines.map((line) => `${prefix}${line}`);
    const replacement = updatedLines.join('\n');

    const nextContent = `${content.slice(0, lineStart)}${replacement}${content.slice(lineEnd)}`;

    return {
        content: nextContent,
        selectionStart: start + prefix.length,
        selectionEnd: end + prefix.length * lines.length,
    };
}

type OrderedListStyle = 'decimal' | 'lower_alpha' | 'upper_alpha' | 'lower_roman' | 'upper_roman';

function toRoman(n: number): string {
    if (n <= 0) return 'i';
    const map: Array<[number, string]> = [
        [1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'],
        [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'],
        [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I'],
    ];
    let value = n;
    let out = '';
    for (const [unit, symbol] of map) {
        while (value >= unit) {
            out += symbol;
            value -= unit;
        }
    }
    return out.toLowerCase();
}

function ordinalMarker(index: number, style: OrderedListStyle): string {
    const n = index + 1;
    switch (style) {
        case 'lower_alpha':
            return `${String.fromCharCode(97 + ((n - 1) % 26))}.`;
        case 'upper_alpha':
            return `${String.fromCharCode(65 + ((n - 1) % 26))}.`;
        case 'lower_roman':
            return `${toRoman(n)}.`;
        case 'upper_roman':
            return `${toRoman(n).toUpperCase()}.`;
        case 'decimal':
        default:
            return `${n}.`;
    }
}

function applyNumberedList(content: string, start: number, end: number, style: OrderedListStyle = 'decimal'): MarkdownCommandResult {
    const lineStart = content.lastIndexOf('\n', Math.max(0, start - 1)) + 1;
    const lineEndCandidate = content.indexOf('\n', end);
    const lineEnd = lineEndCandidate === -1 ? content.length : lineEndCandidate;

    const block = content.slice(lineStart, lineEnd);
    const lines = block.split('\n');
    const updatedLines = lines.map((line, idx) => `${ordinalMarker(idx, style)} ${line}`);
    const replacement = updatedLines.join('\n');

    const nextContent = `${content.slice(0, lineStart)}${replacement}${content.slice(lineEnd)}`;
    const extraChars = updatedLines.reduce((sum, line, idx) => {
        const originalLength = lines[idx].length;
        return sum + (line.length - originalLength);
    }, 0);

    return {
        content: nextContent,
        selectionStart: start + 3,
        selectionEnd: end + extraChars,
    };
}

function applyHeading(content: string, start: number, level: 1 | 2 | 3): MarkdownCommandResult {
    const lineStart = content.lastIndexOf('\n', Math.max(0, start - 1)) + 1;
    const lineEndCandidate = content.indexOf('\n', start);
    const lineEnd = lineEndCandidate === -1 ? content.length : lineEndCandidate;
    const line = content.slice(lineStart, lineEnd);
    // Headings always start at column 1 — strip both any existing heading
    // marker and any leading indentation (matches the auto-dedent applied
    // when typing "#"/"##"/... followed by a space).
    const stripped = line.replace(/^\s*#{1,6}\s+/, '').replace(/^\s+/, '');
    const prefix = '#'.repeat(level) + ' ';
    const replacement = `${prefix}${stripped}`;
    const nextContent = `${content.slice(0, lineStart)}${replacement}${content.slice(lineEnd)}`;

    return {
        content: nextContent,
        selectionStart: lineStart + prefix.length,
        selectionEnd: lineStart + replacement.length,
    };
}

function insertAtCursor(content: string, start: number, end: number, insertion: string): MarkdownCommandResult {
    const nextContent = `${content.slice(0, start)}${insertion}${content.slice(end)}`;
    const cursor = start + insertion.length;
    return { content: nextContent, selectionStart: cursor, selectionEnd: cursor };
}

export interface TableCreatePayload {
    rows: number;
    cols: number;
}

export function applyMarkdownToolbarCommand(
    content: string,
    selectionStart: number,
    selectionEnd: number,
    command: MarkdownToolbarCommand,
    payload?: TableCreatePayload,
): MarkdownCommandResult {
    const { start, end } = normalizeSelection(content, selectionStart, selectionEnd);

    switch (command) {
        case 'bold':
            return wrapInline(content, start, end, '**', '**', 'bold text');
        case 'italic':
            return wrapInline(content, start, end, '*', '*', 'italic text');
        case 'strikethrough':
            return wrapInline(content, start, end, '~~', '~~', 'strikethrough');
        case 'inline_code':
            return wrapInline(content, start, end, '`', '`', 'code');
        case 'highlight':
            return wrapInline(content, start, end, '==', '==', 'highlight');
        case 'heading_1':
            return applyHeading(content, start, 1);
        case 'heading_2':
            return applyHeading(content, start, 2);
        case 'heading_3':
            return applyHeading(content, start, 3);
        case 'blockquote':
            return applyLinePrefix(content, start, end, '> ');
        case 'bulleted_list':
            return applyLinePrefix(content, start, end, '- ');
        case 'numbered_list':
            return applyNumberedList(content, start, end);
        case 'numbered_list_lower_alpha':
            return applyNumberedList(content, start, end, 'lower_alpha');
        case 'numbered_list_upper_alpha':
            return applyNumberedList(content, start, end, 'upper_alpha');
        case 'numbered_list_lower_roman':
            return applyNumberedList(content, start, end, 'lower_roman');
        case 'numbered_list_upper_roman':
            return applyNumberedList(content, start, end, 'upper_roman');
        case 'task_list':
            return applyLinePrefix(content, start, end, '- [ ] ');
        case 'link':
            return wrapInline(content, start, end, '[', '](https://)', 'link text');
        case 'image':
            return insertAtCursor(content, start, end, '![alt text](image-path.png)');
        case 'code_block': {
            const selected = content.slice(start, end);
            const body = selected.length > 0 ? selected : 'code here';
            const replacement = `\n\`\`\`\n${body}\n\`\`\`\n`;
            const nextContent = `${content.slice(0, start)}${replacement}${content.slice(end)}`;
            const bodyStart = start + 5;
            return {
                content: nextContent,
                selectionStart: bodyStart,
                selectionEnd: bodyStart + body.length,
            };
        }
        case 'horizontal_rule':
            return insertAtCursor(content, start, end, '\n---\n');
        case 'table_create':
            return insertTableAt(content, start, end, payload?.rows ?? 3, payload?.cols ?? 3);
        case 'indent': {
            const result = applyLineIndent(content, start, end, 'indent');
            if (!result) return { content, selectionStart: start, selectionEnd: end };
            return result;
        }
        case 'outdent': {
            const result = applyLineIndent(content, start, end, 'outdent');
            if (!result) return { content, selectionStart: start, selectionEnd: end };
            return result;
        }
        case 'table_col_insert_before':
        case 'table_col_insert_after':
        case 'table_col_delete':
        case 'table_col_move_left':
        case 'table_col_move_right':
        case 'table_row_insert_above':
        case 'table_row_insert_below':
        case 'table_row_delete':
        case 'table_row_move_up':
        case 'table_row_move_down':
        case 'table_delete':
        case 'table_align_none':
        case 'table_align_left':
        case 'table_align_center':
        case 'table_align_right':
            return (
                applyTableCommand(content, start, command) ?? {
                    content,
                    selectionStart: start,
                    selectionEnd: end,
                }
            );
        default:
            return { content, selectionStart: start, selectionEnd: end };
    }
}
