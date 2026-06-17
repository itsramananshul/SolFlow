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
import { useBlocksStore } from '@/stores/blocks.store';
import { useToastStore } from '@/stores/toast.store';
import { buildBuiltinPattern } from '@/graph/blocks';
import { typeCssClass } from '@/graph/schema';
import type { GraphEdge, NodeData, NodeKind, SolType } from '@/graph/schema';
import { PALETTE, categoryForKind } from '@/graph/kinds';
import SolNode from './SolNode.vue';
import ContextMenu, { type ContextMenuItem } from './ContextMenu.vue';
import QuickAddPalette, { type SourceContext } from './QuickAddPalette.vue';
import NodeSearchPalette from './NodeSearchPalette.vue';
import ExecutionControls from './ExecutionControls.vue';
import ExecutionTimeline from './ExecutionTimeline.vue';
import { onMounted, onBeforeUnmount } from 'vue';

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();
const blocks = useBlocksStore();
const toasts = useToastStore();
const {
  fitView,
  screenToFlowCoordinate,
  getSelectedNodes,
  onConnectStart,
  onConnectEnd,
  setCenter,
  getNode,
  getViewport,
  setViewport,
  nodes: vueFlowNodes,
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
    // Control edges = the workflow spine. Use a single brighter neutral
    // and a thicker stroke so they read as the primary path.
    // Data edges = type-tinted, thinner, slightly transparent, dashed —
    // they read as "plumbing" instead of competing visually with the
    // spine. Big win on dense graphs.
    let strokeColor: string;
    let strokeWidth: number;
    let strokeDasharray: string | undefined;
    let strokeOpacity: number | undefined;
    if (isControl) {
      strokeColor = '#cbd1de';
      strokeWidth = 2.4;
    } else {
      const src = fn.nodes.find((n) => n.id === e.source.node);
      const port = src?.ports.out.find((p) => p.id === e.source.port);
      const cls = typeCssClass(port?.type);
      strokeColor = cssVarForType(cls);
      strokeWidth = 1.4;
      strokeDasharray = '5 4';
      strokeOpacity = 0.72;
    }
    const active = sim.isEdgeActive(e.id);
    // Focus highlighting: edges incident to either the hovered OR the
    // selected node light up; the rest dim. Hover takes priority — when
    // the user is actively pointing somewhere they want immediate
    // feedback; selection is the steady-state focus when the cursor is
    // elsewhere. Together they let a user select a node, then move the
    // cursor freely without losing the "this is what I'm working on"
    // visual context.
    const hovered = ui.hoveredNodeId;
    const focusId = hovered ?? ui.selectedNodeId;
    const related =
      focusId != null && (e.source.node === focusId || e.target.node === focusId);
    const dim = focusId != null && !related;
    const classes: string[] = [isControl ? 'sf-edge-control' : 'sf-edge-data'];
    if (active) classes.push('sf-edge-active');
    if (related) classes.push('sf-edge-related');
    if (dim) classes.push('sf-edge-dim');
    return {
      id: e.id,
      source: e.source.node,
      target: e.target.node,
      sourceHandle: e.source.port,
      targetHandle: e.target.port,
      // Bezier routes data edges more gently around clutter; smoothstep
      // keeps control edges feeling like an orthogonal program flow.
      type: isControl ? 'smoothstep' : 'bezier',
      class: classes.join(' '),
      style: {
        stroke: strokeColor,
        strokeWidth,
        ...(strokeDasharray ? { strokeDasharray } : {}),
        ...(strokeOpacity !== undefined ? { strokeOpacity } : {}),
      },
      animated: false,
      // Widen the invisible interaction stroke so edges are easy to click/select.
      interactionWidth: 22,
      markerEnd: { type: MarkerType.ArrowClosed, color: strokeColor, width: 12, height: 12 },
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
          // Defer schema checks to loadWorkflow — it owns the canonical
          // shape validation and toasts the specific issue. We only
          // toast success when the store accepts the workflow.
          const ok = graph.loadWorkflow(parsed);
          if (ok) {
            toasts.success('Workflow loaded', `Dropped "${parsed.meta?.name || file.name}" onto the canvas.`);
          }
        })
        .catch((e) => toasts.error('Could not load workflow', (e as Error).message));
      return;
    }
  }

  // CRITICAL: convert via screenToFlowCoordinate, not getBoundingClientRect.
  // The canvas-host rect ignores Vue Flow's viewport transform (pan + zoom),
  // so HTML-relative coords land in the wrong flow position whenever the
  // viewport is panned or zoomed. screenToFlowCoordinate is the single
  // boundary every creation path goes through.
  const flowPos = screenToFlowCoordinate({
    x: event.clientX,
    y: event.clientY,
  });

  // Case 2: user dropped a reusable block (built-in pattern or saved).
  const blockRaw = event.dataTransfer?.getData('application/x-solflow-block');
  if (blockRaw) {
    try {
      const meta = JSON.parse(blockRaw) as { origin: 'user' | 'builtin'; id: string };
      const snapshot =
        meta.origin === 'builtin'
          ? buildBuiltinPattern(meta.id)
          : blocks.findById(meta.id) ?? null;
      if (!snapshot) return;
      const newIds = graph.insertBlock(snapshot, flowPos);
      // Select the first new node so the user can immediately see they
      // landed and the Inspector opens with a useful context.
      if (newIds.length > 0) ui.selectNode(newIds[0]);
    } catch {
      /* ignore malformed block payload */
    }
    return;
  }

  // Case 3: user dropped a palette item.
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
  graph.addNode(kind, flowPos, init as Partial<NodeData> | undefined);
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

  // "Save as reusable block" — operates on the current marquee /
  // multi-select if it exists, otherwise just the right-clicked node.
  // Skips Start nodes since they're per-function singletons.
  const selectedIds = getSelectedNodes.value.map((n) => n.id);
  const saveSet = selectedIds.length > 1 && selectedIds.includes(id)
    ? selectedIds
    : [id];
  const saveableIds = saveSet.filter((nid) => {
    const n = fn?.nodes.find((x) => x.id === nid);
    return n && n.data.kind !== 'start';
  });

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
      label:
        saveableIds.length > 1
          ? `Save ${saveableIds.length} nodes as reusable block…`
          : 'Save as reusable block…',
      disabled: saveableIds.length === 0,
      action: () => saveAsBlock(saveableIds),
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

function saveAsBlock(ids: string[]) {
  if (ids.length === 0) return;
  const snap = graph.snapshotSelection(ids);
  if (!snap) return;
  const defaultName = ids.length === 1 ? 'New block' : `Block (${ids.length} nodes)`;
  const name = window.prompt(
    `Save ${ids.length} node${ids.length === 1 ? '' : 's'} as a reusable block.\n\nName:`,
    defaultName,
  );
  if (name === null) return;
  const description = window.prompt(
    'Optional description (helps when you come back to it later):',
    '',
  );
  blocks.save(name, description ?? '', snap.nodes, snap.edges);
  ui.setSidebarTab('blocks');
}

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

/**
 * Quick-Add selected a reusable block (built-in pattern or user-saved).
 * Builds the snapshot from the right source and inserts at the cursor
 * via graph.insertBlock — which auto-wraps multi-node blocks in a
 * named Frame. Selects the first content node so the Inspector opens
 * with useful context.
 */
function onQuickAddSelectBlock(meta: { origin: 'user' | 'builtin'; id: string }) {
  const snapshot =
    meta.origin === 'builtin'
      ? buildBuiltinPattern(meta.id)
      : blocks.findById(meta.id) ?? null;
  if (!snapshot) return;
  const newIds = graph.insertBlock(snapshot, qaFlowPos.value);
  if (newIds.length > 0) ui.selectNode(newIds[0]);
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

// =============================================================
//  Alignment + distribution
// =============================================================
// Figma-style floating toolbar that appears when ≥2 nodes are
// selected. All operations write through graph.updateNodePosition so
// they participate in the same debounced history snapshot as drag-
// stops — one undo step covers the entire alignment.

const selectedCount = computed(() => getSelectedNodes.value.length);

interface DimRef {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
function selectedDims(): DimRef[] {
  return getSelectedNodes.value.map((n) => ({
    id: n.id,
    x: n.position.x,
    y: n.position.y,
    // Vue Flow measures node dimensions after mount; fall back to a
    // standard estimate (matches SolNode min-width / typical height)
    // so alignment math still produces sensible results before the
    // first measurement.
    w: n.dimensions?.width ?? 220,
    h: n.dimensions?.height ?? 60,
  }));
}

type AlignMode = 'left' | 'right' | 'top' | 'bottom' | 'centerH' | 'centerV';
function alignSelected(mode: AlignMode) {
  const dims = selectedDims();
  if (dims.length < 2) return;
  let updates: { id: string; x: number; y: number }[] = [];
  switch (mode) {
    case 'left': {
      const x = Math.min(...dims.map((d) => d.x));
      updates = dims.map((d) => ({ id: d.id, x, y: d.y }));
      break;
    }
    case 'right': {
      const rightEdge = Math.max(...dims.map((d) => d.x + d.w));
      updates = dims.map((d) => ({ id: d.id, x: rightEdge - d.w, y: d.y }));
      break;
    }
    case 'top': {
      const y = Math.min(...dims.map((d) => d.y));
      updates = dims.map((d) => ({ id: d.id, x: d.x, y }));
      break;
    }
    case 'bottom': {
      const bottomEdge = Math.max(...dims.map((d) => d.y + d.h));
      updates = dims.map((d) => ({ id: d.id, x: d.x, y: bottomEdge - d.h }));
      break;
    }
    case 'centerH': {
      const avgCenter =
        dims.reduce((s, d) => s + (d.x + d.w / 2), 0) / dims.length;
      updates = dims.map((d) => ({
        id: d.id,
        x: avgCenter - d.w / 2,
        y: d.y,
      }));
      break;
    }
    case 'centerV': {
      const avgCenter =
        dims.reduce((s, d) => s + (d.y + d.h / 2), 0) / dims.length;
      updates = dims.map((d) => ({
        id: d.id,
        x: d.x,
        y: avgCenter - d.h / 2,
      }));
      break;
    }
  }
  for (const u of updates) {
    graph.updateNodePosition(u.id, { x: Math.round(u.x), y: Math.round(u.y) });
  }
}

type DistributeMode = 'horizontal' | 'vertical';
function distributeSelected(mode: DistributeMode) {
  const dims = selectedDims();
  if (dims.length < 3) return;
  if (mode === 'horizontal') {
    // Keep leftmost + rightmost in place; space the rest evenly between
    // their centers so the gap LOOKS uniform regardless of node widths.
    const sorted = [...dims].sort((a, b) => a.x + a.w / 2 - (b.x + b.w / 2));
    const first = sorted[0];
    const last = sorted[sorted.length - 1];
    const firstC = first.x + first.w / 2;
    const lastC = last.x + last.w / 2;
    const step = (lastC - firstC) / (sorted.length - 1);
    for (let i = 1; i < sorted.length - 1; i++) {
      const d = sorted[i];
      const targetCenter = firstC + step * i;
      const x = Math.round(targetCenter - d.w / 2);
      graph.updateNodePosition(d.id, { x, y: d.y });
    }
  } else {
    const sorted = [...dims].sort((a, b) => a.y + a.h / 2 - (b.y + b.h / 2));
    const first = sorted[0];
    const last = sorted[sorted.length - 1];
    const firstC = first.y + first.h / 2;
    const lastC = last.y + last.h / 2;
    const step = (lastC - firstC) / (sorted.length - 1);
    for (let i = 1; i < sorted.length - 1; i++) {
      const d = sorted[i];
      const targetCenter = firstC + step * i;
      const y = Math.round(targetCenter - d.h / 2);
      graph.updateNodePosition(d.id, { x: d.x, y });
    }
  }
}

// MiniMap auto-hide threshold. Workflows with very few nodes don't
// benefit from an overview (you can already see the whole graph at
// reasonable zoom); the minimap becomes pure visual clutter. Above
// MINIMAP_THRESHOLD nodes the overview becomes genuinely useful for
// navigation. Counted across all functions in the workflow, not just
// the active function, so the minimap stays visible when you switch
// to a small auxiliary function inside a large workflow.
const MINIMAP_THRESHOLD = 6;
const showMinimap = computed(() => {
  let total = 0;
  for (const fn of graph.workflow.functions) {
    total += fn.nodes.length;
    if (total > MINIMAP_THRESHOLD) return true;
  }
  return false;
});

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
  // Cmd/Ctrl+A → Select all nodes in active function. Vue Flow's
  // internal selection state is the source of truth for multi-select,
  // so we flip `selected` on every node it knows about.
  if (mod && e.key.toLowerCase() === 'a' && !isTypingInInput()) {
    e.preventDefault();
    for (const n of vueFlowNodes.value) {
      n.selected = true;
    }
    return;
  }
  // Cmd/Ctrl+0 → reset zoom to 100% at the current pan center
  if (mod && (e.key === '0') && !isTypingInInput()) {
    e.preventDefault();
    const vp = getViewport();
    setViewport({ x: vp.x, y: vp.y, zoom: 1 }, { duration: 220 });
    return;
  }
  // Cmd/Ctrl+= or +  → zoom in
  // Cmd/Ctrl+-       → zoom out
  // Standard browser-style zoom keystrokes. Step by 1.2× per press;
  // clamped to Vue Flow's configured min/max via setViewport.
  if (mod && (e.key === '=' || e.key === '+') && !isTypingInInput()) {
    e.preventDefault();
    const vp = getViewport();
    const next = Math.min(2, vp.zoom * 1.2);
    setViewport({ x: vp.x, y: vp.y, zoom: next }, { duration: 160 });
    return;
  }
  if (mod && e.key === '-' && !isTypingInInput()) {
    e.preventDefault();
    const vp = getViewport();
    const next = Math.max(0.2, vp.zoom / 1.2);
    setViewport({ x: vp.x, y: vp.y, zoom: next }, { duration: 160 });
    return;
  }
  // Home → fit whole graph (no-selection fitView)
  if (!mod && e.key === 'Home' && !isTypingInInput()) {
    e.preventDefault();
    fitView({ padding: 0.2, duration: 300 });
    return;
  }
  // '1' (no modifier) → Fit selection (or fit whole graph when nothing selected)
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
        stroke: '#6c5ce7',
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
      <Controls :show-interactive="false">
        <!--
          Fit-to-selection button slotted into Vue Flow's built-in
          Controls bar. Discoverable counterpart to the `1` keyboard
          shortcut and the always-available "Fit graph" control above.
          Disabled when there's no selection so it's clear what the
          button operates on.
        -->
        <button
          type="button"
          class="vue-flow__controls-button sf-fit-selection-btn"
          :disabled="selectedCount === 0"
          title="Fit selection (1)"
          aria-label="Fit selection to view"
          @click="fitSelection"
        >
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
            <rect x="3" y="3" width="10" height="10" rx="1" stroke="currentColor" stroke-width="1.4" stroke-dasharray="2 1.6" />
            <path d="M6 8 L8 10 L10 6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      </Controls>
      <!--
        Minimap auto-hides for very small workflows where it's just
        clutter. Above ~6 nodes the overview becomes genuinely useful;
        below that the canvas reads cleanly without it. Threshold
        chosen so the simplest "trigger → let → branch → print → print"
        shape (~5 nodes) keeps the canvas clean.
      -->
      <MiniMap
        v-if="showMinimap"
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
      @select-block="onQuickAddSelectBlock"
      @close="closeQuickAdd"
    />
    <NodeSearchPalette
      :open="searchOpen"
      @jump="onSearchJump"
      @close="closeSearch"
    />
    <ExecutionTimeline />
    <ExecutionControls />

    <!--
      Alignment + distribution toolbar. Visible only when 2+ nodes are
      selected. Anchored top-center of the canvas so it doesn't fight
      the minimap (top-right) or the execution controls (bottom-center).
      Each press writes a batch of updateNodePosition calls; the
      debounced snapshot collapses them into one undo step.
    -->
    <Transition name="align-fade">
      <div v-if="selectedCount >= 2" class="align-toolbar">
        <button
          v-for="b in [
            { mode: 'left',    label: 'Align left',           icon: '⫷' },
            { mode: 'centerH', label: 'Center horizontally',  icon: '↔' },
            { mode: 'right',   label: 'Align right',          icon: '⫸' },
          ]"
          :key="`h:${b.mode}`"
          type="button"
          class="align-btn"
          :title="b.label"
          :aria-label="b.label"
          @click="alignSelected(b.mode as AlignMode)"
        >{{ b.icon }}</button>
        <div class="align-sep" />
        <button
          v-for="b in [
            { mode: 'top',     label: 'Align top',            icon: '⤒' },
            { mode: 'centerV', label: 'Center vertically',    icon: '↕' },
            { mode: 'bottom',  label: 'Align bottom',         icon: '⤓' },
          ]"
          :key="`v:${b.mode}`"
          type="button"
          class="align-btn"
          :title="b.label"
          :aria-label="b.label"
          @click="alignSelected(b.mode as AlignMode)"
        >{{ b.icon }}</button>
        <div class="align-sep" />
        <button
          type="button"
          class="align-btn"
          :disabled="selectedCount < 3"
          title="Distribute horizontally (≥3 nodes)"
          aria-label="Distribute horizontally"
          @click="distributeSelected('horizontal')"
        >⇆</button>
        <button
          type="button"
          class="align-btn"
          :disabled="selectedCount < 3"
          title="Distribute vertically (≥3 nodes)"
          aria-label="Distribute vertically"
          @click="distributeSelected('vertical')"
        >⇅</button>
        <span class="align-count">{{ selectedCount }} selected</span>
      </div>
    </Transition>
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

/*
 * Alignment toolbar — floating pill at the top-center of the canvas
 * while a multi-selection is active. Visual density similar to the
 * ExecutionControls bar so the two share an interaction vocabulary.
 */
.align-toolbar {
  position: absolute;
  left: 50%;
  top: 14px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px 4px 6px;
  background: rgba(17, 17, 17, 0.92);
  border: 1px solid var(--sf-border-strong);
  border-radius: 999px;
  box-shadow: var(--sf-shadow-3);
  backdrop-filter: blur(8px);
  z-index: var(--sf-z-popover);
  font-size: 0.6875rem;
  color: var(--sf-text-1);
}
.align-fade-enter-active,
.align-fade-leave-active {
  transition: opacity 0.14s ease, transform 0.14s ease;
}
.align-fade-enter-from,
.align-fade-leave-to {
  opacity: 0;
  transform: translate(-50%, -6px);
}
.align-btn {
  width: 26px;
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 50%;
  color: var(--sf-text-1);
  cursor: pointer;
  padding: 0;
  font-size: 0.875rem;
  font-family: var(--sf-font-mono);
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.align-btn:hover:not(:disabled) {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border);
}
.align-btn:focus-visible {
  outline: none;
  border-color: var(--sf-accent);
  box-shadow: 0 0 0 1px var(--sf-accent-dim);
}
.align-btn:disabled {
  opacity: 0.34;
  cursor: not-allowed;
}
.align-sep {
  width: 1px;
  height: 16px;
  background: var(--sf-border);
  margin: 0 2px;
}
.align-count {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
  color: var(--sf-text-3);
  padding: 0 6px 0 4px;
  text-transform: uppercase;
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
