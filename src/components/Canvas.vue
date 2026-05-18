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
import { useSimulationStore } from '@/stores/simulation.store';
import { typeCssClass } from '@/graph/schema';
import type { GraphEdge, NodeData, NodeKind, SolType } from '@/graph/schema';
import { PALETTE } from '@/graph/kinds';
import SolNode from './SolNode.vue';
import ContextMenu, { type ContextMenuItem } from './ContextMenu.vue';
import QuickAddPalette, { type SourceContext } from './QuickAddPalette.vue';
import { onMounted, onBeforeUnmount } from 'vue';

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();
const { fitView, screenToFlowCoordinate, getSelectedNodes, onConnectStart, onConnectEnd } =
  useVueFlow();
const flowContainerRef = ref<HTMLDivElement | null>(null);

// Track last cursor screen position so Space hotkey can insert "where I'm looking".
const lastCursor = ref({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
function onMouseMove(e: MouseEvent) {
  lastCursor.value = { x: e.clientX, y: e.clientY };
}

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
// Derive Vue Flow's node-type registry from PALETTE so adding a new kind to
// the palette is enough to make it render through our custom SolNode renderer.
// Previously this was a hand-maintained list; `trigger` was added to the
// palette but missed here, and Vue Flow rendered triggers as default white
// rectangles. Single source of truth eliminates that whole class of bug.
const ALL_KINDS = Array.from(new Set(PALETTE.map((p) => p.kind)));
const nodeTypes = Object.fromEntries(
  ALL_KINDS.map((k) => [k, SolNodeRaw]),
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
    const active = sim.isEdgeActive(e.id);
    const hovered = ui.hoveredNodeId;
    const related =
      hovered != null && (e.source.node === hovered || e.target.node === hovered);
    const dim = hovered != null && !related;
    const classes: string[] = [];
    if (active) classes.push('sf-edge-active');
    if (related) classes.push('sf-edge-related');
    if (dim) classes.push('sf-edge-dim');
    return {
      id: e.id,
      source: e.source.node,
      target: e.target.node,
      sourceHandle: e.source.port,
      targetHandle: e.target.port,
      type: 'smoothstep',
      class: classes.join(' '),
      style: {
        stroke: strokeColor,
        strokeWidth: isControl ? 2.4 : 1.8,
      },
      animated: false,
      // Widen the invisible interaction stroke so edges are easy to click/select.
      interactionWidth: 22,
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
  // Mark that a real connection completed, so connect-end won't open Quick-Add.
  connectCompleted.value = true;
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

  // Case 1: user dropped a .solgraph.json file from the desktop.
  const files = event.dataTransfer?.files;
  if (files && files.length > 0) {
    const file = files[0];
    if (file.name.endsWith('.json') || file.type === 'application/json') {
      file
        .text()
        .then((text) => {
          const parsed = JSON.parse(text);
          if (parsed.schemaVersion !== 1 || !Array.isArray(parsed.functions)) {
            throw new Error('Not a SolFlow workflow file');
          }
          graph.loadWorkflow(parsed);
        })
        .catch((e) => alert(`Could not load workflow: ${(e as Error).message}`));
      return;
    }
  }

  // Case 2: user dropped a palette item.
  const kind = event.dataTransfer?.getData('application/x-solflow-kind') as
    | NodeKind
    | undefined;
  if (!kind) return;
  const initRaw = event.dataTransfer?.getData('application/x-solflow-init');
  let init: object | undefined;
  if (initRaw) {
    try {
      init = JSON.parse(initRaw);
    } catch {
      /* ignore malformed init */
    }
  }
  const target = event.currentTarget as HTMLDivElement;
  const rect = target.getBoundingClientRect();
  const pos = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };
  graph.addNode(kind, pos, init as Partial<NodeData> | undefined);
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

// Right-click context menu state. Supports nodes OR edges (not both at once).
const ctxMenu = ref<{
  open: boolean;
  x: number;
  y: number;
  nodeId?: string;
  edgeId?: string;
}>({
  open: false,
  x: 0,
  y: 0,
});

const ctxItems = computed<ContextMenuItem[]>(() => {
  // Edge menu
  if (ctxMenu.value.edgeId) {
    const id = ctxMenu.value.edgeId;
    return [
      {
        label: 'Delete edge',
        shortcut: 'Del',
        danger: true,
        action: () => graph.removeEdge(id),
      },
    ];
  }
  // Node menu
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
      label: 'Copy',
      shortcut: 'Cmd+C',
      disabled: isStart,
      action: () => graph.copyNodes([id]),
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
  ctxMenu.value = { open: true, x, y, nodeId: event.node.id };
}

function onEdgeContextMenu(event: { event: MouseEvent | TouchEvent; edge: Edge }) {
  const me = event.event as MouseEvent;
  if (typeof me.preventDefault === 'function') me.preventDefault();
  const x = 'clientX' in me ? me.clientX : 0;
  const y = 'clientY' in me ? me.clientY : 0;
  ctxMenu.value = { open: true, x, y, edgeId: event.edge.id };
}

function closeCtxMenu() {
  ctxMenu.value = { ...ctxMenu.value, open: false };
}

// =============================================================
//  Quick-Add Palette
// =============================================================
const qaOpen = ref(false);
const qaScreenPos = ref({ x: 0, y: 0 });
const qaSourceContext = ref<SourceContext | undefined>(undefined);
// Where the inserted node should appear in flow coords.
const qaFlowPos = ref({ x: 0, y: 0 });

// Track the most recent connect-start so we can detect drag-edge-to-empty.
const pendingConnect = ref<{
  nodeId: string;
  portId: string;
  edgeKind: 'control' | 'data';
  type?: SolType;
} | null>(null);
const connectCompleted = ref(false);

onConnectStart((event) => {
  connectCompleted.value = false;
  const nodeId = event?.nodeId;
  const portId = event?.handleId;
  const handleType = event?.handleType; // 'source' | 'target'
  if (!nodeId || !portId || handleType !== 'source') {
    pendingConnect.value = null;
    return;
  }
  const fn = graph.activeFunction;
  const node = fn?.nodes.find((n) => n.id === nodeId);
  const port = node?.ports.out.find((p) => p.id === portId);
  if (!node || !port) {
    pendingConnect.value = null;
    return;
  }
  pendingConnect.value = {
    nodeId,
    portId,
    edgeKind: port.kind,
    type: port.type,
  };
});

onConnectEnd((event) => {
  // If `onConnect` fired (a real connection was made), bail.
  if (connectCompleted.value || !pendingConnect.value) {
    pendingConnect.value = null;
    return;
  }
  // Edge dragged into empty space — open Quick-Add at the drop point.
  const me = event as MouseEvent;
  const x = me?.clientX ?? lastCursor.value.x;
  const y = me?.clientY ?? lastCursor.value.y;
  const flow = screenToFlowCoordinate({ x, y });
  openQuickAdd(x, y, flow, pendingConnect.value);
  pendingConnect.value = null;
});

function openQuickAdd(
  screenX: number,
  screenY: number,
  flowPos: { x: number; y: number },
  source?: SourceContext,
) {
  qaScreenPos.value = { x: screenX, y: screenY };
  qaFlowPos.value = flowPos;
  qaSourceContext.value = source;
  qaOpen.value = true;
}

function closeQuickAdd() {
  qaOpen.value = false;
  qaSourceContext.value = undefined;
}

function onQuickAddSelect(
  kind: NodeKind,
  source?: SourceContext,
  initialData?: Partial<NodeData>,
) {
  const node = graph.addNodeAt(
    kind,
    qaFlowPos.value,
    source
      ? { fromNode: source.nodeId, fromPort: source.portId, edgeKind: source.edgeKind }
      : undefined,
    initialData,
  );
  if (node) ui.selectNode(node.id);
}

// Double-click on pane opens Quick-Add at the clicked spot.
function onPaneDoubleClick(event: MouseEvent) {
  const flow = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
  openQuickAdd(event.clientX, event.clientY, flow);
}

// Space / Cmd+K open Quick-Add at the cursor. Guarded so typing in an
// input or textarea isn't intercepted.
function isTypingInInput(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) {
    return true;
  }
  return false;
}

function getCurrentSelectionIds(): string[] {
  const selectedIds = getSelectedNodes.value.map((n) => n.id);
  if (selectedIds.length > 0) return selectedIds;
  return ui.selectedNodeId ? [ui.selectedNodeId] : [];
}

// Empty-state hint visibility: only when the function contains just the
// auto-placed Start node and zero edges. Hides the second any real node
// or edge appears so it never overlaps graph content.
const isFunctionEmpty = computed(() => {
  const fn = graph.activeFunction;
  if (!fn) return false;
  if (fn.edges.length > 0) return false;
  if (fn.nodes.length !== 1) return false;
  return fn.nodes[0].data.kind === 'start';
});

function onGlobalKey(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey;
  // Cmd/Ctrl+K → Quick-Add at cursor
  if (mod && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    const flow = screenToFlowCoordinate(lastCursor.value);
    openQuickAdd(lastCursor.value.x, lastCursor.value.y, flow);
    return;
  }
  // Cmd/Ctrl+D → duplicate current selection
  if (mod && e.key.toLowerCase() === 'd' && !isTypingInInput()) {
    e.preventDefault();
    const ids = getCurrentSelectionIds();
    if (ids.length === 0) return;
    const newIds = graph.duplicateNodes(ids);
    if (newIds.length > 0) ui.selectNode(newIds[newIds.length - 1]);
    return;
  }
  // Cmd/Ctrl+C → copy selection to internal clipboard
  if (mod && e.key.toLowerCase() === 'c' && !isTypingInInput()) {
    const ids = getCurrentSelectionIds();
    if (ids.length === 0) return;
    e.preventDefault();
    graph.copyNodes(ids);
    return;
  }
  // Cmd/Ctrl+V → paste at cursor (or canvas center)
  if (mod && e.key.toLowerCase() === 'v' && !isTypingInInput()) {
    if (!graph.hasClipboard()) return;
    e.preventDefault();
    const flow = screenToFlowCoordinate(lastCursor.value);
    const newIds = graph.pasteAt(flow);
    if (newIds.length > 0) ui.selectNode(newIds[newIds.length - 1]);
    return;
  }
  // Space (no modifier, not in input, no edge being dragged)
  if (e.key === ' ' && !mod && !e.repeat && !isTypingInInput() && !pendingConnect.value) {
    e.preventDefault();
    const flow = screenToFlowCoordinate(lastCursor.value);
    openQuickAdd(lastCursor.value.x, lastCursor.value.y, flow);
  }
}

onMounted(() => {
  window.addEventListener('keydown', onGlobalKey);
  window.addEventListener('mousemove', onMouseMove);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
  window.removeEventListener('mousemove', onMouseMove);
});
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
      @pane-double-click="onPaneDoubleClick"
      @edge-click="onEdgeClick"
      @nodes-delete="onNodesDelete"
      @edges-delete="onEdgesDelete"
      @node-context-menu="onNodeContextMenu"
      @edge-context-menu="onEdgeContextMenu"
      :connection-line-style="{
        stroke: '#3291ff',
        strokeWidth: 2,
        strokeDasharray: '6 4',
      }"
      :selection-key-code="'Shift'"
      :multi-selection-key-code="'Shift'"
      :pan-on-drag="true"
      :zoom-on-double-click="false"
      :delete-key-code="['Backspace', 'Delete']"
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
      v-if="isFunctionEmpty"
      class="empty-hint"
    >
      <span class="kbd">Space</span>
      <span class="hint-text">to add a node</span>
      <span class="dot">·</span>
      <span class="hint-alts">
        <span class="kbd small">⌘K</span>
        <span class="alt-sep">·</span>
        <span class="alt-label">Double-click</span>
        <span class="alt-sep">·</span>
        <span class="alt-label">Drag from palette</span>
      </span>
    </div>
    <ContextMenu
      :open="ctxMenu.open"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :items="ctxItems"
      @close="closeCtxMenu"
    />
    <QuickAddPalette
      :open="qaOpen"
      :x="qaScreenPos.x"
      :y="qaScreenPos.y"
      :source-context="qaSourceContext"
      @select="onQuickAddSelect"
      @close="closeQuickAdd"
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
/*
 * Empty-canvas hint: a single horizontal pill anchored to the bottom of
 * the canvas viewport, *not* the center. Stays out of node territory
 * (Start is auto-placed at (80, 60); pan-relative since absolute to the
 * viewport this sits at the bottom regardless of canvas pan). Renders
 * at var(--sf-z-ambient) which is below the drawer but above edges /
 * nodes... actually NO — pointer-events:none and a low opacity make
 * it ambient. Subtle.
 */
.empty-hint {
  position: absolute;
  left: 50%;
  bottom: 24px;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: rgba(17, 17, 17, 0.7);
  border: 1px solid var(--sf-border);
  border-radius: 999px;
  color: var(--sf-text-2);
  pointer-events: none;
  opacity: 0.72;
  font-size: 0.6875rem;
  z-index: var(--sf-z-ambient);
  white-space: nowrap;
  transition: opacity 0.18s ease;
  backdrop-filter: blur(6px);
}
.empty-hint:hover {
  opacity: 1;
}
.empty-hint .kbd {
  display: inline-block;
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  font-weight: 500;
  padding: 2px 7px;
  background: var(--sf-bg-3);
  border: 1px solid var(--sf-border-strong);
  border-radius: 4px;
  color: var(--sf-text-0);
  letter-spacing: 0.3px;
}
.empty-hint .kbd.small {
  font-size: 0.5625rem;
  padding: 1px 5px;
  color: var(--sf-text-1);
}
.empty-hint .hint-text {
  color: var(--sf-text-1);
}
.empty-hint .dot {
  color: var(--sf-text-3);
}
.empty-hint .hint-alts {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.empty-hint .alt-sep {
  color: var(--sf-text-3);
  font-size: 0.5625rem;
}
.empty-hint .alt-label {
  color: var(--sf-text-2);
  font-size: 0.625rem;
}
</style>
