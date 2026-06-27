<template>
  <div class="app-status-bar">
    <span
      v-if="activePath"
      class="status-path text-truncate"
      :title="activePath"
    >{{ activePath }}</span>
    <div class="status-spacer" />
    <transition name="status-fade">
      <div v-if="showIndexing" class="status-indexing" title="The background indexer is updating the search index">
        <v-progress-circular indeterminate size="12" width="2" color="primary" />
        <span class="ml-2">Indexing…</span>
      </div>
    </transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue';
import { useIndexingStore } from '@/stores/indexing';
import { useVaultsStore } from '@/stores/vaults';
import { useTabsStore } from '@/stores/tabs';

const indexingStore = useIndexingStore();
const vaultsStore = useVaultsStore();
const tabsStore = useTabsStore();

// Full vault-relative path of the active note (LIB-079).
const activePath = computed(() => tabsStore.activeTab?.filePath ?? '');

const active = computed(() => indexingStore.isIndexing(vaultsStore.activeVaultId));

// Linger briefly after indexing stops so rapid start/stop bursts (e.g. the
// watcher batch loop committing several batches in a row) don't flicker the
// indicator on and off.
const showIndexing = ref(false);
let hideTimer: ReturnType<typeof setTimeout> | null = null;

watch(
  active,
  (on) => {
    if (on) {
      if (hideTimer) {
        clearTimeout(hideTimer);
        hideTimer = null;
      }
      showIndexing.value = true;
    } else if (showIndexing.value && !hideTimer) {
      hideTimer = setTimeout(() => {
        showIndexing.value = false;
        hideTimer = null;
      }, 600);
    }
  },
  { immediate: true },
);

onUnmounted(() => {
  if (hideTimer) clearTimeout(hideTimer);
});
</script>

<style scoped>
.app-status-bar {
  display: flex;
  align-items: center;
  flex: 0 0 auto;
  height: 22px;
  padding: 0 10px;
  font-size: 12px;
  border-top: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
}

.status-path {
  flex: 0 1 auto;
  min-width: 0;
  color: rgba(var(--v-theme-on-surface), 0.65);
}

.status-spacer {
  flex: 1 1 auto;
}

.status-indexing {
  display: flex;
  align-items: center;
  color: rgba(var(--v-theme-on-surface), 0.75);
}

.status-fade-enter-active,
.status-fade-leave-active {
  transition: opacity 0.2s ease;
}

.status-fade-enter-from,
.status-fade-leave-to {
  opacity: 0;
}
</style>
