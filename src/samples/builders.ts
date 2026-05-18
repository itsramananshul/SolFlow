/**
 * Sample-workflow builder helpers.
 *
 * Each sample lives in its own file as a pure function returning a
 * SolWorkflow object. They're loaded through the SAME `loadWorkflow` path
 * the user's file uploads go through — they are data, not hardcoded UI.
 *
 * Builders use `createNode` + `rebuildPorts` so the resulting graph is
 * structurally identical to one a user would construct interactively.
 */

import { nanoid } from 'nanoid';

import { createNode, rebuildPorts, type WorkflowCtx } from '@/graph/factory';
import type {
  EnumDecl,
  EnumVariant,
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
  StructField,
} from '@/graph/schema';

export interface BuilderState {
  workflow: SolWorkflow;
  ctx: WorkflowCtx;
  // Per-function quick lookup for "current function being built"
  activeFnId: string;
}

function nowIso(): string {
  return new Date(2026, 0, 1).toISOString();
}

export function emptyWorkflow(name: string): SolWorkflow {
  return {
    schemaVersion: 1,
    meta: { name, createdAt: nowIso(), updatedAt: nowIso() },
    imports: [],
    structs: [],
    enums: [],
    functions: [],
  };
}

export function createBuilder(name: string): BuilderState {
  const wf = emptyWorkflow(name);
  return {
    workflow: wf,
    ctx: { structs: wf.structs, enums: wf.enums, functions: wf.functions },
    activeFnId: '',
  };
}

export function addImport(b: BuilderState, path: string[], alias: string): ImportDecl {
  const imp: ImportDecl = { id: nanoid(8), path, alias };
  b.workflow.imports.push(imp);
  return imp;
}

export function addStruct(
  b: BuilderState,
  name: string,
  fields: StructField[],
): StructDecl {
  const s: StructDecl = { id: nanoid(8), name, fields };
  b.workflow.structs.push(s);
  return s;
}

export function addEnum(
  b: BuilderState,
  name: string,
  variants: EnumVariant[],
): EnumDecl {
  const e: EnumDecl = { id: nanoid(8), name, variants };
  b.workflow.enums.push(e);
  return e;
}

export function addFunction(
  b: BuilderState,
  name: string,
  params: Param[] = [],
  returnType: SolType = { kind: 'void' },
): FunctionGraph {
  const startNode: GraphNode = {
    id: nanoid(8),
    data: { kind: 'start' },
    position: { x: 80, y: 60 },
    ports: { in: [], out: [{ id: 'next', name: 'next', kind: 'control', required: true }] },
  };
  const fn: FunctionGraph = {
    id: nanoid(8),
    name,
    params,
    returnType,
    nodes: [startNode],
    edges: [],
  };
  b.workflow.functions.push(fn);
  b.activeFnId = fn.id;
  return fn;
}

export function getFn(b: BuilderState): FunctionGraph {
  const fn = b.workflow.functions.find((f) => f.id === b.activeFnId);
  if (!fn) throw new Error('no active function');
  return fn;
}

export function getStart(b: BuilderState): GraphNode {
  const fn = getFn(b);
  const s = fn.nodes.find((n) => n.data.kind === 'start');
  if (!s) throw new Error('start node missing');
  return s;
}

export function setActiveFn(b: BuilderState, id: string) {
  b.activeFnId = id;
}

export function node(
  b: BuilderState,
  kind: NodeKind,
  position: { x: number; y: number },
  data?: Partial<NodeData>,
): GraphNode {
  const fn = getFn(b);
  // `data` for a discriminated union via Partial<NodeData> isn't trivially
  // mergeable — we cast through `unknown` so each call site supplies a
  // compatible kind-specific patch.
  const n = createNode(kind, position, b.ctx, data as Partial<NodeData> & { kind: NodeKind } | undefined);
  fn.nodes.push(n);
  return n;
}

export function ctl(b: BuilderState, from: GraphNode, fromPort: string, to: GraphNode, toPort: string): GraphEdge {
  const fn = getFn(b);
  const edge: GraphEdge = {
    id: nanoid(8),
    source: { node: from.id, port: fromPort },
    target: { node: to.id, port: toPort },
    kind: 'control',
  };
  fn.edges.push(edge);
  return edge;
}

export function dat(b: BuilderState, from: GraphNode, fromPort: string, to: GraphNode, toPort: string): GraphEdge {
  const fn = getFn(b);
  const edge: GraphEdge = {
    id: nanoid(8),
    source: { node: from.id, port: fromPort },
    target: { node: to.id, port: toPort },
    kind: 'data',
  };
  fn.edges.push(edge);
  return edge;
}

/** Re-derive ports for all nodes in all functions (call once after construction). */
export function finalize(b: BuilderState): SolWorkflow {
  for (const fn of b.workflow.functions) {
    for (const node of fn.nodes) {
      node.ports = rebuildPorts(node.data, b.ctx);
    }
  }
  return b.workflow;
}
