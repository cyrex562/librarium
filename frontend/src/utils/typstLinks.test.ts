import { describe, it, expect } from 'vitest';
import { classifyTypstLink, resolveRelativeNotePath, typstLinks } from './typstLinks';

describe('typstLinks', () => {
    it('finds note links by name and path, and web links', () => {
        const src = [
            'See #link("librarium://note/Roadmap")[the roadmap] and #link("notes/other.typ")[other].',
            '#link("../up.md") #link("https://example.com")[site] #link("image.png") #link(<label>)[x]',
            '#link("librarium://note/Roadmap")[again] #link("say \\"hi\\".typ")',
        ].join('\n');
        expect(typstLinks(src)).toEqual([
            { target: 'Roadmap', label: 'the roadmap', kind: 'note-name' },
            { target: 'notes/other.typ', label: 'other', kind: 'note-path' },
            { target: '../up.md', label: '../up.md', kind: 'note-path' },
            { target: 'https://example.com', label: 'site', kind: 'external' },
            { target: 'say "hi".typ', label: 'say "hi".typ', kind: 'note-path' },
        ]);
    });

    it('classifies URLs', () => {
        expect(classifyTypstLink('librarium://note/X')).toBe('note-name');
        expect(classifyTypstLink('librarium://note/')).toBeNull();
        expect(classifyTypstLink('a/b.typ#heading')).toBe('note-path');
        expect(classifyTypstLink('mailto:a@b.c')).toBe('external');
        expect(classifyTypstLink('javascript:alert(1)')).toBeNull();
        expect(classifyTypstLink('pic.png')).toBeNull();
    });

    it('resolves note-relative paths', () => {
        expect(resolveRelativeNotePath('papers/main.typ', 'draft.typ')).toBe('papers/draft.typ');
        expect(resolveRelativeNotePath('papers/sub/main.typ', '../notes/a.md')).toBe('papers/notes/a.md');
        expect(resolveRelativeNotePath('main.typ', './b.typ#sec')).toBe('b.typ');
        expect(resolveRelativeNotePath('a/main.typ', '/root.typ')).toBe('root.typ');
    });
});
