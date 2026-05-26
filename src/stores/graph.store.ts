/**
 * SolFlow Phase A — graph store.
 *
 * Holds the whole `SolWorkflow` reactively. All node/edge mutations go
 * through actions here so reactivity + autosave + future undo are consistent.
 *
 * Autosave: every change debounce-persists to localStorage. On boot,
 * `bootstrap()` decides whether to resume a draft or start empty.
 */

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { nanoid } from 'nanoid';

import type {
  EnumDecl,
  FunctionGraph,
  GraphEdge,
  GraphNode,
  ImportDecl,
  NodeData,
  NodeKind,
  Param,
  SolType,
  SolWorkflow,
  StructDecl,
} from '@/graph/schema';
import { createNode, rebuildPorts, type WorkflowCtx } from '@/graph/factory';
import { bindingsInScope, type ScopeBinding } from '@/graph/scope';
import { emit } from '@/emit/emit';
import { validateWorkflow, type Diagnostic } from '@/graph/validate';

const STORAGE_KEY = 'solflow.draft.v1';

function nowIso(): string {
  return new Date().toISOString();
}

function emptyFunction(name: string, params: Param[] = []): FunctionGraph {
  const startNode: GraphNode = {
    id: nanoid(8),
    data: { kind: 'start' },
    position: { x: 80, y: 60 },
    ports: { in: [], out: [{ id: 'next', name: 'next', kind: 'control', required: true }] },
  };
  return {
    id: nanoid(8),
    name,
    params,
    returnType: { kind: 'void' },
    nodes: [startNode],
    edges: [],
  };
}

function emptyWorkflow(): SolWorkflow {
  return {
    schemaVersion: 1,
    meta: {
      name: 'untitled',
      createdAt: nowIso(),
      updatedAt: nowIso(),
    },
    imports: [],
    structs: [],
    enums: [],
    functions: [emptyFunction('start')],
  };
}

export const useGraphStore = defineStore('graph', () => {
  const workflow = ref<SolWorkflow>(emptyWorkflow());
  const activeFunctionId = ref<string>(workflow.value.functions[0].id);

  // -----------------------------------------------------------
  // Derived
  // -----------------------------------------------------------

  const activeFunction = computed<FunctionGraph | undefined>(() =>
    workflow.value.functions.find((f) => f.id === activeFunctionId.value),
  );

  const ctx = computed<WorkflowCtx>(() => ({
    structs: workflow.value.structs,
    enums: workflow.value.enums,
    functions: workflow.value.functions,
  }));

  const emitted = computed(() => emit(workflow.value));

  const diagnostics = computed<Diagnostic[]>(() => validateWorkflow(workflow.value));

  // -----------------------------------------------------------
  // Bootstrap (resume from localStorage or start fresh)
  // -----------------------------------------------------------

  function bootstrap() {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw) as SolWorkflow;
      if (parsed && parsed.schemaVersion === 1 && parsed.functions?.length) {
        workflow.value = parsed;
        activeFunctionId.value = parsed.functions[0].id;
      }
    } catch {
      /* ignore corrupted draft */
    }
  }

  // -----------------------------------------------------------
  // Functions (multi-function support)
  // -----------------------------------------------------------

  function addFunction(name = 'fn'): FunctionGraph {
    // ensure unique name
    let n = name;
    let i = 1;
    while (workflow.value.functions.find((f) => f.name === n)) {
      n = `${name}${i++}`;
    }
    const fn = emptyFunction(n);
    workflow.value.functions.push(fn);
    activeFunctionId.value = fn.id;
    touch();
    return fn;
  }

  function deleteFunction(id: string) {
    if (workflow.value.functions.length <= 1) return; // keep at least one
    workflow.value.functions = workflow.value.functions.filter((f) => f.id !== id);
    if (activeFunctionId.value === id) {
      activeFunctionId.value = workflow.value.functions[0].id;
    }
    touch();
  }

  function renameFunction(id: string, newName: string) {
    const fn = workflow.value.functions.find((f) => f.id === id);
    if (!fn) return;
    fn.name = newName;
    touch();
  }

  function updateWorkflowMeta(patch: { name?: string; description?: string }) {
    if (patch.name !== undefined) workflow.value.meta.name = patch.name;
    if (patch.description !== undefined)
      workflow.value.meta.description = patch.description;
    touch();
  }

  function updateFunctionSignature(
    id: string,
    params: Param[],
    returnType: SolType,
  ) {
    const fn = workflow.value.functions.find((f) => f.id === id);
    if (!fn) return;
    fn.params = params;
    fn.returnType = returnType;
    // Rebuild ports on every call-node referencing this function.
    for (const otherFn of workflow.value.functions) {
      for (const node of otherFn.nodes) {
        if (node.data.kind === 'call' && node.data.functionId === id) {
          node.ports = rebuildPorts(node.data, ctx.value);
        }
      }
    }
    touch();
  }

  function setActiveFunction(id: string) {
    if (workflow.value.functions.find((f) => f.id === id)) {
      activeFunctionId.value = id;
    }
  }

  // -----------------------------------------------------------
  // Nodes
  // -----------------------------------------------------------

  function addNode(
    kind: NodeKind,
    position: { x: number; y: number },
    init?: Partial<NodeData>,
  ) {
    const fn = activeFunction.value;
    if (!fn) return;
    // Single-Start invariant: a function may have at most one Start
    // node. If the user drags another Start onto the canvas, point them
    // at the existing one instead of creating a duplicate. Multi-Start
    // would silently confuse the interpreter (which picks "the first").
    if (kind === 'start') {
      const existing = fn.nodes.find((n) => n.data.kind === 'start');
      if (existing) return existing;
    }
    const node = createNode(kind, position, ctx.value, init);
    fn.nodes.push(node);
    touch();
    return node;
  }

  function updateNodePosition(nodeId: string, position: { x: number; y: number }) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = fn.nodes.find((n) => n.id === nodeId);
    if (!node) return;
    node.position = position;
    touch();
  }

  /**
   * Duplicate a node. Copies data + ports + inline expressions; gets a
   * new nanoid + a free position near the original. Skips the Start
   * node since there can be only one per function.
   */
  function duplicateNode(nodeId: string): GraphNode | undefined {
    const fn = activeFunction.value;
    if (!fn) return undefined;
    const orig = fn.nodes.find((n) => n.id === nodeId);
    if (!orig) return undefined;
    if (orig.data.kind === 'start') return undefined;
    const pos = findFreePosition(
      { x: orig.position.x + 32, y: orig.position.y + 32 },
      fn.nodes,
    );
    const copy: GraphNode = {
      id: nanoid(8),
      data: JSON.parse(JSON.stringify(orig.data)) as NodeData,
      position: pos,
      ports: JSON.parse(JSON.stringify(orig.ports)),
      expressions: orig.expressions ? { ...orig.expressions } : undefined,
    };
    fn.nodes.push(copy);
    touch();
    return copy;
  }

  /**
   * Duplicate every node id in the array as a single batch (one undo
   * step). Each copy is offset to avoid overlap; if multiple of the
   * input ids referenced the same Start node, it's skipped silently.
   * Returns the new node ids in the same order.
   */
  function duplicateNodes(nodeIds: string[]): string[] {
    const fn = activeFunction.value;
    if (!fn) return [];
    // Single-node case: keep the existing per-node path so the position
    // offset stays predictable. Multi-node selections route through the
    // snapshot-paste helper so internal wiring between selected nodes
    // is preserved (the old per-node loop dropped those edges).
    if (nodeIds.length <= 1) {
      const newIds: string[] = [];
      for (const id of nodeIds) {
        const dup = duplicateNode(id);
        if (dup) newIds.push(dup.id);
      }
      return newIds;
    }
    const snap = captureSnapshot(nodeIds);
    if (!snap) return [];
    // Offset the new cluster down-right so it doesn't sit on top of the
    // originals. 48px diagonal feels intentional without being huge.
    const pos = { x: snap.centroid.x + 48, y: snap.centroid.y + 48 };
    return insertSnapshot(snap, pos);
  }

  // -----------------------------------------------------------
  // Copy / paste / reusable-block insertion (shared snapshot model)
  // -----------------------------------------------------------
  //
  // Three operations all reduce to "drop a snapshot of (nodes, edges,
  // centroid) at a flow position":
  //   - paste from clipboard
  //   - duplicate a multi-node selection (preserves internal wiring)
  //   - insert a saved block / built-in pattern
  //
  // captureSnapshot() builds the payload; insertSnapshot() applies it
  // with id remapping + collision-avoidance positioning. Both used
  // internally and exported as insertBlock() for blocks.store callers.
  //
  // Edges are included only when both endpoints are inside the
  // captured set, so pasting reconstructs internal wiring without
  // dangling refs to the original graph.

  interface SnapshotPayload {
    nodes: GraphNode[];
    edges: GraphEdge[];
    centroid: { x: number; y: number };
  }
  const clipboard = ref<SnapshotPayload | null>(null);

  function captureSnapshot(nodeIds: string[]): SnapshotPayload | null {
    const fn = activeFunction.value;
    if (!fn) return null;
    const set = new Set(nodeIds);
    const nodes = fn.nodes.filter(
      (n) => set.has(n.id) && n.data.kind !== 'start',
    );
    if (nodes.length === 0) return null;
    const edges = fn.edges.filter(
      (e) => set.has(e.source.node) && set.has(e.target.node),
    );
    const cx = nodes.reduce((s, n) => s + n.position.x, 0) / nodes.length;
    const cy = nodes.reduce((s, n) => s + n.position.y, 0) / nodes.length;
    return {
      nodes: JSON.parse(JSON.stringify(nodes)) as GraphNode[],
      edges: JSON.parse(JSON.stringify(edges)) as GraphEdge[],
      centroid: { x: cx, y: cy },
    };
  }

  function insertSnapshot(
    snap: SnapshotPayload,
    flowPos: { x: number; y: number },
  ): string[] {
    const fn = activeFunction.value;
    if (!fn) return [];
    const idMap = new Map<string, string>();
    const newIds: string[] = [];
    for (const n of snap.nodes) {
      // Skip start nodes — a function only ever has one. If the snapshot
      // somehow includes one (e.g. an old saved block), drop it cleanly.
      if (n.data.kind === 'start' && fn.nodes.some((x) => x.data.kind === 'start')) {
        continue;
      }
      const newId = nanoid(8);
      idMap.set(n.id, newId);
      const dx = n.position.x - snap.centroid.x;
      const dy = n.position.y - snap.centroid.y;
      const pos = findFreePosition(
        { x: flowPos.x + dx, y: flowPos.y + dy },
        fn.nodes,
      );
      const clone = JSON.parse(JSON.stringify(n)) as GraphNode;
      clone.id = newId;
      clone.position = pos;
      fn.nodes.push(clone);
      newIds.push(newId);
    }
    for (const e of snap.edges) {
      const sn = idMap.get(e.source.node);
      const tn = idMap.get(e.target.node);
      if (!sn || !tn) continue;
      const clone = JSON.parse(JSON.stringify(e)) as GraphEdge;
      clone.id = nanoid(8);
      clone.source = { node: sn, port: e.source.port };
      clone.target = { node: tn, port: e.target.port };
      fn.edges.push(clone);
    }
    touch();
    return newIds;
  }

  function copyNodes(nodeIds: string[]): number {
    const snap = captureSnapshot(nodeIds);
    if (!snap) return 0;
    clipboard.value = snap;
    return snap.nodes.length;
  }

  function hasClipboard(): boolean {
    return clipboard.value !== null && clipboard.value.nodes.length > 0;
  }

  function pasteAt(flowPos: { x: number; y: number }): string[] {
    const c = clipboard.value;
    if (!c) return [];
    return insertSnapshot(c, flowPos);
  }

  /**
   * Insert a saved (or built-in) block as a fresh cluster of nodes at
   * the given flow position. Used by the BlocksPanel drag-drop and the
   * Quick-Add block-insertion path. Behaves like pasteAt() but takes
   * the snapshot directly instead of from the clipboard, so block
   * insertion doesn't disturb the user's copy/paste workflow.
   *
   * Visual language: when the block has more than one node AND a
   * meaningful name, the inserted cluster is automatically wrapped in
   * a Frame titled after the block. Gives users on-canvas grouping +
   * provenance without a schema change. The frame is a regular node
   * they can resize, rename, or delete like any other.
   *
   * Single-node patterns (e.g. "Extract from payload") drop bare so
   * we don't over-decorate one-shots.
   */
  function insertBlock(
    block: {
      name?: string;
      nodes: GraphNode[];
      edges: GraphEdge[];
      centroid: { x: number; y: number };
    },
    flowPos: { x: number; y: number },
  ): string[] {
    const newIds = insertSnapshot(block, flowPos);
    const shouldWrap = newIds.length >= 2 && !!block.name && block.name.trim() !== '';
    if (!shouldWrap) return newIds;

    const fn = activeFunction.value;
    if (!fn) return newIds;
    const inserted = fn.nodes.filter((n) => newIds.includes(n.id));
    if (inserted.length < 2) return newIds;

    // Tight bounding box around the just-inserted nodes. We use a
    // conservative node footprint (220x60) — actual nodes vary, but
    // the frame is decorative so an estimate is fine; the user can
    // resize via the SE corner handle.
    const NODE_W = 220;
    const NODE_H = 60;
    const PAD = 32;
    const xs = inserted.map((n) => n.position.x);
    const ys = inserted.map((n) => n.position.y);
    const minX = Math.min(...xs);
    const minY = Math.min(...ys);
    const maxX = Math.max(...xs) + NODE_W;
    const maxY = Math.max(...ys) + NODE_H * 1.5;
    const framePos = { x: minX - PAD, y: minY - PAD - 16 };
    const frameW = maxX - minX + PAD * 2;
    const frameH = maxY - minY + PAD * 2 + 16;

    const frame = createNode(
      'frame',
      framePos,
      ctx.value,
      {
        kind: 'frame',
        title: block.name!,
        width: Math.max(220, Math.round(frameW)),
        height: Math.max(160, Math.round(frameH)),
      } as Partial<NodeData>,
    );
    fn.nodes.push(frame);
    touch();
    // Return the original ids — callers (Canvas) select the first new
    // content node, not the frame, so the Inspector opens with the
    // most relevant context.
    return newIds;
  }

  /**
   * Capture-only helper exposed to callers that want to save a
   * selection without depending on the in-app clipboard (e.g. the
   * "Save as reusable block" action). Returns null for empty
   * selections.
   */
  function snapshotSelection(nodeIds: string[]): SnapshotPayload | null {
    return captureSnapshot(nodeIds);
  }

  /**
   * Find a free position near `preferred` that doesn't overlap any
   * existing node within ~48px. Walks a small spiral if needed.
   * Cheap O(nodes * steps); good enough for Phase A graph sizes.
   */
  function findFreePosition(
    preferred: { x: number; y: number },
    nodes: GraphNode[],
  ): { x: number; y: number } {
    const minDist = 36;
    const step = 32;
    const overlaps = (p: { x: number; y: number }) =>
      nodes.some(
        (n) =>
          Math.abs(n.position.x - p.x) < minDist &&
          Math.abs(n.position.y - p.y) < minDist,
      );
    if (!overlaps(preferred)) return preferred;
    // Try in a square spiral.
    for (let r = 1; r <= 8; r++) {
      for (let dx = -r; dx <= r; dx++) {
        for (let dy = -r; dy <= r; dy++) {
          if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
          const p = { x: preferred.x + dx * step, y: preferred.y + dy * step };
          if (!overlaps(p)) return p;
        }
      }
    }
    return preferred;
  }

  /**
   * Add a node at an explicit flow-coordinate position. Optionally
   * auto-create a connecting edge from the given source port. Used by
   * the Quick-Add palette (Space / dbl-click / drag-edge-to-empty).
   * Returns the new node so the caller can select it.
   */
  function addNodeAt(
    kind: NodeKind,
    position: { x: number; y: number },
    autoConnect?: { fromNode: string; fromPort: string; edgeKind: 'control' | 'data' },
    init?: Partial<NodeData>,
  ): GraphNode | undefined {
    const fn = activeFunction.value;
    if (!fn) return undefined;
    // Auto-connect placement: when the user drags from a port and lets go
    // in empty space, prefer a position that's at least one "node-height"
    // below (control) or to the right of (data) the source node. The
    // raw release position can land right on top of the source and look
    // like a glitch — this gives a clean, predictable drop.
    let preferred = position;
    if (autoConnect) {
      const src = fn.nodes.find((n) => n.id === autoConnect.fromNode);
      if (src) {
        const MIN_GAP = 96;
        if (autoConnect.edgeKind === 'control') {
          const minY = src.position.y + MIN_GAP;
          if (preferred.y < minY) {
            preferred = { x: src.position.x, y: minY };
          }
        } else {
          const minX = src.position.x + 280;
          if (preferred.x < minX) {
            preferred = { x: minX, y: src.position.y };
          }
        }
      }
    }
    // Single-Start invariant — mirror of addNode.
    if (kind === 'start') {
      const existing = fn.nodes.find((n) => n.data.kind === 'start');
      if (existing) return existing;
    }
    const safePos = findFreePosition(preferred, fn.nodes);
    const node = createNode(kind, safePos, ctx.value, init);
    fn.nodes.push(node);

    if (autoConnect) {
      const srcNode = fn.nodes.find((n) => n.id === autoConnect.fromNode);
      const srcPort = srcNode?.ports.out.find((p) => p.id === autoConnect.fromPort);
      // Find first compatible input port on the new node.
      let target: { node: string; port: string } | undefined;
      for (const p of node.ports.in) {
        if (p.kind !== autoConnect.edgeKind) continue;
        if (p.kind === 'control') {
          target = { node: node.id, port: p.id };
          break;
        }
        // Data: prefer exact type match, then any.
        if (srcPort?.type && p.type) {
          if (
            srcPort.type.kind === p.type.kind ||
            srcPort.type.kind === 'any' ||
            p.type.kind === 'any'
          ) {
            target = { node: node.id, port: p.id };
            break;
          }
        } else {
          target = { node: node.id, port: p.id };
          break;
        }
      }
      if (target) {
        const edge: GraphEdge = {
          id: nanoid(8),
          source: { node: autoConnect.fromNode, port: autoConnect.fromPort },
          target,
          kind: autoConnect.edgeKind,
        };
        fn.edges.push(edge);
      }
    }

    touch();
    return node;
  }

  function updateNodeData(nodeId: string, patch: Partial<NodeData>) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = fn.nodes.find((n) => n.id === nodeId);
    if (!node) return;

    // Detect variable-name rename for propagation to varGet/assign references.
    let renameFrom: string | undefined;
    let renameTo: string | undefined;
    const oldKind = node.data.kind;
    if (
      oldKind === 'let' &&
      'varName' in patch &&
      typeof (patch as { varName?: unknown }).varName === 'string' &&
      (patch as { varName: string }).varName !== node.data.varName
    ) {
      renameFrom = node.data.varName;
      renameTo = (patch as { varName: string }).varName;
    }
    if (
      oldKind === 'forEach' &&
      'iteratorName' in patch &&
      typeof (patch as { iteratorName?: unknown }).iteratorName === 'string' &&
      (patch as { iteratorName: string }).iteratorName !== node.data.iteratorName
    ) {
      renameFrom = node.data.iteratorName;
      renameTo = (patch as { iteratorName: string }).iteratorName;
    }

    node.data = { ...node.data, ...patch } as NodeData;
    const newPorts = rebuildPorts(node.data, ctx.value);

    // Drop edges that point at ports that no longer exist.
    const validInIds = new Set(newPorts.in.map((p) => p.id));
    const validOutIds = new Set(newPorts.out.map((p) => p.id));
    fn.edges = fn.edges.filter((e) => {
      if (e.source.node === nodeId && !validOutIds.has(e.source.port)) return false;
      if (e.target.node === nodeId && !validInIds.has(e.target.port)) return false;
      return true;
    });

    // Drop inline expressions for ports that no longer exist.
    if (node.expressions) {
      const cleaned: Record<string, string> = {};
      for (const [portId, expr] of Object.entries(node.expressions)) {
        if (validInIds.has(portId)) cleaned[portId] = expr;
      }
      node.expressions = cleaned;
    }

    node.ports = newPorts;

    // Propagate rename to any varGet / assign nodes that reference the
    // old name in the same function. We also rewrite inline expressions
    // where the old name appears as a bare identifier (word boundary
    // match — naive but catches the common case of `counter < 4` →
    // `count < 4` after a let rename).
    if (renameFrom && renameTo && renameFrom !== renameTo) {
      const wordRe = new RegExp(`\\b${escapeRegex(renameFrom)}\\b`, 'g');
      for (const n of fn.nodes) {
        if (n.id === nodeId) continue;
        if (n.data.kind === 'varGet' && n.data.varName === renameFrom) {
          n.data.varName = renameTo;
          n.ports = rebuildPorts(n.data, ctx.value);
        }
        if (n.data.kind === 'assign' && n.data.varName === renameFrom) {
          n.data.varName = renameTo;
        }
        if (n.expressions) {
          for (const k of Object.keys(n.expressions)) {
            const before = n.expressions[k];
            const after = before.replace(wordRe, renameTo);
            if (after !== before) n.expressions[k] = after;
          }
        }
      }
    }

    touch();
  }

  function escapeRegex(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  /**
   * Set/clear an inline SOL expression for a node's input port.
   * Empty string clears it (so the emitter falls back to wired data).
   */
  function updateNodeExpression(nodeId: string, portId: string, text: string) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = fn.nodes.find((n) => n.id === nodeId);
    if (!node) return;
    if (!node.expressions) node.expressions = {};
    if (text.trim() === '') {
      delete node.expressions[portId];
    } else {
      node.expressions[portId] = text;
    }
    touch();
  }

  function removeNode(nodeId: string) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = fn.nodes.find((n) => n.id === nodeId);
    if (!node) return;
    // Entry-point safety: a function must always have at least one entry
    // (Start OR Trigger). Refuse to remove the last one — otherwise the
    // workflow is orphaned and the user has no way back to "where does
    // this start." Both kinds can be deleted freely as long as another
    // entry remains; this lets users replace Start with a Trigger or
    // swap one Trigger for another.
    if (node.data.kind === 'start' || node.data.kind === 'trigger') {
      const otherEntries = fn.nodes.filter(
        (n) =>
          n.id !== nodeId &&
          (n.data.kind === 'start' || n.data.kind === 'trigger'),
      );
      if (otherEntries.length === 0) return;
    }
    fn.nodes = fn.nodes.filter((n) => n.id !== nodeId);
    fn.edges = fn.edges.filter(
      (e) => e.source.node !== nodeId && e.target.node !== nodeId,
    );
    touch();
  }

  // -----------------------------------------------------------
  // Edges
  // -----------------------------------------------------------

  function addEdge(edge: Omit<GraphEdge, 'id'>): GraphEdge | undefined {
    const fn = activeFunction.value;
    if (!fn) return;

    // Validate endpoints exist.
    const src = fn.nodes.find((n) => n.id === edge.source.node);
    const tgt = fn.nodes.find((n) => n.id === edge.target.node);
    if (!src || !tgt) return;
    const srcPort = src.ports.out.find((p) => p.id === edge.source.port);
    const tgtPort = tgt.ports.in.find((p) => p.id === edge.target.port);
    if (!srcPort || !tgtPort) return;
    if (srcPort.kind !== tgtPort.kind) return;
    if (srcPort.kind !== edge.kind) return;

    // For control flow: only one outgoing edge per source port and one incoming per target.
    if (edge.kind === 'control') {
      fn.edges = fn.edges.filter(
        (e) =>
          !(
            e.kind === 'control' &&
            e.source.node === edge.source.node &&
            e.source.port === edge.source.port
          ),
      );
      fn.edges = fn.edges.filter(
        (e) =>
          !(
            e.kind === 'control' &&
            e.target.node === edge.target.node &&
            e.target.port === edge.target.port
          ),
      );
    } else {
      // For data flow: at most one incoming per target input port.
      fn.edges = fn.edges.filter(
        (e) =>
          !(
            e.kind === 'data' &&
            e.target.node === edge.target.node &&
            e.target.port === edge.target.port
          ),
      );
    }

    const newEdge: GraphEdge = { ...edge, id: nanoid(8) };
    fn.edges.push(newEdge);
    touch();
    return newEdge;
  }

  function removeEdge(edgeId: string) {
    const fn = activeFunction.value;
    if (!fn) return;
    fn.edges = fn.edges.filter((e) => e.id !== edgeId);
    touch();
  }

  // -----------------------------------------------------------
  // Structs / enums / imports
  // -----------------------------------------------------------

  function addStruct(name = 'NewStruct'): StructDecl {
    let n = name;
    let i = 1;
    while (workflow.value.structs.find((s) => s.name === n)) n = `${name}${i++}`;
    const s: StructDecl = { id: nanoid(8), name: n, fields: [] };
    workflow.value.structs.push(s);
    touch();
    return s;
  }
  function updateStruct(id: string, patch: Partial<StructDecl>) {
    const s = workflow.value.structs.find((s) => s.id === id);
    if (!s) return;
    Object.assign(s, patch);
    // Rebuild ports on any node referencing this struct.
    rebuildAllPorts();
    touch();
  }
  function deleteStruct(id: string) {
    workflow.value.structs = workflow.value.structs.filter((s) => s.id !== id);
    rebuildAllPorts();
    touch();
  }

  function addEnum(name = 'NewEnum'): EnumDecl {
    let n = name;
    let i = 1;
    while (workflow.value.enums.find((e) => e.name === n)) n = `${name}${i++}`;
    const e: EnumDecl = { id: nanoid(8), name: n, variants: [] };
    workflow.value.enums.push(e);
    touch();
    return e;
  }
  function updateEnum(id: string, patch: Partial<EnumDecl>) {
    const e = workflow.value.enums.find((e) => e.id === id);
    if (!e) return;
    Object.assign(e, patch);
    rebuildAllPorts();
    touch();
  }
  function deleteEnum(id: string) {
    workflow.value.enums = workflow.value.enums.filter((e) => e.id !== id);
    rebuildAllPorts();
    touch();
  }

  function addImport(): ImportDecl {
    const imp: ImportDecl = {
      id: nanoid(8),
      path: ['Router', 'App', 'Endpoint'],
      alias: 'Alias',
    };
    workflow.value.imports.push(imp);
    touch();
    return imp;
  }
  function updateImport(id: string, patch: Partial<ImportDecl>) {
    const imp = workflow.value.imports.find((i) => i.id === id);
    if (!imp) return;
    Object.assign(imp, patch);
    touch();
  }
  function deleteImport(id: string) {
    workflow.value.imports = workflow.value.imports.filter((i) => i.id !== id);
    touch();
  }

  // -----------------------------------------------------------
  // Bulk replace (for Load + Sample)
  // -----------------------------------------------------------

  function loadWorkflow(wf: SolWorkflow) {
    workflow.value = wf;
    activeFunctionId.value = wf.functions[0]?.id ?? '';
    rebuildAllPorts();
    touch();
  }

  function newWorkflow() {
    workflow.value = emptyWorkflow();
    activeFunctionId.value = workflow.value.functions[0].id;
    touch();
  }

  // -----------------------------------------------------------
  // Internals
  // -----------------------------------------------------------

  function rebuildAllPorts() {
    for (const fn of workflow.value.functions) {
      for (const node of fn.nodes) {
        node.ports = rebuildPorts(node.data, ctx.value);
      }
      // Drop dangling edges referencing ports that no longer exist.
      fn.edges = fn.edges.filter((e) => {
        const src = fn.nodes.find((n) => n.id === e.source.node);
        const tgt = fn.nodes.find((n) => n.id === e.target.node);
        if (!src || !tgt) return false;
        return (
          src.ports.out.some((p) => p.id === e.source.port) &&
          tgt.ports.in.some((p) => p.id === e.target.port)
        );
      });
    }
  }

  /**
   * Monotonic version counter that ticks on every store mutation. Used
   * as a cheap reactive cache key — see getScopeBindings below. Vue's
   * computed system can depend on this single ref instead of doing
   * deep reactivity sweeps across the whole workflow.
   */
  const version = ref(0);

  function touch() {
    workflow.value.meta.updatedAt = nowIso();
    version.value++;
  }

  // -----------------------------------------------------------
  // Memoized scope bindings
  // -----------------------------------------------------------
  // `bindingsInScope` does a BFS over a function's control edges every
  // time it's called. Inspector, ExpressionHelper, and BranchCondition-
  // Builder all call it — on the enterprise sample (47 nodes) that's
  // ~150 BFS walks per Inspector render when typing in an inline
  // expression. Most of those return the same answer.
  //
  // This shared cache keys by (functionId, nodeId, version) and clears
  // itself when version moves forward. Consumers wrap calls in a
  // computed; Vue's tracker sees `version.value` and re-evaluates
  // exactly when the graph structure changes — but only the FIRST
  // consumer pays the BFS cost; the rest hit the cache.
  const _scopeCache = new Map<string, ScopeBinding[]>();
  let _scopeCacheVersion = -1;
  function getScopeBindings(nodeId: string): ScopeBinding[] {
    // Reactive dep so Vue computeds that wrap this call re-run on
    // every store mutation. Without it the cache would stay warm
    // forever from the consumer's perspective.
    void version.value;
    if (_scopeCacheVersion !== version.value) {
      _scopeCache.clear();
      _scopeCacheVersion = version.value;
    }
    const fn = activeFunction.value;
    if (!fn) return [];
    const key = `${fn.id}::${nodeId}`;
    const hit = _scopeCache.get(key);
    if (hit) return hit;
    const result = bindingsInScope(fn, nodeId);
    _scopeCache.set(key, result);
    return result;
  }

  // -----------------------------------------------------------
  // Undo / redo
  // -----------------------------------------------------------
  // Snapshot-based history. Every settled change pushes a JSON snapshot
  // onto the stack; undo() restores the previous snapshot, redo() the next.
  // 200ms debounce coalesces rapid typing into single undo entries.

  const HISTORY_LIMIT = 80;
  const history: string[] = [];
  let historyIndex = -1;
  let isReplaying = false;
  let historyTimer: number | undefined;

  function pushHistory() {
    const snap = JSON.stringify(workflow.value);
    if (history[historyIndex] === snap) return;
    history.splice(historyIndex + 1);
    history.push(snap);
    if (history.length > HISTORY_LIMIT) {
      history.shift();
    }
    historyIndex = history.length - 1;
  }
  // Seed history with the initial state so the very first undo is a no-op
  // rather than restoring a phantom "empty" workflow.
  pushHistory();

  function canUndo(): boolean {
    return historyIndex > 0;
  }
  function canRedo(): boolean {
    return historyIndex < history.length - 1;
  }
  function undo() {
    if (!canUndo()) return;
    // Cancel any in-flight debounced snapshot. Without this, a snapshot
    // for the pre-undo state could fire AFTER the restore — splicing
    // off the redo history and making the user lose the operation they
    // just undid. Same applies to redo. This is the single most
    // important undo/redo correctness fix.
    if (historyTimer !== undefined) {
      window.clearTimeout(historyTimer);
      historyTimer = undefined;
    }
    isReplaying = true;
    historyIndex--;
    const snap = history[historyIndex];
    const parsed = JSON.parse(snap) as SolWorkflow;
    workflow.value = parsed;
    if (!parsed.functions.find((f) => f.id === activeFunctionId.value)) {
      activeFunctionId.value = parsed.functions[0]?.id ?? '';
    }
    // Allow the deep watcher to fire & finish before re-enabling capture.
    setTimeout(() => {
      isReplaying = false;
    }, 0);
  }
  function redo() {
    if (!canRedo()) return;
    if (historyTimer !== undefined) {
      window.clearTimeout(historyTimer);
      historyTimer = undefined;
    }
    isReplaying = true;
    historyIndex++;
    const snap = history[historyIndex];
    const parsed = JSON.parse(snap) as SolWorkflow;
    workflow.value = parsed;
    if (!parsed.functions.find((f) => f.id === activeFunctionId.value)) {
      activeFunctionId.value = parsed.functions[0]?.id ?? '';
    }
    setTimeout(() => {
      isReplaying = false;
    }, 0);
  }

  // Debounced autosave + history snapshot. Replays skip both.
  let saveTimer: number | undefined;
  /**
   * Synchronously persist the current workflow to localStorage. Called
   * by both the debounced autosave (after 600ms idle) and the
   * `beforeunload` handler below so changes made within the debounce
   * window are not lost when the user closes the tab. Idempotent and
   * cheap to call repeatedly.
   */
  function flushSave() {
    if (saveTimer !== undefined) {
      window.clearTimeout(saveTimer);
      saveTimer = undefined;
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(workflow.value));
    } catch {
      /* quota or unavailable — ignore */
    }
  }

  watch(
    () => workflow.value,
    () => {
      if (isReplaying) return;
      if (saveTimer !== undefined) window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => {
        flushSave();
      }, 600);

      if (historyTimer !== undefined) window.clearTimeout(historyTimer);
      historyTimer = window.setTimeout(() => {
        pushHistory();
      }, 220);
    },
    { deep: true },
  );

  // R1.7 / T9036 — flush the autosave debounce on tab close so the
  // user doesn't lose changes made in the 600ms before they close.
  // `beforeunload` is the last synchronous opportunity to write to
  // localStorage; we run flushSave even if no save is pending (cheap
  // no-op in that case).
  if (typeof window !== 'undefined') {
    window.addEventListener('beforeunload', () => {
      flushSave();
    });
  }

  return {
    // state
    workflow,
    activeFunctionId,
    // derived
    activeFunction,
    ctx,
    emitted,
    diagnostics,
    // ops
    bootstrap,
    addFunction,
    deleteFunction,
    renameFunction,
    updateFunctionSignature,
    setActiveFunction,
    updateWorkflowMeta,
    addNode,
    addNodeAt,
    updateNodePosition,
    updateNodeData,
    updateNodeExpression,
    duplicateNode,
    duplicateNodes,
    copyNodes,
    pasteAt,
    hasClipboard,
    insertBlock,
    snapshotSelection,
    getScopeBindings,
    version,
    removeNode,
    addEdge,
    removeEdge,
    addStruct,
    updateStruct,
    deleteStruct,
    addEnum,
    updateEnum,
    deleteEnum,
    addImport,
    updateImport,
    deleteImport,
    loadWorkflow,
    newWorkflow,
    undo,
    redo,
    canUndo,
    canRedo,
  };
});
