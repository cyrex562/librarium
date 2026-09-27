import { describe, it, expect } from 'vitest';
import { tokenizeTypst, renderTypstHighlight, type TypstTokenKind } from './typst-highlight';

function kinds(src: string): Array<[TypstTokenKind, string]> {
    return tokenizeTypst(src)
        .filter((t) => t.kind !== null)
        .map((t) => [t.kind!, t.text]);
}

function textOf(html: string): string {
    const el = document.createElement('div');
    el.innerHTML = html;
    return el.textContent ?? '';
}

const SAMPLE = `#metadata((title: "Meeting notes")) <frontmatter>
#set page(margin: 2cm)

= Meeting notes
== Details

Discussed the *release plan* and _Typst_ support, see @roadmap.
Escaped \\* star, snake_case_name, email me@example.com, https://example.com.

- Ship the upgrader
  + measure build time
/ Term: definition

// a line comment
/* a block
   comment */
$ sum_(i=1)^n i = (n(n+1))/2 $ and inline $x^2$.

\`\`\`rust
fn main() { println!("*not strong*"); }
\`\`\`

#link("https://example.com")[a link] and \`inline raw\`.
<ends-with-label>
`;

describe('Typst highlighting', () => {
    it('never changes the text (CodeJar caret offsets depend on it)', () => {
        for (const src of [SAMPLE, '', '*', '_', '$', '`', '#', '\\', '/*', '<', '@', '= ', '#f(', '"', 'a\n\n= b\n']) {
            expect(textOf(renderTypstHighlight(src))).toBe(src);
        }
    });

    it('colors headings, lists and emphasis', () => {
        const k = kinds(SAMPLE);
        expect(k).toContainEqual(['heading', '= Meeting notes']);
        expect(k).toContainEqual(['heading', '== Details']);
        expect(k).toContainEqual(['strong', '*release plan*']);
        expect(k).toContainEqual(['emph', '_Typst_']);
        expect(k).toContainEqual(['list-marker', '-']);
        expect(k).toContainEqual(['list-marker', '+']);
        expect(k).toContainEqual(['list-marker', '/']);
    });

    it('colors code, keywords, strings, labels and references', () => {
        const k = kinds(SAMPLE);
        expect(k).toContainEqual(['code', '#metadata']);
        expect(k).toContainEqual(['keyword', '#set']);
        expect(k).toContainEqual(['string', '"Meeting notes"']);
        expect(k).toContainEqual(['string', '"https://example.com"']);
        expect(k).toContainEqual(['label', '<frontmatter>']);
        expect(k).toContainEqual(['ref', '@roadmap']);
    });

    it('colors comments, math and raw, and nothing inside them', () => {
        const k = kinds(SAMPLE);
        expect(k).toContainEqual(['comment', '// a line comment']);
        expect(k).toContainEqual(['comment', '/* a block\n   comment */']);
        expect(k).toContainEqual(['math', '$ sum_(i=1)^n i = (n(n+1))/2 $']);
        expect(k).toContainEqual(['math', '$x^2$']);
        expect(k).toContainEqual(['raw', '```rust\nfn main() { println!("*not strong*"); }\n```']);
        expect(k).toContainEqual(['raw', '`inline raw`']);
        expect(k.some(([kind, text]) => kind === 'strong' && text.includes('not strong'))).toBe(false);
    });

    it('leaves look-alikes in prose alone', () => {
        const k = kinds(SAMPLE);
        expect(k).toContainEqual(['escape', '\\*']);
        // snake_case, an email address and a URL are not emphasis/ref/comment.
        expect(k.some(([, text]) => text.includes('case'))).toBe(false);
        expect(k).not.toContainEqual(['ref', '@example.com']);
        expect(k.some(([kind, text]) => kind === 'comment' && text.includes('example.com'))).toBe(false);
        // A heading marker needs a following space.
        expect(kinds('=not a heading')).toEqual([]);
    });

    it('escapes HTML', () => {
        expect(renderTypstHighlight('<b> & "x"')).not.toContain('<b>');
    });
});
