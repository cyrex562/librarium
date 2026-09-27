/**
 * Syntax highlighting for Typst markup in the editor's Formatted mode.
 *
 * Like the Markdown highlighter (utils/highlight.ts) this feeds CodeJar, so
 * the returned HTML's textContent MUST equal the source exactly: every
 * character lands in exactly one text node, wrapped in at most one span.
 * It's a lexer, not a parser: it colors the constructs you see while
 * writing notes (headings, emphasis, lists, raw, math, `#` code, comments,
 * labels, references) and leaves everything else as plain text.
 */

export type TypstTokenKind =
    | 'heading'
    | 'strong'
    | 'emph'
    | 'raw'
    | 'math'
    | 'code'
    | 'keyword'
    | 'string'
    | 'comment'
    | 'label'
    | 'ref'
    | 'list-marker'
    | 'escape';

export interface TypstToken {
    kind: TypstTokenKind | null;
    text: string;
}

const KEYWORDS = new Set([
    'let', 'set', 'show', 'import', 'include', 'if', 'else', 'for', 'in',
    'while', 'break', 'continue', 'return', 'context', 'none', 'auto', 'true', 'false',
]);

const IDENT_START = /[\p{L}_]/u;
const IDENT_CHAR = /[\p{L}\p{N}_-]/u;

function escapeHtml(text: string): string {
    return text
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
}

/** Index of the first `close` at or after `from` (not escaped), or -1. */
function findClosing(src: string, from: number, close: string, stopAtNewline: boolean): number {
    for (let i = from; i < src.length; i += 1) {
        const c = src[i];
        if (c === '\\') { i += 1; continue; }
        if (stopAtNewline && c === '\n') return -1;
        if (src.startsWith(close, i)) return i;
    }
    return -1;
}

function isCodeToken(token: TypstToken | undefined): boolean {
    return token?.kind === 'code' || token?.kind === 'keyword';
}

export function tokenizeTypst(src: string): TypstToken[] {
    const tokens: TypstToken[] = [];
    let plain = '';
    const flush = () => {
        if (plain) { tokens.push({ kind: null, text: plain }); plain = ''; }
    };
    const push = (kind: TypstTokenKind, text: string) => {
        flush();
        tokens.push({ kind, text });
    };

    let i = 0;
    let atLineStart = true;
    let argDepth = 0;
    while (i < src.length) {
        const c = src[i];

        // Line-start constructs, after optional indentation.
        if (atLineStart) {
            const indent = /^[ \t]*/.exec(src.slice(i))![0];
            const rest = i + indent.length;
            const heading = /^=+[ \t]/.exec(src.slice(rest));
            if (heading) {
                plain += indent;
                const end = src.indexOf('\n', rest);
                push('heading', src.slice(rest, end === -1 ? src.length : end));
                i = end === -1 ? src.length : end;
                atLineStart = false;
                continue;
            }
            const marker = /^([-+]|\/)[ \t]/.exec(src.slice(rest));
            if (marker) {
                plain += indent;
                push('list-marker', marker[1]);
                i = rest + marker[1].length;
                atLineStart = false;
                continue;
            }
        }
        atLineStart = false;

        if (c === '\n') {
            plain += c;
            i += 1;
            atLineStart = true;
            continue;
        }

        // Comments.
        if (src.startsWith('//', i) && src[i - 1] !== ':') {
            const end = src.indexOf('\n', i);
            push('comment', src.slice(i, end === -1 ? src.length : end));
            i = end === -1 ? src.length : end;
            continue;
        }
        if (src.startsWith('/*', i)) {
            const end = src.indexOf('*/', i + 2);
            const stop = end === -1 ? src.length : end + 2;
            push('comment', src.slice(i, stop));
            i = stop;
            continue;
        }

        // Escapes: \* \_ \# \$ \\ … and a trailing backslash (line break).
        if (c === '\\') {
            push('escape', src.slice(i, i + 2));
            i += 2;
            continue;
        }

        // Raw: ```block``` (may span lines) or `inline`.
        if (c === '`') {
            const fence = /^`{3,}/.exec(src.slice(i));
            const close = fence ? fence[0] : '`';
            const end = src.indexOf(close, i + close.length);
            const stop = end === -1 ? src.length : end + close.length;
            push('raw', src.slice(i, stop));
            i = stop;
            continue;
        }

        // Math: $…$, may span lines.
        if (c === '$') {
            const end = findClosing(src, i + 1, '$', false);
            const stop = end === -1 ? src.length : end + 1;
            push('math', src.slice(i, stop));
            i = stop;
            continue;
        }

        // Code: #ident(.ident)* — keywords separately.
        if (c === '#' && i + 1 < src.length && IDENT_START.test(src[i + 1])) {
            let j = i + 1;
            while (j < src.length && (IDENT_CHAR.test(src[j]) || (src[j] === '.' && IDENT_START.test(src[j + 1] ?? '')))) j += 1;
            const word = src.slice(i + 1, j);
            push(KEYWORDS.has(word) ? 'keyword' : 'code', src.slice(i, j));
            i = j;
            continue;
        }

        // Arguments of a #call(…): track the parens so strings inside them
        // can be colored.
        if (c === '(' && (argDepth > 0 || (plain === '' && isCodeToken(tokens[tokens.length - 1])))) {
            argDepth += 1;
            plain += c;
            i += 1;
            continue;
        }
        if (c === ')' && argDepth > 0) {
            argDepth -= 1;
            plain += c;
            i += 1;
            continue;
        }
        if (c === '"' && argDepth > 0) {
            const end = findClosing(src, i + 1, '"', true);
            if (end !== -1) {
                push('string', src.slice(i, end + 1));
                i = end + 1;
                continue;
            }
        }

        // Labels <name> and references @name.
        if (c === '<') {
            const m = /^<[\p{L}\p{N}_:.-]+>/u.exec(src.slice(i));
            if (m) { push('label', m[0]); i += m[0].length; continue; }
        }
        if (c === '@' && IDENT_START.test(src[i + 1] ?? '') && !/[\p{L}\p{N}]/u.test(src[i - 1] ?? '')) {
            let j = i + 1;
            while (j < src.length && /[\p{L}\p{N}_:.-]/u.test(src[j])) j += 1;
            while (j > i + 1 && /[.:]/.test(src[j - 1])) j -= 1;
            push('ref', src.slice(i, j));
            i = j;
            continue;
        }

        // *strong* and _emphasis_ on one line.
        if (c === '*' || c === '_') {
            const wordBefore = /[\p{L}\p{N}]/u.test(src[i - 1] ?? '');
            if (!(c === '_' && wordBefore)) {
                const end = findClosing(src, i + 1, c, true);
                if (end > i + 1) {
                    push(c === '*' ? 'strong' : 'emph', src.slice(i, end + 1));
                    i = end + 1;
                    continue;
                }
            }
        }

        plain += c;
        i += 1;
    }
    flush();
    return tokens;
}

export function renderTypstHighlight(src: string): string {
    return tokenizeTypst(src)
        .map((t) => (t.kind ? `<span class="typ-${t.kind}">${escapeHtml(t.text)}</span>` : escapeHtml(t.text)))
        .join('');
}
