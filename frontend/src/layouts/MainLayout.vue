<template>
  <v-navigation-drawer
    v-model="sidebarOpen"
    :width="sidebarWidth"
    permanent
    rail-width="0"
    style="background: rgb(var(--v-theme-surface)); border-right: 1px solid rgb(var(--v-theme-border));"
  >
    <div class="d-flex align-center pa-2 gap-2" style="border-bottom: 1px solid rgb(var(--v-theme-border));">
      <v-select
        :items="vaultsStore.vaults"
        :item-title="(v) => v.path_exists === false ? v.name + ' (missing)' : v.name"
        item-value="id"
        :model-value="vaultsStore.activeVaultId"
        placeholder="Select vault…"
        hide-details
        density="compact"
        variant="outlined"
        style="flex: 1; min-width: 0;"
        data-testid="vault-selector"
        @update:model-value="onVaultChange"
      />
      <v-btn icon="mdi-cog" size="small" data-testid="vault-settings-btn" @click="vaultManagerOpen = true" />
    </div>

    <SidebarActions />

    <div style="flex: 1; display: flex; flex-direction: column; overflow: hidden; min-height: 0;">
      <!-- File tree: own scroll region, capped so it uses natural space when
           short but never pushes the panels below off-screen in a large vault. -->
      <div style="flex: 0 1 auto; max-height: 50vh; overflow-y: auto; overflow-x: hidden;">
        <FileTree v-if="vaultsStore.activeVaultId" />
        <div v-else class="pa-4 text-secondary text-caption text-center">
          <div class="mb-2">Create or select a vault to start.</div>
          <v-btn
            size="small"
            variant="tonal"
            prepend-icon="mdi-database-plus-outline"
            @click="vaultManagerOpen = true"
          >
            Manage vaults
          </v-btn>
        </div>
      </div>

      <!-- Context + navigation panels: independent scroll region, always
           reachable regardless of how tall the file tree grows. -->
      <div
        v-if="vaultsStore.activeVaultId"
        style="flex: 1 1 auto; min-height: 0; overflow-y: auto; overflow-x: hidden; border-top: 1px solid rgb(var(--v-theme-border));"
      >
        <template v-if="activeMdContent !== null">
          <MlInsightsPanel
            :vault-id="vaultsStore.activeVaultId"
            :file-path="tabsStore.activeTab?.filePath ?? ''"
            :content="activeMdContent"
          />
          <OutlinePanel :content="activeMdContent" />
          <OutgoingLinksPanel :content="activeMdContent" />
          <BacklinksPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <EntityRelationsPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <NeighboringFilesPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
        </template>

        <BookmarksPanel />
        <RecentFilesPanel />
        <TagsPanel @search="openTagSearch" />
      </div>
    </div>
  </v-navigation-drawer>

  <TopBar @open-search="searchOpen = true" @open-plugins="pluginsOpen = true" />

  <v-main style="height: 100vh; display: flex; flex-direction: column; overflow: hidden;">
    <PaneContainer />
    <StatusBar />
  </v-main>

  <div class="sidebar-resize-handle" @mousedown="startResize" />

  <VaultManager v-model="vaultManagerOpen" />
  <SearchModal v-model="searchOpen" :initial-query="searchInitialQuery" />
  <QuickSwitcher v-model="quickSwitcherOpen" />
  <PluginManager v-model="pluginsOpen" />
  <TemplateSelector v-model="uiStore.templateSelectorOpen" />
  <ConflictResolver v-model="uiStore.conflictResolverOpen" />
  <ImportVaultDialog v-model="uiStore.importDialogOpen" />
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { ApiError } from '@/api/client';
import { useAuthStore } from '@/stores/auth';
import { useVaultsStore } from '@/stores/vaults';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useUiStore } from '@/stores/ui';
import { usePreferencesStore } from '@/stores/preferences';
import { useEditorStore } from '@/stores/editor';
import { useWebSocket } from '@/composables/useWebSocket';
import type { EditorMode, PersistedEditorMode } from '@/api/types';

import TopBar from '@/components/TopBar.vue';
import SidebarActions from '@/components/sidebar/SidebarActions.vue';
import FileTree from '@/components/sidebar/FileTree.vue';
import MlInsightsPanel from '@/components/sidebar/MlInsightsPanel.vue';
import OutlinePanel from '@/components/sidebar/OutlinePanel.vue';
import OutgoingLinksPanel from '@/components/sidebar/OutgoingLinksPanel.vue';
import RecentFilesPanel from '@/components/sidebar/RecentFilesPanel.vue';
import BacklinksPanel from '@/components/sidebar/BacklinksPanel.vue';
import NeighboringFilesPanel from '@/components/sidebar/NeighboringFilesPanel.vue';
import EntityRelationsPanel from '@/components/sidebar/EntityRelationsPanel.vue';
import TagsPanel from '@/components/sidebar/TagsPanel.vue';
import BookmarksPanel from '@/components/sidebar/BookmarksPanel.vue';
import PaneContainer from '@/components/tabs/PaneContainer.vue';
import StatusBar from '@/components/StatusBar.vue';
import VaultManager from '@/components/modals/VaultManager.vue';
import SearchModal from '@/components/modals/SearchModal.vue';
import QuickSwitcher from '@/components/modals/QuickSwitcher.vue';
import PluginManager from '@/components/modals/PluginManager.vue';
import TemplateSelector from '@/components/modals/TemplateSelector.vue';
import ConflictResolver from '@/components/modals/ConflictResolver.vue';
import ImportVaultDialog from '@/components/modals/ImportVaultDialog.vue';

const vaultsStore = useVaultsStore();
const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const uiStore = useUiStore();
const prefsStore = usePreferencesStore();
const editorStore = useEditorStore();
const authStore = useAuthStore();
const router = useRouter();

const sidebarOpen = ref(true);
const sidebarWidth = ref(280);

const activeMdContent = computed<string | null>(() => {
  const tab = tabsStore.activeTab;
  if (!tab?.filePath?.endsWith('.md')) return null;
  return tab.content ?? null;
});
const vaultManagerOpen = ref(false);
const searchOpen = ref(false);
const searchInitialQuery = ref('');
const quickSwitcherOpen = ref(false);
const pluginsOpen = ref(false);

onMounted(async () => {
  try {
    await authStore.ensureFresh();
    await authStore.loadProfile();
  } catch {
    await authStore.logout();
    await router.replace({
      path: '/login',
      query: { redirect: router.currentRoute.value.fullPath || '/' },
    });
    return;
  }

  useWebSocket();

  // The file tree / recent files load reactively via the activeVaultId watcher
  // below, so we only need to populate the vault list here.
  await vaultsStore.loadVaults();

  if (prefsStore.prefs.editor_mode) {
    editorStore.setMode(prefsStore.prefs.editor_mode);
  }

  window.addEventListener('keydown', onGlobalKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKeydown);
});

function onVaultChange(id: string) {
  vaultsStore.setActiveVault(id);
  // Close all tabs when switching vaults. The file tree and recent files are
  // refreshed reactively by the activeVaultId watcher below.
  tabsStore.closeAllTabs();
}

// Load the file tree and recent files whenever the active vault changes,
// regardless of where the change originates (initial restore, the vault
// selector, or the Vault Manager modal). This mirrors how TagsPanel,
// BookmarksPanel and RecentFilesPanel react to activeVaultId, so the file
// listing can never get out of sync with the selected vault.
watch(
  () => vaultsStore.activeVaultId,
  async (id) => {
    if (!id) return;
    await filesStore.loadTree(id);
    await filesStore.loadRecentFiles(id);
  },
  { immediate: true },
);

function onGlobalKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    void saveActiveTabNow();
    return;
  }

  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && ['1', '2', '3'].includes(e.key)) {
    e.preventDefault();
    const modeByShortcut: Record<string, PersistedEditorMode> = {
      '1': 'raw',
      '2': 'formatted_raw',
      '3': 'fully_rendered',
    };
    const mode = modeByShortcut[e.key];
    editorStore.setMode(mode);
    prefsStore.set('editor_mode', mode);
    void prefsStore.save();
    return;
  }

  if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
    if (!vaultsStore.activeVaultId) return;
    e.preventDefault();
    searchOpen.value = true;
    return;
  }

  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && ['p', 'k'].includes(e.key.toLowerCase())) {
    if (!vaultsStore.activeVaultId) return;
    e.preventDefault();
    quickSwitcherOpen.value = true;
  }
}

function openTagSearch(query: string) {
  if (!vaultsStore.activeVaultId) return;
  searchInitialQuery.value = query;
  searchOpen.value = true;
}

watch(
  () => [vaultsStore.activeVaultId, tabsStore.activeTab?.filePath] as const,
  ([vaultId, filePath], [previousVaultId, previousFilePath]) => {
    if (!vaultId || !filePath || filePath.startsWith('__')) {
      return;
    }

    if (vaultId === previousVaultId && filePath === previousFilePath) {
      return;
    }

    filesStore.recordRecentFile(vaultId, filePath);
  },
);

async function saveActiveTabNow() {
  const vaultId = vaultsStore.activeVaultId;
  const tab = tabsStore.activeTab;
  if (!vaultId || !tab || !tab.filePath || !tab.isDirty) return;

  try {
    const saved = await filesStore.writeFile(vaultId, tab.filePath, {
      content: tab.content,
      last_modified: tab.modified || undefined,
      frontmatter: tab.frontmatter,
    });
    tabsStore.markTabClean(tab.id, saved.modified);
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      const latest = await filesStore.readFile(vaultId, tab.filePath);
      uiStore.openConflictResolver({
        tabId: tab.id,
        filePath: tab.filePath,
        yourVersion: tab.content,
        serverVersion: latest.content,
        serverModified: latest.modified,
      });
      return;
    }
    throw error;
  }
}

let resizing = false;
let resizeStartX = 0;
let resizeStartWidth = 280;

function startResize(e: MouseEvent) {
  resizing = true;
  resizeStartX = e.clientX;
  resizeStartWidth = sidebarWidth.value;
  window.addEventListener('mousemove', onResize);
  window.addEventListener('mouseup', stopResize);
}

function onResize(e: MouseEvent) {
  if (!resizing) return;
  const delta = e.clientX - resizeStartX;
  sidebarWidth.value = Math.max(160, Math.min(600, resizeStartWidth + delta));
}

function stopResize() {
  resizing = false;
  window.removeEventListener('mousemove', onResize);
  window.removeEventListener('mouseup', stopResize);
}
</script>

<style scoped>
.sidebar-resize-handle {
  position: fixed;
  left: v-bind(sidebarWidth + 'px');
  top: 0;
  width: 4px;
  height: 100vh;
  cursor: col-resize;
  z-index: 200;
  transition: background 0.15s;
}
.sidebar-resize-handle:hover {
  background: rgb(var(--v-theme-primary));
}
</style>
