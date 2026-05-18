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

  function addNode(kind: NodeKind, position: { x: number; y: number }) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = createNode(kind, position, ctx.value);
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

  function updateNodeData(nodeId: string, patch: Partial<NodeData>) {
    const fn = activeFunction.value;
    if (!fn) return;
    const node = fn.nodes.find((n) => n.id === nodeId);
    if (!node) return;
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
    touch();
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
    if (node.data.kind === 'start') return; // start is non-deletable
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

  function touch() {
    workflow.value.meta.updatedAt = nowIso();
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
  watch(
    () => workflow.value,
    () => {
      if (isReplaying) return;
      if (saveTimer !== undefined) window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => {
        try {
          localStorage.setItem(STORAGE_KEY, JSON.stringify(workflow.value));
        } catch {
          /* quota or unavailable — ignore */
        }
      }, 600);

      if (historyTimer !== undefined) window.clearTimeout(historyTimer);
      historyTimer = window.setTimeout(() => {
        pushHistory();
      }, 220);
    },
    { deep: true },
  );

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
    addNode,
    updateNodePosition,
    updateNodeData,
    updateNodeExpression,
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
