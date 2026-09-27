<template>
  <div class="typst-preview-shell" data-testid="typst-preview">
    <div v-if="unavailable" class="pa-4 text-caption text-secondary" data-testid="typst-preview-unavailable">
      {{ unavailable }}
    </div>
    <template v-else>
      <div v-if="diagnostics.length" class="typst-diagnostics" data-testid="typst-diagnostics">
        <div
          v-for="(d, i) in diagnostics"
          :key="i"
          class="typst-diagnostic"
          :class="`is-${d.severity}`"
        >
          <v-icon
            :icon="d.severity === 'error' ? 'mdi-alert-circle-outline' : 'mdi-alert-outline'"
            size="small"
            class="mr-1"
          />
          <button
            v-if="d.line"
            type="button"
            class="typst-diagnostic-loc"
            :title="`Go to line ${d.line} in the editor`"
            @click="emit('jump', { line: d.line, column: d.column ?? 1 })"
          >
            Line {{ d.line }}
          </button>
          <span class="typst-diagnostic-msg">{{ d.message }}</span>
          <div v-for="(h, j) in d.hints" :key="j" class="typst-diagnostic-hint">hint: {{ h }}</div>
        </div>
      </div>
      <!-- The last successful render stays up while the note has errors. -->
      <div
        ref="bodyEl"
        class="typst-preview-body"
        :class="{ 'is-stale': stale }"
        data-testid="typst-preview-body"
        v-html="html"
      />
    </template>
  </div>
</template>

<script lang="ts">
/**
 * Last successful render per note ("vaultId:path"), shared across mounts:
 * the preview unmounts whenever the editor is shown, and coming back should
 * show the previous render straight away (dimmed if the current text has
 * errors) instead of a blank pane.
 */
const lastGood = new Map<string, string>();
</script>

<script setup lang="ts">
/**
 * Preview for Typst notes (#139): the server compiles the current text with
 * the Typst compiler's HTML export (POST /api/vaults/{id}/render-typst).
 * Requests are debounced while typing and a response that arrives after a
 * newer one was sent is dropped. Compile errors are listed with their line;
 * clicking one asks the editor to jump there.
 */
import { onUnmounted, ref, watch } from 'vue';
import DOMPurify from 'dompurify';
import { ApiError, apiRenderTypst, type TypstDiagnostic } from '@/api/client';

const props = defineProps<{
  vaultId: string;
  filePath: string;
  content: string;
}>();

const emit = defineEmits<{
  jump: [pos: { line: number; column: number }];
}>();

const html = ref('');
const stale = ref(false);
const diagnostics = ref<TypstDiagnostic[]>([]);
const unavailable = ref('');
const bodyEl = ref<HTMLElement | null>(null);

let timer: ReturnType<typeof setTimeout> | null = null;
let latest = 0;

async function render() {
  if (!props.vaultId || !props.filePath) return;
  const seq = ++latest;
  try {
    const result = await apiRenderTypst(props.vaultId, props.filePath, props.content);
    if (seq !== latest) return;
    diagnostics.value = result.diagnostics;
    if (result.html !== null) {
      // Typst's stylesheet only targets MathML elements; keep it with the body.
      const css = result.css ? `<style>${result.css.replaceAll('</', '<\\/')}</style>` : '';
      html.value = DOMPurify.sanitize(css + result.html, {
        USE_PROFILES: { html: true, mathMl: true, svg: true },
        ADD_TAGS: ['style'],
        FORCE_BODY: true,
      });
      stale.value = false;
      lastGood.set(cacheKey(), html.value);
    } else {
      stale.value = html.value !== '';
    }
  } catch (e) {
    if (seq !== latest) return;
    if (e instanceof ApiError && e.status === 501) {
      unavailable.value = "Typst preview isn't available: this server was built without Typst support.";
    } else if (e instanceof ApiError && e.status === 404) {
      unavailable.value = "Typst preview isn't available here.";
    } else {
      diagnostics.value = [{
        severity: 'error',
        message: `Couldn't render the preview: ${e instanceof Error ? e.message : String(e)}`,
        line: null,
        column: null,
        hints: [],
      }];
      stale.value = html.value !== '';
    }
  }
}

function cacheKey() {
  return `${props.vaultId}:${props.filePath}`;
}

watch(() => [props.vaultId, props.filePath], () => {
  html.value = lastGood.get(cacheKey()) ?? '';
  stale.value = false;
  diagnostics.value = [];
  unavailable.value = '';
  void render();
}, { immediate: true });

watch(() => props.content, () => {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void render(), 300);
});

onUnmounted(() => {
  if (timer) clearTimeout(timer);
  latest += 1;
});
</script>

<style scoped>
.typst-preview-shell {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
}
.typst-diagnostics {
  flex-shrink: 0;
  padding: 8px 16px;
  border-bottom: 1px solid rgb(var(--v-theme-border));
  font-size: 13px;
}
.typst-diagnostic { padding: 2px 0; }
.typst-diagnostic.is-error { color: rgb(var(--v-theme-error)); }
.typst-diagnostic.is-warning { color: rgb(var(--v-theme-warning)); }
.typst-diagnostic-loc {
  margin-right: 6px;
  font-weight: 600;
  text-decoration: underline;
  cursor: pointer;
  color: inherit;
}
.typst-diagnostic-msg { color: rgb(var(--v-theme-on-surface)); }
.typst-diagnostic-hint {
  margin-left: 24px;
  color: rgb(var(--v-theme-secondary));
}
.typst-preview-body {
  padding: 16px 24px;
  line-height: 1.6;
  max-width: 900px;
}
.typst-preview-body.is-stale { opacity: 0.55; }
.typst-preview-body :deep(table) { border-collapse: collapse; margin: 12px 0; }
.typst-preview-body :deep(th),
.typst-preview-body :deep(td) {
  border: 1px solid rgb(var(--v-theme-border));
  padding: 4px 10px;
  vertical-align: top;
}
.typst-preview-body :deep(th) { background: rgba(var(--v-theme-on-surface), 0.06); font-weight: 600; }
.typst-preview-body :deep(pre) {
  background: rgba(var(--v-theme-on-surface), 0.05);
  padding: 10px 12px;
  border-radius: 4px;
  overflow-x: auto;
}
.typst-preview-body :deep(img) { max-width: 100%; }
.typst-preview-body :deep(math[display="block"]) { display: block; margin: 12px 0; text-align: center; }
</style>
