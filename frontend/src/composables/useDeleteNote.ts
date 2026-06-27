import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { usePreferencesStore } from '@/stores/preferences';

/**
 * Shared "delete a note from disk" action (LIB-082) used by the file tree and
 * every sidebar panel that lists notes (recent, bookmarks, neighboring), so the
 * same delete is reachable from wherever a note appears.
 *
 * Mirrors the file-tree delete UX: confirm, clear per-path icon prefs, close any
 * open tabs for the path, then delete on the server (which refreshes the tree).
 * Also prunes the path from the recent-files list, which is tracked separately.
 *
 * Returns true if the note was deleted, false if cancelled / no active vault.
 */
export function useDeleteNote() {
  const filesStore = useFilesStore();
  const tabsStore = useTabsStore();
  const vaultsStore = useVaultsStore();
  const prefsStore = usePreferencesStore();

  async function deleteNote(path: string): Promise<boolean> {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId) return false;
    const name = path.split('/').pop() ?? path;
    if (!confirm(`Delete "${name}"? This permanently removes the file from the vault.`)) {
      return false;
    }
    prefsStore.clearIconsUnderPath(path);
    await prefsStore.save();
    tabsStore.closeTabsByPath(path);
    await filesStore.deleteFile(vaultId, path);
    filesStore.recentFiles = filesStore.recentFiles.filter((p) => p !== path);
    return true;
  }

  return { deleteNote };
}
