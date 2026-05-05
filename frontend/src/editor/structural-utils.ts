/**
 * Utilities for the Structural Editor — frontmatter parsing/serialisation,
 * prose sentinel extraction, and file content reconstruction.
 */

import { parse as yamlParse, stringify as yamlStringify } from 'yaml';

const PROSE_BEGIN = '<!-- librarium:prose:begin -->';
const PROSE_END = '<!-- librarium:prose:end -->';
const LEGACY_PROSE_BEGIN = '<!-- codex:prose:begin -->';
const LEGACY_PROSE_END = '<!-- codex:prose:end -->';

// ── Frontmatter ───────────────────────────────────────────────────────────────

/**
 * Parse the YAML frontmatter block from raw markdown content.
 * Throws if the YAML is invalid.
 * Returns an empty object if no frontmatter is present.
 */
export function parseFrontmatter(content: string): Record<string, unknown> {
    if (!content.startsWith('---')) return {};
    const end = content.indexOf('\n---', 3);
    if (end === -1) return {};
    const raw = content.slice(4, end); // skip opening '---\n'
    try {
        const parsed = yamlParse(raw);
        return (parsed && typeof parsed === 'object' && !Array.isArray(parsed))
            ? (parsed as Record<string, unknown>)
            : {};
    } catch (e) {
        throw new Error(`YAML parse error: ${(e as Error).message}`);
    }
}

/**
 * Re-serialise the frontmatter dict back into the file content, replacing the
 * existing frontmatter block (or prepending a new one).
 * The rest of the file (body) is left unchanged.
 */
export function serializeFrontmatter(
    fm: Record<string, unknown>,
    content: string,
): string {
    const yamlStr = yamlStringify(fm).trimEnd();
    const newBlock = `---\n${yamlStr}\n---\n`;

    if (content.startsWith('---')) {
        const end = content.indexOf('\n---', 3);
        if (end !== -1) {
            const body = content.slice(end + 4); // everything after closing ---
            return `${newBlock}${body}`;
        }
    }
    return `${newBlock}\n${content}`;
}

// ── Prose zone ────────────────────────────────────────────────────────────────

/**
 * Extract the text between prose sentinels.
 * Returns the full file body (minus frontmatter) if no sentinels are found.
 */
export function extractProseZone(content: string): string {
    const [beginMarker, endMarker] = content.includes(PROSE_BEGIN)
        ? [PROSE_BEGIN, PROSE_END]
        : [LEGACY_PROSE_BEGIN, LEGACY_PROSE_END];
    const start = content.indexOf(beginMarker);
    const end = content.indexOf(endMarker);
    if (start === -1 || end === -1 || end <= start) {
        // No sentinels — return the body after frontmatter
        const fmEnd = content.indexOf('\n---', 3);
        return fmEnd !== -1 ? content.slice(fmEnd + 4).trim() : content.trim();
    }
    return content.slice(start + beginMarker.length, end).trim();
}

/**
 * Replace the prose zone (between sentinels) with new content.
 * If sentinels are absent the new prose is appended after a divider.
 */
export function replaceProseZone(content: string, newProse: string): string {
    const hasCurrentMarkers = content.includes(PROSE_BEGIN);
    const beginMarker = hasCurrentMarkers ? PROSE_BEGIN : LEGACY_PROSE_BEGIN;
    const endMarker = hasCurrentMarkers ? PROSE_END : LEGACY_PROSE_END;
    const start = content.indexOf(beginMarker);
    const end = content.indexOf(endMarker);
    if (start === -1 || end === -1 || end <= start) {
        return `${content}\n\n${PROSE_BEGIN}\n${newProse}\n${PROSE_END}\n`;
    }
    return (
        content.slice(0, start) +
        PROSE_BEGIN +
        `\n${newProse}\n` +
        content.slice(end + endMarker.length).replace(/^\n?/, `\n${PROSE_END}`)
    );
}
