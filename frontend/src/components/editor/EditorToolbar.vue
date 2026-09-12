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
      <v-btn v-bind="btn" icon="mdi-table-plus" title="Insert table" @mousedown.prevent="emit('command', 'table_create')" />
      <v-btn v-bind="btn" icon="mdi-code-braces-box" title="Code block" @mousedown.prevent="emit('command', 'code_block')" />
      <v-btn v-bind="btn" icon="mdi-minus" title="Horizontal rule" @mousedown.prevent="emit('command', 'horizontal_rule')" />

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
import { computed } from 'vue';
import type { MarkdownToolbarCommand } from '@/editor/markdown-toolbar';
import type { EditorMode } from '@/api/types';

type ToolbarCommand = MarkdownToolbarCommand | 'undo' | 'redo' | 'collapse_all_folds' | 'expand_all_folds';

const props = defineProps<{
  mode: EditorMode;
}>();

const emit = defineEmits<{
  command: [cmd: ToolbarCommand];
  'mode-change': [mode: EditorMode];
}>();

const editorVisible = computed(() => props.mode !== 'fully_rendered' && props.mode !== 'structural');
const isFormattedMode = computed(() => props.mode === 'formatted_raw');

// Shared button binding applied to every icon button in the toolbar
const btn = { size: 'small', variant: 'text' as const, density: 'compact' as const };
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

.toolbar-sep {
  width: 1px;
  height: 20px;
  background: rgb(var(--v-theme-border));
  margin: 0 3px;
  flex-shrink: 0;
}
</style>
