<template>
  <div class="doc-meta-bar">
    <!-- Filename row -->
    <div class="d-flex align-center" style="min-height: 28px; gap: 6px;">
      <v-icon icon="mdi-file-outline" size="14" class="meta-icon" />
      <span
        v-if="!editingName"
        class="doc-filename text-body-2"
        :title="filePath"
        @click="startEditName"
      >{{ fileName }}</span>
      <v-text-field
        v-else
        v-model="nameDraft"
        autofocus
        density="compact"
        variant="plain"
        hide-details
        class="doc-name-field"
        @keyup.enter="commitName"
        @keyup.esc="cancelEditName"
        @blur="commitName"
        @click.stop
      />
      <v-icon
        v-if="!editingName"
        icon="mdi-pencil-outline"
        size="14"
        class="meta-icon edit-hint"
        @click="startEditName"
      />
    </div>

    <v-alert
      v-if="renameError"
      type="error"
      density="compact"
      variant="tonal"
      closable
      class="mt-1"
      @click:close="renameError = ''"
    >{{ renameError }}</v-alert>

    <!-- Tags row (notes: Markdown frontmatter, Typst metadata block) -->
    <div v-if="isMd" class="d-flex align-center flex-wrap" style="min-height: 26px; gap: 4px; margin-top: 4px;">
      <v-icon icon="mdi-tag-multiple-outline" size="14" class="meta-icon" />
      <v-chip
        v-for="(tag, i) in tags"
        :key="tag + i"
        size="x-small"
        closable
        variant="tonal"
        @click:close="removeTag(i)"
      >{{ tag }}</v-chip>
      <v-text-field
        v-if="addingTag"
        v-model="newTagDraft"
        autofocus
        density="compact"
        variant="plain"
        hide-details
        placeholder="tag name…"
        class="tag-input"
        @keyup.enter="commitTag"
        @keyup.esc="cancelTag"
        @blur="cancelTag"
      />
      <v-btn
        v-else
        size="x-small"
        variant="text"
        icon="mdi-plus"
        density="compact"
        title="Add tag"
        @click="addingTag = true"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { usePreferencesStore } from '@/stores/preferences';

const props = defineProps<{
  filePath: string;
  frontmatter: Record<string, unknown>;
  isMd: boolean;
}>();

const emit = defineEmits<{
  'update:frontmatter': [value: Record<string, unknown>];
}>();

const vaultsStore = useVaultsStore();
const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const prefsStore = usePreferencesStore();

// ── Filename ────────────────────────────────────────────────────────────────

const editingName = ref(false);
const nameDraft = ref('');
const renameError = ref('');
const isCommitting = ref(false);

const fileName = computed(() => {
  const idx = props.filePath.lastIndexOf('/');
  return idx >= 0 ? props.filePath.slice(idx + 1) : props.filePath;
});

function startEditName() {
  nameDraft.value = fileName.value;
  renameError.value = '';
  editingName.value = true;
}

function cancelEditName() {
  editingName.value = false;
}

async function commitName() {
  // Re-entrancy guard: the field is wired to both @keyup.enter and @blur so
  // clicking away still commits. Pressing Enter sets editingName to false
  // below *before* the rename call resolves, which unmounts this focused
  // <v-text-field> on the next render — unmounting a focused element fires a
  // native blur, re-entering commitName a second time while the first
  // await is still pending. Without this guard that second call re-sends
  // the identical rename request; whichever one lands second finds the
  // source already gone and surfaces a spurious "Source not found" error
  // even though the rename itself succeeded.
  if (isCommitting.value) return;

  const trimmed = nameDraft.value.trim();
  if (!trimmed || trimmed === fileName.value) {
    editingName.value = false;
    return;
  }
  isCommitting.value = true;
  editingName.value = false;

  try {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId) return;

    const dir = props.filePath.includes('/')
      ? props.filePath.substring(0, props.filePath.lastIndexOf('/') + 1)
      : '';
    const oldPath = props.filePath;
    const newPath = dir + trimmed;

    const resolvedNewPath = await filesStore.renameFile(vaultId, oldPath, newPath);
    tabsStore.remapTabPaths(oldPath, resolvedNewPath);
    prefsStore.remapPathIcon(oldPath, resolvedNewPath);
    await prefsStore.save();
  } catch (e: any) {
    renameError.value = e?.message ?? 'Rename failed.';
  } finally {
    isCommitting.value = false;
  }
}

// ── Tags ────────────────────────────────────────────────────────────────────

const addingTag = ref(false);
const newTagDraft = ref('');

const tags = computed<string[]>(() => {
  const raw = props.frontmatter?.tags;
  if (Array.isArray(raw)) return raw.map(String);
  if (typeof raw === 'string' && raw.trim()) return [raw.trim()];
  return [];
});

// Reset addingTag when the file changes so stale state doesn't linger.
watch(() => props.filePath, () => {
  addingTag.value = false;
  newTagDraft.value = '';
});

function emitTags(next: string[]) {
  emit('update:frontmatter', { ...props.frontmatter, tags: next });
}

function removeTag(index: number) {
  const next = [...tags.value];
  next.splice(index, 1);
  emitTags(next);
}

function commitTag() {
  const t = newTagDraft.value.trim();
  if (t && !tags.value.includes(t)) {
    emitTags([...tags.value, t]);
  }
  cancelTag();
}

function cancelTag() {
  newTagDraft.value = '';
  addingTag.value = false;
}
</script>

<style scoped>
.doc-meta-bar {
  flex-shrink: 0;
  padding: 6px 16px 5px;
  border-bottom: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
}

.meta-icon {
  flex-shrink: 0;
  color: rgb(var(--v-theme-secondary));
}

.edit-hint {
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s;
}
.doc-meta-bar:hover .edit-hint {
  opacity: 0.55;
}

.doc-filename {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  color: rgb(var(--v-theme-on-background));
}

.doc-name-field {
  flex: 1;
  font-size: 13px;
}

.tag-input {
  width: 100px;
  font-size: 12px;
  flex-shrink: 0;
}
</style>
