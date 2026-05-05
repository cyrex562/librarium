<template>
  <div class="graph-view d-flex" style="height: 100%; overflow: hidden;">
    <!-- Sidebar panel -->
    <div class="graph-sidebar pa-3 d-flex flex-column gap-2" style="width: 220px; flex-shrink: 0; border-right: 1px solid rgb(var(--v-theme-border)); overflow-y: auto;">
      <div class="text-subtitle-2 font-weight-bold">Graph View</div>

      <v-text-field
        v-model="graphStore.searchQuery"
        placeholder="Filter nodes…"
        prepend-inner-icon="mdi-magnify"
        density="compact"
        hide-details
        clearable
      />

      <div v-if="graphStore.availableTypes.length" class="mt-1">
        <div class="text-caption text-medium-emphasis mb-1">Entity types</div>
        <v-chip
          v-for="t in graphStore.availableTypes"
          :key="t.id"
          size="small"
          class="ma-1"
          :style="graphStore.visibleTypeIds.has(t.id) ? { background: t.color || '#888', color: '#fff' } : {}"
          :variant="graphStore.visibleTypeIds.has(t.id) ? 'flat' : 'outlined'"
          @click="graphStore.toggleType(t.id)"
        >
          <v-icon start size="x-small">{{ t.icon || 'mdi-cube-outline' }}</v-icon>
          {{ t.name }}
          <span class="ml-1 text-caption opacity-70">{{ nodeCountByType(t.id) }}</span>
        </v-chip>
      </div>

      <v-divider class="my-1" />

      <div class="text-caption text-medium-emphasis">
        {{ graphStore.filteredNodes.length }} nodes · {{ graphStore.filteredEdges.length }} edges
      </div>

      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-refresh"
        :loading="graphStore.loading"
        @click="reload"
      >
        Refresh
      </v-btn>
    </div>

    <!-- Graph canvas -->
    <div ref="canvasWrapper" class="graph-canvas flex-1" style="position: relative; overflow: hidden;">
      <div v-if="graphStore.loading" class="d-flex align-center justify-center" style="height: 100%;">
        <v-progress-circular indeterminate />
      </div>

      <div v-else-if="graphStore.error" class="d-flex align-center justify-center" style="height: 100%;">
        <v-alert type="error" variant="tonal" :text="graphStore.error" />
      </div>

      <div v-else-if="!graphStore.filteredNodes.length" class="d-flex align-center justify-center" style="height: 100%;">
        <v-empty-state
          icon="mdi-graph-outline"
          title="No entities"
          text="No entities have been indexed yet. Open a file and add a librarium_type frontmatter field, then trigger a re-index."
        />
      </div>

      <svg v-else ref="svgEl" style="width: 100%; height: 100%;" />

      <!-- Tooltip -->
      <div
        v-if="tooltip"
        class="graph-tooltip"
        :style="{ left: tooltip.x + 'px', top: tooltip.y + 'px' }"
      >
        <div class="font-weight-bold">{{ tooltip.title }}</div>
        <div v-if="tooltip.type" class="text-caption opacity-70">{{ tooltip.type }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import * as d3force from 'd3-force';
import * as d3sel from 'd3-selection';
// eslint-disable-next-line @typescript-eslint/no-explicit-any
import { zoom as d3zoom } from 'd3-zoom';
// eslint-disable-next-line @typescript-eslint/no-explicit-any
import { drag as d3drag } from 'd3-drag';
import type { SimulationNodeDatum, SimulationLinkDatum } from 'd3-force';
import type { GraphNode, GraphEdge } from '@/api/types';
import { useGraphStore } from '@/stores/graph';
import { useTabsStore } from '@/stores/tabs';

const props = defineProps<{ vaultId: string }>();

const graphStore = useGraphStore();
const tabsStore = useTabsStore();

const svgEl = ref<SVGSVGElement | null>(null);
const canvasWrapper = ref<HTMLDivElement | null>(null);
const tooltip = ref<{ x: number; y: number; title: string; type: string } | null>(null);

type SimNode = GraphNode & SimulationNodeDatum;
type SimLink = SimulationLinkDatum<SimNode> & { data: GraphEdge };

let simulation: d3force.Simulation<SimNode, SimLink> | null = null;
let resizeObserver: ResizeObserver | null = null;

onMounted(async () => {
    await graphStore.loadGraph(props.vaultId);
    await nextTick();
    drawGraph();

    resizeObserver = new ResizeObserver(() => {
        if (svgEl.value) drawGraph();
    });
    if (canvasWrapper.value) resizeObserver.observe(canvasWrapper.value);
});

onUnmounted(() => {
    simulation?.stop();
    resizeObserver?.disconnect();
});

watch(
    [() => graphStore.filteredNodes, () => graphStore.filteredEdges],
    () => nextTick(drawGraph),
    { deep: false },
);

function drawGraph() {
    const svg = svgEl.value;
    if (!svg) return;

    const width = svg.clientWidth || canvasWrapper.value?.clientWidth || 800;
    const height = svg.clientHeight || canvasWrapper.value?.clientHeight || 600;

    d3sel.select(svg).selectAll('*').remove();

    const nodes: SimNode[] = graphStore.filteredNodes.map((n) => ({ ...n }));
    const nodeById = new Map(nodes.map((n) => [n.id, n]));

    const links: SimLink[] = graphStore.filteredEdges
        .map((e) => {
            const source = nodeById.get(e.source);
            const target = nodeById.get(e.target);
            if (!source || !target) return null;
            return { source, target, data: e } as SimLink;
        })
        .filter((l): l is SimLink => l !== null);

    const root = d3sel.select(svg);

    const defs = root.append('defs');
    defs
        .append('marker')
        .attr('id', 'arrow')
        .attr('viewBox', '0 -4 8 8')
        .attr('refX', 18)
        .attr('refY', 0)
        .attr('markerWidth', 6)
        .attr('markerHeight', 6)
        .attr('orient', 'auto')
        .append('path')
        .attr('d', 'M0,-4L8,0L0,4')
        .attr('fill', '#999');

    const g = root.append('g');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const zoomBehavior = d3zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.1, 8])
        .on('zoom', (event: any) => g.attr('transform', event.transform));
    root.call(zoomBehavior as any);

    const linkG = g
        .append('g')
        .selectAll<SVGLineElement, SimLink>('line')
        .data(links)
        .join('line')
        .attr('stroke', '#aaa')
        .attr('stroke-width', 1.5)
        .attr('marker-end', 'url(#arrow)')
        .attr('opacity', 0.6);

    const linkLabel = g
        .append('g')
        .selectAll<SVGTextElement, SimLink>('text')
        .data(links)
        .join('text')
        .attr('font-size', '9px')
        .attr('fill', '#888')
        .attr('text-anchor', 'middle')
        .attr('dominant-baseline', 'middle')
        .text((l) => l.data.relation_type ?? '');

    const nodeG = g
        .append('g')
        .selectAll<SVGGElement, SimNode>('g')
        .data(nodes)
        .join('g')
        .attr('cursor', 'pointer')
        .call(
            d3drag<SVGGElement, SimNode>()
                .on('start', (event: any, d: SimNode) => {
                    if (!event.active) simulation?.alphaTarget(0.3).restart();
                    d.fx = d.x;
                    d.fy = d.y;
                })
                .on('drag', (event: any, d: SimNode) => {
                    d.fx = event.x;
                    d.fy = event.y;
                })
                .on('end', (event: any, d: SimNode) => {
                    if (!event.active) simulation?.alphaTarget(0);
                    d.fx = null;
                    d.fy = null;
                }) as any,
        )
        .on('click', (_event, d) => {
            tabsStore.openTab(tabsStore.activePaneId, d.path, d.path.split('/').pop()!);
        })
        .on('mouseover', (event: MouseEvent, d) => {
            const rect = canvasWrapper.value!.getBoundingClientRect();
            tooltip.value = {
                x: event.clientX - rect.left + 12,
                y: event.clientY - rect.top + 12,
                title: d.title,
                type: d.entity_type ?? '',
            };
        })
        .on('mouseout', () => {
            tooltip.value = null;
        });

    nodeG
        .append('circle')
        .attr('r', 10)
        .attr('fill', (d) => d.color || '#5a7ab8')
        .attr('stroke', '#fff')
        .attr('stroke-width', 1.5);

    nodeG
        .append('text')
        .attr('dy', '1.8em')
        .attr('text-anchor', 'middle')
        .attr('font-size', '10px')
        .attr('fill', 'currentColor')
        .attr('pointer-events', 'none')
        .text((d) => truncate(d.title, 18));

    simulation = d3force
        .forceSimulation<SimNode>(nodes)
        .force(
            'link',
            d3force
                .forceLink<SimNode, SimLink>(links)
                .id((d) => d.id)
                .distance(80),
        )
        .force('charge', d3force.forceManyBody().strength(-180))
        .force('center', d3force.forceCenter(width / 2, height / 2))
        .force('collision', d3force.forceCollide(20))
        .on('tick', () => {
            linkG
                .attr('x1', (l) => (l.source as SimNode).x ?? 0)
                .attr('y1', (l) => (l.source as SimNode).y ?? 0)
                .attr('x2', (l) => (l.target as SimNode).x ?? 0)
                .attr('y2', (l) => (l.target as SimNode).y ?? 0);

            linkLabel
                .attr(
                    'x',
                    (l) =>
                        (((l.source as SimNode).x ?? 0) + ((l.target as SimNode).x ?? 0)) / 2,
                )
                .attr(
                    'y',
                    (l) =>
                        (((l.source as SimNode).y ?? 0) + ((l.target as SimNode).y ?? 0)) / 2,
                );

            nodeG.attr('transform', (d) => `translate(${d.x ?? 0},${d.y ?? 0})`);
        });
}

function nodeCountByType(typeId: string) {
    return graphStore.filteredNodes.filter((n) => n.entity_type === typeId).length;
}

function truncate(s: string, max: number) {
    return s.length > max ? s.slice(0, max - 1) + '…' : s;
}

async function reload() {
    simulation?.stop();
    await graphStore.loadGraph(props.vaultId, true);
    await nextTick();
    drawGraph();
}
</script>

<style scoped>
.graph-view {
    background: rgb(var(--v-theme-surface));
}
.graph-tooltip {
    position: absolute;
    background: rgba(0, 0, 0, 0.75);
    color: #fff;
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 12px;
    pointer-events: none;
    white-space: nowrap;
    z-index: 10;
}
</style>
