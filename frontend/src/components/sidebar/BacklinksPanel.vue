<template>
  <div class="backlinks-panel">
    <div
      class="backlinks-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">BACKLINKS</span>
      <span v-if="backlinks.length" class="text-caption text-secondary ml-1">({{ backlinks.length }})</span>
      <v-progress-circular v-if="loading" size="10" width="1" indeterminate class="ml-auto" />
    </div>
    <div v-if="expanded">
      <div v-if="backlinks.length" class="backlinks-list">
        <div
          v-for="link in backlinks"
          :key="link.path"
          class="backlink-item d-flex align-center px-2 py-1 text-caption"
          :title="link.path"
          @click="openFile(link.path)"
        >
          <v-icon icon="mdi-arrow-left" size="x-small" class="mr-1 flex-shrink-0" color="primary" />
          <span class="text-truncate">{{ link.title || fileName(link.path) }}</span>
        </div>
      </div>
      <div v-else-if="!loading" class="pa-2 text-caption text-secondary text-center">
        No notes link to this file yet.
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { useTabsStore } from '@/stores/tabs';
import { apiGetBacklinks } from '@/api/client';
import type { BacklinkEntry } from '@/api/types';

const props = defineProps<{ filePath: string }>();

const expanded = ref(false); // start collapsed
const loading = ref(false);
const backlinks = ref<BacklinkEntry[]>([]);

const vaultsStore = useVaultsStore();
const tabsStore = useTabsStore();

watch(
  () => props.filePath,
  async (path) => {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId || !path) {
      backlinks.value = [];
      return;
    }
    loading.value = true;
    try {
      backlinks.value = await apiGetBacklinks(vaultId, path);
    } catch {
      backlinks.value = [];
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

function fileName(path: string): string {
  return path.split('/').pop()?.replace(/\.md$/, '') ?? path;
}

function openFile(path: string) {
  tabsStore.openTab(tabsStore.activePaneId, path, fileName(path));
}
</script>

<style scoped>
.backlinks-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.backlink-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.backlink-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
