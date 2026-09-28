/**
 * Links in Typst notes (#144). Librarium's convention, matching the server's
 * `librarium_core::note_links`:
 * - `#link("librarium://note/Target")[label]` names a note like `[[Target]]`;
 * - `#link("other.typ")`, `#link("../notes/a.md")`: a path to a note file,
 *   relative to the linking note;
 * - anything else with a scheme (`https://…`) is an external link.
 */

export const NOTE_LINK_PREFIX = 'librarium://note/';

export interface TypstLink {
    /** Note name (wiki-style), note path, or external URL. */
    target: string;
    label: string;
    kind: 'note-name' | 'note-path' | 'external';
}

const LINK_RE = /link\(\s*"((?:[^"\\]|\\.)*)"\s*\)(?:\[([^\]]*)\])?/g;

function unescapeTypstString(s: string): string {
    return s.replace(/\\(.)/g, (_, c: string) => ({ n: '\n', t: '\t' } as Record<string, string>)[c] ?? c);
}

/** Classify a link URL, or null if it isn't a note link or web link. */
export function classifyTypstLink(url: string): TypstLink['kind'] | null {
    if (url.startsWith(NOTE_LINK_PREFIX)) return url.length > NOTE_LINK_PREFIX.length ? 'note-name' : null;
    if (/^[a-z][a-z0-9+.-]*:/i.test(url)) return /^(https?|mailto):/i.test(url) ? 'external' : null;
    return /\.(md|typ)(#.*)?$/i.test(url) ? 'note-path' : null;
}

/** Links in Typst source, deduplicated by target, in order. */
export function typstLinks(source: string): TypstLink[] {
    const seen = new Set<string>();
    const out: TypstLink[] = [];
    for (const m of source.matchAll(LINK_RE)) {
        const url = unescapeTypstString(m[1]);
        const kind = classifyTypstLink(url);
        if (!kind) continue;
        const target = kind === 'note-name' ? url.slice(NOTE_LINK_PREFIX.length) : url;
        if (seen.has(target)) continue;
        seen.add(target);
        const label = (m[2] ?? '').replace(/\\(.)/g, '$1').trim() || target;
        out.push({ target, label, kind });
    }
    return out;
}

/** Resolve a note-relative path (`../a.md`) against the linking note's path. */
export function resolveRelativeNotePath(fromNote: string, relative: string): string {
    const withoutHash = relative.split('#')[0];
    const base = withoutHash.startsWith('/') ? [] : fromNote.split('/').slice(0, -1);
    for (const part of withoutHash.split('/')) {
        if (part === '' || part === '.') continue;
        if (part === '..') base.pop();
        else base.push(part);
    }
    return base.join('/');
}

interface TreeNodeLike {
    path: string;
    is_directory: boolean;
    children?: TreeNodeLike[] | null;
}

/** Whether `path` is a file in the (loaded) file tree. */
export function treeHasFile(nodes: TreeNodeLike[], path: string): boolean {
    for (const node of nodes) {
        if (!node.is_directory && node.path === path) return true;
        if (node.is_directory && node.children && path.startsWith(`${node.path}/`) && treeHasFile(node.children, path)) {
            return true;
        }
    }
    return false;
}
