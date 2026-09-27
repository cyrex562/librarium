import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useUiStore } from '@/stores/ui';
import { useVaultsStore } from '@/stores/vaults';
import { pdfExportErrorMessage } from '@/utils/pdfExport';

/**
 * Convert to Typst (#141) and Export as PDF (#140), shared by the file tree's
 * context menu and the Markdown editor's toolbar.
 */
export function useNoteConversion() {
    const filesStore = useFilesStore();
    const tabsStore = useTabsStore();
    const uiStore = useUiStore();
    const vaultsStore = useVaultsStore();

    /** Create `<note>.typ` beside a Markdown note, open it, report losses. */
    async function convertToTypst(path: string) {
        const vaultId = vaultsStore.activeVaultId;
        if (!vaultId) return;
        try {
            const result = await filesStore.convertToTypst(vaultId, path);
            tabsStore.openTab(tabsStore.activePaneId, result.path, result.path.split('/').pop()!);
            if (result.warnings.length > 0) {
                uiStore.conversionReport = { source: path, target: result.path, warnings: result.warnings };
            }
        } catch (e) {
            alert(`Couldn't convert to Typst: ${e instanceof Error ? e.message : String(e)}`);
        }
    }

    /** Download a PDF of a note: `content` if given, else the saved file. */
    async function exportPdf(path: string, content?: string) {
        const vaultId = vaultsStore.activeVaultId;
        if (!vaultId) return;
        try {
            await filesStore.exportAsPdf(vaultId, path, content);
        } catch (e) {
            alert(pdfExportErrorMessage(e));
        }
    }

    return { convertToTypst, exportPdf };
}
