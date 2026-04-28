<template>
  <div
    class="file-tree pa-1"
    :class="{ 'file-tree-drop-target': draggingFiles }"
    @dragenter.prevent="onDragEnter"
    @dragover.prevent="onDragOver"
    @dragleave.prevent="onDragLeave"
    @drop.prevent="onDropRoot"
  >
    <FileTreeNode
      v-for="node in sortedTree"
      :key="node.path"
      :node="node"
      :depth="0"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, inject, provide, ref } from 'vue';
import type { Ref } from 'vue';
import type { FileNode } from '@/api/types';
import { ApiError } from '@/api/client';
import { useFilesStore } from '@/stores/files';
import { useUiStore } from '@/stores/ui';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { usePreferencesStore } from '@/stores/preferences';
import { createImportCandidatesFromDataTransfer, hasFilePayload } from '@/utils/importEntries';
import { getFileTreeDragItems, getFileTreeDragPayload, hasFileTreeDragPayload } from '@/utils/fileTreeDrag';
import FileTreeNode from './FileTreeNode.vue';

const filesStore = useFilesStore();
const uiStore = useUiStore();
const tabsStore = useTabsStore();
const vaultsStore = useVaultsStore();
const prefsStore = usePreferencesStore();
const sort = inject<Ref<'asc' | 'desc'>>('fileTreeSort', ref('asc'));
const draggingFiles = ref(false);

function basename(path: string) {
  const idx = path.lastIndexOf('/');
  return idx >= 0 ? path.slice(idx + 1) : path;
}

function chooseConflictStrategy(conflictingNames: string[]): 'skip' | 'overwrite' | 'rename' | 'cancel' {
  const choice = prompt(
    [
      `${conflictingNames.length} item${conflictingNames.length === 1 ? '' : 's'} already exist at the vault root.`,
      'Choose the default behavior: skip, overwrite, rename, or cancel.',
      `Conflicts: ${conflictingNames.slice(0, 8).join(', ')}${conflictingNames.length > 8 ? ', ...' : ''}`,
    ].join('\n'),
    'skip',
  );
  const normalized = choice?.trim().toLowerCase();
  if (!normalized || normalized === 'cancel' || normalized === 'c') return 'cancel';
  if (normalized === 'skip' || normalized === 's') return 'skip';
  if (normalized === 'overwrite' || normalized === 'o') return 'overwrite';
  if (normalized === 'rename' || normalized === 'r' || normalized === 'autorename') return 'rename';
  alert('Move cancelled: expected skip, overwrite, rename, or cancel.');
  return 'cancel';
}

const sortedTree = computed(() => {
  const nodes = [...filesStore.tree];
  return sortNodes(nodes, sort.value);
});

function sortNodes(nodes: FileNode[], dir: 'asc' | 'desc') {
  return [...nodes].sort((a, b) => {
    // Directories first
    if (a.is_directory && !b.is_directory) return -1;
    if (!a.is_directory && b.is_directory) return 1;
    const nameA = a.name.toLowerCase();
    const nameB = b.name.toLowerCase();
    const cmp = nameA.localeCompare(nameB);
    return dir === 'asc' ? cmp : -cmp;
  });
}

function flattenTreePaths(nodes: FileNode[]): string[] {
  return nodes.flatMap((node) => [
    node.path,
    ...(node.children ? flattenTreePaths(sortNodes(node.children, sort.value)) : []),
  ]);
}

const selectionOrder = computed(() => flattenTreePaths(sortedTree.value));

provide('fileTreeSelectionOrder', selectionOrder);

function onDragEnter(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer) && !hasFileTreeDragPayload(e.dataTransfer)) return;
  draggingFiles.value = true;
}

function onDragOver(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer) && !hasFileTreeDragPayload(e.dataTransfer)) return;
  draggingFiles.value = true;

  if (hasFileTreeDragPayload(e.dataTransfer) && e.dataTransfer) {
    e.dataTransfer.dropEffect = 'move';
  }
}

function onDragLeave(e: DragEvent) {
  const nextTarget = e.relatedTarget as Node | null;
  if (nextTarget && (e.currentTarget as HTMLElement | null)?.contains(nextTarget)) {
    return;
  }
  draggingFiles.value = false;
}

async function onDropRoot(e: DragEvent) {
  draggingFiles.value = false;
  const internalPayload = getFileTreeDragPayload(e.dataTransfer);
  if (internalPayload) {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId) return;

    let moves = filesStore
      .buildMoveTargets(getFileTreeDragItems(internalPayload).map((item) => item.path), '')
      .filter((move) => move.to !== move.from);
    if (moves.length === 0) return;

    let strategy: 'fail' | 'overwrite' | 'rename' = 'fail';
    const conflicts = moves.filter((move) => filesStore.destinationExists(move.to));
    if (conflicts.length > 0) {
      const conflictStrategy = chooseConflictStrategy(conflicts.map((move) => basename(move.from)));
      if (conflictStrategy === 'cancel') return;
      if (conflictStrategy === 'skip') {
        moves = moves.filter((move) => !filesStore.destinationExists(move.to));
        if (moves.length === 0) return;
      } else {
        strategy = conflictStrategy;
      }
    }

    try {
      const completed = await filesStore.moveFiles(vaultId, moves, strategy);
      completed.forEach((move) => {
        tabsStore.remapTabPaths(move.from, move.to);
        prefsStore.remapPathIcon(move.from, move.to);
      });
      await prefsStore.save();
    } catch (error) {
      if (error instanceof ApiError && error.status === 409) {
        alert('A file or folder already exists at the vault root.');
        return;
      }
      throw error;
    }
    return;
  }

  if (!e.dataTransfer || !hasFilePayload(e.dataTransfer)) return;
  const candidates = await createImportCandidatesFromDataTransfer(e.dataTransfer);
  if (candidates.length > 0) {
    uiStore.openImportDialog({ targetPath: '', entries: candidates });
  }
}
</script>

<style scoped>
.file-tree {
  user-select: none;
}

.file-tree-drop-target {
  background: rgba(var(--v-theme-primary), 0.08);
  outline: 1px dashed rgba(var(--v-theme-primary), 0.55);
  outline-offset: -2px;
  border-radius: 8px;
}
</style>
