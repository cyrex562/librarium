<template>
  <div class="markdown-editor-shell">
    <div ref="editorEl" class="markdown-editor" :class="{ 'is-formatted-mode': mode === 'formatted_raw' }" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue';
import { useUndoRedo } from '@/composables/useUndoRedo';
import { renderFormattedMarkdown, highlightPlainText } from '@/utils/highlight';
import {
  applyMarkdownToolbarCommand,
  type MarkdownToolbarCommand,
  type MarkdownCommandResult,
  type TableCreatePayload,
} from '@/editor/markdown-toolbar';
import {
  findTableAt,
  locateCursor,
  handleTableEnterAt,
  handleTableTabAt,
  type ColumnAlignment,
} from '@/editor/table';
import { applyListIndent } from '@/editor/list-indent';
import { applyLineIndent } from '@/editor/line-indent';
import { applyHeadingSpaceDedent, applyHeadingEnter } from '@/editor/heading-behavior';
import { ApiError } from '@/api/client';
import { useVaultsStore } from '@/stores/vaults';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import type { EditorMode } from '@/api/types';

const foldStateByNoteKey = new Map<string, Set<number>>();

const props = defineProps<{
  tabId: string;
  content: string;
  filePath?: string;
  mode: EditorMode;
}>();

/** Where the caret sits inside a table, for the contextual toolbar group. */
export interface TableContext {
  rowIndex: number;
  colIndex: number;
  rowCount: number;
  colCount: number;
  alignment: ColumnAlignment;
}

const emit = defineEmits<{
  update: [value: string];
  'table-context': [ctx: TableContext | null];
}>();

const editorEl = ref<HTMLElement | null>(null);
let jar: any = null;
let ignoreNextChange = false;
let isEditorFocused = false;
let availableFoldStarts = new Set<number>();
let collapsedFoldStarts = new Set<number>();
const vaultsStore = useVaultsStore();
const filesStore = useFilesStore();
const tabsStore = useTabsStore();

// Bug: this previously passed props.tabId (a UUID string) as the initial
// *content* — the first undo past the first edit would replace the whole
// document with the tab id. useUndoRedo(initialContent, ...) wants the
// actual starting text.
const { recordChange, undo, redo, reset: resetUndoRedo } = useUndoRedo(props.content) as any;

function currentNoteFoldKey(): string {
  return props.filePath ? `path:${props.filePath.toLowerCase()}` : `tab:${props.tabId}`;
}

function loadFoldStateForCurrentNote() {
  collapsedFoldStarts = new Set(foldStateByNoteKey.get(currentNoteFoldKey()) ?? []);
}

function persistFoldStateForCurrentNote() {
  const key = currentNoteFoldKey();
  if (collapsedFoldStarts.size === 0) {
    foldStateByNoteKey.delete(key);
    return;
  }
  foldStateByNoteKey.set(key, new Set(collapsedFoldStarts));
}

function highlightForCurrentMode(editor: HTMLElement) {
  if (props.mode === 'raw') {
    availableFoldStarts = new Set();
    highlightPlainText(editor);
    return;
  }

  const result = renderFormattedMarkdown(editor.textContent ?? '', collapsedFoldStarts);
  availableFoldStarts = new Set(result.foldRegions.map((region) => region.startLine));
  collapsedFoldStarts = new Set([...collapsedFoldStarts].filter((startLine) => availableFoldStarts.has(startLine)));
  persistFoldStateForCurrentNote();

  if (editor.innerHTML !== result.html) {
    editor.innerHTML = result.html;
  }
}

function rerenderWithoutChangingContent() {
  if (!editorEl.value) return;
  const { start, end } = getSelectionOffsets(editorEl.value);
  highlightForCurrentMode(editorEl.value);
  // Restore synchronously (closes the race with CodeJar's own ~30ms
  // debounced highlight/save/restore cycle, which otherwise can snapshot a
  // collapsed (editor, 0) selection right after highlightForCurrentMode
  // replaces the DOM — see setSelectionOffsets' doc comment) and again next
  // frame as a safety net.
  setSelectionOffsets(editorEl.value, start, end);
  requestAnimationFrame(() => {
    if (!editorEl.value) return;
    setSelectionOffsets(editorEl.value, start, end);
  });
}

function onEditorClick(event: MouseEvent) {
  if (props.mode !== 'formatted_raw' || !editorEl.value) return;
  const target = event.target as HTMLElement | null;
  const toggle = target?.closest('.editor-md-fold-toggle') as HTMLElement | null;
  if (!toggle) return;

  event.preventDefault();
  event.stopPropagation();

  const startLine = Number(toggle.dataset.foldStart);
  if (!Number.isFinite(startLine) || !availableFoldStarts.has(startLine)) return;

  const next = new Set(collapsedFoldStarts);
  if (next.has(startLine)) next.delete(startLine);
  else next.add(startLine);
  collapsedFoldStarts = next;
  persistFoldStateForCurrentNote();
  rerenderWithoutChangingContent();
}

function collapseAllFolds() {
  if (props.mode !== 'formatted_raw') return;
  if (availableFoldStarts.size === 0) return;
  collapsedFoldStarts = new Set(availableFoldStarts);
  persistFoldStateForCurrentNote();
  rerenderWithoutChangingContent();
}

function expandAllFolds() {
  if (collapsedFoldStarts.size === 0) return;
  collapsedFoldStarts = new Set();
  persistFoldStateForCurrentNote();
  rerenderWithoutChangingContent();
}

onMounted(async () => {
  if (!editorEl.value) return;
  loadFoldStateForCurrentNote();
  // CodeJar is a ~1kB editor; loaded dynamically from vendor dir or npm
  const { CodeJar } = await import('codejar');
  jar = CodeJar(editorEl.value, highlightForCurrentMode, { tab: '  ' });
  jar.updateCode(props.content);
  jar.onUpdate((code: string) => {
    if (ignoreNextChange) { ignoreNextChange = false; return; }
    recordChange(code);
    emit('update', code);
  });

  editorEl.value.addEventListener('keydown', onKeydown, true);
  editorEl.value.addEventListener('click', onEditorClick, true);
  editorEl.value.addEventListener('focus', onEditorFocus);
  editorEl.value.addEventListener('blur', onEditorBlur);
  // Caret movement drives the contextual table toolbar. keyup covers arrow
  // keys and typing; click covers pointer placement.
  editorEl.value.addEventListener('keyup', onCaretMaybeMoved);
  editorEl.value.addEventListener('click', onCaretMaybeMoved);
});

onUnmounted(() => {
  editorEl.value?.removeEventListener('keydown', onKeydown, true);
  editorEl.value?.removeEventListener('click', onEditorClick, true);
  editorEl.value?.removeEventListener('focus', onEditorFocus);
  editorEl.value?.removeEventListener('blur', onEditorBlur);
  editorEl.value?.removeEventListener('keyup', onCaretMaybeMoved);
  editorEl.value?.removeEventListener('click', onCaretMaybeMoved);
  jar = null;
});

function onEditorFocus() {
  isEditorFocused = true;
}

function onEditorBlur() {
  isEditorFocused = false;
}

// Sync external content changes (e.g. WS reload) without double-emitting.
// Preserves the caret when the editor is focused — this previously had no
// cursor handling at all, which is one of the ways the caret could snap back
// to the top of the document mid-edit: whenever the server round-tripped
// content that differed even slightly from the live buffer (frontmatter
// re-serialization, newline normalization, ...), jar.updateCode() wiped every
// text node with nothing restoring the selection afterward.
watch(() => props.content, (newVal) => {
  if (!jar || !editorEl.value) return;
  if (jar.toString() !== newVal) {
    ignoreNextChange = true;
    if (isEditorFocused) {
      const { start, end } = getSelectionOffsets(editorEl.value);
      jar.updateCode(newVal);
      restoreSelection(Math.min(start, newVal.length), Math.min(end, newVal.length));
    } else {
      jar.updateCode(newVal);
    }
  }
});

watch(() => props.mode, () => {
  if (!jar || !editorEl.value) return;
  const { start, end } = getSelectionOffsets(editorEl.value);
  const code = jar.toString();
  jar.updateCode(code);
  restoreSelection(Math.min(start, code.length), Math.min(end, code.length));
});

watch(() => [props.tabId, props.filePath], () => {
  // Undo/redo history is per-document. MarkdownEditor is a single component
  // instance reused across every tab switch (never remounted per-tab), so
  // without this an undo after switching notes could silently replace the
  // currently open note's content with a *different* note's history.
  resetUndoRedo(props.content);
  availableFoldStarts = new Set();
  loadFoldStateForCurrentNote();
  rerenderWithoutChangingContent();
});

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
    e.preventDefault();
    e.stopImmediatePropagation();
    const prev = undo();
    if (prev != null && prev !== jar?.toString() && editorEl.value) {
      const { start, end } = getSelectionOffsets(editorEl.value);
      ignoreNextChange = true;
      jar?.updateCode(prev);
      emit('update', prev);
      restoreSelection(Math.min(start, prev.length), Math.min(end, prev.length));
    }
    return;
  }

  if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.shiftKey && e.key === 'z'))) {
    e.preventDefault();
    e.stopImmediatePropagation();
    const next = redo();
    if (next != null && next !== jar?.toString() && editorEl.value) {
      const { start, end } = getSelectionOffsets(editorEl.value);
      ignoreNextChange = true;
      jar?.updateCode(next);
      emit('update', next);
      restoreSelection(Math.min(start, next.length), Math.min(end, next.length));
    }
    return;
  }

  // Automatic List Management
  if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
    if (applyHandlerResult(handleTableEnterAt(currentSource(), caretOffset()), e)) return;
    if (handleHeadingEnterKey(e)) return;
    if (handleListEnter(e)) return;
  }

  if (e.key === ' ' && !e.ctrlKey && !e.metaKey && !e.altKey) {
    if (handleHeadingSpaceKey(e)) return;
  }

  if (e.key === 'Tab' && !e.ctrlKey && !e.metaKey && !e.altKey) {
    if (applyHandlerResult(handleTableTabAt(currentSource(), caretOffset(), e.shiftKey), e)) return;
    if (handleListTab(e, e.shiftKey)) return;
    if (e.shiftKey && handleGenericDedent(e)) return;
  }
}

// ── Table behaviour (pure logic lives in @/editor/table) ─────────────────────

function onCaretMaybeMoved() {
  emitTableContext();
}

function currentSource(): string {
  return (jar?.toString() as string) ?? '';
}

/**
 * Last caret position known to be inside the editor.
 *
 * Toolbar menus (`v-menu` + `v-list-item`) take focus when opened, unlike the
 * plain icon buttons which suppress it with `@mousedown.prevent`. Once focus
 * leaves, `window.getSelection()` no longer points into the editor and
 * `getSelectionOffsets` falls back to end-of-document — which silently
 * retargets or no-ops the command. Remembering the last in-editor caret makes
 * menu-driven commands act on the cell the user was actually in.
 */
let lastSelection = { start: 0, end: 0 };

function selectionIsInEditor(): boolean {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || !editorEl.value) return false;
  const range = sel.getRangeAt(0);
  return (
    editorEl.value.contains(range.startContainer) &&
    editorEl.value.contains(range.endContainer)
  );
}

function currentSelection(): { start: number; end: number } {
  if (editorEl.value && selectionIsInEditor()) {
    lastSelection = getSelectionOffsets(editorEl.value);
  }
  return lastSelection;
}

function caretOffset(): number {
  return currentSelection().start;
}

/** Apply a pure handler's result, if it produced one. Returns true if handled. */
function applyHandlerResult(result: MarkdownCommandResult | null, e: KeyboardEvent): boolean {
  if (!result || !jar) return false;
  e.preventDefault();
  e.stopImmediatePropagation();
  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.selectionStart, result.selectionEnd);
  emitTableContext();
  return true;
}

/**
 * Tell the toolbar whether the caret is inside a table, and where. Uses the
 * same `findTableAt` predicate the commands use, so the toolbar can never
 * offer an action the command layer would refuse.
 */
function emitTableContext() {
  if (!jar || !editorEl.value) {
    emit('table-context', null);
    return;
  }
  const content = currentSource();
  const offset = caretOffset();
  const table = findTableAt(content, offset);
  if (!table) {
    emit('table-context', null);
    return;
  }
  const cursor = locateCursor(table, content, offset);
  emit('table-context', {
    rowIndex: cursor.rowIndex,
    colIndex: cursor.colIndex,
    rowCount: table.rows.length,
    colCount: table.header.length,
    alignment: table.alignments[cursor.colIndex] ?? 'none',
  });
}

// ── Automatic List Management ─────────────────────────────────────────────────

function getLineRange(content: string, cursorPos: number): { lineStart: number; lineEnd: number; line: string } {
  const lineStart = content.lastIndexOf('\n', cursorPos - 1) + 1;
  const nlPos = content.indexOf('\n', cursorPos);
  const lineEnd = nlPos === -1 ? content.length : nlPos;
  return { lineStart, lineEnd, line: content.slice(lineStart, lineEnd) };
}

function romanToInt(marker: string): number | null {
  const s = marker.toUpperCase();
  const values: Record<string, number> = { I: 1, V: 5, X: 10, L: 50, C: 100, D: 500, M: 1000 };
  let total = 0;
  for (let i = 0; i < s.length; i += 1) {
    const cur = values[s[i]];
    const next = i + 1 < s.length ? values[s[i + 1]] : 0;
    if (!cur) return null;
    total += cur < next ? -cur : cur;
  }
  return total > 0 ? total : null;
}

function intToRoman(n: number): string {
  const map: Array<[number, string]> = [
    [1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'],
    [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'],
    [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I'],
  ];
  let value = Math.max(1, n);
  let out = '';
  for (const [unit, symbol] of map) {
    while (value >= unit) {
      out += symbol;
      value -= unit;
    }
  }
  return out;
}

function nextOrderedMarker(marker: string): string {
  if (/^\d+$/.test(marker)) return `${parseInt(marker, 10) + 1}`;
  if (/^[a-z]$/.test(marker)) return String.fromCharCode(((marker.charCodeAt(0) - 97 + 1) % 26) + 97);
  if (/^[A-Z]$/.test(marker)) return String.fromCharCode(((marker.charCodeAt(0) - 65 + 1) % 26) + 65);
  if (/^[ivxlcdm]+$/.test(marker)) {
    const num = romanToInt(marker);
    return num ? intToRoman(num + 1).toLowerCase() : 'i';
  }
  if (/^[IVXLCDM]+$/.test(marker)) {
    const num = romanToInt(marker);
    return num ? intToRoman(num + 1) : 'I';
  }
  return marker;
}

function handleListEnter(e: KeyboardEvent): boolean {
  if (!jar || !editorEl.value) return false;
  const content = jar.toString() as string;
  const { start, end } = getSelectionOffsets(editorEl.value);
  if (start !== end) return false;

  const { lineStart, lineEnd, line } = getLineRange(content, start);

  // Detect list type — task must be checked before bullet
  const taskM = line.match(/^(\s*)([-*+]) \[([ xX])\] (.*)/);
  const bulletM = !taskM ? line.match(/^(\s*)([-*+]) (.*)/) : null;
  const orderedM = !taskM && !bulletM ? line.match(/^(\s*)(\d+|[a-zA-Z]|[ivxlcdmIVXLCDM]+)\. (.*)/) : null;

  if (!taskM && !bulletM && !orderedM) return false;

  e.preventDefault();
  e.stopImmediatePropagation();

  let itemContent: string;
  let prefix: string;

  if (taskM) {
    itemContent = taskM[4];
    prefix = `${taskM[1]}${taskM[2]} [ ] `;
  } else if (bulletM) {
    itemContent = bulletM[3];
    prefix = `${bulletM[1]}${bulletM[2]} `;
  } else {
    itemContent = orderedM![3];
    const next = nextOrderedMarker(orderedM![2]);
    prefix = `${orderedM![1]}${next}. `;
  }

  let newContent: string;
  let newCursor: number;

  if (!itemContent) {
    // Empty list item → end the list; leave an empty line
    newContent = content.slice(0, lineStart) + content.slice(lineEnd);
    newCursor = lineStart;
  } else {
    // Continue the list with matching prefix
    newContent = content.slice(0, start) + '\n' + prefix + content.slice(start);
    newCursor = start + 1 + prefix.length;
  }

  recordChange(newContent);
  ignoreNextChange = true;
  jar.updateCode(newContent);
  emit('update', newContent);
  restoreSelection(newCursor, newCursor);
  return true;
}

function handleListTab(e: KeyboardEvent, dedent: boolean): boolean {
  if (!jar || !editorEl.value) return false;
  const content = jar.toString() as string;
  const { start } = getSelectionOffsets(editorEl.value);

  // Delegates all list-parsing, indent, and renumbering to the pure helper.
  // Returns null when the caret isn't on a list item, or when Shift-Tab would
  // outdent past column 0 — in either case we absorb the event without a
  // mutation (matching the previous handler's contract).
  const result = applyListIndent(content, start, dedent ? 'outdent' : 'indent');
  if (result === null) {
    // Not a list line → let the default Tab behavior run.
    const line = getLineRange(content, start).line;
    if (!line.match(/^\s*([-*+]|(\d+|[a-zA-Z]|[ivxlcdmIVXLCDM]+)\.) /)) {
      return false;
    }
    // Was a list line but at top level on Shift-Tab → absorb without changes.
    e.preventDefault();
    e.stopImmediatePropagation();
    return true;
  }

  e.preventDefault();
  e.stopImmediatePropagation();

  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.cursor, result.cursor);
  return true;
}

// Shift-Tab on any line handleListTab didn't already own (i.e. not a list
// item) — dedents the current line, or every line touched by a selection.
// Reached only for Shift-Tab (see onKeydown): plain Tab keeps its existing
// behavior on non-list lines (CodeJar's default insert-at-caret), which
// wasn't reported broken.
function handleGenericDedent(e: KeyboardEvent): boolean {
  if (!jar || !editorEl.value) return false;
  const content = jar.toString() as string;
  const { start, end } = getSelectionOffsets(editorEl.value);
  const result = applyLineIndent(content, start, end, 'outdent');
  if (!result) {
    // Nothing to dedent (no leading whitespace anywhere in range) — absorb
    // the event anyway rather than falling through to CodeJar's own
    // caret-position-based Shift-Tab, which deletes characters before the
    // caret regardless of the line's actual indentation and is unreliable
    // inside formatted-mode's contenteditable="false" spans.
    e.preventDefault();
    e.stopImmediatePropagation();
    return true;
  }

  e.preventDefault();
  e.stopImmediatePropagation();

  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.selectionStart, result.selectionEnd);
  return true;
}

// Enter on a heading line starts the new line at column 1 — never inherits
// the heading's indentation the way CodeJar's default newline handling
// otherwise would (it carries the current line's leading whitespace onto the
// new line).
function handleHeadingEnterKey(e: KeyboardEvent): boolean {
  if (!jar || !editorEl.value) return false;
  const content = jar.toString() as string;
  const { start, end } = getSelectionOffsets(editorEl.value);
  if (start !== end) return false;

  const result = applyHeadingEnter(content, start);
  if (!result) return false;

  e.preventDefault();
  e.stopImmediatePropagation();

  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.cursor, result.cursor);
  return true;
}

// Typing the space that completes "#"/"##"/... on an indented line
// auto-dedents that line back to column 1 — headings never start indented.
function handleHeadingSpaceKey(e: KeyboardEvent): boolean {
  if (!jar || !editorEl.value) return false;
  const content = jar.toString() as string;
  const { start, end } = getSelectionOffsets(editorEl.value);
  if (start !== end) return false;

  const result = applyHeadingSpaceDedent(content, start);
  if (!result) return false;

  e.preventDefault();
  e.stopImmediatePropagation();

  recordChange(result.content);
  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.cursor, result.cursor);
  return true;
}

function dirname(path: string): string {
  const idx = path.lastIndexOf('/');
  return idx >= 0 ? path.slice(0, idx) : '';
}

function basename(path: string): string {
  const idx = path.lastIndexOf('/');
  return idx >= 0 ? path.slice(idx + 1) : path;
}

function basenameWithoutExt(path: string): string {
  return basename(path).replace(/\.md$/i, '');
}

function normalizeNoteFileName(input: string): string {
  const cleaned = input
    .replace(/[\\/:*?"<>|]/g, '')
    .trim()
    .replace(/\s+/g, ' ');
  const fallback = cleaned || 'Extracted Note';
  return /\.md$/i.test(fallback) ? fallback : `${fallback}.md`;
}

function extractCurrentHeadingSection(content: string, cursorPos: number): { start: number; end: number; text: string; title: string } | null {
  const lines = content.split('\n');
  const offsets: number[] = [];
  let acc = 0;
  for (const line of lines) {
    offsets.push(acc);
    acc += line.length + 1;
  }

  let cursorLine = 0;
  for (let i = 0; i < offsets.length; i += 1) {
    const lineStart = offsets[i];
    const lineEnd = lineStart + lines[i].length;
    if (cursorPos >= lineStart && cursorPos <= lineEnd + 1) {
      cursorLine = i;
      break;
    }
  }

  let headingLine = -1;
  let headingLevel = 0;
  for (let i = cursorLine; i >= 0; i -= 1) {
    const m = lines[i].match(/^(#{1,6})\s+(.+)$/);
    if (m) {
      headingLine = i;
      headingLevel = m[1].length;
      break;
    }
  }
  if (headingLine === -1) return null;

  let endLine = lines.length;
  for (let i = headingLine + 1; i < lines.length; i += 1) {
    const m = lines[i].match(/^(#{1,6})\s+(.+)$/);
    if (m && m[1].length <= headingLevel) {
      endLine = i;
      break;
    }
  }

  const start = offsets[headingLine];
  const end = endLine < offsets.length ? offsets[endLine] - 1 : content.length;
  const text = content.slice(start, end).trim();
  const title = (lines[headingLine].match(/^(#{1,6})\s+(.+)$/)?.[2] ?? 'Extracted Note').trim();
  return { start, end, text, title };
}

async function extractSelectionToNote() {
  if (!jar || !editorEl.value) return;
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;

  const source = jar.toString() as string;
  let { start, end } = getSelectionOffsets(editorEl.value);
  let selected = source.slice(start, end).trim();
  let defaultNameHint = selected
    .split(/\r?\n/)[0]
    .replace(/[\[\]#*`>]/g, '')
    .trim()
    .slice(0, 60);

  if (start === end || !selected) {
    const section = extractCurrentHeadingSection(source, start);
    if (!section) {
      alert('Select text or place the cursor inside a heading section to extract.');
      return;
    }
    start = section.start;
    end = section.end;
    selected = section.text;
    defaultNameHint = section.title.slice(0, 60);
  }

  const defaultName = defaultNameHint || 'Extracted Note';

  const userName = prompt('New note file name', defaultName);
  if (userName == null) return;

  const fileName = normalizeNoteFileName(userName);
  const folder = props.filePath ? dirname(props.filePath) : '';
  const targetPath = folder ? `${folder}/${fileName}` : fileName;
  const targetWikilink = basenameWithoutExt(fileName);
  const currentWikilink = basenameWithoutExt(props.filePath ?? 'Current Note');
  const noteBody = `> Extracted from [[${currentWikilink}]]\n\n${selected}\n`;

  let noteExists = false;
  try {
    await filesStore.createFile(vaultId, targetPath, noteBody);
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      noteExists = true;
    } else {
      throw error;
    }
  }

  const replacement = `[[${targetWikilink}]]`;
  const nextContent = `${source.slice(0, start)}${replacement}${source.slice(end)}`;

  recordChange(nextContent);
  ignoreNextChange = true;
  jar.updateCode(nextContent);
  emit('update', nextContent);

  const cursor = start + replacement.length;
  restoreSelection(cursor, cursor);

  tabsStore.openTab(tabsStore.activePaneId, targetPath, fileName);
  if (noteExists) {
    alert(`Note already existed. Linked to [[${targetWikilink}]].`);
  }
}

async function applyCommand(command: MarkdownToolbarCommand, payload?: TableCreatePayload) {
  if (!jar || !editorEl.value) return;

  if (command === 'extract_to_note') {
    await extractSelectionToNote();
    return;
  }

  const source = jar.toString() as string;
  const { start, end } = currentSelection();
  const result = applyMarkdownToolbarCommand(source, start, end, command, payload);

  ignoreNextChange = true;
  jar.updateCode(result.content);
  emit('update', result.content);
  restoreSelection(result.selectionStart, result.selectionEnd);
  emitTableContext();
}

function getSelectionOffsets(root: HTMLElement): { start: number; end: number } {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) {
    const len = jar?.toString()?.length ?? 0;
    return { start: len, end: len };
  }

  const range = selection.getRangeAt(0);
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) {
    const len = jar?.toString()?.length ?? 0;
    return { start: len, end: len };
  }

  const preStart = range.cloneRange();
  preStart.selectNodeContents(root);
  preStart.setEnd(range.startContainer, range.startOffset);

  const preEnd = range.cloneRange();
  preEnd.selectNodeContents(root);
  preEnd.setEnd(range.endContainer, range.endOffset);

  return {
    start: preStart.toString().length,
    end: preEnd.toString().length,
  };
}

function setSelectionOffsets(root: HTMLElement, start: number, end: number) {
  const selection = window.getSelection();
  if (!selection) return;

  // Clamp to the actual available text length first. An out-of-range offset
  // (content shifted slightly between measuring and restoring — e.g. a
  // stale offset from before an external update) used to fall all the way
  // through to the root.focus() fallback below, which drops the caret at
  // the very start of the document — this is the "cursor randomly jumps to
  // the top" bug. Clamping means that fallback is now only ever reached for
  // a genuinely empty document, where focus-at-start is correct.
  const totalLength = root.textContent?.length ?? 0;
  const clampedStart = Math.max(0, Math.min(start, totalLength));
  const clampedEnd = Math.max(clampedStart, Math.min(end, totalLength));

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let currentOffset = 0;
  let startNode: Text | null = null;
  let endNode: Text | null = null;
  let startNodeOffset = 0;
  let endNodeOffset = 0;

  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const nextOffset = currentOffset + node.textContent!.length;

    if (!startNode && clampedStart <= nextOffset) {
      startNode = node;
      startNodeOffset = Math.max(0, clampedStart - currentOffset);
    }

    if (!endNode && clampedEnd <= nextOffset) {
      endNode = node;
      endNodeOffset = Math.max(0, clampedEnd - currentOffset);
      break;
    }

    currentOffset = nextOffset;
  }

  if (!startNode || !endNode) {
    root.focus();
    return;
  }

  const range = document.createRange();
  range.setStart(startNode, Math.min(startNodeOffset, startNode.length));
  range.setEnd(endNode, Math.min(endNodeOffset, endNode.length));
  selection.removeAllRanges();
  selection.addRange(range);
}

// Restore the caret synchronously — closes the race with CodeJar's own
// ~30ms debounced highlight/save/restore cycle (see setSelectionOffsets'
// doc comment) outright, since 0ms is always ahead of a 30ms debounce; no
// rAF wait is needed for that. Also re-applies once more next frame as a
// safety net for anything that still moves the caret between here and then
// (CodeJar's own restore, a fold re-render, ...) — but ONLY if the caret is
// still exactly where we just put it. Without that guard, fast typing right
// after a handler-triggered mutation (e.g. typing the rest of a heading
// right after the auto-dedent-on-space rewrite) could race the rAF: it
// fires with the *stale* pre-typing position captured in its closure and
// snaps the caret backward mid-word, splitting whatever was typed in
// between. Confirmed via browser testing — this exact scenario corrupted
// "New Heading" into "w HeadingNe" before this guard was added.
function restoreSelection(start: number, end: number = start) {
  if (!editorEl.value) return;
  setSelectionOffsets(editorEl.value, start, end);
  editorEl.value.focus();
  requestAnimationFrame(() => {
    if (!editorEl.value) return;
    const current = getSelectionOffsets(editorEl.value);
    if (current.start !== start || current.end !== end) return;
    setSelectionOffsets(editorEl.value, start, end);
    editorEl.value.focus();
  });
}

// ── Exposed API for EditorPane toolbar ───────────────────────────────────────

function callUndo() {
  const prev = undo();
  if (prev != null && prev !== jar?.toString() && editorEl.value) {
    const { start, end } = getSelectionOffsets(editorEl.value);
    ignoreNextChange = true;
    jar?.updateCode(prev);
    emit('update', prev);
    restoreSelection(Math.min(start, prev.length), Math.min(end, prev.length));
  }
}

function callRedo() {
  const next = redo();
  if (next != null && next !== jar?.toString() && editorEl.value) {
    const { start, end } = getSelectionOffsets(editorEl.value);
    ignoreNextChange = true;
    jar?.updateCode(next);
    emit('update', next);
    restoreSelection(Math.min(start, next.length), Math.min(end, next.length));
  }
}

defineExpose({ applyCommand, callUndo, callRedo, collapseAllFolds, expandAllFolds });
</script>

<style scoped>
.markdown-editor-shell {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.markdown-editor {
  flex: 1;
  font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
  font-size: 14px;
  line-height: 1.6;
  padding: 16px;
  outline: none;
  white-space: pre-wrap;
  overflow-wrap: break-word;
  background: rgb(var(--v-theme-background));
  color: rgb(var(--v-theme-on-background));
  overflow-y: auto;
  tab-size: 2;
  caret-color: rgb(var(--v-theme-primary));
}

.markdown-editor.is-formatted-mode {
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  font-size: 15px;
  line-height: 1.8;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-syntax) {
  color: rgba(var(--v-theme-on-background), 0.34);
}

.markdown-editor.is-formatted-mode :deep(.editor-md-line) {
  display: inline;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-line.is-hidden) {
  display: none;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-line.is-nested) {
  margin-left: calc(var(--fold-depth) * 4px);
  padding-left: calc(var(--fold-depth) * 6px);
  border-left: 1px solid rgba(var(--v-theme-primary), 0.18);
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-toggle),
.markdown-editor.is-formatted-mode :deep(.editor-md-fold-spacer) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.25em;
  height: 1.25em;
  margin-right: 0.25em;
  color: rgba(var(--v-theme-on-background), 0.55);
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-toggle::before) {
  content: '▾';
  font-size: 0.92em;
  line-height: 1;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-toggle.is-collapsed::before) {
  content: '▸';
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-toggle) {
  cursor: pointer;
  border-radius: 0.25em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-toggle:hover) {
  background: rgba(var(--v-theme-primary), 0.12);
  color: rgb(var(--v-theme-primary));
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-summary) {
  display: inline-flex;
  align-items: center;
  margin-left: 0.5em;
  padding: 0.05em 0.45em;
  border-radius: 999px;
  background: rgba(var(--v-theme-primary), 0.1);
  color: rgb(var(--v-theme-secondary));
  font-size: 0.86em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fold-summary::before) {
  content: '… ' attr(data-hidden-lines) ' lines hidden';
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading) {
  color: rgb(var(--v-theme-on-background));
  font-weight: 700;
  display: inline-block;
  margin: 0.18em 0;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading-1) {
  font-size: 1.9em;
  letter-spacing: -0.02em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading-2) {
  font-size: 1.55em;
  letter-spacing: -0.015em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading-3) {
  font-size: 1.3em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading-4) {
  font-size: 1.15em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-heading-5),
.markdown-editor.is-formatted-mode :deep(.editor-md-heading-6) {
  font-size: 1.05em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-blockquote) {
  display: inline-block;
  padding: 0.05em 0 0.05em 0.9em;
  margin: 0.08em 0;
  border-left: 3px solid rgba(var(--v-theme-primary), 0.75);
  color: rgb(var(--v-theme-secondary));
  background: rgba(var(--v-theme-primary), 0.06);
}

.markdown-editor.is-formatted-mode :deep(.editor-md-blockquote-content) {
  font-style: italic;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-list-marker) {
  color: rgb(var(--v-theme-primary));
  font-weight: 700;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-checkbox) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.4em;
  padding: 0 0.2em;
  border-radius: 0.35em;
  border: 1px solid rgba(var(--v-theme-on-background), 0.25);
  background: rgba(var(--v-theme-on-background), 0.06);
  color: rgb(var(--v-theme-secondary));
  font-size: 0.9em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-checkbox.is-checked) {
  border-color: rgba(var(--v-theme-primary), 0.85);
  background: rgba(var(--v-theme-primary), 0.18);
  color: rgb(var(--v-theme-primary));
}

.markdown-editor.is-formatted-mode :deep(.editor-md-task-text) {
  color: rgb(var(--v-theme-on-background));
}

.markdown-editor.is-formatted-mode :deep(.editor-md-wikilink),
.markdown-editor.is-formatted-mode :deep(.editor-md-link) {
  color: rgb(var(--v-theme-primary));
  background: rgba(var(--v-theme-primary), 0.1);
  border-radius: 0.4em;
  padding: 0.04em 0.28em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-tag) {
  color: rgb(var(--v-theme-tag));
  background: rgba(var(--v-theme-tag), 0.12);
  border: 1px solid rgba(var(--v-theme-tag), 0.2);
  border-radius: 0.42em;
  padding: 0.03em 0.3em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-inline-code) {
  color: rgb(var(--v-theme-inline-code));
  background: rgba(var(--v-theme-inline-code), 0.12);
  border: 1px solid rgba(var(--v-theme-inline-code), 0.16);
  border-radius: 0.4em;
  padding: 0.06em 0.32em;
  font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
  font-size: 0.92em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-strong) {
  color: rgb(var(--v-theme-on-background));
  font-weight: 700;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-emphasis) {
  color: rgb(var(--v-theme-on-background));
  font-style: italic;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-strikethrough) {
  color: rgb(var(--v-theme-secondary));
  text-decoration: line-through;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-highlight) {
  color: rgb(var(--v-theme-on-background));
  background: rgba(250, 204, 21, 0.18);
  border-radius: 0.28em;
  padding: 0 0.1em;
}

.markdown-editor.is-formatted-mode :deep(.editor-md-fence),
.markdown-editor.is-formatted-mode :deep(.editor-md-hr) {
  color: rgb(var(--v-theme-secondary));
}

.markdown-editor.is-formatted-mode :deep(.editor-md-table-row) {
  display: inline-block;
  width: fit-content;
  min-width: min(100%, 480px);
  padding: 0.05em 0.4em;
  border-radius: 0.28em;
  background: rgba(var(--v-theme-on-background), 0.04);
}

.markdown-editor.is-formatted-mode :deep(.editor-md-table-divider) {
  color: rgb(var(--v-theme-secondary));
  background: rgba(var(--v-theme-primary), 0.08);
}
</style>
