<template>
  <div ref="previewEl" class="markdown-preview markdown-body pa-4" v-html="renderedHtml" />
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import DOMPurify from 'dompurify';
import { applyTableMerges } from '@/editor/table-merge-layout';
import { apiRenderMarkdownInVault, apiResolveWikiLink } from '@/api/client';
import { openExternalUrl } from '@/utils/tauri';
import { useTabsStore } from '@/stores/tabs';

let mermaidReady = false;
async function getMermaid() {
  const mod = await import('mermaid');
  const m = mod.default;
  if (!mermaidReady) {
    m.initialize({ startOnLoad: false, theme: 'default', securityLevel: 'loose' });
    mermaidReady = true;
  }
  return m;
}

const props = defineProps<{
  content: string;
  vaultId: string;
  currentFile?: string;
}>();

const tabsStore = useTabsStore();
const previewEl = ref<HTMLElement | null>(null);
const renderedHtml = ref('');
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

onMounted(() => {
  render();
  previewEl.value?.addEventListener('click', onPreviewClick);
});

onUnmounted(() => {
  previewEl.value?.removeEventListener('click', onPreviewClick);
});

watch(() => [props.content, props.vaultId, props.currentFile], () => {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(render, 400);
});

async function render() {
  if (!props.vaultId || !props.content) {
    renderedHtml.value = '';
    return;
  }
  try {
    const html = await apiRenderMarkdownInVault(props.vaultId, props.content, props.currentFile);
    // Sanitize server-returned HTML before inserting into DOM (XSS prevention)
    renderedHtml.value = DOMPurify.sanitize(html, {
      ADD_TAGS: ['iframe'],
      ADD_ATTR: ['allow', 'allowfullscreen', 'frameborder', 'scrolling'],
    });
    await nextTick();
    // `^^` / `<<` merged cells: the server renderer has no spans, so it emits
    // the markers as text — apply them here, with the same rules as the editor.
    if (previewEl.value) applyTableMerges(previewEl.value);
    attachCodeBlockCopyButtons();
    void renderMermaidDiagrams();
  } catch {
    // Fall through silently on transient errors; keep last rendered content
  }
}

function attachCodeBlockCopyButtons() {
  const root = previewEl.value;
  if (!root) return;

  const codeBlocks = root.querySelectorAll('pre > code:not(.language-mermaid)');
  codeBlocks.forEach((codeEl) => {
    const pre = codeEl.parentElement as HTMLElement | null;
    if (!pre) return;
    if (pre.querySelector('.code-copy-btn')) return;

    pre.style.position = 'relative';

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'code-copy-btn';
    btn.textContent = 'Copy';
    btn.setAttribute('aria-label', 'Copy code block');
    btn.title = 'Copy code block';

    btn.addEventListener('click', async (event) => {
      event.preventDefault();
      event.stopPropagation();

      const text = codeEl.textContent ?? '';
      if (!text) return;

      try {
        await navigator.clipboard.writeText(text);
        const prev = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => {
          btn.textContent = prev;
        }, 1200);
      } catch {
        btn.textContent = 'Failed';
        setTimeout(() => {
          btn.textContent = 'Copy';
        }, 1200);
      }
    });

    pre.appendChild(btn);
  });
}

async function renderMermaidDiagrams() {
  const root = previewEl.value;
  if (!root) return;

  const mermaidBlocks = root.querySelectorAll('pre > code.language-mermaid');
  if (!mermaidBlocks.length) return;

  let m: Awaited<ReturnType<typeof getMermaid>>;
  try {
    m = await getMermaid();
  } catch {
    return;
  }

  let idx = 0;
  for (const codeEl of mermaidBlocks) {
    const pre = codeEl.parentElement;
    if (!pre || pre.dataset.mermaidRendered) continue;

    const text = codeEl.textContent ?? '';
    const id = `mermaid-render-${Date.now()}-${idx++}`;
    try {
      const { svg } = await m.render(id, text);
      const container = document.createElement('div');
      container.className = 'mermaid-diagram';
      // Sanitize the SVG produced by mermaid before inserting
      container.innerHTML = DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true, svgFilters: true } });
      pre.replaceWith(container);
    } catch {
      pre.dataset.mermaidRendered = 'error';
    }
  }
}

async function onPreviewClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  const anchor = target?.closest('a') as HTMLAnchorElement | null;
  if (!anchor) return;

  if (anchor.classList.contains('wiki-link')) {
    event.preventDefault();
    event.stopPropagation();

    const originalLink = anchor.getAttribute('data-original-link') ?? anchor.textContent ?? '';
    const [linkTarget] = originalLink.split('#');
    if (!props.vaultId || !linkTarget) return;

    try {
      const resolved = await apiResolveWikiLink(props.vaultId, linkTarget, props.currentFile);
      if (resolved.exists) {
        tabsStore.openTab(tabsStore.activePaneId, resolved.path, resolved.path.split('/').pop() ?? resolved.path);
      }
    } catch {
      // no-op for now
    }
    return;
  }

  const href = anchor.getAttribute('href');
  if (!href) return;

  // External URLs: route to the OS default handler. In a Tauri WebView, a
  // plain click on an http/https anchor otherwise navigates the WebView
  // itself away from the app shell — hence the explicit invoke through
  // `openExternalUrl` (which uses `window.open` in a browser context).
  if (isExternalUrl(href)) {
    event.preventDefault();
    event.stopPropagation();
    void openExternalUrl(href);
    return;
  }

  // In-page fragment anchors let the browser handle scroll-to-target.
  if (href.startsWith('#')) return;

  // Anything else is treated as a relative path to a vault file.
  event.preventDefault();
  tabsStore.openTab(tabsStore.activePaneId, href, href.split('/').pop() ?? href);
}

// Kept in sync with the Rust-side scheme allow-list in
// `crates/librarium-tauri/src/main.rs::is_allowed_external_url`.
function isExternalUrl(href: string): boolean {
  return /^(https?:|mailto:|tel:)/i.test(href);
}
</script>

<style scoped>
.markdown-preview {
  flex: 1;
  overflow-y: auto;
  background: rgb(var(--v-theme-background));
  color: rgb(var(--v-theme-on-background));
  border-left: 1px solid rgb(var(--v-theme-border));
  font-size: 15px;
  line-height: 1.7;
}

:deep(.code-copy-btn) {
  position: absolute;
  top: 8px;
  right: 8px;
  border: 1px solid rgb(var(--v-theme-border));
  background: rgba(var(--v-theme-surface), 0.92);
  color: rgb(var(--v-theme-on-background));
  border-radius: 6px;
  padding: 2px 8px;
  font-size: 12px;
  cursor: pointer;
  transition: background 0.12s ease;
}

:deep(.code-copy-btn:hover) {
  background: rgba(var(--v-theme-primary), 0.25);
}
</style>
