<script setup lang="ts">
import { computed, markRaw, nextTick, ref, watch } from 'vue';
import {
  VueFlow,
  MarkerType,
  useVueFlow,
  type Edge,
  type Node as VueFlowNode,
  type Connection,
  type NodeTypesObject,
} from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { MiniMap } from '@vue-flow/minimap';

import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { typeCssClass } from '@/graph/schema';
import type { GraphEdge, NodeKind } from '@/graph/schema';
import SolNode from './SolNode.vue';
import ContextMenu, { type ContextMenuItem } from './ContextMenu.vue';

const graph = useGraphStore();
const ui = useUIStore();
const { fitView } = useVueFlow();
const flowContainerRef = ref<HTMLDivElement | null>(null);

// Auto-fit the viewport whenever the active function changes (e.g. when
// loading a sample workflow, switching function tab, or creating a new
// workflow). nextTick lets Vue Flow finish rendering the new node set
// before we measure.
watch(
  () => graph.activeFunctionId,
  async () => {
    await nextTick();
    setTimeout(() => {
      try {
        fitView({ padding: 0.2, duration: 250 });
      } catch {
        /* fitView is no-op before mount */
      }
    }, 30);
  },
  { immediate: true },
);

const SolNodeRaw = markRaw(SolNode);
const kindList = [
  'start', 'let', 'assign', 'print', 'return', 'branch', 'while', 'forEach',
  'binaryOp', 'unaryOp', 'varGet', 'literal', 'arrayLiteral', 'structLiteral',
  'fieldAccess', 'fieldSet', 'indexRead', 'indexSet', 'enumVariant', 'call',
];
// Vue Flow's strict typing of NodeComponent doesn't accept generic component
// instances cleanly — we cast at the boundary since SolNode legitimately
// renders for every kind. Acceptable adapter boundary.
const nodeTypes = Object.fromEntries(
  kindList.map((k) => [k, SolNodeRaw]),
) as unknown as NodeTypesObject;

const flowNodes = computed<VueFlowNode[]>(() => {
  const fn = graph.activeFunction;
  if (!fn) return [];
  return fn.nodes.map((n) => ({
    id: n.id,
    type: n.data.kind,
    position: n.position,
    data: n,
    selected: ui.selectedNodeId === n.id,
  }));
});

const flowEdges = computed<Edge[]>(() => {
  const fn = graph.activeFunction;
  if (!fn) return [];
  return fn.edges.map((e) => {
    const isControl = e.kind === 'control';
    // Determine edge type tint
    let strokeColor = '#cbd1de';
    if (!isControl) {
      const src = fn.nodes.find((n) => n.id === e.source.node);
      const port = src?.ports.out.find((p) => p.id === e.source.port);
      const cls = typeCssClass(port?.type);
      strokeColor = cssVarForType(cls);
    }
    return {
      id: e.id,
      source: e.source.node,
      target: e.target.node,
      sourceHandle: e.source.port,
      targetHandle: e.target.port,
      type: 'smoothstep',
      style: {
        stroke: strokeColor,
        strokeWidth: isControl ? 2.5 : 2,
      },
      animated: false,
      markerEnd: { type: MarkerType.ArrowClosed, color: strokeColor, width: 14, height: 14 },
    } as Edge;
  });
});

function cssVarForType(cls: string): string {
  const map: Record<string, string> = {
    'data-int': '#f2c97d',
    'data-float': '#d19a66',
    'data-bool': '#c678dd',
    'data-str': '#98c379',
    'data-char': '#8edc8a',
    'data-array': '#e5c07b',
    'data-struct': '#61afef',
    'data-enum': '#56b6c2',
    'data-any': '#abb2bf',
  };
  return map[cls] ?? '#abb2bf';
}

function onConnect(c: Connection) {
  if (!c.source || !c.target || !c.sourceHandle || !c.targetHandle) return;
  const fn = graph.activeFunction;
  if (!fn) return;
  const src = fn.nodes.find((n) => n.id === c.source);
  const tgt = fn.nodes.find((n) => n.id === c.target);
  if (!src || !tgt) return;
  const srcPort = src.ports.out.find((p) => p.id === c.sourceHandle);
  const tgtPort = tgt.ports.in.find((p) => p.id === c.targetHandle);
  if (!srcPort || !tgtPort) return;
  if (srcPort.kind !== tgtPort.kind) return;
  graph.addEdge({
    source: { node: src.id, port: srcPort.id },
    target: { node: tgt.id, port: tgtPort.id },
    kind: srcPort.kind,
  });
}

function onNodeDragStop(event: { node: VueFlowNode }) {
  graph.updateNodePosition(event.node.id, {
    x: event.node.position.x,
    y: event.node.position.y,
  });
}

function onNodeClick(event: { node: VueFlowNode }) {
  ui.selectNode(event.node.id);
}

function onPaneClick() {
  ui.selectNode(null);
}

function onEdgeClick(event: { edge: Edge }) {
  // Allow keyboard-driven deletion via Vue Flow's built-in handling.
  if (event.edge?.id) {
    /* selection is handled internally; nothing to do here */
  }
}

function onEdgesDelete(edges: Edge[]) {
  for (const e of edges) graph.removeEdge(e.id);
}

function onNodesDelete(nodes: VueFlowNode[]) {
  for (const n of nodes) {
    if (n.data?.data?.kind !== 'start') graph.removeNode(n.id);
  }
}

function onDragOver(event: DragEvent) {
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
}

function onDrop(event: DragEvent) {
  event.preventDefault();
  const kind = event.dataTransfer?.getData('application/x-solflow-kind') as NodeKind | undefined;
  if (!kind) return;
  // Convert drop coordinates to flow coordinates via the flow's own helper.
  // For simplicity we just place at the cursor relative to the canvas.
  const target = event.currentTarget as HTMLDivElement;
  const rect = target.getBoundingClientRect();
  const pos = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };
  graph.addNode(kind, pos);
}

function isValidConnection(c: Connection): boolean {
  if (!c.source || !c.target || !c.sourceHandle || !c.targetHandle) return false;
  if (c.source === c.target) return false;
  const fn = graph.activeFunction;
  if (!fn) return false;
  const src = fn.nodes.find((n) => n.id === c.source);
  const tgt = fn.nodes.find((n) => n.id === c.target);
  if (!src || !tgt) return false;
  const srcPort = src.ports.out.find((p) => p.id === c.sourceHandle);
  const tgtPort = tgt.ports.in.find((p) => p.id === c.targetHandle);
  if (!srcPort || !tgtPort) return false;
  return srcPort.kind === tgtPort.kind;
}

// Right-click context menu state.
const ctxMenu = ref<{ open: boolean; x: number; y: number; nodeId?: string }>({
  open: false,
  x: 0,
  y: 0,
});

const ctxItems = computed<ContextMenuItem[]>(() => {
  const id = ctxMenu.value.nodeId;
  if (!id) return [];
  const node = graph.activeFunction?.nodes.find((n) => n.id === id);
  const isStart = node?.data.kind === 'start';
  return [
    {
      label: 'Duplicate',
      shortcut: '⎘',
      disabled: isStart,
      action: () => {
        const dup = graph.duplicateNode(id);
        if (dup) ui.selectNode(dup.id);
      },
    },
    {
      label: 'Delete',
      shortcut: 'Del',
      danger: true,
      disabled: isStart,
      action: () => {
        graph.removeNode(id);
        if (ui.selectedNodeId === id) ui.selectNode(null);
      },
    },
  ];
});

function onNodeContextMenu(event: { event: MouseEvent | TouchEvent; node: VueFlowNode }) {
  const me = event.event as MouseEvent;
  if (typeof me.preventDefault === 'function') me.preventDefault();
  const x = 'clientX' in me ? me.clientX : 0;
  const y = 'clientY' in me ? me.clientY : 0;
  ctxMenu.value = {
    open: true,
    x,
    y,
    nodeId: event.node.id,
  };
}

function closeCtxMenu() {
  ctxMenu.value = { ...ctxMenu.value, open: false };
}
</script>

<template>
  <div class="canvas-host" @dragover="onDragOver" @drop="onDrop">
    <VueFlow
      :nodes="flowNodes"
      :edges="flowEdges"
      :node-types="nodeTypes"
      :default-viewport="{ x: 0, y: 0, zoom: 0.9 }"
      :min-zoom="0.2"
      :max-zoom="2"
      :snap-to-grid="true"
      :snap-grid="[16, 16]"
      :is-valid-connection="isValidConnection"
      @connect="onConnect"
      @node-drag-stop="onNodeDragStop"
      @node-click="onNodeClick"
      @pane-click="onPaneClick"
      @edge-click="onEdgeClick"
      @nodes-delete="onNodesDelete"
      @edges-delete="onEdgesDelete"
      @node-context-menu="onNodeContextMenu"
      :connection-line-style="{ stroke: '#3291ff', strokeWidth: 2 }"
    >
      <Background variant="dots" :pattern-color="'rgba(255, 255, 255, 0.06)'" :gap="20" :size="1" />
      <Controls :show-interactive="false" />
      <MiniMap
        pannable
        zoomable
        node-color="#262626"
        node-stroke-color="rgba(255, 255, 255, 0.1)"
        mask-color="rgba(0, 0, 0, 0.78)"
      />
    </VueFlow>
    <div
      v-if="(graph.activeFunction?.nodes.length ?? 0) <= 1 && (graph.activeFunction?.edges.length ?? 0) === 0"
      class="empty-hint"
    >
      <div class="hint-title">Empty function body</div>
      <div class="hint-body">
        Drag a node from the <strong>Nodes</strong> palette on the left,<br />
        or load a sample workflow from the <strong>Samples</strong> menu in the toolbar.
      </div>
    </div>
    <ContextMenu
      :open="ctxMenu.open"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :items="ctxItems"
      @close="closeCtxMenu"
    />
  </div>
</template>

<style scoped>
.canvas-host {
  flex: 1;
  background: var(--sf-canvas-bg);
  position: relative;
  min-height: 0;
}
.empty-hint {
  position: absolute;
  top: 55%;
  left: 50%;
  transform: translate(-50%, -50%);
  color: var(--sf-text-3);
  pointer-events: none;
  text-align: center;
}
.hint-title {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--sf-text-2);
  margin-bottom: 8px;
  font-family: var(--sf-font-mono);
}
.hint-body {
  font-size: 0.75rem;
  line-height: 1.6;
}
.hint-body strong {
  color: var(--sf-text-1);
  font-weight: 500;
}
</style>
