<template>
  <div
    class="tab-bar d-flex align-center"
    style="overflow-x: auto; flex-shrink: 0; border-bottom: 1px solid rgb(var(--v-theme-border));"
  >
    <div
      v-for="tab in tabs"
      :key="tab.id"
      class="tab-item d-flex align-center"
      :class="{ 'tab-active': tab.id === activeTabId }"
      @click="tabsStore.activateTab(tab.id)"
      @contextmenu.prevent="openTabMenu($event, tab.id)"
      @mousedown.middle.prevent="requestCloseTab(tab.id)"
    >
      <v-icon :icon="tabIcon(tab)" size="14" class="mr-1" />
      <span class="tab-title text-caption">{{ tab.fileName }}</span>
      <span v-if="tab.isDirty" class="tab-dirty ml-1">●</span>
      <v-btn
        icon="mdi-close"
        size="x-small"
        density="compact"
        variant="plain"
        class="ml-1 tab-close-btn"
        @click.stop="requestCloseTab(tab.id)"
      />
    </div>

    <!-- Spacer + split controls -->
    <div class="d-flex align-center ml-auto pl-2" style="flex-shrink: 0;">
      <v-btn
        v-if="tabsStore.panes.length < 4"
        icon="mdi-flip-horizontal"
        size="x-small"
        density="compact"
        variant="plain"
        title="Split pane"
        @click="tabsStore.splitPane(paneId)"
      />
      <v-btn
        v-if="tabsStore.panes.length > 1"
        icon="mdi-close"
        size="x-small"
        density="compact"
        variant="plain"
        title="Close pane"
        @click="tabsStore.closePane(paneId)"
      />
    </div>

    <v-menu v-model="tabMenuOpen" :style="{ top: tabMenuY + 'px', left: tabMenuX + 'px' }" style="position: fixed;">
      <v-list density="compact" min-width="180">
        <v-list-item title="Close" prepend-icon="mdi-close" @click="closeContextTab" />
        <v-list-item
          title="Close to the right"
          prepend-icon="mdi-arrow-collapse-right"
          :disabled="contextTabIdsToRight.length === 0"
          @click="closeTabsToRight"
        />
        <v-list-item
          title="Close other tabs"
          prepend-icon="mdi-close-box-outline"
          :disabled="contextOtherTabIds.length === 0"
          @click="closeOtherTabs"
        />
        <v-list-item
          title="Close all in pane"
          prepend-icon="mdi-close-box-multiple-outline"
          :disabled="tabs.length === 0"
          @click="closeAllTabsInPane"
        />
      </v-list>
    </v-menu>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { useTabsStore } from '@/stores/tabs';
import type { Tab } from '@/api/types';

const props = defineProps<{ paneId: string }>();
const tabsStore = useTabsStore();

const tabs = computed(() => tabsStore.tabsForPane(props.paneId));
const activeTabId = computed(() => {
  const pane = tabsStore.panes.find(p => p.id === props.paneId);
  return pane?.activeTabId;
});
const tabMenuOpen = ref(false);
const tabMenuX = ref(0);
const tabMenuY = ref(0);
const contextTabId = ref<string | null>(null);
const contextTabIdsToRight = computed(() => (
  contextTabId.value ? tabsStore.tabIdsToRight(props.paneId, contextTabId.value) : []
));
const contextOtherTabIds = computed(() => (
  contextTabId.value ? tabsStore.tabIdsExcept(props.paneId, contextTabId.value) : []
));

function requestCloseTab(tabId: string) {
  const tab = tabsStore.tabs.get(tabId);
  if (!tab) return;
  if (tab.isDirty && !confirm(`Close \"${tab.fileName}\" without saving?`)) {
    return;
  }
  tabsStore.closeTab(tabId);
}

function requestCloseTabs(tabIds: string[]) {
  const closeable: string[] = [];
  for (const tabId of tabIds) {
    const tab = tabsStore.tabs.get(tabId);
    if (!tab) continue;
    if (tab.isDirty && !confirm(`Close \"${tab.fileName}\" without saving?`)) {
      continue;
    }
    closeable.push(tabId);
  }
  tabsStore.closeTabs(closeable);
}

function openTabMenu(event: MouseEvent, tabId: string) {
  tabMenuX.value = event.clientX;
  tabMenuY.value = event.clientY;
  contextTabId.value = tabId;
  tabMenuOpen.value = true;
}

function closeContextTab() {
  if (!contextTabId.value) return;
  requestCloseTab(contextTabId.value);
}

function closeTabsToRight() {
  requestCloseTabs(contextTabIdsToRight.value);
}

function closeOtherTabs() {
  requestCloseTabs(contextOtherTabIds.value);
}

function closeAllTabsInPane() {
  requestCloseTabs(tabsStore.tabIdsInPane(props.paneId));
}

function tabIcon(tab: Tab): string {
  const ext = tab.filePath?.split('.').pop()?.toLowerCase() ?? '';
  if (ext === 'md') return 'mdi-language-markdown-outline';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(ext)) return 'mdi-image-outline';
  if (ext === 'pdf') return 'mdi-file-pdf-box';
  if (['mp4', 'webm', 'ogv', 'mov'].includes(ext)) return 'mdi-video-outline';
  if (['mp3', 'ogg', 'wav', 'flac', 'm4a'].includes(ext)) return 'mdi-music-note-outline';
  return 'mdi-file-outline';
}
</script>

<style scoped>
.tab-bar {
  min-height: 36px;
  background: rgb(var(--v-theme-surface));
  gap: 2px;
  padding: 0 4px;
}
.tab-item {
  height: 32px;
  padding: 0 8px;
  border-radius: 4px;
  cursor: pointer;
  flex-shrink: 0;
  max-width: 180px;
  transition: background 0.1s;
  color: rgb(var(--v-theme-on-background));
}
.tab-item:hover {
  background: rgba(var(--v-theme-surface-bright), 0.7);
}
.tab-item.tab-active {
  background: rgba(var(--v-theme-primary), 0.18);
}
.tab-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 120px;
}
.tab-dirty {
  color: rgb(var(--v-theme-primary));
  font-size: 10px;
  line-height: 1;
}
.tab-close-btn {
  opacity: 0;
  transition: opacity 0.1s;
}
.tab-item:hover .tab-close-btn,
.tab-item.tab-active .tab-close-btn {
  opacity: 1;
}
</style>
