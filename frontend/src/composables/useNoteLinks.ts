import { apiResolveWikiLink } from '@/api/client';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { resolveRelativeNotePath, treeHasFile, type TypstLink } from '@/utils/typstLinks';

/**
 * Follow a link from a Typst note (#144): a note name resolves like a wiki
 * link, a path is tried relative to the note and then to the vault root, and
 * a web link opens in a new browser tab (never replacing the app).
 */
export function useNoteLinks() {
    const filesStore = useFilesStore();
    const tabsStore = useTabsStore();
    const vaultsStore = useVaultsStore();

    function open(path: string) {
        tabsStore.openTab(tabsStore.activePaneId, path, path.split('/').pop() ?? path);
    }

    async function followTypstLink(link: Pick<TypstLink, 'target' | 'kind'>, fromNote: string) {
        if (link.kind === 'external') {
            window.open(link.target, '_blank', 'noopener,noreferrer');
            return;
        }
        const vaultId = vaultsStore.activeVaultId;
        if (!vaultId) return;
        if (link.kind === 'note-name') {
            try {
                const resolved = await apiResolveWikiLink(vaultId, link.target.split('#')[0], fromNote);
                if (resolved.exists) open(resolved.path);
            } catch {
                // Unresolvable link: nothing to open.
            }
            return;
        }
        const relative = resolveRelativeNotePath(fromNote, link.target);
        const fromRoot = resolveRelativeNotePath('', link.target);
        open(treeHasFile(filesStore.tree, relative) || !treeHasFile(filesStore.tree, fromRoot) ? relative : fromRoot);
    }

    return { followTypstLink };
}
