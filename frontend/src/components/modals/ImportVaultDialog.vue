<template>
  <v-dialog
    :model-value="modelValue"
    max-width="760"
    persistent
    no-click-animation
    @update:model-value="onDialogToggle"
    @keydown.esc.stop="onEscClose"
  >
    <v-card
      @dragenter.prevent="onCardDragEnter"
      @dragover.prevent="onCardDragOver"
      @dragleave.prevent="onCardDragLeave"
      @drop.prevent="onCardDrop"
    >
      <v-card-title class="d-flex align-center justify-space-between">
        <span>Import files and folders</span>
        <v-chip size="small" variant="tonal" color="primary">
          {{ summaryLabel }}
        </v-chip>
      </v-card-title>

      <v-card-text>
        <v-alert v-if="error" type="error" variant="tonal" class="mb-3">
          {{ error }}
        </v-alert>
        <v-alert v-if="success" type="success" variant="tonal" class="mb-3">
          <div>{{ success }}</div>
          <div v-if="skippedPaths.length > 0" class="mt-2">
            <div class="text-caption font-weight-medium mb-1">Skipped files</div>
            <ul class="skipped-list">
              <li v-for="path in skippedPaths" :key="path">{{ path }}</li>
            </ul>
          </div>
        </v-alert>

        <v-combobox
          v-model="targetPath"
          v-model:search="targetSearch"
          label="Target folder inside vault"
          :items="folderOptions"
          hint="Leave blank to import into the vault root. Dropping onto a folder pre-fills this for you."
          persistent-hint
          density="comfortable"
          variant="outlined"
          clearable
          prepend-inner-icon="mdi-folder-outline"
          @update:search="onTargetSearchUpdate"
        />

        <v-list density="compact" class="folder-picker mb-3" data-testid="import-folder-picker">
          <v-list-subheader>Folders</v-list-subheader>
          <v-list-item
            v-for="folder in folderTargets"
            :key="folder.path || '__root__'"
            :title="folder.title"
            :subtitle="folder.path || 'Vault root'"
            prepend-icon="mdi-folder-outline"
            :active="normalizedTargetPath === folder.path"
            @click="selectTargetFolder(folder.path)"
          />
        </v-list>

        <div
          class="import-dropzone pa-6 mb-3"
          :class="{ 'import-dropzone-active': dragging }"
          data-testid="import-dropzone"
          @dragenter.prevent.stop="onDragEnter"
          @dragover.prevent.stop="onDragOver"
          @dragleave.prevent.stop="onDragLeave"
          @drop.prevent.stop="onDrop"
        >
          <v-icon icon="mdi-tray-arrow-up" size="40" color="primary" class="mb-2" />
          <div class="text-subtitle-2 mb-1">Drag files or whole folders here</div>
          <div class="text-body-2 text-medium-emphasis mb-4">
            Folder structure is preserved automatically during import.
          </div>

          <div class="d-flex flex-wrap ga-2 justify-center">
            <v-btn color="primary" variant="tonal" @click="pickFiles">Choose files</v-btn>
            <v-btn color="primary" variant="outlined" @click="pickFolder">Choose folder</v-btn>
            <v-btn variant="text" :disabled="entries.length === 0 || importing" @click="clearEntries">Clear list</v-btn>
          </div>
        </div>

        <input
          ref="filesInput"
          data-testid="import-files-input"
          type="file"
          multiple
          style="display: none"
          @change="onFilesSelected"
        />
        <input
          ref="folderInput"
          data-testid="import-folder-input"
          type="file"
          multiple
          webkitdirectory
          directory
          style="display: none"
          @change="onFolderSelected"
        />

        <div class="d-flex align-center justify-space-between mb-2">
          <div class="text-caption text-medium-emphasis">
            {{ entries.length }} item<span v-if="entries.length !== 1">s</span> queued · {{ formattedTotalSize }}
            <span v-if="archiveCount > 0" class="ml-1 text-primary">({{ archiveCount }} archive<span v-if="archiveCount !== 1">s</span> will be extracted)</span>
          </div>
          <div class="text-caption text-medium-emphasis" v-if="normalizedTargetPath">
            Importing into <code>{{ normalizedTargetPath }}</code>
          </div>
        </div>

        <v-select
          v-model="conflictStrategy"
          label="If a file already exists"
          :items="conflictOptions"
          density="comfortable"
          variant="outlined"
          class="mb-2"
        />

        <v-list v-if="entries.length > 0" density="compact" class="import-list mb-3">
          <v-list-item
            v-for="entry in entriesPreview"
            :key="entry.relativePath + entry.file.lastModified"
            :title="entry.relativePath"
            :subtitle="formatBytes(entry.file.size)"
          />
          <v-list-item v-if="entries.length > entriesPreview.length" :title="`+${entries.length - entriesPreview.length} more`" />
        </v-list>
        <div v-else class="text-body-2 text-medium-emphasis mb-3">
          No files queued yet. Pick files, pick a folder, or drop them into the box above.
        </div>

        <div v-if="importing || progress" class="mt-2">
          <div class="d-flex justify-space-between text-caption mb-1">
            <span>{{ progressLabel }}</span>
            <span>{{ percentage }}%</span>
          </div>
          <v-progress-linear :model-value="percentage" color="primary" height="10" rounded />
        </div>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn v-if="!importing" @click="close">Close</v-btn>
        <v-btn v-else color="warning" variant="tonal" @click="requestCancelImport">Cancel transfer</v-btn>
        <v-btn
          color="primary"
          :loading="importing"
          :disabled="entries.length === 0 || !vaultsStore.activeVaultId"
          @click="startImport"
        >
          Import {{ entries.length > 0 ? entries.length : '' }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useFilesStore } from '@/stores/files';
import { useUiStore } from '@/stores/ui';
import { useVaultsStore } from '@/stores/vaults';
import type { FileNode, ImportCandidate, ImportProgress } from '@/api/types';
import {
  createImportCandidatesFromDataTransfer,
  createImportCandidatesFromFileList,
  hasFilePayload,
  normalizeImportPath,
} from '@/utils/importEntries';

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>();

const filesStore = useFilesStore();
const uiStore = useUiStore();
const vaultsStore = useVaultsStore();

const filesInput = ref<HTMLInputElement | null>(null);
const folderInput = ref<HTMLInputElement | null>(null);
const dragging = ref(false);
const importing = ref(false);
const error = ref('');
const success = ref('');
const skippedPaths = ref<string[]>([]);
const progress = ref<ImportProgress | null>(null);
const targetPath = ref('');
const targetSearch = ref('');
const conflictStrategy = ref<'fail' | 'overwrite' | 'skip' | 'rename_with_timestamp'>('rename_with_timestamp');
let importAbortController: AbortController | null = null;

const conflictOptions = [
    { title: 'Keep both (append timestamp)', value: 'rename_with_timestamp' },
    { title: 'Overwrite existing file', value: 'overwrite' },
    { title: 'Skip existing file', value: 'skip' },
    { title: 'Stop with an error', value: 'fail' },
];

interface FolderTarget {
  path: string;
  title: string;
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      targetPath.value = uiStore.importTargetPath;
      targetSearch.value = uiStore.importTargetPath;
      error.value = '';
      success.value = '';
      skippedPaths.value = [];
    }
  },
  { immediate: true },
);

watch(targetPath, (value) => {
  uiStore.importTargetPath = normalizeImportPath(coerceTargetPath(value));
});

const entries = computed(() => uiStore.importEntries);
const entriesPreview = computed(() => entries.value.slice(0, 12));
const totalSize = computed(() => entries.value.reduce((sum, entry) => sum + entry.file.size, 0));
const formattedTotalSize = computed(() => formatBytes(totalSize.value));
const normalizedTargetPath = computed(() => normalizeImportPath(coerceTargetPath(targetPath.value || targetSearch.value)));
const folderTargets = computed<FolderTarget[]>(() => [
  { path: '', title: 'Vault root' },
  ...collectFolderTargets(filesStore.tree),
]);
const folderOptions = computed(() => folderTargets.value.map((folder) => folder.path));
const archiveCount = computed(() =>
    entries.value.filter((e) => {
        const name = e.file.name.toLowerCase();
        return name.endsWith('.zip') || name.endsWith('.tar') || name.endsWith('.tar.gz') || name.endsWith('.tgz');
    }).length,
);
const summaryLabel = computed(() => {
  if (entries.value.length === 0) return 'Ready';
  return `${entries.value.length} queued`;
});
const percentage = computed(() => {
  if (!progress.value || progress.value.totalBytes === 0) return 0;
  return Math.max(0, Math.min(100, Math.round((progress.value.uploadedBytes / progress.value.totalBytes) * 100)));
});
const progressLabel = computed(() => {
  if (!progress.value) return 'Waiting to import';
  const current = progress.value.currentFile ? ` · ${progress.value.currentFile}` : '';
  return `Imported ${progress.value.completedFiles}/${progress.value.totalFiles}${current}`;
});

function onDialogToggle(value: boolean) {
  if (!value && importing.value) return;
  emit('update:modelValue', value);
  if (!value) {
    uiStore.closeImportDialog();
  }
}

function close() {
  onDialogToggle(false);
}

function onEscClose() {
  if (importing.value) {
    requestCancelImport();
    return;
  }
  close();
}

function requestCancelImport() {
  if (!importing.value || !importAbortController) return;
  if (!confirm('Cancel the current transfer? Files already uploaded will remain in the vault.')) return;
  importAbortController.abort();
}

function collectFolderTargets(nodes: FileNode[]): FolderTarget[] {
  const folders: FolderTarget[] = [];
  const visit = (node: FileNode) => {
    if (!node.is_directory) return;
    folders.push({ path: node.path, title: node.name });
    for (const child of node.children ?? []) {
      visit(child);
    }
  };
  for (const node of nodes) {
    visit(node);
  }
  return folders.sort((a, b) => a.path.localeCompare(b.path));
}

function selectTargetFolder(path: string) {
  targetPath.value = path;
  targetSearch.value = path;
}

function onTargetSearchUpdate(value: string | null) {
  targetSearch.value = value ?? '';
  targetPath.value = targetSearch.value;
}

function coerceTargetPath(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function pickFiles() {
  filesInput.value?.click();
}

function pickFolder() {
  folderInput.value?.click();
}

function clearEntries() {
  uiStore.clearImportEntries();
  success.value = '';
  error.value = '';
  skippedPaths.value = [];
}

function queueEntries(nextEntries: ImportCandidate[]) {
  if (nextEntries.length === 0) return;
  uiStore.setImportEntries(dedupeEntries([...entries.value, ...nextEntries]));
  success.value = '';
  error.value = '';
  skippedPaths.value = [];
}

function onFilesSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;
  queueEntries(createImportCandidatesFromFileList(input.files));
  input.value = '';
}

function onFolderSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;
  queueEntries(createImportCandidatesFromFileList(input.files));
  input.value = '';
}

function onDragEnter(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer)) return;
  dragging.value = true;
}

function onDragOver(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer)) return;
  dragging.value = true;
}

function onDragLeave(e: DragEvent) {
  const nextTarget = e.relatedTarget as Node | null;
  if (nextTarget && (e.currentTarget as HTMLElement | null)?.contains(nextTarget)) {
    return;
  }
  dragging.value = false;
}

async function onDrop(e: DragEvent) {
  dragging.value = false;
  if (!e.dataTransfer || !hasFilePayload(e.dataTransfer)) return;
  queueEntries(await createImportCandidatesFromDataTransfer(e.dataTransfer));
}

function onCardDragEnter(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer)) return;
  dragging.value = true;
}

function onCardDragOver(e: DragEvent) {
  if (!hasFilePayload(e.dataTransfer)) return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
  dragging.value = true;
}

function onCardDragLeave(e: DragEvent) {
  const nextTarget = e.relatedTarget as Node | null;
  if (nextTarget && (e.currentTarget as HTMLElement | null)?.contains(nextTarget)) {
    return;
  }
  dragging.value = false;
}

async function onCardDrop(e: DragEvent) {
  await onDrop(e);
}

async function startImport() {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId || entries.value.length === 0) return;

  importing.value = true;
  error.value = '';
  success.value = '';
  skippedPaths.value = [];
  progress.value = null;
  importAbortController = new AbortController();

  try {
    const result = await filesStore.importCandidates(
      vaultId,
      entries.value,
      normalizedTargetPath.value,
      (nextProgress) => {
        progress.value = nextProgress;
      },
      conflictStrategy.value,
      importAbortController.signal,
    );

    skippedPaths.value = result.skipped.map((item) => item.path);
    const importedLabel = `${result.uploaded.length} file${result.uploaded.length === 1 ? '' : 's'}`;
    if (result.skipped.length > 0) {
      success.value = `Imported ${importedLabel}; skipped ${result.skipped.length} existing file${result.skipped.length === 1 ? '' : 's'}.`;
    } else {
      success.value = `Imported ${importedLabel} successfully.`;
    }
    uiStore.clearImportEntries();
  } catch (e: any) {
    if (e?.name === 'AbortError') {
      error.value = 'Import canceled.';
    } else {
      error.value = e?.message ?? 'Import failed.';
    }
  } finally {
    importing.value = false;
    importAbortController = null;
  }
}

function formatBytes(value: number): string {
  if (value === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const exponent = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const size = value / 1024 ** exponent;
  return `${size.toFixed(size >= 10 || exponent === 0 ? 0 : 1)} ${units[exponent]}`;
}

function dedupeEntries(nextEntries: ImportCandidate[]): ImportCandidate[] {
  const seen = new Set<string>();
  return nextEntries.filter((entry) => {
    const key = [entry.relativePath, entry.file.size, entry.file.lastModified].join('::');
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
</script>

<style scoped>
.import-dropzone {
  border: 2px dashed rgba(var(--v-theme-primary), 0.35);
  border-radius: 12px;
  text-align: center;
  background: rgba(var(--v-theme-primary), 0.04);
  transition: background 0.15s ease, border-color 0.15s ease;
}

.import-dropzone-active {
  background: rgba(var(--v-theme-primary), 0.1);
  border-color: rgba(var(--v-theme-primary), 0.7);
}

.import-list {
  max-height: 260px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 8px;
}

.folder-picker {
  max-height: 170px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 8px;
}

.skipped-list {
  max-height: 140px;
  overflow-y: auto;
  padding-left: 1rem;
}
</style>
