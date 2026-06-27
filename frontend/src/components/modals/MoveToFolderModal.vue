<template>
  <v-dialog :model-value="modelValue" max-width="540" @update:model-value="emit('update:modelValue', $event)">
    <v-card>
      <v-card-title class="text-subtitle-1 d-flex align-center">
        <v-icon icon="mdi-folder-move-outline" size="small" class="mr-2" />
        Move {{ sourceLabel }}
      </v-card-title>
      <v-card-subtitle class="text-caption">Pick a destination folder.</v-card-subtitle>

      <v-card-text>
        <v-text-field
          v-model="filter"
          density="compact"
          placeholder="Filter folders…"
          prepend-inner-icon="mdi-magnify"
          hide-details
          clearable
          class="mb-2"
        />
        <div class="folder-list">
          <div
            v-for="folder in visibleFolders"
            :key="folder.path"
            class="folder-row d-flex align-center px-2 py-1 text-caption"
            :class="{ selected: selected === folder.path, disabled: folder.disabled }"
            :style="{ paddingLeft: folder.depth * 16 + 8 + 'px' }"
            :title="folder.disabled ? 'Not a valid destination' : folder.path || '(vault root)'"
            @click="!folder.disabled && (selected = folder.path)"
          >
            <v-icon
              :icon="folder.path === '' ? 'mdi-folder-home-outline' : 'mdi-folder-outline'"
              size="16"
              class="mr-1 flex-shrink-0"
              color="secondary"
            />
            <span class="text-truncate">{{ folder.label }}</span>
          </div>
          <div v-if="visibleFolders.length === 0" class="pa-3 text-center text-secondary text-caption">
            No matching folders.
          </div>
        </div>
      </v-card-text>

      <v-card-actions>
        <v-alert v-if="error" type="error" density="compact" variant="tonal" class="flex-1 mr-2 mb-0 py-1">{{ error }}</v-alert>
        <v-spacer />
        <v-btn variant="text" size="small" @click="emit('update:modelValue', false)">Cancel</v-btn>
        <v-btn
          color="primary"
          variant="flat"
          size="small"
          :disabled="selected === null || moving"
          :loading="moving"
          @click="doMove"
        >
          Move here
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import type { FileNode } from '@/api/types';
import { ApiError } from '@/api/client';
import { useFilesStore } from '@/stores/files';
import { useVaultsStore } from '@/stores/vaults';
import { useTabsStore } from '@/stores/tabs';
import { usePreferencesStore } from '@/stores/preferences';

const props = defineProps<{ modelValue: boolean; sourcePaths: string[] }>();
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>();

const filesStore = useFilesStore();
const vaultsStore = useVaultsStore();
const tabsStore = useTabsStore();
const prefsStore = usePreferencesStore();

const filter = ref('');
const selected = ref<string | null>(null);
const moving = ref(false);
const error = ref('');

// Reset state each time the dialog opens.
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      filter.value = '';
      selected.value = null;
      error.value = '';
    }
  },
);

const sourceLabel = computed(() => {
  if (props.sourcePaths.length === 1) {
    const p = props.sourcePaths[0];
    return `“${p.split('/').pop() ?? p}”`;
  }
  return `${props.sourcePaths.length} items`;
});

interface FolderRow {
  path: string;
  label: string;
  depth: number;
  disabled: boolean;
}

// Flatten the vault's directory tree (plus the vault root) into an indented list.
const allFolders = computed<FolderRow[]>(() => {
  const rows: FolderRow[] = [
    { path: '', label: '/ (vault root)', depth: 0, disabled: isInvalidDest('') },
  ];
  const walk = (nodes: FileNode[], depth: number) => {
    const dirs = [...nodes]
      .filter((n) => n.is_directory)
      .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
    for (const dir of dirs) {
      rows.push({
        path: dir.path,
        label: dir.name,
        depth: depth + 1,
        disabled: isInvalidDest(dir.path),
      });
      if (dir.children) walk(dir.children, depth + 1);
    }
  };
  walk(filesStore.tree, 0);
  return rows;
});

const visibleFolders = computed(() => {
  const q = filter.value.trim().toLowerCase();
  if (!q) return allFolders.value;
  return allFolders.value.filter(
    (f) => f.label.toLowerCase().includes(q) || f.path.toLowerCase().includes(q),
  );
});

// A destination is invalid if it IS a source or a descendant of one (you can't
// move a folder into itself or its own subtree).
function isInvalidDest(dest: string): boolean {
  return props.sourcePaths.some((src) => dest === src || dest.startsWith(`${src}/`));
}

function basename(path: string) {
  const idx = path.lastIndexOf('/');
  return idx >= 0 ? path.slice(idx + 1) : path;
}

async function doMove() {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId || selected.value === null) return;
  const destDir = selected.value;

  if (isInvalidDest(destDir)) {
    error.value = 'You cannot move a folder into itself or one of its descendants.';
    return;
  }

  let moves = filesStore
    .buildMoveTargets(props.sourcePaths, destDir)
    .filter((m) => m.to !== m.from);
  if (moves.length === 0) {
    // Everything is already in the chosen folder.
    emit('update:modelValue', false);
    return;
  }

  let strategy: 'fail' | 'overwrite' | 'rename' = 'fail';
  const conflicts = moves.filter((m) => filesStore.destinationExists(m.to));
  if (conflicts.length > 0) {
    const names = conflicts.map((m) => basename(m.from)).slice(0, 8).join(', ');
    const keepBoth = confirm(
      `${conflicts.length} item(s) already exist in the destination (${names}${
        conflicts.length > 8 ? ', …' : ''
      }).\n\nOK = keep both (auto-rename the moved copies). Cancel = abort the move.`,
    );
    if (!keepBoth) return;
    strategy = 'rename';
  }

  moving.value = true;
  error.value = '';
  try {
    const completed = await filesStore.moveFiles(vaultId, moves, strategy);
    completed.forEach((m) => {
      tabsStore.remapTabPaths(m.from, m.to);
      prefsStore.remapPathIcon(m.from, m.to);
    });
    await prefsStore.save();
    emit('update:modelValue', false);
  } catch (e) {
    error.value =
      e instanceof ApiError && e.status === 409
        ? 'A file or folder already exists in the destination.'
        : e instanceof Error
          ? e.message
          : 'Failed to move.';
  } finally {
    moving.value = false;
  }
}
</script>

<style scoped>
.folder-list {
  max-height: 46vh;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 8px;
}
.folder-row {
  cursor: pointer;
  user-select: none;
  border-radius: 4px;
}
.folder-row:hover:not(.disabled) {
  background: rgba(var(--v-theme-surface-bright), 0.6);
}
.folder-row.selected {
  background: rgba(var(--v-theme-primary), 0.18);
  outline: 1px solid rgba(var(--v-theme-primary), 0.6);
}
.folder-row.disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
