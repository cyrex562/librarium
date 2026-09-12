<template>
  <div class="editor-toolbar">
    <!-- Formatting buttons — only actionable when editor is editable (not pure preview) -->
    <template v-if="editorVisible">
      <!-- Undo / Redo -->
      <v-btn v-bind="btn" icon="mdi-undo" title="Undo (Ctrl+Z)" @mousedown.prevent="emit('command', 'undo')" />
      <v-btn v-bind="btn" icon="mdi-redo" title="Redo (Ctrl+Y)" @mousedown.prevent="emit('command', 'redo')" />
      <v-btn
        v-bind="btn"
        icon="mdi-unfold-less-horizontal"
        title="Collapse all foldable sections"
        :disabled="!isFormattedMode"
        @mousedown.prevent="emit('command', 'collapse_all_folds')"
      />
      <v-btn
        v-bind="btn"
        icon="mdi-unfold-more-horizontal"
        title="Expand all folded sections"
        :disabled="!isFormattedMode"
        @mousedown.prevent="emit('command', 'expand_all_folds')"
      />

      <div class="toolbar-sep" />

      <!-- Headings -->
      <v-btn v-bind="btn" icon="mdi-format-header-1" title="Heading 1" @mousedown.prevent="emit('command', 'heading_1')" />
      <v-btn v-bind="btn" icon="mdi-format-header-2" title="Heading 2" @mousedown.prevent="emit('command', 'heading_2')" />
      <v-btn v-bind="btn" icon="mdi-format-header-3" title="Heading 3" @mousedown.prevent="emit('command', 'heading_3')" />

      <div class="toolbar-sep" />

      <!-- Inline text formatting -->
      <v-btn v-bind="btn" icon="mdi-format-bold" title="Bold" @mousedown.prevent="emit('command', 'bold')" />
      <v-btn v-bind="btn" icon="mdi-format-italic" title="Italic" @mousedown.prevent="emit('command', 'italic')" />
      <v-btn v-bind="btn" icon="mdi-format-strikethrough-variant" title="Strikethrough" @mousedown.prevent="emit('command', 'strikethrough')" />
      <v-btn v-bind="btn" icon="mdi-marker" title="Highlight" @mousedown.prevent="emit('command', 'highlight')" />
      <v-btn v-bind="btn" icon="mdi-code-tags" title="Inline code" @mousedown.prevent="emit('command', 'inline_code')" />

      <div class="toolbar-sep" />

      <!-- Links & media -->
      <v-btn v-bind="btn" icon="mdi-link-variant" title="Insert link" @mousedown.prevent="emit('command', 'link')" />
      <v-btn v-bind="btn" icon="mdi-image-plus-outline" title="Insert image" @mousedown.prevent="emit('command', 'image')" />

      <div class="toolbar-sep" />

      <!-- Lists & blocks -->
      <v-btn v-bind="btn" icon="mdi-format-quote-open" title="Blockquote" @mousedown.prevent="emit('command', 'blockquote')" />
      <v-btn v-bind="btn" icon="mdi-format-list-bulleted" title="Bulleted list" @mousedown.prevent="emit('command', 'bulleted_list')" />
      <v-btn v-bind="btn" icon="mdi-format-list-numbered" title="Numbered list" @mousedown.prevent="emit('command', 'numbered_list')" />
      <v-btn v-bind="btn" icon="mdi-format-list-checks" title="Task list" @mousedown.prevent="emit('command', 'task_list')" />
      <v-btn v-bind="btn" icon="mdi-format-indent-decrease" title="Decrease indent (Shift+Tab)" @mousedown.prevent="emit('command', 'outdent')" />
      <v-btn v-bind="btn" icon="mdi-format-indent-increase" title="Increase indent (Tab)" @mousedown.prevent="emit('command', 'indent')" />

      <div class="toolbar-sep" />

      <!-- Inserts -->
      <v-menu v-model="gridMenu" :close-on-content-click="false" location="bottom start">
        <template #activator="{ props: menuProps }">
          <v-btn v-bind="{ ...btn, ...menuProps }" icon="mdi-table-plus" title="Insert table" />
        </template>
        <v-card class="pa-3">
          <div class="text-caption mb-2">{{ gridRows }} × {{ gridCols }}</div>
          <div v-for="r in 8" :key="r" class="d-flex">
            <div
              v-for="c in 10"
              :key="c"
              class="grid-cell"
              :class="{ 'is-active': r <= gridRows && c <= gridCols }"
              @mouseenter="gridRows = r; gridCols = c"
              @click="emitCreate(r, c)"
            />
          </div>
          <div class="d-flex ga-2 mt-3 align-center">
            <v-text-field
              v-model.number="gridRows"
              label="Rows"
              type="number"
              density="compact"
              hide-details
              min="1"
              style="max-width: 90px;"
            />
            <v-text-field
              v-model.number="gridCols"
              label="Columns"
              type="number"
              density="compact"
              hide-details
              min="1"
              style="max-width: 110px;"
            />
            <v-btn size="small" @click="emitCreate(gridRows, gridCols)">Insert</v-btn>
          </div>
        </v-card>
      </v-menu>
      <v-btn v-bind="btn" icon="mdi-code-braces-box" title="Code block" @mousedown.prevent="emit('command', 'code_block')" />
      <v-btn v-bind="btn" icon="mdi-minus" title="Horizontal rule" @mousedown.prevent="emit('command', 'horizontal_rule')" />

      <!-- Table controls — only while the caret is inside a table -->
      <template v-if="inTable">
        <div class="toolbar-sep" />
        <v-btn v-bind="btn" icon="mdi-table-column-plus-before" title="Add column left" @mousedown.prevent="emit('command', 'table_col_insert_before')" />
        <v-btn v-bind="btn" icon="mdi-table-column-plus-after" title="Add column right" @mousedown.prevent="emit('command', 'table_col_insert_after')" />
        <v-btn v-bind="btn" icon="mdi-table-column-remove" title="Delete column" @mousedown.prevent="emit('command', 'table_col_delete')" />
        <v-btn v-bind="btn" icon="mdi-table-row-plus-before" title="Add row above" @mousedown.prevent="emit('command', 'table_row_insert_above')" />
        <v-btn v-bind="btn" icon="mdi-table-row-plus-after" title="Add row below" @mousedown.prevent="emit('command', 'table_row_insert_below')" />
        <v-btn v-bind="btn" icon="mdi-table-row-remove" title="Delete row" @mousedown.prevent="emit('command', 'table_row_delete')" />

        <v-menu location="bottom start">
          <template #activator="{ props: menuProps }">
            <v-btn v-bind="{ ...btn, ...menuProps }" icon="mdi-table-cog" title="More table options" />
          </template>
          <v-list density="compact" min-width="220">
            <v-list-subheader>Column alignment</v-list-subheader>
            <v-list-item prepend-icon="mdi-format-align-left" title="Left" :active="activeAlignment === 'left'" @click="emit('command', 'table_align_left')" />
            <v-list-item prepend-icon="mdi-format-align-center" title="Center" :active="activeAlignment === 'center'" @click="emit('command', 'table_align_center')" />
            <v-list-item prepend-icon="mdi-format-align-right" title="Right" :active="activeAlignment === 'right'" @click="emit('command', 'table_align_right')" />
            <v-list-item prepend-icon="mdi-format-align-justify" title="Default" :active="activeAlignment === 'none'" @click="emit('command', 'table_align_none')" />
            <v-divider class="my-1" />
            <v-list-subheader>Reorder</v-list-subheader>
            <v-list-item prepend-icon="mdi-arrow-up" title="Move row up" @click="emit('command', 'table_row_move_up')" />
            <v-list-item prepend-icon="mdi-arrow-down" title="Move row down" @click="emit('command', 'table_row_move_down')" />
            <v-list-item prepend-icon="mdi-arrow-left" title="Move column left" @click="emit('command', 'table_col_move_left')" />
            <v-list-item prepend-icon="mdi-arrow-right" title="Move column right" @click="emit('command', 'table_col_move_right')" />
            <v-divider class="my-1" />
            <v-list-item prepend-icon="mdi-table-remove" title="Delete table" @click="emit('command', 'table_delete')" />
          </v-list>
        </v-menu>
      </template>

      <div class="toolbar-sep" />

      <!-- Overflow: less-common actions -->
      <v-menu location="bottom start">
        <template #activator="{ props: menuProps }">
          <v-btn v-bind="{ ...btn, ...menuProps }" icon="mdi-dots-horizontal" title="More options" />
        </template>
        <v-list density="compact" min-width="240">
          <v-list-item
            prepend-icon="mdi-note-plus-outline"
            title="Extract selection to note"
            @click="emit('command', 'extract_to_note')"
          />
          <v-divider class="my-1" />
          <v-list-subheader>Ordered list styles</v-list-subheader>
          <v-list-item prepend-icon="mdi-format-list-numbered" title="a, b, c …" @click="emit('command', 'numbered_list_lower_alpha')" />
          <v-list-item prepend-icon="mdi-format-list-numbered" title="A, B, C …" @click="emit('command', 'numbered_list_upper_alpha')" />
          <v-list-item prepend-icon="mdi-format-list-numbered" title="i, ii, iii …" @click="emit('command', 'numbered_list_lower_roman')" />
          <v-list-item prepend-icon="mdi-format-list-numbered" title="I, II, III …" @click="emit('command', 'numbered_list_upper_roman')" />
        </v-list>
      </v-menu>
    </template>

    <span v-else class="text-caption text-secondary ml-1">Preview mode — switch to Plain or Formatted to edit</span>

    <v-spacer />

    <!-- View mode toggle (always visible) -->
    <v-btn-toggle
      :model-value="mode"
      mandatory
      density="compact"
      variant="outlined"
      divided
      style="flex-shrink: 0;"
      @update:model-value="(v) => emit('mode-change', v as EditorMode)"
    >
      <v-btn value="raw" size="x-small" title="Plain text editor">Plain</v-btn>
      <v-btn value="formatted_raw" size="x-small" title="Markdown text with inline formatting">Formatted</v-btn>
      <v-btn value="fully_rendered" size="x-small" title="Rendered preview only">Preview</v-btn>
      <v-btn value="structural" size="x-small" title="Structural entity editor">Structural</v-btn>
    </v-btn-toggle>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import type { MarkdownToolbarCommand } from '@/editor/markdown-toolbar';
import type { EditorMode } from '@/api/types';
import type { TableContext } from './MarkdownEditor.vue';

type ToolbarCommand = MarkdownToolbarCommand | 'undo' | 'redo' | 'collapse_all_folds' | 'expand_all_folds';

const props = defineProps<{
  mode: EditorMode;
  tableContext?: TableContext | null;
}>();

const emit = defineEmits<{
  command: [cmd: ToolbarCommand, payload?: { rows: number; cols: number }];
  'mode-change': [mode: EditorMode];
}>();

const editorVisible = computed(() => props.mode !== 'fully_rendered' && props.mode !== 'structural');
const isFormattedMode = computed(() => props.mode === 'formatted_raw');

// Shared button binding applied to every icon button in the toolbar
const btn = { size: 'small', variant: 'text' as const, density: 'compact' as const };

// Table controls appear only while the caret is inside a table.
const inTable = computed(() => !!props.tableContext);
const activeAlignment = computed(() => props.tableContext?.alignment ?? 'none');

// Grid picker state for "insert table".
const gridMenu = ref(false);
const gridRows = ref(3);
const gridCols = ref(3);

function emitCreate(rows: number, cols: number) {
  gridMenu.value = false;
  emit('command', 'table_create', { rows: Math.max(1, rows), cols: Math.max(1, cols) });
}
</script>

<style scoped>
.editor-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 8px;
  border-bottom: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
  flex-shrink: 0;
  flex-wrap: wrap;
  min-height: 40px;
}

/* Narrow screens: keep the toolbar a single horizontally-scrollable row
   instead of wrapping to 2-3 rows — vertical space is precious on a phone,
   especially with the on-screen keyboard up. */
@media (max-width: 959px) {
  .editor-toolbar {
    flex-wrap: nowrap;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    scrollbar-width: none; /* Firefox */
  }
  .editor-toolbar::-webkit-scrollbar {
    display: none;
  }
}

.grid-cell {
  width: 16px;
  height: 16px;
  margin: 1px;
  border: 1px solid rgb(var(--v-theme-border));
  cursor: pointer;
}
.grid-cell.is-active {
  background: rgb(var(--v-theme-primary));
}

.toolbar-sep {
  width: 1px;
  height: 20px;
  background: rgb(var(--v-theme-border));
  margin: 0 3px;
  flex-shrink: 0;
}
</style>
