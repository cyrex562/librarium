<template>
  <!-- Under the local transport, sync must be paired (and at least one
       vault mapped) before there's anything to browse — block on that
       first, ahead of the normal vault/editor UI below. Desktop/browser
       (httpTransport) never enters this branch: `syncBootstrapped` only
       becomes true after `isLocalMode` bootstrap runs. -->
  <template v-if="isLocalMode && !syncBootstrapped">
    <v-container class="fill-height d-flex align-center justify-center">
      <v-progress-circular indeterminate color="primary" size="32" />
    </v-container>
  </template>

  <PairingGate v-else-if="isLocalMode && (!syncStore.isPaired || !syncStore.hasAnyMapping)" />

  <template v-else>
  <!-- Sidebar. Desktop: permanent, resizable. Mobile: overlay (temporary) that
       slides over the content, opened from the TopBar hamburger and auto-closed
       when a note is opened. -->
  <v-navigation-drawer
    v-model="sidebarOpen"
    :width="isMobile ? mobileSidebarWidth : sidebarWidth"
    :permanent="!isMobile"
    :temporary="isMobile"
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
            v-if="canUseMlOrganize"
            :vault-id="vaultsStore.activeVaultId"
            :file-path="tabsStore.activeTab?.filePath ?? ''"
            :content="activeMdContent"
          />
          <OutlinePanel :content="activeMdContent" />
          <OutgoingLinksPanel :content="activeMdContent" />
          <BacklinksPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <EntityRelationsPanel v-if="canUseEntityGraph" :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <NeighboringFilesPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
        </template>
        <!-- Typst notes (#144, #145): links, entity relations and neighbors; outline and ML are Markdown-only. -->
        <template v-else-if="activeTypstContent !== null">
          <OutgoingLinksPanel :content="activeTypstContent" language="typst" />
          <BacklinksPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <EntityRelationsPanel v-if="canUseEntityGraph" :file-path="tabsStore.activeTab?.filePath ?? ''" />
          <NeighboringFilesPanel :file-path="tabsStore.activeTab?.filePath ?? ''" />
        </template>

        <FavoritesPanel />
        <BookmarksPanel />
        <RecentFilesPanel />
        <TagsPanel @search="openTagSearch" />
      </div>
    </div>
  </v-navigation-drawer>

  <TopBar
    @open-search="searchOpen = true"
    @open-plugins="pluginsOpen = true"
    @toggle-sidebar="sidebarOpen = !sidebarOpen"
  />

  <v-main style="height: 100vh; display: flex; flex-direction: column; overflow: hidden;">
    <PaneContainer />
    <StatusBar />
  </v-main>

  <div v-if="!isMobile" class="sidebar-resize-handle" @mousedown="startResize" />

  <VaultManager v-model="vaultManagerOpen" />
  <SearchModal v-model="searchOpen" :initial-query="searchInitialQuery" />
  <QuickSwitcher v-model="quickSwitcherOpen" />
  <PluginManager v-if="canUsePlugins" v-model="pluginsOpen" />
  <TemplateSelector v-model="uiStore.templateSelectorOpen" />
  <ConflictResolver v-model="uiStore.conflictResolverOpen" />
  <ConversionReportDialog />
  <ImportVaultDialog v-model="uiStore.importDialogOpen" />
  <MoveToFolderModal v-model="uiStore.moveDialogOpen" :source-paths="uiStore.moveSourcePaths" />
  </template>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { ApiError, isSessionInvalid } from '@/api/client';
import { getLogger } from '@/utils/logger';

const log = getLogger('mainLayout');
import { useAuthStore } from '@/stores/auth';
import { useVaultsStore } from '@/stores/vaults';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useUiStore } from '@/stores/ui';
import { usePreferencesStore } from '@/stores/preferences';
import { useEditorStore } from '@/stores/editor';
import { useSyncStore } from '@/stores/sync';
import { startBackgroundSyncService } from '@/utils/tauri';
import { useWebSocket } from '@/composables/useWebSocket';
import { useMobile } from '@/composables/useMobile';
import { useCapabilities } from '@/composables/useCapabilities';
import type { EditorMode, PersistedEditorMode } from '@/api/types';

import TopBar from '@/components/TopBar.vue';
import PairingGate from '@/components/sync/PairingGate.vue';
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
import FavoritesPanel from '@/components/sidebar/FavoritesPanel.vue';
import PaneContainer from '@/components/tabs/PaneContainer.vue';
import StatusBar from '@/components/StatusBar.vue';
import VaultManager from '@/components/modals/VaultManager.vue';
import SearchModal from '@/components/modals/SearchModal.vue';
import QuickSwitcher from '@/components/modals/QuickSwitcher.vue';
import PluginManager from '@/components/modals/PluginManager.vue';
import TemplateSelector from '@/components/modals/TemplateSelector.vue';
import ConflictResolver from '@/components/modals/ConflictResolver.vue';
import ConversionReportDialog from '@/components/modals/ConversionReportDialog.vue';
import ImportVaultDialog from '@/components/modals/ImportVaultDialog.vue';
import MoveToFolderModal from '@/components/modals/MoveToFolderModal.vue';

const vaultsStore = useVaultsStore();
const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const uiStore = useUiStore();
const prefsStore = usePreferencesStore();
const editorStore = useEditorStore();
const authStore = useAuthStore();
const syncStore = useSyncStore();
const router = useRouter();

const { isMobile } = useMobile();
const { canUseMlOrganize, canUseEntityGraph, canUsePlugins, isLocalMode } = useCapabilities();

const syncBootstrapped = computed(() => syncStore.pairingLoaded && syncStore.statusLoaded);

// Desktop starts with the sidebar visible; mobile starts on the content with
// the drawer closed (opened via the TopBar hamburger).
const sidebarOpen = ref(!isMobile.value);
const sidebarWidth = ref(280);
// Overlay drawer width on phones: near-full-width but always leaving a strip
// of the underlying page visible as a "tap outside to close" affordance.
const mobileSidebarWidth = computed(() =>
  Math.min(320, Math.round(window.innerWidth * 0.85)),
);

// Crossing the breakpoint resets the drawer to that mode's natural state
// (desktop: shown; mobile: hidden) so e.g. rotating a tablet never strands
// the user with a full-screen drawer they didn't open.
watch(isMobile, (mobile) => {
  sidebarOpen.value = !mobile;
});

// On mobile, opening a note closes the drawer so the content is immediately
// visible (the drawer overlays the editor).
watch(
  () => tabsStore.activeTab?.filePath,
  (path, prev) => {
    if (isMobile.value && path && path !== prev) {
      sidebarOpen.value = false;
    }
  },
);

const activeMdContent = computed<string | null>(() => {
  const tab = tabsStore.activeTab;
  if (!tab?.filePath?.endsWith('.md')) return null;
  return tab.content ?? null;
});
const activeTypstContent = computed<string | null>(() => {
  const tab = tabsStore.activeTab;
  if (!tab?.filePath?.toLowerCase().endsWith('.typ')) return null;
  return tab.content ?? null;
});
const vaultManagerOpen = ref(false);
const searchOpen = ref(false);
const searchInitialQuery = ref('');
const quickSwitcherOpen = ref(false);
const pluginsOpen = ref(false);

onMounted(async () => {
  log.info('MainLayout onMounted');

  // The local transport has no token-based auth lifecycle at all (#54's
  // remote credentials live in Rust secure storage, never the WebView) —
  // same reasoning `ensureFreshForRequest` in api/client.ts already applies
  // per-request. There is nothing to refresh/load and no HTTP server to ask.
  //
  // A server with auth disabled never sets an `AuthenticatedUser` on the
  // request (librarium-server's AuthMiddleware skips that step entirely in
  // this mode — see `checkServerAuthEnabled`'s doc comment), so `/api/auth/me`
  // always 401s here regardless of the router guard already having let this
  // navigation through. Skip the profile load rather than bouncing back to
  // /login over a call that can never succeed.
  const authRequired = !isLocalMode && (await authStore.checkServerAuthEnabled());
  if (authRequired) {
    try {
      await authStore.ensureFresh();
      await authStore.loadProfile();
    } catch (err) {
      // Only redirect to /login on a real 401. On transient network errors,
      // continue mounting the layout — the tokens are still valid and
      // subsequent requests will retry naturally.
      log.warn('MainLayout mount ensureFresh/loadProfile failed', {
        sessionInvalid: isSessionInvalid(err),
        message: (err as Error)?.message ?? String(err),
      });
      if (isSessionInvalid(err)) {
        await authStore.logout();
        await router.replace({
          path: '/login',
          query: { redirect: router.currentRoute.value.fullPath || '/' },
        });
        return;
      }
    }
  }

  useWebSocket();

  // The file tree / recent files load reactively via the activeVaultId watcher
  // below, so we only need to populate the vault list here.
  await vaultsStore.loadVaults();

  if (prefsStore.prefs.editor_mode) {
    editorStore.setMode(prefsStore.prefs.editor_mode);
  }

  if (isLocalMode) {
    await Promise.all([syncStore.loadPairing(), syncStore.refreshStatus()]);
    syncStore.startPolling();
    // Android background reconcile service (#64) — a no-op everywhere else
    // (browser context, and the plugin's commands aren't even registered on
    // desktop). Started once per app launch here rather than left to the
    // user, since it's what keeps edits syncing while backgrounded.
    void startBackgroundSyncService();
  }

  window.addEventListener('keydown', onGlobalKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKeydown);
  syncStore.stopPolling();
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
