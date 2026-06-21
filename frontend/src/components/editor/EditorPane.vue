<template>
  <div class="editor-pane" style="flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column;">
    <!-- Welcome/empty state -->
    <div v-if="!activeTab" class="d-flex flex-column align-center justify-center" style="flex: 1; color: rgb(var(--v-theme-secondary));">
      <v-icon icon="mdi-note-text-outline" size="64" style="opacity: 0.25;" />
      <p class="mt-4 text-caption">Open a file from the sidebar to start editing.</p>
    </div>

    <!-- File content router -->
    <template v-else>
      <!-- Frontmatter panel above editor -->
      <FrontmatterPanel
        v-if="isMd"
        :frontmatter="activeTab.frontmatter ?? {}"
        :tab-id="activeTab.id"
        @update:frontmatter="onFrontmatterUpdate"
      />

      <!-- Editor toolbar (formatting + mode toggle) -->
      <EditorToolbar
        v-if="isMd"
        :mode="normalizedEditorMode"
        @command="onToolbarCommand"
        @mode-change="onModeChange"
      />

      <!-- Markdown: mode-aware editor -->
      <div v-if="isMd && normalizedEditorMode !== 'structural'" class="d-flex" style="flex: 1; min-height: 0; overflow: hidden;">
        <!-- Editor visible in plain/formatted modes -->
        <MarkdownEditor
          v-if="normalizedEditorMode !== 'fully_rendered'"
          ref="markdownEditorRef"
          :tab-id="activeTab.id"
          :content="activeTab.content ?? ''"
          :file-path="activeTab.filePath"
          :mode="normalizedEditorMode"
          class="editor-column"
          @update="onEditorUpdate"
        />
        <!-- Preview only in preview mode -->
        <MarkdownPreview
          v-if="normalizedEditorMode === 'fully_rendered'"
          :content="activeTab.content ?? ''"
          :vault-id="vaultsStore.activeVaultId ?? ''"
          :current-file="activeTab.filePath"
          class="editor-column"
        />
      </div>

      <!-- Structural entity editor -->
      <StructuralEditor
        v-else-if="isMd && normalizedEditorMode === 'structural'"
        :tab-id="activeTab.id"
        style="flex: 1; min-height: 0;"
      />

      <!-- Image viewer -->
      <ImageViewer v-else-if="isImage" :vault-id="vaultsStore.activeVaultId ?? ''" :path="activeTab.filePath ?? ''" />

      <!-- PDF viewer -->
      <PdfViewer v-else-if="isPdf" :vault-id="vaultsStore.activeVaultId ?? ''" :path="activeTab.filePath ?? ''" />

      <!-- Audio/Video viewer -->
      <AudioVideoViewer v-else-if="isAv" :vault-id="vaultsStore.activeVaultId ?? ''" :path="activeTab.filePath ?? ''" />

      <!-- Graph view -->
      <GraphView v-else-if="isGraph" :vault-id="graphVaultId" style="flex: 1; min-height: 0;" />

      <!-- Canvas view -->
      <CanvasView
        v-else-if="isCanvas"
        :vault-id="vaultsStore.activeVaultId ?? ''"
        :file-path="activeTab.filePath"
        style="flex: 1; min-height: 0;"
      />

      <!-- Generic binary notice -->
      <div v-else class="d-flex align-center justify-center" style="flex: 1;">
        <span class="text-caption text-secondary">Binary file — cannot be edited here.</span>
      </div>

      <!-- Word count status bar (markdown only) -->
      <div
        v-if="isMd"
        class="word-count-bar d-flex align-center px-2"
        style="border-top: 1px solid rgb(var(--v-theme-border)); background: rgb(var(--v-theme-surface));"
      >
        <span class="text-caption text-secondary">{{ wordCount }} words · {{ charCount }} characters</span>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, defineAsyncComponent } from 'vue';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { useEditorStore } from '@/stores/editor';
import { usePreferencesStore } from '@/stores/preferences';
import { useFilesStore } from '@/stores/files';
import { ApiError } from '@/api/client';
import { useUiStore } from '@/stores/ui';
import type { EditorMode } from '@/api/types';

import FrontmatterPanel from './FrontmatterPanel.vue';
import MarkdownEditor from './MarkdownEditor.vue';
import EditorToolbar from './EditorToolbar.vue';
const MarkdownPreview = defineAsyncComponent(() => import('./MarkdownPreview.vue'));
const StructuralEditor = defineAsyncComponent(() => import('./StructuralEditor.vue'));
const ImageViewer = defineAsyncComponent(() => import('@/components/viewers/ImageViewer.vue'));
const PdfViewer = defineAsyncComponent(() => import('@/components/viewers/PdfViewer.vue'));
const AudioVideoViewer = defineAsyncComponent(() => import('@/components/viewers/AudioVideoViewer.vue'));
const GraphView = defineAsyncComponent(() => import('@/components/graph/GraphView.vue'));
const CanvasView = defineAsyncComponent(() => import('@/components/graph/CanvasView.vue'));

const props = defineProps<{ paneId: string }>();

const tabsStore = useTabsStore();
const markdownEditorRef = ref<InstanceType<typeof MarkdownEditor> | null>(null);

const vaultsStore = useVaultsStore();
const editorStore = useEditorStore();
const prefsStore = usePreferencesStore();
const filesStore = useFilesStore();
const uiStore = useUiStore();

const activeTab = computed(() => {
  const pane = tabsStore.panes.find(p => p.id === props.paneId);
  if (!pane?.activeTabId) return null;
  return tabsStore.tabs.get(pane.activeTabId) ?? null;
});

const normalizedEditorMode = computed<EditorMode>(() => {
  // Legacy compatibility: old split mode now maps to formatted text mode.
  if (editorStore.mode === 'side_by_side') return 'formatted_raw';
  return editorStore.mode;
});

const ext = computed(() => activeTab.value?.filePath?.split('.').pop()?.toLowerCase() ?? '');
const isMd = computed(() => ext.value === 'md');
const isImage = computed(() => ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(ext.value));
const isPdf = computed(() => ext.value === 'pdf');
const isAv = computed(() => ['mp4', 'webm', 'ogv', 'mov', 'mp3', 'ogg', 'wav', 'flac', 'm4a'].includes(ext.value));
const isGraph = computed(() => activeTab.value?.fileType === 'graph');
const isCanvas = computed(() => activeTab.value?.fileType === 'canvas');
const graphVaultId = computed(() => {
  const fp = activeTab.value?.filePath ?? '';
  return fp.startsWith('__graph__:') ? fp.slice('__graph__:'.length) : '';
});

const wordCount = computed(() => {
  const text = activeTab.value?.content ?? '';
  return text.trim() === '' ? 0 : text.trim().split(/\s+/).length;
});

const charCount = computed(() => (activeTab.value?.content ?? '').length);

watch(activeTab, async (tab) => {
  if (!tab || tab.content !== '' || !tab.filePath) return;
  // Graph and canvas tabs manage their own content loading
  if (tab.fileType === 'graph' || tab.fileType === 'canvas') return;
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;
  const fc = await filesStore.readFile(vaultId, tab.filePath);
  tabsStore.updateTabContent(tab.id, fc.content);
  tabsStore.markTabClean(tab.id, fc.modified ?? '');
  if (fc.frontmatter && Object.keys(fc.frontmatter).length > 0) {
    tabsStore.updateTabFrontmatter(tab.id, fc.frontmatter);
    tabsStore.markTabClean(tab.id, fc.modified ?? '');
  }
}, { immediate: true });

function onEditorUpdate(newContent: string) {
  const tab = activeTab.value;
  if (!tab) return;
  tabsStore.updateTabContent(tab.id, newContent);
  scheduleTabSave(tab.id, newContent, tab.frontmatter ?? {});
}

function onFrontmatterUpdate(frontmatter: Record<string, unknown>) {
  const tab = activeTab.value;
  if (!tab) return;
  tabsStore.updateTabFrontmatter(tab.id, frontmatter);
  scheduleTabSave(tab.id, tab.content ?? '', frontmatter);
}

function scheduleTabSave(tabId: string, content: string, frontmatter: Record<string, unknown>) {
  // Schedule auto-save in 2s
  editorStore.scheduleAutoSave(tabId, 2000, async () => {
    const vaultId = vaultsStore.activeVaultId;
    const tab = tabsStore.tabs.get(tabId);
    if (!vaultId || !tab?.filePath) return;
    try {
      const saved = await filesStore.writeFile(vaultId, tab.filePath, {
        content,
        last_modified: tab.modified || undefined,
        frontmatter,
      });
      tabsStore.markTabClean(tab.id, saved.modified);
    } catch (error) {
      if (error instanceof ApiError && error.status === 409) {
        const latest = await filesStore.readFile(vaultId, tab.filePath);
        uiStore.openConflictResolver({
          tabId: tab.id,
          filePath: tab.filePath,
          yourVersion: content,
          serverVersion: latest.content,
          serverModified: latest.modified,
        });
        return;
      }
      throw error;
    }
  });
}

function onModeChange(value: EditorMode | null) {
  if (!value) return;
  editorStore.setMode(value);
  // 'structural' is a UI-only mode; it is not persisted to the backend.
  if (value !== 'structural') {
    prefsStore.set('editor_mode', value);
    void prefsStore.save();
  }
}

function onToolbarCommand(cmd: string) {
  if (cmd === 'undo') { markdownEditorRef.value?.callUndo(); return; }
  if (cmd === 'redo') { markdownEditorRef.value?.callRedo(); return; }
  if (cmd === 'collapse_all_folds') { markdownEditorRef.value?.collapseAllFolds(); return; }
  if (cmd === 'expand_all_folds') { markdownEditorRef.value?.expandAllFolds(); return; }
  markdownEditorRef.value?.applyCommand(cmd as any);
}
</script>

<style scoped>
.editor-column {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: auto;
}
.word-count-bar {
  height: 22px;
  flex-shrink: 0;
  font-size: 11px;
}
</style>
