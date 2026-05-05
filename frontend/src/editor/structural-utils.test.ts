import { describe, it, expect } from 'vitest';
import {
    parseFrontmatter,
    serializeFrontmatter,
    extractProseZone,
    replaceProseZone,
} from './structural-utils';

// ── parseFrontmatter ──────────────────────────────────────────────────────────

describe('parseFrontmatter', () => {
    it('returns empty object when no frontmatter', () => {
        expect(parseFrontmatter('# Just a heading\n\nNo frontmatter.')).toEqual({});
    });

    it('returns empty object for empty string', () => {
        expect(parseFrontmatter('')).toEqual({});
    });

    it('parses a simple frontmatter block', () => {
        const content = '---\nlibrarium_type: character\nfull_name: Alice\n---\n# Alice\n';
        const fm = parseFrontmatter(content);
        expect(fm.librarium_type).toBe('character');
        expect(fm.full_name).toBe('Alice');
    });

    it('parses arrays in frontmatter', () => {
        const content = '---\nlibrarium_labels:\n  - graphable\n  - person\n---\n# Content\n';
        const fm = parseFrontmatter(content) as { librarium_labels: string[] };
        expect(fm.librarium_labels).toEqual(['graphable', 'person']);
    });

    it('returns empty object when closing --- is missing', () => {
        const content = '---\nlibrarium_type: character\n# Missing closing delimiter';
        expect(parseFrontmatter(content)).toEqual({});
    });

    it('throws on invalid YAML', () => {
        const content = '---\n: invalid: yaml: here\n---\n# Content';
        expect(() => parseFrontmatter(content)).toThrow();
    });

    it('handles numeric and boolean values', () => {
        const content = '---\nage: 42\nactive: true\n---\n';
        const fm = parseFrontmatter(content);
        expect(fm.age).toBe(42);
        expect(fm.active).toBe(true);
    });

    it('handles nested objects', () => {
        const content = '---\nmeta:\n  created: 2024-01-01\n  author: Bob\n---\n';
        const fm = parseFrontmatter(content) as { meta: Record<string, string> };
        expect(fm.meta.author).toBe('Bob');
    });
});

// ── serializeFrontmatter ──────────────────────────────────────────────────────

describe('serializeFrontmatter', () => {
    it('replaces existing frontmatter while keeping body', () => {
        const original = '---\nlibrarium_type: character\n---\n# Alice\n\nSome prose.';
        const fm = { librarium_type: 'character', full_name: 'Alice Smith' };
        const result = serializeFrontmatter(fm, original);
        expect(result).toContain('full_name: Alice Smith');
        expect(result).toContain('# Alice');
        expect(result).toContain('Some prose.');
    });

    it('prepends frontmatter when no existing block', () => {
        const original = '# Just a heading\n\nBody text.';
        const fm = { librarium_type: 'location' };
        const result = serializeFrontmatter(fm, original);
        expect(result.startsWith('---\n')).toBe(true);
        expect(result).toContain('librarium_type: location');
        expect(result).toContain('# Just a heading');
    });

    it('round-trips frontmatter', () => {
        const content = '---\nlibrarium_type: character\nfull_name: Alice\n---\n# Alice\n';
        const fm = parseFrontmatter(content);
        const reserialized = serializeFrontmatter(fm, content);
        const reparsed = parseFrontmatter(reserialized);
        expect(reparsed.librarium_type).toBe('character');
        expect(reparsed.full_name).toBe('Alice');
    });

    it('produces valid --- delimited output', () => {
        const fm = { librarium_type: 'faction', name: 'The Guild' };
        const result = serializeFrontmatter(fm, '');
        expect(result.startsWith('---\n')).toBe(true);
        expect(result).toContain('\n---\n');
    });
});

// ── extractProseZone ──────────────────────────────────────────────────────────

const PROSE_BEGIN = '<!-- librarium:prose:begin -->';
const PROSE_END = '<!-- librarium:prose:end -->';

describe('extractProseZone', () => {
    it('extracts content between prose sentinels', () => {
        const content = `---\nlibrarium_type: character\n---\n${PROSE_BEGIN}\nThis is the prose.\n${PROSE_END}\n`;
        expect(extractProseZone(content)).toBe('This is the prose.');
    });

    it('returns body when no sentinels present', () => {
        const content = '---\nlibrarium_type: character\n---\n# Alice\n\nThe body.';
        const prose = extractProseZone(content);
        expect(prose).toContain('Alice');
        expect(prose).toContain('body');
    });

    it('returns trimmed content between sentinels', () => {
        const content = `---\nfoo: bar\n---\n${PROSE_BEGIN}\n\n   Trimmed.\n\n${PROSE_END}\n`;
        expect(extractProseZone(content)).toBe('Trimmed.');
    });

    it('returns empty string for file with only frontmatter and empty body', () => {
        const content = '---\nlibrarium_type: character\n---\n';
        const prose = extractProseZone(content);
        expect(prose).toBe('');
    });

    it('handles multiline prose correctly', () => {
        const lines = 'Line one.\n\nLine two.\n\nLine three.';
        const content = `---\nfoo: bar\n---\n${PROSE_BEGIN}\n${lines}\n${PROSE_END}\n`;
        expect(extractProseZone(content)).toBe(lines);
    });
});

// ── replaceProseZone ──────────────────────────────────────────────────────────

describe('replaceProseZone', () => {
    it('replaces text between existing sentinels', () => {
        const original = `---\nfoo: bar\n---\n${PROSE_BEGIN}\nOld prose.\n${PROSE_END}\n`;
        const result = replaceProseZone(original, 'New prose.');
        expect(result).toContain('New prose.');
        expect(result).not.toContain('Old prose.');
    });

    it('appends new prose zone when no sentinels present', () => {
        const content = '---\nfoo: bar\n---\n# Heading';
        const result = replaceProseZone(content, 'Added prose.');
        expect(result).toContain(PROSE_BEGIN);
        expect(result).toContain(PROSE_END);
        expect(result).toContain('Added prose.');
        // Original heading still present
        expect(result).toContain('# Heading');
    });

    it('preserves frontmatter when replacing prose', () => {
        const original = `---\nlibrarium_type: character\n---\n${PROSE_BEGIN}\nOld.\n${PROSE_END}\n`;
        const result = replaceProseZone(original, 'New.');
        expect(result.startsWith('---')).toBe(true);
        expect(result).toContain('librarium_type: character');
        expect(result).toContain('New.');
    });

    it('idempotent sentinel wrapping when called on already-wrapped content', () => {
        const content = '---\nfoo: bar\n---\n';
        const first = replaceProseZone(content, 'Prose.');
        const second = replaceProseZone(first, 'Updated.');
        // Should only have one set of sentinels
        expect(second.split(PROSE_BEGIN).length).toBe(2);
        expect(second).toContain('Updated.');
        expect(second).not.toContain('Prose.');
    });
});
