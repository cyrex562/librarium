<template>
  <div
    ref="containerRef"
    class="canvas-view"
    tabindex="0"
    @mousedown.self="onBgMousedown"
    @wheel.prevent="onWheel"
    @keydown="onKeydown"
  >
    <!-- SVG layer for edges (rendered below nodes) -->
    <svg class="canvas-edges" :style="svgStyle">
      <defs>
        <marker
          v-for="color in edgeColors"
          :key="color"
          :id="`arrow-${color}`"
          markerWidth="8"
          markerHeight="8"
          refX="6"
          refY="3"
          orient="auto"
        >
          <path d="M0,0 L0,6 L8,3 z" :fill="color" />
        </marker>
      </defs>
      <g v-for="edge in canvasData.edges" :key="edge.id">
        <path
          :d="edgePath(edge)"
          :stroke="edge.color || '#888'"
          stroke-width="2"
          fill="none"
          :marker-end="`url(#arrow-${edge.color || '#888'})`"
          class="canvas-edge-path"
          @click="selectEdge(edge.id, $event)"
          :class="{ selected: selectedIds.has(edge.id) }"
        />
        <text
          v-if="edge.label"
          :x="edgeMidpoint(edge).x"
          :y="edgeMidpoint(edge).y"
          class="canvas-edge-label"
          text-anchor="middle"
          dominant-baseline="middle"
        >{{ edge.label }}</text>
      </g>
      <!-- In-progress edge while drawing -->
      <path
        v-if="drawingEdge"
        :d="drawingEdgePath"
        stroke="#555"
        stroke-width="2"
        stroke-dasharray="6 3"
        fill="none"
      />
    </svg>

    <!-- Node layer -->
    <div
      v-for="node in canvasData.nodes"
      :key="node.id"
      class="canvas-node"
      :class="[`canvas-node--${node.type}`, { selected: selectedIds.has(node.id) }]"
      :style="nodeStyle(node)"
      @mousedown.stop="onNodeMousedown(node, $event)"
      @dblclick.stop="onNodeDblclick(node)"
    >
      <!-- Group node -->
      <template v-if="node.type === 'group'">
        <div class="canvas-node__group-label">{{ (node as CanvasGroupNode).label }}</div>
      </template>

      <!-- Text node -->
      <template v-else-if="node.type === 'text'">
        <div
          v-if="editingNodeId !== node.id"
          class="canvas-node__text"
          v-html="renderMarkdown((node as CanvasTextNode).text)"
        />
        <textarea
          v-else
          ref="editTextareaRef"
          class="canvas-node__textarea"
          :value="(node as CanvasTextNode).text"
          @input="onTextInput(node.id, ($event.target as HTMLTextAreaElement).value)"
          @blur="commitTextEdit(node.id)"
          @keydown.esc.stop="commitTextEdit(node.id)"
        />
      </template>

      <!-- File node -->
      <template v-else-if="node.type === 'file'">
        <div class="canvas-node__file-header">
          <v-icon size="small" class="mr-1">mdi-file-document-outline</v-icon>
          <span class="canvas-node__filename">{{ fileBasename((node as CanvasFileNode).file) }}</span>
        </div>
        <div class="canvas-node__file-path">{{ (node as CanvasFileNode).file }}</div>
      </template>

      <!-- Link node -->
      <template v-else-if="node.type === 'link'">
        <div class="canvas-node__link-header">
          <v-icon size="small" class="mr-1">mdi-link</v-icon>
          <a :href="(node as CanvasLinkNode).url" target="_blank" rel="noopener" @click.stop>
            {{ (node as CanvasLinkNode).url }}
          </a>
        </div>
      </template>

      <!-- Port anchors for edge drawing -->
      <div
        v-for="side in (['top','right','bottom','left'] as const)"
        :key="side"
        class="canvas-port"
        :class="`canvas-port--${side}`"
        @mousedown.stop="startEdge(node.id, side, $event)"
      />

      <!-- Resize handle -->
      <div class="canvas-resize-handle" @mousedown.stop="onResizeStart(node, $event)" />
    </div>

    <!-- Toolbar -->
    <div class="canvas-toolbar">
      <v-btn-group density="compact" variant="elevated">
        <v-btn icon="mdi-text" size="small" title="Add text node" @click="addTextNode" />
        <v-btn icon="mdi-file-document-outline" size="small" title="Add file node" @click="addFileNode" />
        <v-btn icon="mdi-link" size="small" title="Add link node" @click="addLinkNode" />
        <v-btn icon="mdi-select-group" size="small" title="Add group" @click="addGroupNode" />
        <v-divider vertical />
        <v-btn icon="mdi-delete-outline" size="small" title="Delete selected" :disabled="selectedIds.size === 0" @click="deleteSelected" />
        <v-divider vertical />
        <v-btn icon="mdi-fit-to-screen" size="small" title="Fit to screen" @click="fitToScreen" />
        <v-btn icon="mdi-magnify-plus-outline" size="small" title="Zoom in" @click="zoom(1.2)" />
        <v-btn icon="mdi-magnify-minus-outline" size="small" title="Zoom out" @click="zoom(1 / 1.2)" />
      </v-btn-group>
      <span class="canvas-toolbar__zoom">{{ Math.round(viewport.scale * 100) }}%</span>
    </div>

    <!-- Empty state -->
    <div v-if="canvasData.nodes.length === 0" class="canvas-empty">
      <v-icon size="64" style="opacity: 0.2">mdi-vector-square</v-icon>
      <p class="mt-3 text-caption text-secondary">Double-click to add a text node, or use the toolbar above.</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch, nextTick, onMounted, onUnmounted, defineAsyncComponent } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import { useFilesStore } from '@/stores/files';
import { useTabsStore } from '@/stores/tabs';
import type {
    CanvasData, CanvasNode, CanvasEdge, CanvasNodeSide,
    CanvasTextNode, CanvasFileNode, CanvasLinkNode, CanvasGroupNode,
} from '@/api/types';

const props = defineProps<{ filePath: string }>();

const vaultsStore = useVaultsStore();
const filesStore = useFilesStore();
const tabsStore = useTabsStore();

// ── State ─────────────────────────────────────────────────────────────────────

const containerRef = ref<HTMLElement | null>(null);
const editTextareaRef = ref<HTMLTextAreaElement | null>(null);

const canvasData = reactive<CanvasData>({ nodes: [], edges: [] });
const selectedIds = ref<Set<string>>(new Set());
const editingNodeId = ref<string | null>(null);

const viewport = reactive({ x: 0, y: 0, scale: 1 });

// Panning state
const panning = ref(false);
let panStart = { mx: 0, my: 0, vx: 0, vy: 0 };

// Dragging nodes state
const dragging = ref(false);
let dragStart: { mx: number; my: number; nodes: { id: string; x: number; y: number }[] } | null = null;

// Resizing state
const resizing = ref(false);
let resizeState: { node: CanvasNode; mx: number; my: number; ow: number; oh: number } | null = null;

// Edge-drawing state
const drawingEdge = ref(false);
let edgeFrom: { nodeId: string; side: CanvasNodeSide } | null = null;
let drawCursor = { x: 0, y: 0 };
const drawingEdgePath = ref('');

// Save debounce
let saveTimer: ReturnType<typeof setTimeout> | null = null;

// ── Load / save ────────────────────────────────────────────────────────────────

async function loadCanvas() {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId || !props.filePath) return;
    try {
        const fc = await filesStore.readFile(vaultId, props.filePath);
        const parsed: CanvasData = fc.content ? JSON.parse(fc.content) : { nodes: [], edges: [] };
        canvasData.nodes = parsed.nodes ?? [];
        canvasData.edges = parsed.edges ?? [];
    } catch {
        canvasData.nodes = [];
        canvasData.edges = [];
    }
}

function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveCanvas, 1000);
}

async function saveCanvas() {
    const vaultId = vaultsStore.activeVaultId;
    if (!vaultId || !props.filePath) return;
    const content = JSON.stringify({ nodes: canvasData.nodes, edges: canvasData.edges }, null, 2);
    try {
        const saved = await filesStore.writeFile(vaultId, props.filePath, { content });
        const tab = [...tabsStore.tabs.values()].find(t => t.filePath === props.filePath);
        if (tab) tabsStore.markTabClean(tab.id, saved.modified);
    } catch { /* best-effort */ }
}

onMounted(async () => {
    await loadCanvas();
    fitToScreen();
});

onUnmounted(() => {
    if (saveTimer) { clearTimeout(saveTimer); void saveCanvas(); }
    window.removeEventListener('mousemove', onGlobalMousemove);
    window.removeEventListener('mouseup', onGlobalMouseup);
});

watch(() => props.filePath, async () => {
    await loadCanvas();
    fitToScreen();
});

// ── Viewport helpers ──────────────────────────────────────────────────────────

const svgStyle = computed(() => ({
    position: 'absolute',
    inset: '0',
    width: '100%',
    height: '100%',
    overflow: 'visible',
    pointerEvents: 'none',
}));

function nodeStyle(node: CanvasNode) {
    const tx = viewport.x + node.x * viewport.scale;
    const ty = viewport.y + node.y * viewport.scale;
    return {
        position: 'absolute',
        left: `${tx}px`,
        top: `${ty}px`,
        width: `${node.width * viewport.scale}px`,
        height: `${node.height * viewport.scale}px`,
        backgroundColor: node.type === 'group' ? ((node as CanvasGroupNode).background ?? 'rgba(100,100,200,0.08)') : undefined,
        borderColor: node.color ?? undefined,
        fontSize: `${Math.max(10, 14 * viewport.scale)}px`,
    };
}

// Convert screen coords to canvas coords
function screenToCanvas(sx: number, sy: number) {
    const rect = containerRef.value!.getBoundingClientRect();
    return {
        x: (sx - rect.left - viewport.x) / viewport.scale,
        y: (sy - rect.top - viewport.y) / viewport.scale,
    };
}

function fitToScreen() {
    if (!containerRef.value || canvasData.nodes.length === 0) {
        viewport.x = 0; viewport.y = 0; viewport.scale = 1;
        return;
    }
    const rect = containerRef.value.getBoundingClientRect();
    const padding = 48;
    const minX = Math.min(...canvasData.nodes.map(n => n.x));
    const minY = Math.min(...canvasData.nodes.map(n => n.y));
    const maxX = Math.max(...canvasData.nodes.map(n => n.x + n.width));
    const maxY = Math.max(...canvasData.nodes.map(n => n.y + n.height));
    const cw = maxX - minX || 1, ch = maxY - minY || 1;
    const scale = Math.min(
        (rect.width - padding * 2) / cw,
        (rect.height - padding * 2) / ch,
        1.5,
    );
    viewport.scale = scale;
    viewport.x = (rect.width - cw * scale) / 2 - minX * scale;
    viewport.y = (rect.height - ch * scale) / 2 - minY * scale;
}

function zoom(factor: number, cx?: number, cy?: number) {
    const rect = containerRef.value!.getBoundingClientRect();
    const pivotX = cx ?? rect.width / 2;
    const pivotY = cy ?? rect.height / 2;
    const newScale = Math.min(4, Math.max(0.1, viewport.scale * factor));
    viewport.x = pivotX - (pivotX - viewport.x) * (newScale / viewport.scale);
    viewport.y = pivotY - (pivotY - viewport.y) * (newScale / viewport.scale);
    viewport.scale = newScale;
}

// ── Background interactions ───────────────────────────────────────────────────

function onBgMousedown(e: MouseEvent) {
    if (e.button === 1 || (e.button === 0 && e.altKey)) {
        // Middle-mouse or Alt+drag → pan
        startPan(e);
    } else if (e.button === 0) {
        selectedIds.value = new Set();
        if (drawingEdge.value) cancelEdge();
    }
}

function onWheel(e: WheelEvent) {
    const rect = containerRef.value!.getBoundingClientRect();
    zoom(e.deltaY < 0 ? 1.1 : 1 / 1.1, e.clientX - rect.left, e.clientY - rect.top);
}

function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Delete' || e.key === 'Backspace') {
        if (editingNodeId.value) return;
        deleteSelected();
    }
    if (e.key === 'Escape') {
        if (editingNodeId.value) { commitTextEdit(editingNodeId.value); return; }
        if (drawingEdge.value) { cancelEdge(); return; }
        selectedIds.value = new Set();
    }
}

// ── Pan ───────────────────────────────────────────────────────────────────────

function startPan(e: MouseEvent) {
    panning.value = true;
    panStart = { mx: e.clientX, my: e.clientY, vx: viewport.x, vy: viewport.y };
    window.addEventListener('mousemove', onGlobalMousemove);
    window.addEventListener('mouseup', onGlobalMouseup);
}

// ── Node interactions ─────────────────────────────────────────────────────────

function onNodeMousedown(node: CanvasNode, e: MouseEvent) {
    if (e.button !== 0) return;
    if (!selectedIds.value.has(node.id)) {
        if (!e.shiftKey && !e.ctrlKey) selectedIds.value = new Set([node.id]);
        else selectedIds.value = new Set([...selectedIds.value, node.id]);
    }
    dragging.value = true;
    dragStart = {
        mx: e.clientX,
        my: e.clientY,
        nodes: [...selectedIds.value].map(id => {
            const n = canvasData.nodes.find(n => n.id === id)!;
            return { id, x: n.x, y: n.y };
        }),
    };
    window.addEventListener('mousemove', onGlobalMousemove);
    window.addEventListener('mouseup', onGlobalMouseup);
}

function onNodeDblclick(node: CanvasNode) {
    if (node.type === 'text') {
        editingNodeId.value = node.id;
        nextTick(() => editTextareaRef.value?.focus());
    } else if (node.type === 'file') {
        const tab = [...tabsStore.tabs.values()].find(t => t.filePath === props.filePath);
        const paneId = tab?.paneId ?? tabsStore.activePaneId;
        tabsStore.openTab(paneId, (node as CanvasFileNode).file, (node as CanvasFileNode).file.split('/').pop()!);
    }
}

function onTextInput(nodeId: string, value: string) {
    const node = canvasData.nodes.find(n => n.id === nodeId) as CanvasTextNode | undefined;
    if (node) { node.text = value; scheduleSave(); }
}

function commitTextEdit(nodeId: string) {
    if (editingNodeId.value === nodeId) editingNodeId.value = null;
}

// ── Resize ────────────────────────────────────────────────────────────────────

function onResizeStart(node: CanvasNode, e: MouseEvent) {
    resizing.value = true;
    resizeState = { node, mx: e.clientX, my: e.clientY, ow: node.width, oh: node.height };
    window.addEventListener('mousemove', onGlobalMousemove);
    window.addEventListener('mouseup', onGlobalMouseup);
}

// ── Edge drawing ──────────────────────────────────────────────────────────────

function startEdge(nodeId: string, side: CanvasNodeSide, e: MouseEvent) {
    drawingEdge.value = true;
    edgeFrom = { nodeId, side };
    drawCursor = { x: e.clientX, y: e.clientY };
    window.addEventListener('mousemove', onGlobalMousemove);
    window.addEventListener('mouseup', onGlobalMouseup);
}

function cancelEdge() {
    drawingEdge.value = false;
    edgeFrom = null;
    drawingEdgePath.value = '';
}

function finishEdge(toNodeId: string, toSide: CanvasNodeSide) {
    if (!edgeFrom || edgeFrom.nodeId === toNodeId) { cancelEdge(); return; }
    const edge: CanvasEdge = {
        id: uid(),
        fromNode: edgeFrom.nodeId,
        fromSide: edgeFrom.side,
        toNode: toNodeId,
        toSide,
        color: '#888',
    };
    canvasData.edges.push(edge);
    cancelEdge();
    scheduleSave();
}

function selectEdge(id: string, e: MouseEvent) {
    if (!e.shiftKey && !e.ctrlKey) selectedIds.value = new Set([id]);
    else selectedIds.value = new Set([...selectedIds.value, id]);
}

// ── Global mouse events ───────────────────────────────────────────────────────

function onGlobalMousemove(e: MouseEvent) {
    if (panning.value) {
        viewport.x = panStart.vx + (e.clientX - panStart.mx);
        viewport.y = panStart.vy + (e.clientY - panStart.my);
    }
    if (dragging.value && dragStart) {
        const dx = (e.clientX - dragStart.mx) / viewport.scale;
        const dy = (e.clientY - dragStart.my) / viewport.scale;
        for (const { id, x, y } of dragStart.nodes) {
            const node = canvasData.nodes.find(n => n.id === id);
            if (node) { node.x = Math.round(x + dx); node.y = Math.round(y + dy); }
        }
    }
    if (resizing.value && resizeState) {
        const dx = (e.clientX - resizeState.mx) / viewport.scale;
        const dy = (e.clientY - resizeState.my) / viewport.scale;
        resizeState.node.width = Math.max(80, Math.round(resizeState.ow + dx));
        resizeState.node.height = Math.max(40, Math.round(resizeState.oh + dy));
    }
    if (drawingEdge.value && edgeFrom) {
        drawCursor = { x: e.clientX, y: e.clientY };
        const fromNode = canvasData.nodes.find(n => n.id === edgeFrom!.nodeId);
        if (fromNode) {
            const fp = portCenter(fromNode, edgeFrom.side);
            const rect = containerRef.value!.getBoundingClientRect();
            const tp = { x: e.clientX - rect.left, y: e.clientY - rect.top };
            drawingEdgePath.value = bezier(fp, edgeFrom.side, tp, 'left');
        }
    }
}

function onGlobalMouseup(e: MouseEvent) {
    if (dragging.value) scheduleSave();
    if (resizing.value) scheduleSave();
    panning.value = false;
    dragging.value = false;
    dragStart = null;
    resizing.value = false;
    resizeState = null;
    if (drawingEdge.value) {
        // Check if mouse is over a port
        const el = document.elementFromPoint(e.clientX, e.clientY);
        if (el?.classList.contains('canvas-port')) {
            const nodeEl = el.closest('.canvas-node') as HTMLElement | null;
            const nodeId = nodeEl?.dataset.nodeId;
            const side = (el.className.match(/canvas-port--(\w+)/) ?? [])[1] as CanvasNodeSide | undefined;
            if (nodeId && side) { finishEdge(nodeId, side); return; }
        }
        cancelEdge();
    }
    window.removeEventListener('mousemove', onGlobalMousemove);
    window.removeEventListener('mouseup', onGlobalMouseup);
}

// ── Edge geometry ─────────────────────────────────────────────────────────────

const edgeColors = computed(() => [...new Set(canvasData.edges.map(e => e.color ?? '#888'))]);

function portCenter(node: CanvasNode, side: CanvasNodeSide) {
    const x = viewport.x + node.x * viewport.scale;
    const y = viewport.y + node.y * viewport.scale;
    const w = node.width * viewport.scale;
    const h = node.height * viewport.scale;
    return {
        top:    { x: x + w / 2, y },
        right:  { x: x + w,     y: y + h / 2 },
        bottom: { x: x + w / 2, y: y + h },
        left:   { x,            y: y + h / 2 },
    }[side];
}

function bezier(
    from: { x: number; y: number }, fromSide: CanvasNodeSide,
    to: { x: number; y: number }, _toSide: CanvasNodeSide,
) {
    const dist = Math.max(50, Math.hypot(to.x - from.x, to.y - from.y) * 0.4);
    const fc = sideCtrl(from, fromSide, dist);
    const tc = sideCtrl(to, _toSide, dist);
    return `M${from.x},${from.y} C${fc.x},${fc.y} ${tc.x},${tc.y} ${to.x},${to.y}`;
}

function sideCtrl(p: { x: number; y: number }, side: CanvasNodeSide, d: number) {
    return {
        top:    { x: p.x,     y: p.y - d },
        right:  { x: p.x + d, y: p.y },
        bottom: { x: p.x,     y: p.y + d },
        left:   { x: p.x - d, y: p.y },
    }[side];
}

function edgePath(edge: CanvasEdge) {
    const from = canvasData.nodes.find(n => n.id === edge.fromNode);
    const to   = canvasData.nodes.find(n => n.id === edge.toNode);
    if (!from || !to) return '';
    return bezier(portCenter(from, edge.fromSide), edge.fromSide, portCenter(to, edge.toSide), edge.toSide);
}

function edgeMidpoint(edge: CanvasEdge) {
    const from = canvasData.nodes.find(n => n.id === edge.fromNode);
    const to   = canvasData.nodes.find(n => n.id === edge.toNode);
    if (!from || !to) return { x: 0, y: 0 };
    const fp = portCenter(from, edge.fromSide);
    const tp = portCenter(to, edge.toSide);
    return { x: (fp.x + tp.x) / 2, y: (fp.y + tp.y) / 2 };
}

// ── Add nodes ─────────────────────────────────────────────────────────────────

function viewportCenter() {
    const rect = containerRef.value?.getBoundingClientRect() ?? { width: 800, height: 600 };
    return screenToCanvas(rect.width / 2, rect.height / 2);
}

function addTextNode() {
    const { x, y } = viewportCenter();
    canvasData.nodes.push({ id: uid(), type: 'text', x: Math.round(x - 100), y: Math.round(y - 60), width: 200, height: 120, text: '' });
    scheduleSave();
}

function addFileNode() {
    const path = prompt('File path (e.g. Notes/my-note.md)');
    if (!path) return;
    const { x, y } = viewportCenter();
    canvasData.nodes.push({ id: uid(), type: 'file', x: Math.round(x - 100), y: Math.round(y - 40), width: 200, height: 80, file: path });
    scheduleSave();
}

function addLinkNode() {
    const url = prompt('URL');
    if (!url) return;
    const { x, y } = viewportCenter();
    canvasData.nodes.push({ id: uid(), type: 'link', x: Math.round(x - 120), y: Math.round(y - 40), width: 240, height: 80, url });
    scheduleSave();
}

function addGroupNode() {
    const label = prompt('Group label');
    if (label === null) return;
    const { x, y } = viewportCenter();
    canvasData.nodes.push({ id: uid(), type: 'group', x: Math.round(x - 150), y: Math.round(y - 100), width: 300, height: 200, label });
    scheduleSave();
}

// ── Delete selected ───────────────────────────────────────────────────────────

function deleteSelected() {
    canvasData.nodes = canvasData.nodes.filter(n => !selectedIds.value.has(n.id));
    canvasData.edges = canvasData.edges.filter(
        e => !selectedIds.value.has(e.id) && !selectedIds.value.has(e.fromNode) && !selectedIds.value.has(e.toNode),
    );
    selectedIds.value = new Set();
    scheduleSave();
}

// ── Markdown rendering (minimal, for text nodes) ──────────────────────────────

function renderMarkdown(text: string): string {
    if (!text) return '<span class="text-secondary text-caption">Empty — double-click to edit</span>';
    return text
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
        .replace(/\*(.+?)\*/g, '<em>$1</em>')
        .replace(/`(.+?)`/g, '<code>$1</code>')
        .replace(/\n/g, '<br>');
}

function fileBasename(path: string) {
    return path.split('/').pop() ?? path;
}

function uid() {
    return Math.random().toString(36).slice(2, 10);
}
</script>

<style scoped>
.canvas-view {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background:
    radial-gradient(circle, rgb(var(--v-theme-on-surface), 0.08) 1px, transparent 1px) 0 0 / 24px 24px;
  background-color: rgb(var(--v-theme-background));
  cursor: default;
  outline: none;
}

.canvas-edges {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  overflow: visible;
}

.canvas-edge-path {
  pointer-events: stroke;
  cursor: pointer;
  transition: stroke 0.15s;
}
.canvas-edge-path:hover { stroke-width: 3; }
.canvas-edge-path.selected { stroke-width: 3; stroke-dasharray: 6 3; }

.canvas-edge-label {
  font-size: 11px;
  fill: rgb(var(--v-theme-on-surface));
  background: rgb(var(--v-theme-surface));
  pointer-events: none;
}

.canvas-node {
  position: absolute;
  box-sizing: border-box;
  border: 1.5px solid rgb(var(--v-theme-border));
  border-radius: 6px;
  background: rgb(var(--v-theme-surface));
  overflow: hidden;
  user-select: none;
  cursor: grab;
  transition: box-shadow 0.1s, border-color 0.1s;
}
.canvas-node:hover { border-color: rgb(var(--v-theme-primary)); }
.canvas-node.selected {
  border-color: rgb(var(--v-theme-primary));
  box-shadow: 0 0 0 2px rgb(var(--v-theme-primary), 0.3);
}
.canvas-node--group {
  border-style: dashed;
  background: rgba(100, 100, 200, 0.06);
  z-index: 0;
}
.canvas-node:not(.canvas-node--group) { z-index: 10; }

.canvas-node__text,
.canvas-node__file-header,
.canvas-node__file-path,
.canvas-node__link-header,
.canvas-node__group-label {
  padding: 8px;
  overflow: hidden;
  font-size: inherit;
}
.canvas-node__text { height: 100%; }
.canvas-node__file-path { opacity: 0.5; font-size: 0.8em; }
.canvas-node__group-label {
  font-weight: 600;
  color: rgb(var(--v-theme-on-surface), 0.6);
  font-size: 0.85em;
  pointer-events: none;
}
.canvas-node__textarea {
  width: 100%;
  height: 100%;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: inherit;
  font-size: inherit;
  padding: 8px;
  box-sizing: border-box;
  cursor: text;
}

/* Port anchors */
.canvas-port {
  position: absolute;
  width: 10px;
  height: 10px;
  background: rgb(var(--v-theme-primary));
  border-radius: 50%;
  opacity: 0;
  cursor: crosshair;
  z-index: 20;
  transition: opacity 0.15s;
}
.canvas-node:hover .canvas-port { opacity: 0.7; }
.canvas-port--top    { top: -5px;  left: 50%; transform: translateX(-50%); }
.canvas-port--right  { right: -5px; top: 50%; transform: translateY(-50%); }
.canvas-port--bottom { bottom: -5px; left: 50%; transform: translateX(-50%); }
.canvas-port--left   { left: -5px; top: 50%; transform: translateY(-50%); }

/* Resize handle */
.canvas-resize-handle {
  position: absolute;
  right: 0;
  bottom: 0;
  width: 12px;
  height: 12px;
  cursor: se-resize;
  background: linear-gradient(135deg, transparent 50%, rgb(var(--v-theme-on-surface), 0.3) 50%);
  border-bottom-right-radius: 6px;
}

/* Toolbar */
.canvas-toolbar {
  position: absolute;
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  z-index: 100;
  background: rgb(var(--v-theme-surface));
  border: 1px solid rgb(var(--v-theme-border));
  border-radius: 8px;
  padding: 4px 8px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.12);
}
.canvas-toolbar__zoom {
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface), 0.5);
  min-width: 36px;
  text-align: center;
}

/* Empty state */
.canvas-empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}
</style>
