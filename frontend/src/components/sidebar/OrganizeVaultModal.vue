<template>
  <v-dialog :model-value="modelValue" max-width="900" @update:model-value="emit('update:modelValue', $event)">
    <v-card>
      <v-card-title class="d-flex align-center">
        <span>Organize vault</span>
        <v-chip size="x-small" variant="tonal" color="primary" class="ml-2">suggest-only</v-chip>
        <v-spacer />
        <v-btn icon="mdi-close" variant="text" size="small" @click="emit('update:modelValue', false)" />
      </v-card-title>

      <v-card-subtitle class="text-caption">
        Review the proposed plan and apply only the changes you select. Nothing is changed until you click
        <strong>Apply selected</strong>.
      </v-card-subtitle>

      <v-card-text>
        <div class="d-flex ga-2 mb-3 align-center">
          <v-btn
            size="small"
            color="primary"
            variant="tonal"
            prepend-icon="mdi-refresh"
            :loading="loadingPlan"
            :disabled="applying"
            @click="loadPlan"
          >
            {{ plan ? 'Recompute plan' : 'Compute plan' }}
          </v-btn>
          <span v-if="plan" class="text-caption text-secondary">
            {{ plan.rows.length }} note(s){{ plan.cluster_count ? `, ${plan.cluster_count} cluster(s)` : '' }}
          </span>
        </div>

        <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mb-2">{{ error }}</v-alert>
        <v-alert v-if="applyMessage" type="info" variant="tonal" density="compact" class="mb-2">{{ applyMessage }}</v-alert>

        <div v-if="plan && actionableRows.length" class="plan-table">
          <table class="w-100 text-caption">
            <thead>
              <tr class="text-secondary">
                <th class="text-left pa-1">Note</th>
                <th class="text-left pa-1">Add tags</th>
                <th class="text-left pa-1">Rename</th>
                <th class="text-left pa-1">Move to</th>
                <th class="text-right pa-1">Conf.</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in actionableRows" :key="row.file_path" class="plan-row">
                <td class="pa-1 text-truncate" style="max-width: 220px;" :title="row.file_path">{{ row.file_path }}</td>
                <td class="pa-1">
                  <v-checkbox
                    v-if="row.suggested_tags.length"
                    v-model="selections[row.file_path].tags"
                    density="compact"
                    hide-details
                    :label="row.suggested_tags.map((t) => `#${t}`).join(' ')"
                  />
                  <span v-else class="text-disabled">—</span>
                </td>
                <td class="pa-1">
                  <v-checkbox
                    v-if="row.suggested_name"
                    v-model="selections[row.file_path].name"
                    density="compact"
                    hide-details
                    :label="row.suggested_name"
                  />
                  <span v-else class="text-disabled">—</span>
                </td>
                <td class="pa-1">
                  <div v-if="folderOptions(row).length" class="d-flex align-center ga-1">
                    <v-checkbox
                      v-model="selections[row.file_path].folder"
                      density="compact"
                      hide-details
                    />
                    <v-select
                      v-model="selections[row.file_path].folderChoice"
                      :items="folderOptions(row)"
                      item-title="label"
                      item-value="path"
                      density="compact"
                      hide-details
                      variant="plain"
                      style="min-width: 180px;"
                      :disabled="!selections[row.file_path].folder"
                    />
                  </div>
                  <span v-else class="text-disabled">—</span>
                </td>
                <td class="pa-1 text-right">{{ Math.round(row.confidence * 100) }}%</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div v-else-if="plan" class="text-caption text-secondary py-4 text-center">
          No actionable changes were proposed for this vault.
        </div>
      </v-card-text>

      <v-card-actions>
        <v-btn
          v-if="lastGroupId"
          color="warning"
          variant="tonal"
          size="small"
          prepend-icon="mdi-undo"
          :loading="undoing"
          @click="undoLast"
        >
          Undo last organize
        </v-btn>
        <v-spacer />
        <v-btn variant="text" size="small" @click="emit('update:modelValue', false)">Close</v-btn>
        <v-btn
          color="primary"
          variant="flat"
          size="small"
          :disabled="!selectedCount || applying"
          :loading="applying"
          @click="applySelected"
        >
          Apply selected ({{ selectedCount }})
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { apiApplyPlan, apiOrganizeVault, apiUndoMlGroup, ApiError } from '@/api/client';
import type { ApplyPlanRow, OrganizationPlan, OrganizationPlanRow } from '@/api/types';

const props = defineProps<{
  modelValue: boolean;
  vaultId: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void;
  (e: 'applied'): void;
}>();

const loadingPlan = ref(false);
const applying = ref(false);
const undoing = ref(false);
const error = ref('');
const applyMessage = ref('');
const plan = ref<OrganizationPlan | null>(null);
const lastGroupId = ref<string | null>(null);

interface RowSelection {
  tags: boolean;
  name: boolean;
  folder: boolean;
  folderChoice: string;
}
const selections = reactive<Record<string, RowSelection>>({});

// Destination-folder choices for a row: the ranked candidates (existing folders
// first, then a proposed new one), falling back to a single legacy target.
function folderOptions(row: OrganizationPlanRow): Array<{ path: string; label: string }> {
  const candidates = row.folder_candidates ?? [];
  if (candidates.length) {
    return candidates.map((c) => ({
      path: c.path,
      label: `${c.path}${c.is_new ? '  (new)' : ''}  ·  ${Math.round(c.confidence * 100)}%`,
    }));
  }
  return row.target_folder ? [{ path: row.target_folder, label: row.target_folder }] : [];
}

// Only rows that propose at least one action are worth showing.
const actionableRows = computed<OrganizationPlanRow[]>(() =>
  (plan.value?.rows ?? []).filter(
    (r) =>
      r.suggested_tags.length > 0 ||
      !!r.suggested_name ||
      !!r.target_folder ||
      (r.folder_candidates?.length ?? 0) > 0,
  ),
);

const selectedCount = computed(() => {
  let n = 0;
  for (const row of actionableRows.value) {
    const sel = selections[row.file_path];
    if (!sel) continue;
    if (sel.tags && row.suggested_tags.length) n += 1;
    if (sel.name && row.suggested_name) n += 1;
    if (sel.folder && sel.folderChoice) n += 1;
  }
  return n;
});

watch(
  () => props.modelValue,
  (open) => {
    if (open && !plan.value && props.vaultId) {
      void loadPlan();
    }
  },
);

async function loadPlan() {
  if (!props.vaultId) return;
  loadingPlan.value = true;
  error.value = '';
  applyMessage.value = '';
  try {
    plan.value = await apiOrganizeVault(props.vaultId, {});
    // Default every available action to selected.
    for (const key of Object.keys(selections)) delete selections[key];
    for (const row of plan.value.rows) {
      const options = folderOptions(row);
      // Default must be one of the select's options so the checkbox is never
      // checked against an empty/unlisted choice (LIB-093). Prefer the
      // recommended target only when it's actually among the candidates.
      const recommended = options.find((o) => o.path === row.target_folder)?.path;
      selections[row.file_path] = {
        tags: row.suggested_tags.length > 0,
        name: !!row.suggested_name,
        folder: options.length > 0,
        folderChoice: recommended ?? options[0]?.path ?? '',
      };
    }
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to compute organization plan.';
  } finally {
    loadingPlan.value = false;
  }
}

async function applySelected() {
  if (!props.vaultId || !plan.value) return;
  applying.value = true;
  error.value = '';
  applyMessage.value = '';

  // LIB-093: a folder checkbox left checked with no destination would be
  // silently dropped below. Surface it instead of quietly doing nothing.
  const missingFolder = actionableRows.value.filter((row) => {
    const sel = selections[row.file_path];
    return sel?.folder && !sel.folderChoice;
  });
  if (missingFolder.length) {
    error.value = `Pick a destination folder for ${missingFolder.length} selected note(s), or uncheck "Move to".`;
    return;
  }

  const rows: ApplyPlanRow[] = [];
  for (const row of actionableRows.value) {
    const sel = selections[row.file_path];
    if (!sel) continue;
    const apply: ApplyPlanRow = { file_path: row.file_path };
    let any = false;
    if (sel.tags && row.suggested_tags.length) {
      apply.apply_tags = row.suggested_tags;
      any = true;
    }
    if (sel.name && row.suggested_name) {
      apply.apply_name = row.suggested_name;
      any = true;
    }
    if (sel.folder && sel.folderChoice) {
      apply.apply_folder = sel.folderChoice;
      any = true;
    }
    if (any) {
      // Reinforcement (LIB-075): on a row the user engaged with, any offered
      // folder candidate they didn't pick — and suggested tags they didn't
      // apply — count as reject signals.
      const rejectFolders = (row.folder_candidates ?? [])
        .map((c) => c.path)
        .filter((p) => p !== apply.apply_folder);
      if (rejectFolders.length) apply.reject_folders = rejectFolders;
      if (!apply.apply_tags && row.suggested_tags.length) {
        apply.reject_tags = row.suggested_tags;
      }
      rows.push(apply);
    }
  }

  try {
    const result = await apiApplyPlan(props.vaultId, { plan_id: plan.value.plan_id, rows, dry_run: false });
    const errs = result.results.filter((r) => r.error);
    lastGroupId.value = result.group_id ?? null;
    applyMessage.value = errs.length
      ? `Applied with ${errs.length} error(s): ${errs.map((e) => `${e.file_path}: ${e.error}`).join('; ')}`
      : `Applied changes to ${result.results.length} note(s).`;
    emit('applied');
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : 'Failed to apply plan.';
  } finally {
    applying.value = false;
  }
}

async function undoLast() {
  if (!props.vaultId || !lastGroupId.value) return;
  undoing.value = true;
  error.value = '';
  try {
    const result = await apiUndoMlGroup(props.vaultId, lastGroupId.value);
    applyMessage.value = `${result.description} (${result.undone_count ?? 0} change(s) reverted).`;
    lastGroupId.value = null;
    emit('applied');
    await loadPlan();
  } catch (err) {
    if (err instanceof ApiError && err.status === 404) {
      lastGroupId.value = null;
      applyMessage.value = 'This organize batch was already undone or expired.';
    } else {
      error.value = err instanceof ApiError ? err.message : 'Failed to undo organize batch.';
    }
  } finally {
    undoing.value = false;
  }
}
</script>

<style scoped>
.plan-table {
  max-height: 420px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-border), 1);
  border-radius: 8px;
}

.plan-table table {
  border-collapse: collapse;
}

.plan-row {
  border-top: 1px solid rgba(var(--v-theme-border), 0.6);
}
</style>
