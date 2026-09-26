<template>
  <div class="favorites-panel">
    <div
      class="favorites-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">FAVORITES</span>
      <span v-if="favorites.length" class="text-caption text-secondary ml-1">({{ favorites.length }})</span>
      <v-btn
        :icon="activeIsFavorite ? 'mdi-star' : 'mdi-star-plus-outline'"
        size="x-small"
        variant="text"
        density="compact"
        class="ml-auto"
        :disabled="!activeFilePath"
        :title="activeIsFavorite ? 'Remove current note from favorites' : 'Add current note to favorites'"
        @click.stop="toggleActive"
      />
    </div>
    <div v-if="expanded">
      <div v-if="favorites.length" class="favorites-list">
        <div
          v-for="fav in favorites"
          :key="fav.path"
          class="favorite-item d-flex align-center px-2 py-1 text-caption"
          :title="fav.path"
          @click="openFile(fav.path)"
        >
          <v-icon icon="mdi-star" size="x-small" class="mr-1 flex-shrink-0" color="warning" />
          <span class="text-truncate flex-1">{{ displayName(fav.path) }}</span>
          <v-btn
            icon="mdi-close"
            size="x-small"
            variant="text"
            density="compact"
            class="ml-1 flex-shrink-0"
            title="Remove from favorites"
            @click.stop="removeFavorite(fav.path)"
          />
        </div>
      </div>
      <div v-else class="pa-2 text-caption text-secondary text-center">
        No favorites yet. Click the star to save the current note.
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, watch } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { useTabsStore } from '@/stores/tabs';
import { useFavoritesStore } from '@/stores/favorites';

const vaultsStore = useVaultsStore();
const tabsStore = useTabsStore();
const favoritesStore = useFavoritesStore();

// Expanded state is per-mount (not persisted). Matches BookmarksPanel /
// RecentFilesPanel behavior; a session-scoped default is enough.
import { ref } from 'vue';
const expanded = ref(false); // start collapsed

const activeFilePath = computed(() => tabsStore.activeTab?.filePath ?? null);

const favorites = computed(() => {
  const vaultId = vaultsStore.activeVaultId;
  return vaultId ? favoritesStore.favoritesForVault(vaultId) : [];
});

const activeIsFavorite = computed(() => {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId || !activeFilePath.value) return false;
  return favoritesStore.isFavorite(vaultId, activeFilePath.value);
});

// Load favorites whenever the active vault changes. Errors are swallowed —
// the panel just renders empty; the star toggle will surface API errors
// through its own catch below.
watch(
  () => vaultsStore.activeVaultId,
  async (vaultId) => {
    if (!vaultId) return;
    if (favoritesStore.loaded.has(vaultId)) return;
    try {
      await favoritesStore.load(vaultId);
    } catch {
      /* no-op */
    }
  },
  { immediate: true },
);

async function toggleActive() {
  const vaultId = vaultsStore.activeVaultId;
  const path = activeFilePath.value;
  if (!vaultId || !path) return;
  try {
    await favoritesStore.toggle(vaultId, path);
  } catch {
    /* no-op */
  }
}

async function removeFavorite(path: string) {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;
  try {
    await favoritesStore.remove(vaultId, path);
  } catch {
    /* no-op */
  }
}

function displayName(path: string): string {
  return path.split('/').pop()?.replace(/\.md$/, '') ?? path;
}

function openFile(path: string) {
  tabsStore.openTab(tabsStore.activePaneId, path, displayName(path));
}
</script>

<style scoped>
.favorites-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.favorite-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.favorite-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-warning));
}
</style>
