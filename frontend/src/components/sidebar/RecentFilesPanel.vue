<template>
  <div class="recent-files-panel">
    <div
      class="recent-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">RECENT FILES</span>
    </div>
    <div v-if="expanded">
      <div v-if="filesStore.recentFiles.length" class="recent-list">
        <div
          v-for="filePath in filesStore.recentFiles.slice(0, 20)"
          :key="filePath"
          class="recent-item d-flex align-center px-2 py-1 text-caption"
          :title="filePath"
          @click="openFile(filePath)"
          @contextmenu.prevent="openMenu($event, filePath)"
        >
          <v-icon icon="mdi-file-document-outline" size="x-small" class="mr-1 flex-shrink-0" color="secondary" />
          <span class="text-truncate">{{ fileName(filePath) }}</span>
        </div>
      </div>
      <div v-else class="pa-2 text-caption text-secondary text-center">
        No recent files
      </div>
    </div>

    <v-menu v-model="menuOpen" :target="menuTarget" location="end">
      <v-list density="compact">
        <v-list-item base-color="error" @click="onDelete">
          <template #prepend><v-icon icon="mdi-delete-outline" size="small" /></template>
          <v-list-item-title class="text-caption">Delete note</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useDeleteNote } from '@/composables/useDeleteNote';

const expanded = ref(true);
const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const { deleteNote } = useDeleteNote();

const menuOpen = ref(false);
const menuTarget = ref<[number, number]>([0, 0]);
const menuPath = ref('');

function fileName(filePath: string): string {
  return filePath.split('/').pop() ?? filePath;
}

function openFile(filePath: string) {
  tabsStore.openTab(tabsStore.activePaneId, filePath, fileName(filePath));
}

function openMenu(e: MouseEvent, filePath: string) {
  menuPath.value = filePath;
  menuTarget.value = [e.clientX, e.clientY];
  menuOpen.value = true;
}

async function onDelete() {
  menuOpen.value = false;
  if (menuPath.value) await deleteNote(menuPath.value);
}
</script>

<style scoped>
.recent-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.recent-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.recent-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
