<template>
  <div
    class="canvas-view"
    @mousedown="onBackgroundMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @mouseleave="onMouseUp"
    @wheel="onWheel"
  >
    <div
      class="canvas-container"
      :style="{
        transform: `translate(${panX}px, ${panY}px) scale(${zoom})`,
        transformOrigin: '0 0'
      }"
    >
      <!-- Edges (Rendered behind nodes) -->
      <svg class="canvas-edges">
        <defs>
          <marker
            id="arrowhead"
            markerWidth="10"
            markerHeight="7"
            refX="9"
            refY="3.5"
            orient="auto"
          >
            <polygon points="0 0, 10 3.5, 0 7" fill="rgb(var(--v-theme-secondary))" />
          </marker>
        </defs>
        <path
          v-for="edge in edges"
          :key="edge.id"
          :d="getEdgePath(edge)"
          fill="none"
          stroke="rgb(var(--v-theme-secondary))"
          stroke-width="2"
          marker-end="url(#arrowhead)"
        />
      </svg>

      <!-- Nodes -->
      <div
        v-for="node in nodes"
        :key="node.id"
        class="canvas-node"
        :class="[`node-type-${node.type}`, { selected: selectedNodeId === node.id }]"
        :style="getNodeStyle(node)"
        @mousedown.stop="onNodeMouseDown($event, node)"
      >
        <!-- Group nodes have a label at the top -->
        <div v-if="node.type === 'group'" class="node-group-label">{{ node.label }}</div>

        <!-- Text nodes -->
        <div v-if="node.type === 'text'" class="node-content text-content">
          {{ node.text }}
        </div>

        <!-- File nodes -->
        <div v-if="node.type === 'file'" class="node-content file-content">
          <div class="file-name">
            <v-icon size="small" icon="mdi-file-outline" class="mr-1" />
            {{ node.file }}
          </div>
        </div>

        <!-- Link nodes -->
        <div v-if="node.type === 'link'" class="node-content link-content">
          <a :href="node.url" target="_blank">{{ node.url }}</a>
        </div>
      </div>
    </div>

    <!-- Zoom controls overlay -->
    <div class="canvas-controls">
      <v-btn icon="mdi-plus" size="x-small" variant="tonal" @click="zoomIn" />
      <v-btn icon="mdi-minus" size="x-small" variant="tonal" @click="zoomOut" />
      <v-btn icon="mdi-crosshairs-gps" size="x-small" variant="tonal" @click="resetView" />
      <div class="text-caption mt-1">{{ Math.round(zoom * 100) }}%</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useTabsStore } from '@/stores/tabs';

const props = defineProps<{
  vaultId: string;
  path: string;
}>();

const tabsStore = useTabsStore();

// Find the tab containing the canvas JSON content
const activeTab = computed(() => {
  for (const tab of tabsStore.tabs.values()) {
    if (tab.filePath === props.path) return tab;
  }
  return null;
});

const canvasData = computed(() => {
  try {
    return JSON.parse(activeTab.value?.content ?? '{}');
  } catch {
    return { nodes: [], edges: [] };
  }
});

const nodes = computed(() => canvasData.value.nodes ?? []);
const edges = computed(() => canvasData.value.edges ?? []);

// ── Viewport state ───────────────────────────────────────────────────────────
const panX = ref(0);
const panY = ref(0);
const zoom = ref(1.0);

const isPanning = ref(false);
const isDraggingNode = ref(false);
const draggedNodeId = ref<string | null>(null);
const selectedNodeId = ref<string | null>(null);

let lastMouseX = 0;
let lastMouseY = 0;
let dragStartX = 0;
let dragStartY = 0;
let nodeStartPosX = 0;
let nodeStartPosY = 0;

function resetView() {
  panX.value = 0;
  panY.value = 0;
  zoom.value = 1.0;
}

function zoomIn() { zoom.value = Math.min(zoom.value * 1.2, 5.0); }
function zoomOut() { zoom.value = Math.max(zoom.value / 1.2, 0.1); }

function onWheel(e: WheelEvent) {
  e.preventDefault();
  const delta = e.deltaY;
  const factor = delta > 0 ? 0.9 : 1.1;
  const newZoom = Math.max(0.1, Math.min(5.0, zoom.value * factor));

  // Zoom towards mouse position
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const mouseX = e.clientX - rect.left;
  const mouseY = e.clientY - rect.top;

  const worldX = (mouseX - panX.value) / zoom.value;
  const worldY = (mouseY - panY.value) / zoom.value;

  panX.value = mouseX - worldX * newZoom;
  panY.value = mouseY - worldY * newZoom;
  zoom.value = newZoom;
}

function onBackgroundMouseDown(e: MouseEvent) {
  if (e.button !== 0) return; // Left click only
  isPanning.value = true;
  lastMouseX = e.clientX;
  lastMouseY = e.clientY;
  selectedNodeId.value = null;
}

function onNodeMouseDown(e: MouseEvent, node: any) {
  if (e.button !== 0) return;
  selectedNodeId.value = node.id;
  isDraggingNode.value = true;
  draggedNodeId.value = node.id;
  dragStartX = e.clientX;
  dragStartY = e.clientY;
  nodeStartPosX = node.x;
  nodeStartPosY = node.y;
}

function onMouseMove(e: MouseEvent) {
  if (isPanning.value) {
    const dx = e.clientX - lastMouseX;
    const dy = e.clientY - lastMouseY;
    panX.value += dx;
    panY.value += dy;
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;
  } else if (isDraggingNode.value && draggedNodeId.value) {
    const dx = (e.clientX - dragStartX) / zoom.value;
    const dy = (e.clientY - dragStartY) / zoom.value;
    
    // Update node position in-place for reactivity
    const node = nodes.value.find((n: any) => n.id === draggedNodeId.value);
    if (node) {
      node.x = Math.round(nodeStartPosX + dx);
      node.y = Math.round(nodeStartPosY + dy);
    }
  }
}

function onMouseUp() {
  if (isDraggingNode.value) {
    saveCanvas();
  }
  isPanning.value = false;
  isDraggingNode.value = false;
  draggedNodeId.value = null;
}

function saveCanvas() {
  if (!activeTab.value) return;
  const newContent = JSON.stringify({
    nodes: nodes.value,
    edges: edges.value
  }, null, 2);
  tabsStore.updateTabContent(activeTab.value.id, newContent);
  // Auto-save logic in EditorPane will handle the actual file write
}

// ── Rendering helpers ────────────────────────────────────────────────────────
function getNodeStyle(node: any) {
  return {
    left: `${node.x}px`,
    top: `${node.y}px`,
    width: `${node.width}px`,
    height: `${node.height}px`,
    backgroundColor: node.color ? `var(--canvas-color-${node.color}, rgb(var(--v-theme-surface)))` : 'rgb(var(--v-theme-surface))',
    borderColor: node.color ? `var(--canvas-color-${node.color}-border, rgb(var(--v-theme-border)))` : 'rgb(var(--v-theme-border))',
  };
}

function getEdgePath(edge: any) {
  const fromNode = nodes.value.find((n: any) => n.id === edge.fromNode);
  const toNode = nodes.value.find((n: any) => n.id === edge.toNode);
  if (!fromNode || !toNode) return '';

  const start = getAttachmentPoint(fromNode, edge.fromSide);
  const end = getAttachmentPoint(toNode, edge.toSide);

  // Simple quadratic curve
  const dx = Math.abs(end.x - start.x);
  const dy = Math.abs(end.y - start.y);
  const curvature = 0.5;

  let cp1x = start.x;
  let cp1y = start.y;
  let cp2x = end.x;
  let cp2y = end.y;

  if (edge.fromSide === 'top') cp1y -= dy * curvature;
  if (edge.fromSide === 'bottom') cp1y += dy * curvature;
  if (edge.fromSide === 'left') cp1x -= dx * curvature;
  if (edge.fromSide === 'right') cp1x += dx * curvature;

  if (edge.toSide === 'top') cp2y += dy * curvature;
  if (edge.toSide === 'bottom') cp2y -= dy * curvature;
  if (edge.toSide === 'left') cp2x += dx * curvature;
  if (edge.toSide === 'right') cp2x -= dx * curvature;

  return `M ${start.x} ${start.y} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${end.x} ${end.y}`;
}

function getAttachmentPoint(node: any, side: string) {
  switch (side) {
    case 'top': return { x: node.x + node.width / 2, y: node.y };
    case 'bottom': return { x: node.x + node.width / 2, y: node.y + node.height };
    case 'left': return { x: node.x, y: node.y + node.height / 2 };
    case 'right': return { x: node.x + node.width, y: node.y + node.height / 2 };
    default: return { x: node.x + node.width / 2, y: node.y + node.height / 2 };
  }
}
</script>

<style scoped>
.canvas-view {
  position: relative;
  width: 100%;
  height: 100%;
  background-color: rgb(var(--v-theme-background));
  background-image: radial-gradient(circle, rgb(var(--v-theme-border)) 1px, transparent 1px);
  background-size: 20px 20px;
  overflow: hidden;
  cursor: grab;
}

.canvas-view:active {
  cursor: grabbing;
}

.canvas-container {
  position: absolute;
  top: 0;
  left: 0;
  width: 0;
  height: 0;
  pointer-events: none;
}

.canvas-edges {
  position: absolute;
  top: 0;
  left: 0;
  width: 100000px; /* Arbitrary large size */
  height: 100000px;
  pointer-events: none;
  overflow: visible;
}

.canvas-node {
  position: absolute;
  pointer-events: auto;
  border: 2px solid rgb(var(--v-theme-border));
  border-radius: 4px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition: box-shadow 0.2s, border-color 0.2s;
}

.canvas-node.selected {
  border-color: rgb(var(--v-theme-primary));
  box-shadow: 0 0 0 2px rgba(var(--v-theme-primary), 0.2);
  z-index: 100;
}

.node-type-group {
  background-color: rgba(var(--v-theme-surface), 0.5);
  border-style: dashed;
}

.node-group-label {
  padding: 4px 8px;
  font-size: 12px;
  font-weight: bold;
  background: rgba(0, 0, 0, 0.05);
  border-bottom: 1px solid rgba(0, 0, 0, 0.05);
}

.node-content {
  flex: 1;
  padding: 8px;
  font-size: 14px;
  overflow: auto;
}

.file-content {
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(var(--v-theme-surface), 0.8);
}

.file-name {
  text-align: center;
  font-weight: 500;
  word-break: break-all;
}

.canvas-controls {
  position: absolute;
  bottom: 16px;
  right: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: rgba(var(--v-theme-surface), 0.8);
  padding: 8px;
  border-radius: 8px;
  border: 1px solid rgb(var(--v-theme-border));
  align-items: center;
  backdrop-filter: blur(4px);
}

/* Color variables based on Obsidian defaults */
:root {
  --canvas-color-1: #ff3333;
  --canvas-color-2: #ff9933;
  --canvas-color-3: #ffff33;
  --canvas-color-4: #33ff33;
  --canvas-color-5: #33ffff;
  --canvas-color-6: #3333ff;
}
</style>
