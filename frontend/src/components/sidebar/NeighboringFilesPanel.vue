<template>
  <div class="neighboring-files-panel">
    <div
      class="neighboring-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">NEIGHBORING FILES</span>
    </div>
    <div v-if="expanded">
      <div v-if="prev || next" class="neighboring-list">
        <div
          v-if="prev"
          class="neighbor-item d-flex align-center px-2 py-1 text-caption"
          :title="prev"
          @click="openFile(prev)"
          @contextmenu.prevent="openMenu($event, prev)"
        >
          <v-icon icon="mdi-arrow-up" size="x-small" class="mr-1 flex-shrink-0" color="secondary" />
          <span class="text-truncate">{{ fileName(prev) }}</span>
        </div>
        <div
          v-if="next"
          class="neighbor-item d-flex align-center px-2 py-1 text-caption"
          :title="next"
          @click="openFile(next)"
          @contextmenu.prevent="openMenu($event, next)"
        >
          <v-icon icon="mdi-arrow-down" size="x-small" class="mr-1 flex-shrink-0" color="secondary" />
          <span class="text-truncate">{{ fileName(next) }}</span>
        </div>
      </div>
      <div v-else class="pa-2 text-caption text-secondary text-center">
        No previous or next markdown file.
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
import { ref, computed } from 'vue';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { useDeleteNote } from '@/composables/useDeleteNote';
import type { FileNode } from '@/api/types';

const props = defineProps<{ filePath: string }>();

const expanded = ref(true);
const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const { deleteNote } = useDeleteNote();

const menuOpen = ref(false);
const menuTarget = ref<[number, number]>([0, 0]);
const menuPath = ref('');

function openMenu(e: MouseEvent, path: string) {
  menuPath.value = path;
  menuTarget.value = [e.clientX, e.clientY];
  menuOpen.value = true;
}

async function onDelete() {
  menuOpen.value = false;
  if (menuPath.value) await deleteNote(menuPath.value);
}

function flattenTree(nodes: FileNode[]): string[] {
  const result: string[] = [];
  for (const node of nodes) {
    if (node.is_directory && node.children) {
      result.push(...flattenTree(node.children));
    } else if (!node.is_directory && node.path.endsWith('.md')) {
      result.push(node.path);
    }
  }
  return result;
}

const allFiles = computed(() => flattenTree(filesStore.tree));

const currentIndex = computed(() => allFiles.value.indexOf(props.filePath));

const prev = computed(() =>
  currentIndex.value > 0 ? allFiles.value[currentIndex.value - 1] : null,
);

const next = computed(() =>
  currentIndex.value >= 0 && currentIndex.value < allFiles.value.length - 1
    ? allFiles.value[currentIndex.value + 1]
    : null,
);

function fileName(path: string): string {
  return path.split('/').pop()?.replace(/\.md$/, '') ?? path;
}

function openFile(path: string) {
  tabsStore.openTab(tabsStore.activePaneId, path, fileName(path));
}
</script>

<style scoped>
.neighboring-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.neighbor-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.neighbor-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
