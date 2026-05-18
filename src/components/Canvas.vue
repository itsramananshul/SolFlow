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
import { PALETTE, categoryForKind } from '@/graph/kinds';
import SolNode from './SolNode.vue';
import ContextMenu, { type ContextMenuItem } from './ContextMenu.vue';
import QuickAddPalette, { type SourceContext } from './QuickAddPalette.vue';
import NodeSearchPalette from './NodeSearchPalette.vue';
import { onMounted, onBeforeUnmount } from 'vue';

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();
const {
  fitView,
  screenToFlowCoordinate,
  getSelectedNodes,
  onConnectStart,
  onConnectEnd,
  setCenter,
  getNode,
  getViewport,
} = useVueFlow();
const flowContainerRef = ref<HTMLDivElement | null>(null);

// Track last cursor screen position so Space hotkey can insert "where I'm looking".
const lastCursor = ref({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
function onMouseMove(e: MouseEvent) {
  lastCursor.value = { x: e.clientX, y: e.clientY };
}

// Auto-fit the viewport whenever the active function changes (e.g. when
// loading a sample workflow, switching function tab, or creating a new
// workflow). nextTick lets Vue Flow finish rendering the new node set
// before we measure. Also dismiss any open Quick-Add palette so it
// doesn't hover stale over the new graph.
watch(
  () => graph.activeFunctionId,
  async () => {
    if (qaOpen.value) closeQuickAdd();
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
  return fn.nodes.map((n) => {
    // Frames render BENEATH normal nodes so the workflow content stays
    // on top of the region wrapper. Notes get a slightly elevated
    // z-index so they don't get hidden by overlapping graph elements.
    let zIndex: number | undefined;
    if (n.data.kind === 'frame') zIndex = -1;
    return {
      id: n.id,
      type: n.data.kind,
      position: n.position,
      data: n,
      selected: ui.selectedNodeId === n.id,
      ...(zIndex !== undefined ? { zIndex } : {}),
    };
  });
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

// Tint each MiniMap dot by category so the overview map reads as a
// summary of the workflow — entry points pop amber/green, flow nodes
// pop blue, plumbing fades into the dark surface. Vue Flow needs a
// resolved color string (not a CSS var), so we pass hex literals that
// mirror tokens.css. Kept in sync manually — there's only one of these.
const CAT_HEX: Record<string, string> = {
  trigger: '#e8a657',
  flow: '#5d8acf',
  variable: '#3a3a3a',
  operator: '#3a3a3a',
  literal: '#2e2e2e',
  access: '#2e2e2e',
  call: '#5d8acf',
  io: '#7e5a5a',
  entry: '#00cc88',
};
function minimapNodeColor(node: VueFlowNode): string {
  const data = node.data as { data: { kind: NodeKind } } | undefined;
  if (!data) return '#2e2e2e';
  const cat = categoryForKind(data.data.kind);
  return CAT_HEX[cat] ?? '#2e2e2e';
}
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

// Track each frame node's pre-drag position so we can compute the delta
// and apply it to every node that was inside the frame's bounds.
const frameDragStarts = ref<Map<string, { x: number; y: number; ids: string[] }>>(
  new Map(),
);

function onNodeDragStart(event: { node: VueFlowNode }) {
  const fn = graph.activeFunction;
  if (!fn) return;
  const dragged = fn.nodes.find((n) => n.id === event.node.id);
  if (!dragged || dragged.data.kind !== 'frame') return;
  // Record the frame's starting position + which nodes are visually
  // inside it. Dragging the frame will then translate them together so
  // sections move as a unit — without the heavy machinery of true
  // Vue Flow parent/child hierarchies.
  const fx = dragged.position.x;
  const fy = dragged.position.y;
  const fw = dragged.data.width;
  const fh = dragged.data.height;
  const containedIds: string[] = [];
  for (const n of fn.nodes) {
    if (n.id === dragged.id) continue;
    // Center-of-node test: cheap and matches user intent — "if the dot
    // of the node sits inside the frame, it's part of the section."
    const cx = n.position.x + 110; // approximate half-width of standard node
    const cy = n.position.y + 28; // approximate half-height
    if (cx >= fx && cx <= fx + fw && cy >= fy && cy <= fy + fh) {
      containedIds.push(n.id);
    }
  }
  frameDragStarts.value.set(dragged.id, { x: fx, y: fy, ids: containedIds });
}

function onNodeDragStop(event: { node: VueFlowNode }) {
  const fn = graph.activeFunction;
  if (!fn) {
    graph.updateNodePosition(event.node.id, {
      x: event.node.position.x,
      y: event.node.position.y,
    });
    return;
  }
  const dragged = fn.nodes.find((n) => n.id === event.node.id);

  // Frame moved → translate every contained node by the same delta.
  if (dragged && dragged.data.kind === 'frame') {
    const start = frameDragStarts.value.get(dragged.id);
    graph.updateNodePosition(event.node.id, {
      x: event.node.position.x,
      y: event.node.position.y,
    });
    if (start) {
      const dx = event.node.position.x - start.x;
      const dy = event.node.position.y - start.y;
      if (dx !== 0 || dy !== 0) {
        for (const id of start.ids) {
          const n = fn.nodes.find((nn) => nn.id === id);
          if (!n) continue;
          graph.updateNodePosition(id, {
            x: n.position.x + dx,
            y: n.position.y + dy,
          });
        }
      }
      frameDragStarts.value.delete(dragged.id);
    }
    return;
  }

  graph.updateNodePosition(event.node.id, {
    x: event.node.position.x,
    y: event.node.position.y,
  });
}

function onNodeClick(event: { node: VueFlowNode }) {
  ui.selectNode(event.node.id);
  // Explicit close: the capture-phase listener in QuickAdd already
  // catches this, but having the canvas drive the close too gives a
  // clear single-place audit trail for popover lifecycle.
  if (qaOpen.value) closeQuickAdd();
}

function onPaneClick() {
  ui.selectNode(null);
  if (qaOpen.value) closeQuickAdd();
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
  // CRITICAL: convert via screenToFlowCoordinate, not getBoundingClientRect.
  // The canvas-host rect ignores Vue Flow's viewport transform (pan + zoom),
  // so HTML-relative coords land in the wrong flow position whenever the
  // viewport is panned or zoomed. screenToFlowCoordinate is the single
  // boundary every creation path goes through.
  const pos = screenToFlowCoordinate({
    x: event.clientX,
    y: event.clientY,
  });
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
  const fn = graph.activeFunction;
  const node = fn?.nodes.find((n) => n.id === id);
  const isStart = node?.data.kind === 'start';
  const isEntry = node?.data.kind === 'start' || node?.data.kind === 'trigger';
  // Entry-node deletion blocked iff this would orphan the function
  // (no other entry would remain).
  const isLastEntry =
    !!fn &&
    isEntry &&
    !fn.nodes.some(
      (n) => n.id !== id && (n.data.kind === 'start' || n.data.kind === 'trigger'),
    );
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
      disabled: isLastEntry,
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
//  Node Search Palette (⌘F)
// =============================================================
const searchOpen = ref(false);

function openSearch() {
  searchOpen.value = true;
}
function closeSearch() {
  searchOpen.value = false;
}
function onSearchJump(nodeId: string) {
  focusOnNode(nodeId);
}

/**
 * Pan + select a node by id. Used by the search palette and by any
 * component that calls ui.requestFocus(). Centralized here so the
 * setCenter math (account for node dimensions; preserve sane zoom)
 * lives in one place.
 */
function focusOnNode(nodeId: string) {
  const flowNode = getNode.value(nodeId);
  if (!flowNode) return;
  setCenter(
    flowNode.position.x + (flowNode.dimensions?.width ?? 110),
    flowNode.position.y + (flowNode.dimensions?.height ?? 28),
    { duration: 350, zoom: Math.max(getViewport().zoom, 1.0) },
  );
  ui.selectNode(nodeId);
}

// Listen for focus requests from anywhere in the app (diagnostics row
// click, future outline panel, etc.). After acting on the request,
// clear it so subsequent same-node requests still trigger via the
// bumpId-driven re-fire pattern in the UI store.
watch(
  () => ui.focusRequest,
  (req) => {
    if (!req) return;
    focusOnNode(req.nodeId);
    ui.clearFocusRequest();
  },
  { deep: true },
);

// Fit-selection: zoom to whatever's currently selected. Falls back to
// the standard fitView when nothing is selected.
function fitSelection() {
  const selected = getSelectedNodes.value;
  if (selected.length > 0) {
    fitView({
      padding: 0.25,
      duration: 300,
      nodes: selected.map((n) => n.id),
    });
  } else {
    fitView({ padding: 0.2, duration: 300 });
  }
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
  // For Start, addNodeAt may return the existing Start instead of a new
  // node when one is already present. Selecting it teaches the user
  // "there's only one Start, here it is" without an explicit warning.
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
// auto-placed Start node and zero edges. Hides the moment any real node
// or edge appears so it never overlaps graph content.
const isFunctionEmpty = computed(() => {
  const fn = graph.activeFunction;
  if (!fn) return false;
  if (fn.edges.length > 0) return false;
  if (fn.nodes.length !== 1) return false;
  return fn.nodes[0].data.kind === 'start';
});

// Second-step nudge: a trigger has been added but there are no edges yet
// — point users at the next thing to do (drag a wire). Disappears as
// soon as any edge exists.
const needsFirstConnection = computed(() => {
  const fn = graph.activeFunction;
  if (!fn) return false;
  if (fn.edges.length > 0) return false;
  // Has a trigger but hasn't wired it to anything.
  return fn.nodes.some((n) => n.data.kind === 'trigger');
});

function onGlobalKey(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey;
  // Cmd/Ctrl+F → Workflow search (jump to node)
  if (mod && e.key.toLowerCase() === 'f' && !isTypingInInput()) {
    e.preventDefault();
    openSearch();
    return;
  }
  // Shift+1 (or just '1') → Fit selection (or fit view when nothing selected)
  if (!mod && e.key === '1' && !isTypingInInput()) {
    e.preventDefault();
    fitSelection();
    return;
  }
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
      @node-drag-start="onNodeDragStart"
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
        :node-color="minimapNodeColor"
        node-stroke-color="rgba(255, 255, 255, 0.18)"
        mask-color="rgba(0, 0, 0, 0.78)"
      />
    </VueFlow>
    <div
      v-if="isFunctionEmpty"
      class="empty-hint"
    >
      <span class="hint-text">Start by adding a trigger — what should kick this workflow off?</span>
      <span class="dot">·</span>
      <span class="hint-alts">
        <span class="alt-label">Drag from the left panel</span>
        <span class="alt-sep">or press</span>
        <span class="kbd small">Space</span>
      </span>
    </div>
    <div
      v-else-if="needsFirstConnection"
      class="empty-hint nudge"
    >
      <span class="hint-text">
        Now connect your trigger — drag from the dot under it to the next step.
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
    <NodeSearchPalette
      :open="searchOpen"
      @jump="onSearchJump"
      @close="closeSearch"
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
.empty-hint .alt-sep {
  font-size: 0.625rem;
  font-weight: 400;
  letter-spacing: 0;
}
.empty-hint.nudge {
  background: rgba(232, 166, 87, 0.10);
  border-color: rgba(232, 166, 87, 0.28);
  color: var(--sf-text-1);
}
.empty-hint.nudge .hint-text {
  color: var(--sf-cat-trigger);
}
</style>
