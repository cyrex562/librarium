<template>
  <div class="bookmarks-panel">
    <div
      class="bookmarks-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">BOOKMARKS</span>
      <span v-if="bookmarks.length" class="text-caption text-secondary ml-1">({{ bookmarks.length }})</span>
      <v-btn
        icon="mdi-bookmark-plus-outline"
        size="x-small"
        variant="text"
        density="compact"
        class="ml-auto"
        :disabled="!activeFilePath"
        title="Bookmark current file"
        @click.stop="addBookmark"
      />
      <v-progress-circular v-if="loading" size="10" width="1" indeterminate class="ml-1" />
    </div>
    <div v-if="expanded">
      <div v-if="bookmarks.length" class="bookmarks-list">
        <div
          v-for="bm in bookmarks"
          :key="bm.id"
          class="bookmark-item d-flex align-center px-2 py-1 text-caption"
          :title="bm.path"
          @click="openFile(bm.path)"
          @contextmenu.prevent="openMenu($event, bm)"
        >
          <v-icon icon="mdi-bookmark-outline" size="x-small" class="mr-1 flex-shrink-0" color="primary" />
          <span class="text-truncate flex-1">{{ bm.title || fileName(bm.path) }}</span>
          <v-btn
            icon="mdi-close"
            size="x-small"
            variant="text"
            density="compact"
            class="ml-1 flex-shrink-0"
            title="Remove bookmark"
            @click.stop="removeBookmark(bm.id)"
          />
        </div>
      </div>
      <div v-else-if="!loading" class="pa-2 text-caption text-secondary text-center">
        No bookmarks yet. Click + to save the current note.
      </div>
    </div>

    <v-menu v-model="menuOpen" :target="menuTarget" location="end">
      <v-list density="compact">
        <v-list-item base-color="error" @click="onDeleteNote">
          <template #prepend><v-icon icon="mdi-delete-outline" size="small" /></template>
          <v-list-item-title class="text-caption">Delete note</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { useTabsStore } from '@/stores/tabs';
import { apiListBookmarks, apiCreateBookmark, apiDeleteBookmark } from '@/api/client';
import { useDeleteNote } from '@/composables/useDeleteNote';
import type { Bookmark } from '@/api/types';

const expanded = ref(true);
const loading = ref(false);
const bookmarks = ref<Bookmark[]>([]);

const vaultsStore = useVaultsStore();
const tabsStore = useTabsStore();
const { deleteNote } = useDeleteNote();

const menuOpen = ref(false);
const menuTarget = ref<[number, number]>([0, 0]);
const menuBookmark = ref<Bookmark | null>(null);

const activeFilePath = computed(() => tabsStore.activeTab?.filePath ?? null);

async function loadBookmarks(vaultId: string) {
  loading.value = true;
  try {
    bookmarks.value = await apiListBookmarks(vaultId);
  } catch {
    bookmarks.value = [];
  } finally {
    loading.value = false;
  }
}

watch(
  () => vaultsStore.activeVaultId,
  async (vaultId) => {
    if (vaultId) {
      await loadBookmarks(vaultId);
    } else {
      bookmarks.value = [];
    }
  },
  { immediate: true },
);

async function addBookmark() {
  const vaultId = vaultsStore.activeVaultId;
  const path = activeFilePath.value;
  if (!vaultId || !path) return;
  const title = fileName(path);
  try {
    await apiCreateBookmark(vaultId, path, title);
    await loadBookmarks(vaultId);
  } catch {
    // already bookmarked or error — silently ignore
  }
}

async function removeBookmark(bookmarkId: string) {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;
  try {
    await apiDeleteBookmark(vaultId, bookmarkId);
    bookmarks.value = bookmarks.value.filter((b) => b.id !== bookmarkId);
  } catch {
    // no-op
  }
}

function fileName(path: string): string {
  return path.split('/').pop()?.replace(/\.md$/, '') ?? path;
}

function openFile(path: string) {
  tabsStore.openTab(tabsStore.activePaneId, path, fileName(path));
}

function openMenu(e: MouseEvent, bm: Bookmark) {
  menuBookmark.value = bm;
  menuTarget.value = [e.clientX, e.clientY];
  menuOpen.value = true;
}

async function onDeleteNote() {
  menuOpen.value = false;
  const bm = menuBookmark.value;
  if (!bm) return;
  const deleted = await deleteNote(bm.path);
  // The note is gone, so drop its now-dangling bookmark too.
  if (deleted) await removeBookmark(bm.id);
}
</script>

<style scoped>
.bookmarks-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.bookmark-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.bookmark-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
