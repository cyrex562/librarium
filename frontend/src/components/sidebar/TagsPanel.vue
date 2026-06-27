<template>
  <div class="tags-panel">
    <div
      class="tags-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">TAGS</span>
      <span v-if="tags.length" class="text-caption text-secondary ml-1">({{ tags.length }})</span>
      <v-progress-circular v-if="loading" size="10" width="1" indeterminate class="ml-auto" />
    </div>
    <div v-if="expanded">
      <div
        v-if="undoState"
        class="d-flex align-center px-2 py-1 text-caption"
        style="background: rgba(var(--v-theme-warning), 0.12); border-bottom: 1px solid rgb(var(--v-theme-border));"
      >
        <span class="text-truncate flex-1">Deleted “#{{ undoState.tag }}” from {{ undoState.count }} note(s).</span>
        <v-btn size="x-small" variant="text" color="primary" :loading="undoing" @click="undoDelete">Undo</v-btn>
        <v-btn size="x-small" variant="text" icon="mdi-close" @click="undoState = null" />
      </div>
      <div v-if="tags.length" class="tags-list">
        <div
          v-for="entry in sortedTags"
          :key="entry.tag"
          class="tag-item d-flex align-center px-2 py-1 text-caption"
          :title="`Search ${entry.tag} (${entry.count} note(s)) — right-click to delete`"
          @click="searchTag(entry.tag)"
          @contextmenu.prevent="openMenu($event, entry.tag)"
        >
          <v-icon icon="mdi-tag-outline" size="x-small" class="mr-1 flex-shrink-0" color="secondary" />
          <span class="text-truncate flex-1">{{ entry.tag }}</span>
          <span class="text-secondary ml-1">{{ entry.count }}</span>
        </div>
      </div>
      <div v-else-if="!loading" class="pa-2 text-caption text-secondary text-center">
        No tags found in this vault.
      </div>
    </div>

    <v-menu v-model="menuOpen" :target="menuTarget" location="end">
      <v-list density="compact">
        <v-list-item base-color="error" :disabled="deleting" @click="deleteTag">
          <template #prepend>
            <v-icon icon="mdi-delete-outline" size="small" />
          </template>
          <v-list-item-title class="text-caption">Delete tag “{{ menuTag }}”</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { apiListTags, apiDeleteTag, apiUndoMlGroup } from '@/api/client';
import type { TagEntry } from '@/api/types';

const emit = defineEmits<{ search: [query: string] }>();

const expanded = ref(true);
const loading = ref(false);
const tags = ref<TagEntry[]>([]);

// Undo affordance shown after a vault-wide tag delete.
const undoState = ref<{ groupId: string; tag: string; count: number } | null>(null);
const undoing = ref(false);

const menuOpen = ref(false);
const menuTarget = ref<[number, number]>([0, 0]);
const menuTag = ref('');
const deleting = ref(false);

const vaultsStore = useVaultsStore();

const sortedTags = computed(() =>
  [...tags.value].sort((a, b) => (b.count - a.count) || a.tag.localeCompare(b.tag)),
);

watch(
  () => vaultsStore.activeVaultId,
  async (vaultId) => {
    if (!vaultId) {
      tags.value = [];
      return;
    }
    loading.value = true;
    try {
      tags.value = await apiListTags(vaultId);
    } catch {
      tags.value = [];
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

function searchTag(tag: string) {
  const normalized = tag.startsWith('#') ? tag : `#${tag}`;
  emit('search', normalized);
}

function openMenu(e: MouseEvent, tag: string) {
  menuTag.value = tag;
  menuTarget.value = [e.clientX, e.clientY];
  menuOpen.value = true;
}

async function deleteTag() {
  menuOpen.value = false;
  const vaultId = vaultsStore.activeVaultId;
  const tag = menuTag.value;
  if (!vaultId || !tag) return;
  deleting.value = true;
  try {
    // Preview the impact (no writes) so the user sees exactly how many notes a
    // vault-wide rewrite will touch before committing.
    const preview = await apiDeleteTag(vaultId, tag, true);
    if (preview.count === 0) {
      alert(`"${tag}" isn't used in any note (outside code blocks). Nothing to delete.`);
      return;
    }
    if (
      !confirm(
        `Delete the tag "${tag}" from ${preview.count} note(s)? ` +
          `This rewrites frontmatter and inline #${tag} across the vault. You can undo it right after.`,
      )
    ) {
      return;
    }
    const result = await apiDeleteTag(vaultId, tag, false);
    if (result.group_id) {
      undoState.value = { groupId: result.group_id, tag, count: result.files_modified };
    }
    tags.value = await apiListTags(vaultId);
  } catch {
    // Best-effort: the tag list reload below reflects the actual state.
    try {
      tags.value = await apiListTags(vaultId);
    } catch {
      /* ignore */
    }
  } finally {
    deleting.value = false;
  }
}

async function undoDelete() {
  const vaultId = vaultsStore.activeVaultId;
  const info = undoState.value;
  if (!vaultId || !info) return;
  undoing.value = true;
  try {
    await apiUndoMlGroup(vaultId, info.groupId);
    undoState.value = null;
    tags.value = await apiListTags(vaultId);
  } catch {
    // Keep the banner so the user can retry the undo.
  } finally {
    undoing.value = false;
  }
}
</script>

<style scoped>
.tags-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}
.tag-item {
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface));
  border-left: 2px solid transparent;
}
.tag-item:hover {
  background: rgb(var(--v-theme-surface-variant));
  border-left-color: rgb(var(--v-theme-primary));
}
</style>
