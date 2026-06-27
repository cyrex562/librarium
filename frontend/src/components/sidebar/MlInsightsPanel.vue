<template>
  <div class="ml-insights-panel">
    <div
      class="ml-header d-flex align-center px-2 py-1"
      style="cursor: pointer; border-bottom: 1px solid rgb(var(--v-theme-border));"
      @click="expanded = !expanded"
    >
      <v-icon :icon="expanded ? 'mdi-chevron-down' : 'mdi-chevron-right'" size="x-small" />
      <span class="text-caption text-secondary ml-1 font-weight-medium">AI INSIGHTS</span>
      <v-chip size="x-small" variant="tonal" color="primary" class="ml-2">suggest-only</v-chip>
      <v-progress-circular v-if="loading" size="10" width="1" indeterminate class="ml-auto" />
    </div>

    <div v-if="expanded" class="px-2 py-2">
      <div v-if="!vaultId || !filePath" class="text-caption text-secondary text-center py-2">
        Open a markdown note to generate outline and organization suggestions.
      </div>

      <template v-else>
        <div class="d-flex flex-wrap ga-2 mb-2">
          <v-btn
            size="x-small"
            color="primary"
            variant="tonal"
            prepend-icon="mdi-format-list-bulleted"
            :disabled="loading"
            @click="runOutline"
          >
            Generate outline
          </v-btn>
          <v-btn
            size="x-small"
            color="primary"
            variant="outlined"
            prepend-icon="mdi-lightbulb-outline"
            :disabled="loading"
            @click="runSuggestions"
          >
            Suggest organization
          </v-btn>
          <v-btn
            size="x-small"
            color="primary"
            variant="outlined"
            prepend-icon="mdi-rename-box"
            :loading="renameLoading"
            :disabled="loading"
            @click="runRename"
          >
            Suggest rename
          </v-btn>
          <v-btn
            size="x-small"
            color="primary"
            variant="text"
            prepend-icon="mdi-folder-cog-outline"
            :disabled="loading || !vaultId"
            @click="organizeOpen = true"
          >
            Organize vault…
          </v-btn>
        </div>

        <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mb-2">
          {{ error }}
        </v-alert>
        <v-alert v-if="applyMessage" type="info" variant="tonal" density="compact" class="mb-2">
          {{ applyMessage }}
        </v-alert>

        <div v-if="outline" class="mb-3">
          <div class="text-caption text-secondary font-weight-medium mb-1">Outline summary</div>
          <div class="text-body-2 mb-2">{{ outline.summary }}</div>

          <div v-if="outline.sections.length" class="outline-list">
            <div
              v-for="(section, index) in outline.sections"
              :key="`${section.line_number}-${index}`"
              class="outline-item text-caption"
              :style="{ paddingLeft: `${(section.level - 1) * 10 + 4}px` }"
            >
              <span class="text-truncate d-block">{{ section.title }}</span>
            </div>
          </div>
          <div v-else class="text-caption text-secondary">No headings detected.</div>
        </div>

        <div v-if="analysis && analysis.keyphrases.length" class="mb-3">
          <div class="text-caption text-secondary font-weight-medium mb-1">Key phrases</div>
          <div class="d-flex flex-wrap ga-1">
            <v-chip
              v-for="kp in analysis.keyphrases"
              :key="kp.phrase"
              size="x-small"
              variant="tonal"
              color="secondary"
              :title="`relevance ${Math.round(kp.score * 100)}%`"
            >
              {{ kp.phrase }}
            </v-chip>
          </div>
        </div>

        <div v-if="renameSuggestion" class="mb-3">
          <div class="text-caption text-secondary font-weight-medium mb-1">Rename</div>
          <div class="suggestion-item pa-2">
            <template v-if="renameSuggestion.suggestion && renameSuggestion.proposed_name">
              <div class="text-caption mb-1">{{ renameSuggestion.rationale }}</div>
              <div class="text-caption d-flex align-center ga-1 mb-1">
                <span class="text-disabled">{{ renameSuggestion.current_name }}</span>
                <v-icon icon="mdi-arrow-right" size="x-small" />
                <span class="font-weight-medium">{{ renameSuggestion.proposed_name }}</span>
              </div>
              <div
                v-if="dryRunLinkCounts.get(renameSuggestion.suggestion.id) !== undefined"
                class="text-caption text-info mb-1"
              >
                {{ dryRunLinkCounts.get(renameSuggestion.suggestion.id) }} inbound link(s) will be updated.
              </div>
              <div class="d-flex ga-2 mt-2">
                <v-btn
                  size="x-small"
                  variant="outlined"
                  color="primary"
                  :loading="applyingSuggestionId === `${renameSuggestion.suggestion.id}:dry`"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="applySuggestion(renameSuggestion.suggestion, true)"
                >
                  Dry run
                </v-btn>
                <v-btn
                  size="x-small"
                  variant="tonal"
                  color="primary"
                  :loading="applyingSuggestionId === `${renameSuggestion.suggestion.id}:apply`"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="applySuggestion(renameSuggestion.suggestion, false)"
                >
                  Apply
                </v-btn>
                <v-btn
                  v-if="undoReceipts.get(renameSuggestion.suggestion.id)"
                  size="x-small"
                  variant="tonal"
                  color="warning"
                  :loading="undoingSuggestionId === renameSuggestion.suggestion.id"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="undoSuggestion(renameSuggestion.suggestion)"
                >
                  Undo
                </v-btn>
              </div>
            </template>
            <div v-else class="text-caption text-secondary">{{ renameSuggestion.rationale }}</div>
          </div>
        </div>

        <div v-if="suggestions" class="mb-1">
          <div class="text-caption text-secondary font-weight-medium mb-1">Organization suggestions</div>

          <div v-if="suggestions.suggestions.length" class="d-flex flex-column ga-1">
            <div
              v-for="suggestion in suggestions.suggestions"
              :key="suggestion.id"
              class="suggestion-item pa-2"
            >
              <div class="d-flex align-center justify-space-between mb-1">
                <div class="d-flex align-center ga-1">
                  <v-chip size="x-small" color="primary" variant="tonal">{{ formatKind(suggestion.kind) }}</v-chip>
                  <v-chip v-if="formatSource(suggestion.source)" size="x-small" variant="outlined" color="secondary">
                    {{ formatSource(suggestion.source) }}
                  </v-chip>
                </div>
                <span class="text-caption text-secondary">{{ formatConfidence(suggestion.confidence) }}</span>
              </div>
              <div class="text-caption mb-1">
                {{ suggestion.rationale }}
              </div>
              <div class="text-caption font-weight-medium" v-if="suggestion.tag">#{{ suggestion.tag }}</div>
              <div class="text-caption font-weight-medium" v-else-if="suggestion.category">Category: {{ suggestion.category }}</div>
              <div class="text-caption font-weight-medium" v-else-if="suggestion.target_folder">Folder: {{ suggestion.target_folder }}</div>

              <div class="d-flex ga-2 mt-2">
                <v-btn
                  size="x-small"
                  variant="outlined"
                  color="primary"
                  :loading="applyingSuggestionId === `${suggestion.id}:dry`"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="applySuggestion(suggestion, true)"
                >
                  Dry run
                </v-btn>
                <v-btn
                  size="x-small"
                  variant="tonal"
                  color="primary"
                  :loading="applyingSuggestionId === `${suggestion.id}:apply`"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="applySuggestion(suggestion, false)"
                >
                  Apply
                </v-btn>
                <v-btn
                  v-if="undoReceipts.get(suggestion.id)"
                  size="x-small"
                  variant="tonal"
                  color="warning"
                  :loading="undoingSuggestionId === suggestion.id"
                  :disabled="loading || !!applyingSuggestionId || !!undoingSuggestionId"
                  @click="undoSuggestion(suggestion)"
                >
                  Undo
                </v-btn>
              </div>
            </div>
          </div>

          <div v-else class="text-caption text-secondary">No suggestions generated for this note.</div>

          <div v-if="suggestions.existing_tags.length" class="mt-2">
            <div class="text-caption text-secondary mb-1">Existing tags</div>
            <div class="d-flex flex-wrap ga-1">
              <v-chip
                v-for="tag in suggestions.existing_tags"
                :key="tag"
                size="x-small"
                variant="outlined"
              >
                #{{ tag }}
              </v-chip>
            </div>
          </div>
        </div>
      </template>
    </div>

    <OrganizeVaultModal
      v-model="organizeOpen"
      :vault-id="vaultId"
      @applied="onOrganizeApplied"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue';
import {
  apiAnalyzeNote,
  apiApplyOrganizationSuggestion,
  apiGenerateOrganizationSuggestions,
  apiGenerateOutline,
  apiRenameSuggestion,
  apiUndoMlAction,
  ApiError,
} from '@/api/client';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import { usePreferencesStore } from '@/stores/preferences';
import OrganizeVaultModal from './OrganizeVaultModal.vue';
import type {
  ApplyOrganizationSuggestionResponse,
  FileContent,
  NoteAnalysis,
  NoteOutlineResponse,
  OrganizationSuggestion,
  OrganizationSuggestionKind,
  OrganizationSuggestionsResponse,
  OrganizationSuggestionSource,
  RenameSuggestionResponse,
  UndoMlActionResponse,
} from '@/api/types';

const props = defineProps<{
  vaultId: string | null;
  filePath: string;
  content: string;
}>();

const filesStore = useFilesStore();
const tabsStore = useTabsStore();
const prefsStore = usePreferencesStore();

// Default expanded so the AI actions (outline / suggest organization / rename /
// organize vault) are visible without having to discover and click the header.
const expanded = ref(true);
const loading = ref(false);
const applyingSuggestionId = ref<string | null>(null);
const undoingSuggestionId = ref<string | null>(null);
const error = ref('');
const applyMessage = ref('');
const outline = ref<NoteOutlineResponse | null>(null);
const suggestions = ref<OrganizationSuggestionsResponse | null>(null);
const analysis = ref<NoteAnalysis | null>(null);
const renameSuggestion = ref<RenameSuggestionResponse | null>(null);
const renameLoading = ref(false);
const organizeOpen = ref(false);
const undoReceipts = ref<Map<string, string>>(new Map());
// Inbound-link counts captured from a rename dry-run, keyed by suggestion id.
const dryRunLinkCounts = ref<Map<string, number>>(new Map());

watch(
  () => [props.vaultId, props.filePath],
  () => {
    error.value = '';
    applyMessage.value = '';
    outline.value = null;
    suggestions.value = null;
    analysis.value = null;
    renameSuggestion.value = null;
    undoReceipts.value = new Map();
    dryRunLinkCounts.value = new Map();
  },
);

function formatKind(kind: OrganizationSuggestionKind): string {
  if (kind === 'move_to_folder') return 'Move to folder';
  if (kind === 'category') return 'Category';
  if (kind === 'rename') return 'Rename';
  return 'Tag';
}

function formatSource(source?: OrganizationSuggestionSource): string {
  if (source === 'semantic') return 'semantic';
  if (source === 'keyphrase') return 'keyphrase';
  if (source === 'rule') return 'rule';
  return '';
}

function formatConfidence(value: number): string {
  const pct = Math.max(0, Math.min(100, Math.round(value * 100)));
  return `${pct}% confidence`;
}

async function runOutline() {
  if (!props.vaultId || !props.filePath) return;
  loading.value = true;
  error.value = '';
  try {
    outline.value = await apiGenerateOutline(props.vaultId, {
      file_path: props.filePath,
      content: props.content,
      max_sections: 24,
    });
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to generate outline.';
  } finally {
    loading.value = false;
  }
}

async function runSuggestions() {
  if (!props.vaultId || !props.filePath) return;
  loading.value = true;
  error.value = '';
  try {
    const [s, a] = await Promise.all([
      apiGenerateOrganizationSuggestions(props.vaultId, {
        file_path: props.filePath,
        content: props.content,
        max_suggestions: 8,
      }),
      apiAnalyzeNote(props.vaultId, { file_path: props.filePath, content: props.content }).catch(
        () => null,
      ),
    ]);
    suggestions.value = s;
    analysis.value = a;
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to generate suggestions.';
  } finally {
    loading.value = false;
  }
}

async function runRename() {
  if (!props.vaultId || !props.filePath) return;
  renameLoading.value = true;
  error.value = '';
  try {
    renameSuggestion.value = await apiRenameSuggestion(props.vaultId, {
      file_path: props.filePath,
      content: props.content,
    });
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to suggest a name.';
  } finally {
    renameLoading.value = false;
  }
}

function formatApplyResult(result: ApplyOrganizationSuggestionResponse): string {
  const action = result.dry_run ? 'Dry run' : result.applied ? 'Applied' : 'No changes applied';
  const details = result.changes.map((c) => c.description).join('; ');
  const moved = result.updated_file_path ? ` New path: ${result.updated_file_path}.` : '';
  return `${action}: ${details || 'No changes.'}.${moved}`;
}

function hydrateOpenTabsFromFile(filePath: string, file: FileContent) {
  tabsStore.tabs.forEach((tab) => {
    if (tab.filePath !== filePath) return;

    tabsStore.updateTabContent(tab.id, file.content);
    tabsStore.updateTabFrontmatter(tab.id, file.frontmatter ?? {});
    tabsStore.markTabClean(tab.id, file.modified);
  });
}

async function refreshUiAfterMlMutation(originalPath: string, updatedPath?: string) {
  if (!props.vaultId) return;

  const destinationPath = updatedPath && updatedPath.trim().length > 0
    ? updatedPath
    : originalPath;

  if (destinationPath !== originalPath) {
    tabsStore.remapTabPaths(originalPath, destinationPath);
    prefsStore.remapPathIcon(originalPath, destinationPath);
    await prefsStore.save();
  }

  await filesStore.loadTree(props.vaultId);

  const openTabAtDestination = Array.from(tabsStore.tabs.values())
    .some((tab) => tab.filePath === destinationPath);

  if (openTabAtDestination) {
    const latest = await filesStore.readFile(props.vaultId, destinationPath);
    hydrateOpenTabsFromFile(destinationPath, latest);
    await filesStore.recordRecentFile(props.vaultId, destinationPath);
  }

  await nextTick();

  if (outline.value) {
    await runOutline();
  }

  if (suggestions.value) {
    await runSuggestions();
  }
}

async function applySuggestion(suggestion: OrganizationSuggestion, dryRun: boolean) {
  if (!props.vaultId || !props.filePath) return;

  error.value = '';
  applyMessage.value = '';
  applyingSuggestionId.value = `${suggestion.id}:${dryRun ? 'dry' : 'apply'}`;

  try {
    const result = await apiApplyOrganizationSuggestion(
      props.vaultId,
      props.filePath,
      suggestion,
      dryRun,
    );
    applyMessage.value = formatApplyResult(result);
    if (dryRun && typeof result.updated_links === 'number') {
      const counts = new Map(dryRunLinkCounts.value);
      counts.set(suggestion.id, result.updated_links);
      dryRunLinkCounts.value = counts;
    }
    if (!dryRun && result.receipt_id) {
      const newMap = new Map(undoReceipts.value);
      newMap.set(suggestion.id, result.receipt_id);
      undoReceipts.value = newMap;
    } else if (!dryRun) {
      const newMap = new Map(undoReceipts.value);
      newMap.delete(suggestion.id);
      undoReceipts.value = newMap;
    }

    if (!dryRun && result.applied) {
      await refreshUiAfterMlMutation(props.filePath, result.updated_file_path);
    }
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to apply suggestion.';
  } finally {
    applyingSuggestionId.value = null;
  }
}

async function undoSuggestion(suggestion: OrganizationSuggestion) {
  if (!props.vaultId) return;
  const receiptId = undoReceipts.value.get(suggestion.id);
  if (!receiptId) return;

  error.value = '';
  applyMessage.value = '';
  undoingSuggestionId.value = suggestion.id;

  try {
    const result: UndoMlActionResponse = await apiUndoMlAction(props.vaultId, receiptId);
    applyMessage.value = result.description;
    const newMap = new Map(undoReceipts.value);
    newMap.delete(suggestion.id);
    undoReceipts.value = newMap;
    await refreshUiAfterMlMutation(props.filePath, result.file_path);
  } catch (err) {
    if (err instanceof ApiError && err.status === 404) {
      const newMap = new Map(undoReceipts.value);
      newMap.delete(suggestion.id);
      undoReceipts.value = newMap;
      applyMessage.value = 'Undo is no longer available for this action (it was already undone or expired).';
      return;
    }
    error.value = err instanceof ApiError ? err.message : 'Failed to undo action.';
  } finally {
    undoingSuggestionId.value = null;
  }
}

// A vault-wide organize batch changed files on disk; refresh the tree and any
// open tabs so the UI reflects moves/renames.
async function onOrganizeApplied() {
  if (!props.vaultId) return;
  await filesStore.loadTree(props.vaultId);
  if (props.filePath) {
    await refreshUiAfterMlMutation(props.filePath);
  }
}
</script>

<style scoped>
.ml-header:hover {
  background: rgb(var(--v-theme-surface-variant));
}

.outline-list {
  max-height: 160px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 6px;
  padding: 4px;
}

.outline-item {
  padding: 2px 0;
  border-left: 2px solid transparent;
}

.outline-item:hover {
  border-left-color: rgb(var(--v-theme-primary));
  background: rgba(var(--v-theme-primary), 0.06);
}

.suggestion-item {
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 8px;
  background: rgba(var(--v-theme-surface-variant), 0.25);
}
</style>
