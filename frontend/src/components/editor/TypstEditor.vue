<template>
  <div class="typst-editor-shell" data-testid="typst-editor">
    <div class="typst-toolbar">
      <template v-if="!previewing">
        <v-btn v-bind="btn" icon="mdi-undo" title="Undo (Ctrl+Z)" @mousedown.prevent="callUndo" />
        <v-btn v-bind="btn" icon="mdi-redo" title="Redo (Ctrl+Y)" @mousedown.prevent="callRedo" />
      </template>
      <span class="text-caption text-secondary ml-2">Typst</span>
      <v-spacer />
      <v-btn
        v-if="canUseTypstRendering"
        v-bind="btn"
        icon="mdi-file-pdf-box"
        title="Export as PDF"
        data-testid="typst-export-pdf"
        :loading="exporting"
        @click="exportPdf"
      />
      <v-btn-toggle
        :model-value="toggleValue"
        mandatory
        density="compact"
        variant="outlined"
        divided
        @update:model-value="(v) => emit('mode-change', v as EditorMode)"
      >
        <v-btn value="raw" size="x-small" title="Plain text editor">Plain</v-btn>
        <v-btn value="formatted_raw" size="x-small" title="Typst text with syntax highlighting">Formatted</v-btn>
        <v-btn v-if="canUseTypstRendering" value="fully_rendered" size="x-small" title="Rendered with the Typst compiler">Preview</v-btn>
      </v-btn-toggle>
    </div>
    <!-- Kept mounted (hidden) during Preview so undo history and caret survive. -->
    <div
      v-show="!previewing"
      ref="editorEl"
      class="typst-editor"
      :class="{ 'is-highlighted': highlighted }"
      spellcheck="false"
    />
    <TypstPreview
      v-if="previewing"
      :vault-id="vaultId"
      :file-path="filePath ?? ''"
      :content="content"
      class="typst-preview"
      @jump="jumpTo"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * Editor for Typst (.typ) notes: CodeJar over the raw text, with Typst
 * syntax highlighting in Formatted mode (utils/typst-highlight.ts). Deliberately
 * leaner than MarkdownEditor: none of its Markdown-specific key handling
 * (list/heading Enter, tables, wiki-link completion) applies to Typst.
 * Preview (#139) renders through the server's Typst compiler
 * (TypstPreview.vue); the Markdown-only Structural mode shows the
 * highlighted editor.
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import TypstPreview from './TypstPreview.vue';
import { ApiError } from '@/api/client';
import { useFilesStore } from '@/stores/files';
import { useCapabilities } from '@/composables/useCapabilities';
import { pdfExportErrorMessage } from '@/utils/pdfExport';
import { useUndoRedo } from '@/composables/useUndoRedo';
import { highlightPlainText } from '@/utils/highlight';
import { renderTypstHighlight } from '@/utils/typst-highlight';
import { getSelectionOffsets, setSelectionOffsets } from '@/editor/selection-offsets';
import type { EditorMode } from '@/api/types';

const props = defineProps<{
  tabId: string;
  vaultId: string;
  content: string;
  filePath: string | null | undefined;
  mode: EditorMode;
}>();

const emit = defineEmits<{
  update: [content: string];
  'mode-change': [mode: EditorMode];
}>();

const btn = { size: 'small', variant: 'text' as const, density: 'compact' as const };
const { canUseTypstRendering } = useCapabilities();
const filesStore = useFilesStore();
const exporting = ref(false);
// Preview compiles on the server; in local (thin-client) mode it isn't offered.
const previewing = computed(() => canUseTypstRendering && props.mode === 'fully_rendered');
const highlighted = computed(() => props.mode !== 'raw');
const toggleValue = computed(() => (previewing.value ? 'fully_rendered' : highlighted.value ? 'formatted_raw' : 'raw'));

const editorEl = ref<HTMLElement | null>(null);
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let jar: any = null;
let ignoreNextChange = false;
let isEditorFocused = false;

const { recordChange, undo, redo, reset: resetUndoRedo } = useUndoRedo(props.content);

function highlight(editor: HTMLElement) {
  if (!highlighted.value) {
    highlightPlainText(editor);
    return;
  }
  const html = renderTypstHighlight(editor.textContent ?? '');
  if (editor.innerHTML !== html) editor.innerHTML = html;
}

function offsets() {
  return getSelectionOffsets(editorEl.value!, jar?.toString()?.length ?? 0);
}

function restoreSelection(start: number, end: number = start) {
  if (!editorEl.value) return;
  setSelectionOffsets(editorEl.value, start, end);
  editorEl.value.focus();
}

/** Replace the whole buffer (undo/redo), keeping the caret where it was. */
function replaceContent(next: string) {
  if (!editorEl.value || next === jar?.toString()) return;
  const { start, end } = offsets();
  ignoreNextChange = true;
  jar.updateCode(next);
  emit('update', next);
  restoreSelection(Math.min(start, next.length), Math.min(end, next.length));
}

function callUndo() {
  const prev = undo();
  if (prev != null) replaceContent(prev);
}

function callRedo() {
  const next = redo();
  if (next != null) replaceContent(next);
}

/**
 * Export the current text (saved or not) as PDF. If the note doesn't
 * compile, open Preview, which lists the errors with jump-to-line.
 */
async function exportPdf() {
  if (!props.vaultId || !props.filePath || exporting.value) return;
  exporting.value = true;
  try {
    await filesStore.exportAsPdf(props.vaultId, props.filePath, jar?.toString() ?? props.content);
  } catch (e) {
    if (e instanceof ApiError && e.status === 422) {
      emit('mode-change', 'fully_rendered');
    } else {
      alert(pdfExportErrorMessage(e));
    }
  } finally {
    exporting.value = false;
  }
}

/** From a Preview diagnostic: back to the editor, caret at line:column. */
async function jumpTo(pos: { line: number; column: number }) {
  emit('mode-change', 'formatted_raw');
  await nextTick();
  if (!editorEl.value) return;
  const lines = (jar?.toString() ?? props.content).split('\n');
  const lineIdx = Math.min(Math.max(pos.line, 1), lines.length) - 1;
  let offset = 0;
  for (let i = 0; i < lineIdx; i += 1) offset += lines[i].length + 1;
  offset += Math.min(Math.max(pos.column - 1, 0), lines[lineIdx].length);
  isEditorFocused = true;
  restoreSelection(offset);
}

function onKeydown(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key === 'z' && !e.shiftKey) {
    e.preventDefault();
    e.stopImmediatePropagation();
    callUndo();
  } else if (mod && (e.key === 'y' || (e.shiftKey && (e.key === 'z' || e.key === 'Z')))) {
    e.preventDefault();
    e.stopImmediatePropagation();
    callRedo();
  }
}

const onFocus = () => { isEditorFocused = true; };
const onBlur = () => { isEditorFocused = false; };

onMounted(async () => {
  if (!editorEl.value) return;
  const { CodeJar } = await import('codejar');
  // addClosing off: CodeJar inserts a closing bracket or quote but never types
  // over it, so typing `f(x)` would leave `f(x))`.
  jar = CodeJar(editorEl.value, highlight, { tab: '  ', history: false, addClosing: false });
  jar.updateCode(props.content);
  jar.onUpdate((code: string) => {
    if (ignoreNextChange) { ignoreNextChange = false; return; }
    recordChange(code);
    emit('update', code);
  });
  editorEl.value.addEventListener('keydown', onKeydown, true);
  editorEl.value.addEventListener('focus', onFocus);
  editorEl.value.addEventListener('blur', onBlur);
});

onUnmounted(() => {
  editorEl.value?.removeEventListener('keydown', onKeydown, true);
  editorEl.value?.removeEventListener('focus', onFocus);
  editorEl.value?.removeEventListener('blur', onBlur);
  jar?.destroy();
  jar = null;
});

// External content changes (file load, WebSocket reload) without re-emitting.
watch(() => props.content, (next) => {
  if (!jar || !editorEl.value || jar.toString() === next) return;
  ignoreNextChange = true;
  if (isEditorFocused) {
    const { start, end } = offsets();
    jar.updateCode(next);
    restoreSelection(Math.min(start, next.length), Math.min(end, next.length));
  } else {
    jar.updateCode(next);
  }
});

watch(highlighted, () => {
  if (!jar || !editorEl.value) return;
  const { start, end } = offsets();
  const code = jar.toString();
  jar.updateCode(code);
  if (isEditorFocused) restoreSelection(Math.min(start, code.length), Math.min(end, code.length));
});

// History is per document; the component is reused across tab switches.
watch(() => [props.tabId, props.filePath], () => resetUndoRedo(props.content));

defineExpose({ callUndo, callRedo });
</script>

<style scoped>
.typst-editor-shell {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.typst-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 36px;
  flex-shrink: 0;
  padding: 0 4px;
  border-bottom: 1px solid rgb(var(--v-theme-border));
  background: rgb(var(--v-theme-surface));
}
.typst-preview {
  flex: 1;
}
.typst-editor {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 16px 24px;
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
  font-size: 14px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  outline: none;
  tab-size: 2;
}
.typst-editor :deep(.typ-heading) { font-weight: 700; color: rgb(var(--v-theme-primary)); }
.typst-editor :deep(.typ-strong) { font-weight: 700; }
.typst-editor :deep(.typ-emph) { font-style: italic; }
.typst-editor :deep(.typ-raw) { color: #b35900; }
.typst-editor :deep(.typ-math) { color: #0b7a75; }
.typst-editor :deep(.typ-code) { color: #4b69c6; }
.typst-editor :deep(.typ-keyword) { color: #d73948; }
.typst-editor :deep(.typ-string) { color: #198810; }
.typst-editor :deep(.typ-comment) { color: #8a8a8a; font-style: italic; }
.typst-editor :deep(.typ-label),
.typst-editor :deep(.typ-ref) { color: #9b4dca; }
.typst-editor :deep(.typ-list-marker) { color: rgb(var(--v-theme-primary)); font-weight: 700; }
.typst-editor :deep(.typ-escape) { color: #8a8a8a; }
</style>
