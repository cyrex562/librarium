<template>
  <div class="typst-editor-shell" data-testid="typst-editor">
    <div class="typst-toolbar">
      <v-btn v-bind="btn" icon="mdi-undo" title="Undo (Ctrl+Z)" @mousedown.prevent="callUndo" />
      <v-btn v-bind="btn" icon="mdi-redo" title="Redo (Ctrl+Y)" @mousedown.prevent="callRedo" />
      <span class="text-caption text-secondary ml-2">Typst</span>
      <v-spacer />
      <v-btn-toggle
        :model-value="highlighted ? 'formatted_raw' : 'raw'"
        mandatory
        density="compact"
        variant="outlined"
        divided
        @update:model-value="(v) => emit('mode-change', v as EditorMode)"
      >
        <v-btn value="raw" size="x-small" title="Plain text editor">Plain</v-btn>
        <v-btn value="formatted_raw" size="x-small" title="Typst text with syntax highlighting">Formatted</v-btn>
      </v-btn-toggle>
    </div>
    <div ref="editorEl" class="typst-editor" :class="{ 'is-highlighted': highlighted }" spellcheck="false" />
  </div>
</template>

<script setup lang="ts">
/**
 * Editor for Typst (.typ) notes: CodeJar over the raw text, with Typst
 * syntax highlighting in Formatted mode (utils/typst-highlight.ts). Deliberately
 * leaner than MarkdownEditor: none of its Markdown-specific key handling
 * (list/heading Enter, tables, wiki-link completion) applies to Typst.
 * Preview arrives with #139; until then, the Markdown-only modes (Preview,
 * Structural) show the highlighted editor.
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useUndoRedo } from '@/composables/useUndoRedo';
import { highlightPlainText } from '@/utils/highlight';
import { renderTypstHighlight } from '@/utils/typst-highlight';
import { getSelectionOffsets, setSelectionOffsets } from '@/editor/selection-offsets';
import type { EditorMode } from '@/api/types';

const props = defineProps<{
  tabId: string;
  content: string;
  filePath: string | null | undefined;
  mode: EditorMode;
}>();

const emit = defineEmits<{
  update: [content: string];
  'mode-change': [mode: EditorMode];
}>();

const btn = { size: 'small', variant: 'text' as const, density: 'compact' as const };
const highlighted = computed(() => props.mode !== 'raw');

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
  jar = CodeJar(editorEl.value, highlight, { tab: '  ', history: false });
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
