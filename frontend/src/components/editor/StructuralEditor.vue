<template>
  <div class="structural-editor d-flex flex-column" style="flex: 1; min-height: 0; overflow: hidden;">
    <!-- Loading state -->
    <div v-if="loading" class="d-flex align-center justify-center" style="flex: 1;">
      <v-progress-circular indeterminate size="32" />
    </div>

    <!-- Error: no librarium_type in frontmatter -->
    <div v-else-if="state === 'no_type'" class="structural-error pa-6" style="flex: 1; overflow-y: auto;">
      <v-icon icon="mdi-alert-circle-outline" color="warning" size="48" class="mb-4" />
      <h3 class="text-h6 mb-2">Not a typed entity</h3>
      <p class="text-body-2 text-secondary mb-4">
        This file needs a <code>librarium_type</code> field in its frontmatter before it can be edited as a
        structured entity.
      </p>
      <v-list density="compact" class="bg-surface rounded mb-4">
        <v-list-subheader>To use structural editing:</v-list-subheader>
        <v-list-item prepend-icon="mdi-numeric-1-circle-outline" title="Add librarium_type to frontmatter" />
        <v-list-item prepend-icon="mdi-numeric-2-circle-outline" title="Set librarium_plugin to the plugin that defines the type" />
        <v-list-item prepend-icon="mdi-numeric-3-circle-outline" title="Or use File → New Entity to create a pre-typed file" />
      </v-list>
      <v-select
        v-if="availableTypes.length"
        v-model="selectedTypeToApply"
        :items="availableTypeItems"
        label="Apply entity type to this file"
        density="compact"
        variant="outlined"
        hide-details
        class="mb-3"
        style="max-width: 400px;"
      />
      <v-btn
        v-if="selectedTypeToApply"
        color="primary"
        @click="applyEntityType"
      >
        Apply type &amp; open in structural editor
      </v-btn>
    </div>

    <!-- Error: parse failure -->
    <div v-else-if="state === 'parse_error'" class="structural-error pa-6" style="flex: 1; overflow-y: auto;">
      <v-icon icon="mdi-file-alert-outline" color="error" size="48" class="mb-4" />
      <h3 class="text-h6 mb-2">Frontmatter parse error</h3>
      <p class="text-body-2 text-secondary mb-4">The YAML frontmatter in this file could not be parsed.</p>
      <v-alert type="error" variant="tonal" class="mb-4">{{ parseError }}</v-alert>
      <p class="text-body-2">Switch to <strong>Plain</strong> or <strong>Formatted</strong> mode to fix the frontmatter manually.</p>
    </div>

    <!-- Schema not found for the declared type -->
    <div v-else-if="state === 'schema_missing'" class="structural-error pa-6" style="flex: 1; overflow-y: auto;">
      <v-icon icon="mdi-help-circle-outline" color="warning" size="48" class="mb-4" />
      <h3 class="text-h6 mb-2">Unknown entity type: {{ declaredType }}</h3>
      <p class="text-body-2 text-secondary">
        No schema was found for this entity type. Make sure the plugin that defines it is installed and
        enabled, then reload.
      </p>
    </div>

    <!-- Structural edit form -->
    <div v-else-if="state === 'ready' && entitySchema" class="d-flex flex-column" style="flex: 1; min-height: 0; overflow: hidden;">
      <!-- Header row: type badge + type name -->
      <div class="d-flex align-center ga-3 px-4 py-2" style="border-bottom: 1px solid rgb(var(--v-theme-border)); flex-shrink: 0;">
        <v-chip
          :color="entitySchema.color ?? 'primary'"
          size="small"
          :prepend-icon="entitySchema.icon ? `mdi-${entitySchema.icon}` : 'mdi-tag'"
        >
          {{ entitySchema.name }}
        </v-chip>
        <span class="text-caption text-secondary">{{ tabFilePath }}</span>
        <v-spacer />
        <v-btn
          size="x-small"
          variant="text"
          prepend-icon="mdi-refresh"
          title="Re-run reindex for this file"
          :loading="reindexing"
          @click="onReindex"
        >Reindex</v-btn>
      </div>

      <!-- Scrollable fields + prose zone -->
      <div class="structural-scroll" style="flex: 1; overflow-y: auto; padding: 16px 20px;">
        <!-- Field list -->
        <div
          v-for="field in entitySchema.fields"
          :key="field.key"
          class="mb-5"
        >
          <label class="field-label d-block mb-1">
            {{ field.label }}
            <span v-if="field.required" class="text-error ml-1">*</span>
            <v-tooltip v-if="field.description" location="right">
              <template #activator="{ props: tp }">
                <v-icon v-bind="tp" icon="mdi-information-outline" size="14" class="ml-1 text-secondary" />
              </template>
              {{ field.description }}
            </v-tooltip>
          </label>

          <!-- string → single-line text -->
          <v-text-field
            v-if="field.field_type === 'string'"
            :model-value="getField(field.key) as string"
            density="compact"
            variant="outlined"
            hide-details="auto"
            @update:model-value="setField(field.key, $event)"
          />

          <!-- text → multi-line textarea -->
          <v-textarea
            v-else-if="field.field_type === 'text'"
            :model-value="getField(field.key) as string"
            density="compact"
            variant="outlined"
            hide-details="auto"
            rows="4"
            auto-grow
            @update:model-value="setField(field.key, $event)"
          />

          <!-- number -->
          <v-text-field
            v-else-if="field.field_type === 'number'"
            :model-value="getField(field.key)"
            type="number"
            density="compact"
            variant="outlined"
            hide-details="auto"
            @update:model-value="setField(field.key, $event === '' ? null : Number($event))"
          />

          <!-- date -->
          <v-text-field
            v-else-if="field.field_type === 'date'"
            :model-value="getField(field.key) as string"
            type="date"
            density="compact"
            variant="outlined"
            hide-details="auto"
            @update:model-value="setField(field.key, $event)"
          />

          <!-- boolean -->
          <v-switch
            v-else-if="field.field_type === 'boolean'"
            :model-value="!!getField(field.key)"
            color="primary"
            density="compact"
            hide-details
            @update:model-value="setField(field.key, $event)"
          />

          <!-- enum → select -->
          <v-select
            v-else-if="field.field_type === 'enum'"
            :model-value="getField(field.key) as string | null | undefined"
            :items="field.values"
            density="compact"
            variant="outlined"
            hide-details="auto"
            clearable
            @update:model-value="setField(field.key, $event)"
          />

          <!-- entity_ref → wiki-link autocomplete -->
          <EntityRefField
            v-else-if="field.field_type === 'entity_ref'"
            :model-value="(getField(field.key) as string | null | undefined)"
            :target-label="field.target_label"
            :vault-id="vaultId ?? ''"
            @update:model-value="setField(field.key, $event)"
          />

          <!-- list<T> → repeating items -->
          <ListField
            v-else-if="field.field_type === 'list'"
            :model-value="getField(field.key) as unknown[]"
            :item-type="field.item_type ?? 'string'"
            :item-values="field.values"
            :target-label="field.target_label"
            :vault-id="vaultId ?? ''"
            @update:model-value="setField(field.key, $event)"
          />

          <!-- fallback -->
          <v-text-field
            v-else
            :model-value="String(getField(field.key) ?? '')"
            density="compact"
            variant="outlined"
            hide-details="auto"
            @update:model-value="setField(field.key, $event)"
          />
        </div>

        <!-- Labels display (read-only from schema) -->
        <div v-if="entitySchema.labels.length" class="mb-5">
          <label class="field-label d-block mb-1">Labels</label>
          <div class="d-flex flex-wrap ga-1">
            <v-chip v-for="lbl in allLabels" :key="lbl" size="x-small" variant="tonal">{{ lbl }}</v-chip>
          </div>
        </div>

        <!-- Prose zone heading -->
        <div class="prose-zone-heading d-flex align-center ga-2 mb-2">
          <v-icon icon="mdi-text-long" size="16" class="text-secondary" />
          <span class="text-caption font-weight-bold text-secondary">PROSE ZONE</span>
        </div>

        <!-- Prose zone editor (using MarkdownEditor in formatted_raw mode) -->
        <div class="prose-zone-wrapper" style="min-height: 200px; border: 1px solid rgb(var(--v-theme-border)); border-radius: 4px; overflow: hidden;">
          <MarkdownEditor
            v-if="tabId"
            :tab-id="`${tabId}::prose`"
            :content="proseContent"
            :file-path="tabFilePath ?? ''"
            mode="formatted_raw"
            style="min-height: 200px;"
            @update="onProseUpdate"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useTabsStore } from '@/stores/tabs';
import { useVaultsStore } from '@/stores/vaults';
import { apiListEntityTypes, apiTriggerReindex } from '@/api/client';
import type { EntityTypeSchema } from '@/api/types';
import MarkdownEditor from './MarkdownEditor.vue';
import EntityRefField from './structural/EntityRefField.vue';
import ListField from './structural/ListField.vue';
import { parseFrontmatter, serializeFrontmatter, extractProseZone, replaceProseZone } from '@/editor/structural-utils';

const props = defineProps<{ tabId: string }>();

const tabsStore = useTabsStore();
const vaultsStore = useVaultsStore();

type EditorState = 'loading' | 'no_type' | 'parse_error' | 'schema_missing' | 'ready';

const state = ref<EditorState>('loading');
const loading = ref(true);
const parseError = ref('');
const declaredType = ref('');
const availableTypes = ref<EntityTypeSchema[]>([]);
const entitySchema = ref<EntityTypeSchema | null>(null);
const fieldValues = ref<Record<string, unknown>>({});
const proseContent = ref('');
const selectedTypeToApply = ref<string | null>(null);
const reindexing = ref(false);

const activeTab = computed(() => tabsStore.tabs.get(props.tabId) ?? null);
const tabFilePath = computed(() => activeTab.value?.filePath ?? null);
const vaultId = computed(() => vaultsStore.activeVaultId);
const allLabels = computed(() => {
    const labels = entitySchema.value?.labels ?? [];
    const fm = activeTab.value?.frontmatter;
    const fmLabelsRaw = fm?.librarium_labels ?? fm?.codex_labels;
    const fmLabels = Array.isArray(fmLabelsRaw) ? (fmLabelsRaw as string[]) : [];
    return [...new Set([...labels, ...fmLabels])];
});
const availableTypeItems = computed(() =>
    availableTypes.value.map(t => ({ title: t.name, value: t.id }))
);

async function init() {
    loading.value = true;
    state.value = 'loading';

    // Load all entity type schemas
    try {
        const resp = await apiListEntityTypes();
        availableTypes.value = resp.entity_types;
    } catch {
        availableTypes.value = [];
    }

    const content = activeTab.value?.content ?? '';
    const frontmatter = activeTab.value?.frontmatter;

    // 1. Check if frontmatter was parsed by the backend already
    if (!frontmatter && !content.startsWith('---')) {
        state.value = 'no_type';
        loading.value = false;
        return;
    }

    // 2. Try to parse frontmatter from raw content
    let fm: Record<string, unknown> = {};
    try {
        fm = frontmatter ? { ...frontmatter } : parseFrontmatter(content);
    } catch (e) {
        parseError.value = String(e);
        state.value = 'parse_error';
        loading.value = false;
        return;
    }

    // 3. Check for librarium_type
    const librariumType = (fm?.librarium_type ?? fm?.codex_type) as string | undefined;
    if (!librariumType) {
        state.value = 'no_type';
        loading.value = false;
        return;
    }
    declaredType.value = librariumType;

    // 4. Find matching schema
    const schema = availableTypes.value.find(t => t.id === librariumType || t.name === librariumType) ?? null;
    if (!schema) {
        state.value = 'schema_missing';
        loading.value = false;
        return;
    }
    entitySchema.value = schema;

    // 5. Populate field values from frontmatter (strip codex_ reserved keys)
    const reserved = new Set([
        'librarium_type',
        'librarium_plugin',
        'librarium_labels',
        'codex_type',
        'codex_plugin',
        'codex_labels',
    ]);
    const vals: Record<string, unknown> = {};
    for (const field of schema.fields) {
        vals[field.key] = fm[field.key] ?? field.default ?? null;
    }
    fieldValues.value = vals;

    // 6. Extract prose zone from raw content
    proseContent.value = extractProseZone(content);

    // 7. Ensure prose sentinels exist; insert if missing
    if (!content.includes('<!-- librarium:prose:begin -->') && !content.includes('<!-- codex:prose:begin -->')) {
        await insertSentinels();
    }

    state.value = 'ready';
    loading.value = false;
}

function getField(key: string): unknown {
    return fieldValues.value[key] ?? null;
}

function setField(key: string, value: unknown) {
    fieldValues.value = { ...fieldValues.value, [key]: value };
    scheduleWrite();
}

function onProseUpdate(newProse: string) {
    proseContent.value = newProse;
    scheduleWrite();
}

let writeTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleWrite() {
    if (writeTimer) clearTimeout(writeTimer);
    writeTimer = setTimeout(() => {
        commitToTab();
    }, 600);
}

function commitToTab() {
    const tab = activeTab.value;
    if (!tab) return;
    const content = tab.content ?? '';
    const fm = activeTab.value?.frontmatter ?? {};

    // Merge field values into frontmatter
    const newFm: Record<string, unknown> = { ...fm };
    for (const [k, v] of Object.entries(fieldValues.value)) {
        newFm[k] = v;
    }

    // Reconstruct full file content
    const newContent = replaceProseZone(
        serializeFrontmatter(newFm, content),
        proseContent.value,
    );
    tabsStore.updateTabContent(tab.id, newContent);
    tabsStore.updateTabFrontmatter(tab.id, newFm);
}

async function insertSentinels() {
    const tab = activeTab.value;
    if (!tab) return;
    const content = tab.content ?? '';
    // Find where frontmatter ends
    let body = content;
    const fmEnd = content.indexOf('\n---', 3);
    if (fmEnd !== -1) {
        const afterFm = content.slice(fmEnd + 4).trimStart();
        const header = content.slice(0, fmEnd + 4);
        body = `${header}\n\n<!-- librarium:prose:begin -->\n${afterFm}\n<!-- librarium:prose:end -->\n`;
    } else {
        body = `${content}\n\n<!-- librarium:prose:begin -->\n\n<!-- librarium:prose:end -->\n`;
    }
    tabsStore.updateTabContent(tab.id, body);
}

async function applyEntityType() {
    const type = availableTypes.value.find(t => t.id === selectedTypeToApply.value);
    if (!type) return;
    const tab = activeTab.value;
    if (!tab) return;
    const fm = { ...(tab.frontmatter ?? {}), librarium_type: type.id, librarium_plugin: type.plugin_id };
    tabsStore.updateTabFrontmatter(tab.id, fm);
    // Re-run init to show the structural form
    await init();
}

async function onReindex() {
    if (!vaultId.value) return;
    reindexing.value = true;
    try {
        await apiTriggerReindex(vaultId.value);
    } finally {
        reindexing.value = false;
    }
}

// Watch for tab content changes (e.g. file reloaded from disk)
watch(() => activeTab.value?.content, () => {
    if (state.value !== 'loading') init();
}, { deep: false });

onMounted(init);
</script>

<style scoped>
.structural-error {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
}

.field-label {
  font-size: 12px;
  font-weight: 600;
  color: rgb(var(--v-theme-secondary));
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.prose-zone-heading {
  margin-top: 8px;
  padding-top: 12px;
  border-top: 1px dashed rgb(var(--v-theme-border));
}
</style>
