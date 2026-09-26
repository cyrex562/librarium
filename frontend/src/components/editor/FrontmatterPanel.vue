<template>
  <v-expansion-panels v-model="open" flat style="background: transparent; border-bottom: 1px solid rgb(var(--v-theme-border));">
    <v-expansion-panel>
      <v-expansion-panel-title class="text-caption font-weight-bold" style="min-height: 32px;">
        Frontmatter
      </v-expansion-panel-title>
      <v-expansion-panel-text>
        <div class="d-flex align-center justify-space-between mb-2 ga-2">
          <v-btn-toggle
            :model-value="mode"
            density="compact"
            mandatory
            variant="outlined"
            @update:model-value="onModeChange"
          >
            <v-btn value="form" size="x-small">Form</v-btn>
            <v-btn value="raw" size="x-small">Raw</v-btn>
          </v-btn-toggle>
          <v-btn size="x-small" variant="text" prepend-icon="mdi-plus" @click="addingKey = true">
            Add field
          </v-btn>
        </div>

        <div v-if="mode === 'form'" class="d-flex flex-column ga-2 py-1">
          <div
            v-for="(entry, index) in entries"
            :key="`${entry.key}-${index}`"
            class="d-flex align-center ga-2"
          >
            <v-text-field
              :model-value="entry.key"
              label="Field"
              hide-details
              density="compact"
              variant="outlined"
              style="max-width: 220px;"
              @update:model-value="onEntryKeyChange(index, $event)"
            />
            <v-text-field
              :model-value="entry.value"
              label="Value"
              hint="YAML/JSON value (examples: true, 42, [a, b], {x: 1})"
              persistent-hint
              hide-details="auto"
              density="compact"
              variant="outlined"
              style="flex: 1; min-width: 120px;"
              @update:model-value="onEntryValueChange(index, $event)"
            />
            <v-btn size="x-small" icon="mdi-delete-outline" variant="text" @click="removeEntry(index)" />
          </div>

          <div v-if="entries.length === 0" class="text-caption text-secondary">
            No frontmatter fields yet. Use “Add field” to create one.
          </div>
        </div>

        <div v-else class="d-flex flex-column ga-2">
          <v-textarea
            v-model="rawText"
            label="Frontmatter (YAML/JSON object)"
            rows="6"
            auto-grow
            variant="outlined"
            hide-details
            @blur="applyRawText"
          />
          <div v-if="rawError" class="text-caption" style="color: rgb(var(--v-theme-error));">
            {{ rawError }}
          </div>
          <div class="text-caption text-secondary">
            Tip: use YAML like <code>title: My Note</code> (JSON object syntax also works).
          </div>
        </div>

        <div v-if="addingKey" class="d-flex align-center ga-2 mt-2">
          <v-text-field
            v-model="newKey"
            placeholder="field"
            autofocus
            hide-details
            density="compact"
            variant="outlined"
            style="max-width: 180px;"
            @keyup.enter="confirmAdd"
            @keyup.esc="cancelAdd"
          />
          <v-text-field
            v-model="newVal"
            placeholder="value"
            hide-details
            density="compact"
            variant="outlined"
            style="max-width: 260px;"
            @keyup.enter="confirmAdd"
            @keyup.esc="cancelAdd"
          />
          <v-btn size="x-small" icon="mdi-check" @click="confirmAdd" />
          <v-btn size="x-small" icon="mdi-close" variant="text" @click="cancelAdd" />
        </div>
      </v-expansion-panel-text>
    </v-expansion-panel>
  </v-expansion-panels>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { parse as parseYaml, stringify as stringifyYaml } from 'yaml';

const props = defineProps<{
  tabId: string;
  frontmatter: Record<string, unknown>;
}>();

const emit = defineEmits<{
  'update:frontmatter': [value: Record<string, unknown>];
}>();

// Expanded by default on desktop; collapsed on mobile where vertical space is
// scarce (the user can still tap the header to expand it).
const open = ref<number | undefined>(undefined); // start collapsed
const mode = ref<'form' | 'raw'>('form');
const addingKey = ref(false);
const newKey = ref('');
const newVal = ref('');
const rawText = ref('');
const rawError = ref('');

const entries = ref<Array<{ key: string; value: string }>>([]);

function formatFormValue(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  if (Array.isArray(value) || typeof value === 'object') {
    try {
      return stringifyYaml(value).trimEnd();
    } catch {
      return JSON.stringify(value);
    }
  }
  return String(value);
}

function parseFormValue(valueText: string): unknown {
  const trimmed = valueText.trim();
  if (!trimmed) return '';
  try {
    const parsed = parseYaml(trimmed);
    return parsed;
  } catch {
    return valueText;
  }
}

function rebuildEntriesFromFrontmatter(frontmatter: Record<string, unknown>) {
  entries.value = Object.entries(frontmatter).map(([key, value]) => ({
    key,
    value: formatFormValue(value),
  }));
  rawText.value = stringifyFrontmatter(frontmatter);
  rawError.value = '';
}

function emitFromEntries() {
  const next: Record<string, unknown> = {};
  for (const entry of entries.value) {
    const key = entry.key.trim();
    if (!key) continue;
    next[key] = parseFormValue(entry.value);
  }
  rawText.value = stringifyFrontmatter(next);
  rawError.value = '';
  emit('update:frontmatter', next);
}

watch(() => props.frontmatter, (fm) => {
  rebuildEntriesFromFrontmatter(fm ?? {});
}, { immediate: true });

function onModeChange(next: 'form' | 'raw' | null) {
  if (!next) return;
  mode.value = next;
  if (next === 'raw') {
    rawText.value = stringifyFrontmatter(toObjectFromEntries());
  }
}

function stringifyFrontmatter(value: Record<string, unknown>): string {
  const yaml = stringifyYaml(value ?? {});
  return yaml.trimEnd();
}

function toObjectFromEntries(): Record<string, unknown> {
  const next: Record<string, unknown> = {};
  for (const entry of entries.value) {
    const key = entry.key.trim();
    if (!key) continue;
    next[key] = parseFormValue(entry.value);
  }
  return next;
}

function onEntryKeyChange(index: number, value: string) {
  const entry = entries.value[index];
  if (!entry) return;
  entry.key = value;
  emitFromEntries();
}

function onEntryValueChange(index: number, value: string) {
  const entry = entries.value[index];
  if (!entry) return;
  entry.value = value;
  emitFromEntries();
}

function removeEntry(index: number) {
  entries.value.splice(index, 1);
  emitFromEntries();
}

function confirmAdd() {
  const key = newKey.value.trim();
  if (!key) return;
  entries.value.push({ key, value: newVal.value });
  emitFromEntries();
  cancelAdd();
}

function cancelAdd() {
  newKey.value = '';
  newVal.value = '';
  addingKey.value = false;
}

function applyRawText() {
  try {
    const parsed = parseYaml(rawText.value);
    if (parsed == null) {
      rebuildEntriesFromFrontmatter({});
      emit('update:frontmatter', {});
      return;
    }

    if (typeof parsed !== 'object' || Array.isArray(parsed)) {
      rawError.value = 'Frontmatter raw mode expects a YAML object (mapping of key/value fields).';
      return;
    }

    rebuildEntriesFromFrontmatter(parsed as Record<string, unknown>);
    emit('update:frontmatter', parsed as Record<string, unknown>);
  } catch {
    rawError.value = 'Invalid YAML/JSON. Please fix the syntax and try again.';
  }
}
</script>
