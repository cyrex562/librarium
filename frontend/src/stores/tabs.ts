import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Tab, Pane, FileType } from '@/api/types';

function getFileType(filePath: string): FileType {
    const ext = filePath.split('.').pop()?.toLowerCase();
    if (!ext) return 'other';
    if (ext === 'md') return 'markdown';
    if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext)) return 'image';
    if (ext === 'pdf') return 'pdf';
    if (ext === 'canvas') return 'canvas';
    if (['mp3', 'wav', 'ogg'].includes(ext)) return 'audio';
    if (['mp4', 'webm'].includes(ext)) return 'video';
    if (['txt', 'json', 'js', 'ts', 'css', 'html', 'xml', 'rs', 'py', 'java', 'c', 'cpp', 'h', 'go', 'yaml', 'yml', 'toml', 'ini', 'sh', 'bat', 'mdx'].includes(ext)) return 'text';
    return 'other';
}

function makeTabId(filePath: string, paneId: string): string {
    return `${paneId}::${filePath}`;
}

export const useTabsStore = defineStore('tabs', () => {
    // All open tabs, keyed by tab id
    const tabs = ref<Map<string, Tab>>(new Map());

    // Pane layout
    const panes = ref<Pane[]>([{ id: 'pane-1', flex: 1, activeTabId: null }]);
    const activePaneId = ref<string>('pane-1');
    const splitOrientation = ref<'vertical' | 'horizontal'>('vertical');

    // ── Computed ────────────────────────────────────────────────────────────

    const activeTab = computed((): Tab | null => {
        const pane = panes.value.find((p) => p.id === activePaneId.value);
        if (!pane?.activeTabId) return null;
        return tabs.value.get(pane.activeTabId) ?? null;
    });

    const tabsForPane = (paneId: string): Tab[] => {
        const result: Tab[] = [];
        tabs.value.forEach((tab) => {
            if (tab.paneId === paneId) result.push(tab);
        });
        return result;
    };

    const dirtyTabs = computed((): Tab[] => {
        const result: Tab[] = [];
        tabs.value.forEach((tab) => { if (tab.isDirty) result.push(tab); });
        return result;
    });

    // ── Tab management ───────────────────────────────────────────────────────

    function openTab(paneId: string, filePath: string, fileName?: string): Tab {
        const targetPaneId = paneId ?? activePaneId.value;
        const id = makeTabId(filePath, targetPaneId);

        if (tabs.value.has(id)) {
            // Already open — just activate it
            activateTab(id, targetPaneId);
            return tabs.value.get(id)!;
        }

        const tab: Tab = {
            id,
            filePath,
            fileName: fileName ?? filePath.split('/').pop() ?? filePath,
            content: '',
            modified: '',
            isDirty: false,
            paneId: targetPaneId,
            fileType: getFileType(filePath),
        };

        tabs.value.set(id, tab);
        activateTab(id, targetPaneId);
        return tab;
    }

    function closeTab(tabId: string) {
        const tab = tabs.value.get(tabId);
        if (!tab) return;

        tabs.value.delete(tabId);

        // If this was the active tab in its pane, activate another
        const pane = panes.value.find((p) => p.id === tab.paneId);
        if (pane && pane.activeTabId === tabId) {
            const remaining = tabsForPane(tab.paneId);
            pane.activeTabId = remaining.length > 0 ? remaining[remaining.length - 1].id : null;
        }
    }

    function closeAllTabs() {
        tabs.value.clear();
        panes.value.forEach((pane) => {
            pane.activeTabId = null;
        });
    }

    function closeTabs(tabIds: string[]) {
        tabIds.forEach((tabId) => closeTab(tabId));
    }

    function tabIdsInPane(paneId: string): string[] {
        return tabsForPane(paneId).map((tab) => tab.id);
    }

    function tabIdsToRight(paneId: string, tabId: string): string[] {
        const paneTabs = tabsForPane(paneId);
        const index = paneTabs.findIndex((tab) => tab.id === tabId);
        if (index < 0) return [];
        return paneTabs.slice(index + 1).map((tab) => tab.id);
    }

    function tabIdsExcept(paneId: string, tabId: string): string[] {
        return tabsForPane(paneId)
            .filter((tab) => tab.id !== tabId)
            .map((tab) => tab.id);
    }

    function activateTab(tabId: string, paneId?: string) {
        const tab = tabs.value.get(tabId);
        if (!tab) return;
        const targetPaneId = paneId ?? tab.paneId;
        const pane = panes.value.find((p) => p.id === targetPaneId);
        if (pane) pane.activeTabId = tabId;
        activePaneId.value = targetPaneId;
    }

    function updateTabContent(tabId: string, content: string) {
        const tab = tabs.value.get(tabId);
        if (!tab) return;
        tab.content = content;
        tab.isDirty = true;
    }

    function markTabClean(tabId: string, modified?: string) {
        const tab = tabs.value.get(tabId);
        if (!tab) return;
        tab.isDirty = false;
        if (modified !== undefined) tab.modified = modified;
    }

    function updateTabFrontmatter(tabId: string, frontmatter: Record<string, unknown>) {
        const tab = tabs.value.get(tabId);
        if (!tab) return;
        tab.frontmatter = frontmatter;
        tab.isDirty = true;
    }

    function remapTabPaths(fromPath: string, toPath: string) {
        const updatedTabs = new Map<string, Tab>();
        const remappedIds = new Map<string, string>();
        const prefix = `${fromPath}/`;
        const newPrefix = `${toPath}/`;

        tabs.value.forEach((tab, tabId) => {
            let nextPath = tab.filePath;
            if (tab.filePath === fromPath) {
                nextPath = toPath;
            } else if (tab.filePath.startsWith(prefix)) {
                nextPath = `${newPrefix}${tab.filePath.slice(prefix.length)}`;
            }

            if (nextPath !== tab.filePath) {
                const nextId = makeTabId(nextPath, tab.paneId);
                remappedIds.set(tabId, nextId);
                updatedTabs.set(nextId, {
                    ...tab,
                    id: nextId,
                    filePath: nextPath,
                    fileName: nextPath.split('/').pop() ?? nextPath,
                    fileType: getFileType(nextPath),
                });
                return;
            }

            updatedTabs.set(tabId, tab);
        });

        tabs.value = updatedTabs;

        panes.value.forEach((pane) => {
            if (pane.activeTabId && remappedIds.has(pane.activeTabId)) {
                pane.activeTabId = remappedIds.get(pane.activeTabId) ?? pane.activeTabId;
            }
        });
    }

    function closeTabsByPath(filePath: string) {
        const tabIdsToClose: string[] = [];
        tabs.value.forEach((tab, tabId) => {
            if (tab.filePath === filePath || tab.filePath.startsWith(`${filePath}/`)) {
                tabIdsToClose.push(tabId);
            }
        });
        tabIdsToClose.forEach((tabId) => closeTab(tabId));
    }

    // ── Pane management ──────────────────────────────────────────────────────

    function splitPane(sourcePaneId?: string, orientation: 'vertical' | 'horizontal' = 'vertical'): string | null {
        if (panes.value.length >= 4) return null; // max 4 panes
        splitOrientation.value = orientation;
        const newId = `pane-${Date.now()}`;
        panes.value.push({ id: newId, flex: 1, activeTabId: null });
        // Equalize flex
        const share = 100 / panes.value.length;
        panes.value.forEach((p) => { p.flex = share; });
        activePaneId.value = newId;
        return newId;
    }

    function closePane(paneId: string) {
        if (panes.value.length <= 1) return;
        // Close all tabs in pane
        tabs.value.forEach((tab, id) => {
            if (tab.paneId === paneId) tabs.value.delete(id);
        });
        panes.value = panes.value.filter((p) => p.id !== paneId);
        if (activePaneId.value === paneId) {
            activePaneId.value = panes.value[0].id;
        }
        // Re-equalize flex
        const share = 100 / panes.value.length;
        panes.value.forEach((p) => { p.flex = share; });
    }

    function setActivePaneId(paneId: string) {
        activePaneId.value = paneId;
    }

    function openGraphTab(paneId: string, vaultId: string): Tab {
        const filePath = `__graph__:${vaultId}`;
        const targetPaneId = paneId ?? activePaneId.value;
        const id = makeTabId(filePath, targetPaneId);

        if (tabs.value.has(id)) {
            activateTab(id, targetPaneId);
            return tabs.value.get(id)!;
        }

        const tab: Tab = {
            id,
            filePath,
            fileName: 'Graph',
            content: '',
            modified: '',
            isDirty: false,
            paneId: targetPaneId,
            fileType: 'graph',
        };

        tabs.value.set(id, tab);
        activateTab(id, targetPaneId);
        return tab;
    }

    return {
        tabs,
        panes,
        activePaneId,
        splitOrientation,
        activeTab,
        tabsForPane,
        dirtyTabs,
        openTab,
        openGraphTab,
        closeTab,
        closeTabs,
        closeAllTabs,
        tabIdsInPane,
        tabIdsToRight,
        tabIdsExcept,
        activateTab,
        updateTabContent,
        markTabClean,
        updateTabFrontmatter,
        remapTabPaths,
        closeTabsByPath,
        splitPane,
        closePane,
        setActivePaneId,
    };
});
