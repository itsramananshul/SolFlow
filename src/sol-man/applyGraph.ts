/**
 * Translate a validated GeneratedGraphSpec into real SolFlow
 * GraphNodes + GraphEdges. Uses the same `createNode` factory the
 * user-facing editor uses, so Sol Man output is structurally
 * indistinguishable from hand-built workflows.
 *
 * Two flavors:
 *   - specToWorkflow(spec)         : produces a brand-new SolWorkflow
 *                                    containing one function `start`.
 *                                    Used by "Apply as new workflow."
 *   - specToInsertSnapshot(spec, fnCtx) : produces a SnapshotPayload
 *                                    (nodes/edges/centroid) the graph
 *                                    store's insertBlock pipeline can
 *                                    drop into the active function
 *                                    at the cursor. Used by "Insert
 *                                    into current function."
 *
 * Frames + notes from the spec render as Frame / Note nodes the user
 * can resize, rename, or delete.
 */

import { nanoid } from 'nanoid';

import { createNode, rebuildPorts, type WorkflowCtx } from '@/graph/factory';
import type {
  FunctionGraph,
  GraphEdge,
  GraphNode,
  NodeData,
  NodeKind,
  SolWorkflow,
} from '@/graph/schema';

import { autoLayout } from './autoLayout';
import type {
  GeneratedGraphSpec,
  GeneratedNode,
  GeneratedPrimitive,
} from './types';

const NODE_FOOTPRINT_W = 220;
const NODE_FOOTPRINT_H = 60;
const FRAME_PAD = 32;

const EMPTY_CTX: WorkflowCtx = { structs: [], enums: [], functions: [] };

/** Compute the GraphNode `data` payload for an LLM node + collect any
 *  inline expressions that should land in node.expressions. */
function dataFor(
  g: GeneratedNode,
  ctx: WorkflowCtx,
): { data: NodeData; inlinePort?: string; inlineExpr?: string } {
  const t = (p: GeneratedPrimitive | undefined): { kind: GeneratedPrimitive } => ({
    kind: p ?? 'int',
  });
  switch (g.kind) {
    case 'trigger':
      return {
        data: {
          kind: 'trigger',
          triggerKind: g.triggerKind ?? 'manual',
          eventName: g.eventName ?? 'event',
          payloadSchema: '{ "type": "object" }',
          samplePayload: g.samplePayload ?? '{}',
          webhookPath: g.webhookPath,
          cronExpr: g.cronExpr,
          httpMethod: g.httpMethod,
          httpPath: g.httpPath,
        },
      };
    case 'let':
      return {
        data: {
          kind: 'let',
          varName: g.varName ?? 'value',
          varType: t(g.varType),
        },
        inlinePort: 'value',
        inlineExpr: g.value,
      };
    case 'assign':
      return {
        data: { kind: 'assign', varName: g.varName ?? '' },
        inlinePort: 'value',
        inlineExpr: g.value,
      };
    case 'print':
      return {
        data: { kind: 'print' },
        inlinePort: 'value',
        inlineExpr: g.value,
      };
    case 'return': {
      const hasValue = g.hasValue ?? !!g.value;
      return {
        data: { kind: 'return', hasValue },
        inlinePort: hasValue ? 'value' : undefined,
        inlineExpr: hasValue ? g.value : undefined,
      };
    }
    case 'branch':
      return {
        data: { kind: 'branch', hasElse: g.hasElse ?? true },
        inlinePort: 'cond',
        inlineExpr: g.cond,
      };
    case 'while':
      return {
        data: { kind: 'while' },
        inlinePort: 'cond',
        inlineExpr: g.cond,
      };
    case 'forEach':
      return {
        data: {
          kind: 'forEach',
          iteratorName: g.iteratorName ?? 'item',
          iteratorType: t(g.iteratorType),
        },
        inlinePort: 'array',
        inlineExpr: g.value,
      };
    case 'call': {
      // Try to resolve callTarget against existing workflow functions;
      // if not found we leave functionId empty and surface as a warning
      // in the consumer (sol-man.store) via the assumptions list.
      const fn = ctx.functions.find((f) => f.name === (g.callTarget ?? ''));
      return { data: { kind: 'call', functionId: fn?.id ?? '' } };
    }
  }
}

/** Build real GraphNodes + GraphEdges from the LLM spec. Positions
 *  are in LAYOUT space (entry at 0,0); add an origin offset before
 *  use. Returns an id-map for any downstream consumer. */
function translateSpec(
  spec: GeneratedGraphSpec,
  ctx: WorkflowCtx,
): {
  nodes: GraphNode[];
  edges: GraphEdge[];
  idMap: Map<string, string>;
  warnings: string[];
} {
  const layout = autoLayout(spec);
  const idMap = new Map<string, string>();
  const nodes: GraphNode[] = [];
  const warnings: string[] = [];

  // 1. Translate every LLM node into a real GraphNode.
  for (const g of spec.nodes) {
    const { data, inlinePort, inlineExpr } = dataFor(g, ctx);
    const pos = layout.get(g.id) ?? { x: 0, y: 0 };
    const node = createNode(g.kind as NodeKind, pos, ctx, data as Partial<NodeData>);
    if (inlinePort && inlineExpr && inlineExpr.trim() !== '') {
      node.expressions = { ...(node.expressions ?? {}), [inlinePort]: inlineExpr };
    }
    if (g.kind === 'call' && data.kind === 'call' && !data.functionId) {
      warnings.push(
        `Call node references "${g.callTarget ?? '(unset)'}" — no matching function exists yet. Create it, then pick it from the Call node's Inspector.`,
      );
    }
    idMap.set(g.id, node.id);
    nodes.push(node);
  }

  // 2. Translate edges. Drop edges whose source/target port doesn't
  //    exist on the resolved node — happens when the LLM specifies
  //    e.g. branch fromPort='else' on a branch with hasElse:false.
  const nodeById = new Map(nodes.map((n) => [n.id, n]));
  const edges: GraphEdge[] = [];
  for (const e of spec.edges) {
    const sourceRealId = idMap.get(e.from);
    const targetRealId = idMap.get(e.to);
    if (!sourceRealId || !targetRealId) continue;
    const src = nodeById.get(sourceRealId);
    const tgt = nodeById.get(targetRealId);
    if (!src || !tgt) continue;
    const fromPort = e.fromPort ?? 'next';
    const toPort = e.toPort ?? 'prev';
    const kind = e.kind ?? 'control';
    const srcPort = src.ports.out.find((p) => p.id === fromPort);
    const tgtPort = tgt.ports.in.find((p) => p.id === toPort);
    if (!srcPort) {
      warnings.push(
        `Edge from "${e.from}" → "${e.to}" referenced port "${fromPort}" which doesn't exist on the source node; dropped.`,
      );
      continue;
    }
    if (!tgtPort) {
      warnings.push(
        `Edge from "${e.from}" → "${e.to}" referenced port "${toPort}" which doesn't exist on the target node; dropped.`,
      );
      continue;
    }
    edges.push({
      id: nanoid(8),
      source: { node: src.id, port: srcPort.id },
      target: { node: tgt.id, port: tgtPort.id },
      kind,
    });
  }

  // 3. Frames — wrap declared groups in a Frame node sized to the
  //    bounding box of contained nodes plus padding.
  if (spec.frames) {
    for (const f of spec.frames) {
      const containedIds = f.nodeIds
        .map((llmId) => idMap.get(llmId))
        .filter((x): x is string => !!x);
      const contained = nodes.filter((n) => containedIds.includes(n.id));
      if (contained.length === 0) continue;
      const xs = contained.map((n) => n.position.x);
      const ys = contained.map((n) => n.position.y);
      const minX = Math.min(...xs);
      const minY = Math.min(...ys);
      const maxX = Math.max(...xs) + NODE_FOOTPRINT_W;
      const maxY = Math.max(...ys) + NODE_FOOTPRINT_H * 1.5;
      const frame = createNode(
        'frame',
        { x: minX - FRAME_PAD, y: minY - FRAME_PAD - 16 },
        ctx,
        {
          kind: 'frame',
          title: f.title,
          width: Math.max(220, Math.round(maxX - minX + FRAME_PAD * 2)),
          height: Math.max(160, Math.round(maxY - minY + FRAME_PAD * 2 + 16)),
        } as Partial<NodeData>,
      );
      nodes.push(frame);
    }
  }

  // 4. Notes — drop them to the right of the layout, stacked.
  if (spec.notes) {
    const allXs = nodes.map((n) => n.position.x);
    const rightEdge = allXs.length > 0 ? Math.max(...allXs) + NODE_FOOTPRINT_W + 60 : 400;
    let stackY = 0;
    for (const n of spec.notes) {
      const note = createNode(
        'note',
        { x: rightEdge, y: stackY },
        ctx,
        { kind: 'note', text: n.text } as Partial<NodeData>,
      );
      nodes.push(note);
      stackY += 110;
    }
  }

  return { nodes, edges, idMap, warnings };
}

/**
 * Apply-as-new path: returns a brand-new SolWorkflow with one
 * function `start` populated from the spec. Replaces the user's
 * current workflow when handed to graph.loadWorkflow().
 */
export function specToWorkflow(spec: GeneratedGraphSpec): {
  workflow: SolWorkflow;
  warnings: string[];
} {
  const { nodes, edges, warnings } = translateSpec(spec, EMPTY_CTX);
  // Shift everything so the entry sits at (200, 100) instead of the
  // origin — gives the new workflow some breathing room from the
  // canvas top-left.
  const entry = nodes.find((n) => n.data.kind === 'trigger') ?? nodes[0];
  if (entry) {
    const dx = 200 - entry.position.x;
    const dy = 100 - entry.position.y;
    for (const n of nodes) {
      n.position = { x: n.position.x + dx, y: n.position.y + dy };
    }
  }
  const fn: FunctionGraph = {
    id: nanoid(8),
    name: 'start',
    params: [],
    returnType: { kind: 'void' },
    nodes,
    edges,
  };
  // Rebuild ports against the WorkflowCtx (now that this fn is part
  // of the workflow) so call-nodes' arg ports populate correctly.
  const workflow: SolWorkflow = {
    schemaVersion: 1,
    meta: {
      name: spec.meta.name,
      description: spec.meta.description,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    imports: [],
    structs: [],
    enums: [],
    functions: [fn],
  };
  const fullCtx: WorkflowCtx = {
    structs: workflow.structs,
    enums: workflow.enums,
    functions: workflow.functions,
  };
  for (const n of fn.nodes) {
    n.ports = rebuildPorts(n.data, fullCtx);
  }
  return { workflow, warnings };
}

/**
 * Insert-into-current path: returns a snapshot the graph store can
 * paste at flowPos. The active function's WorkflowCtx is passed in
 * so call-nodes resolve against existing functions if they match.
 */
export function specToInsertSnapshot(
  spec: GeneratedGraphSpec,
  ctx: WorkflowCtx,
  flowPos: { x: number; y: number },
): {
  snapshot: {
    name: string;
    nodes: GraphNode[];
    edges: GraphEdge[];
    centroid: { x: number; y: number };
  };
  warnings: string[];
} {
  const { nodes, edges, warnings } = translateSpec(spec, ctx);
  // Centroid in layout space → caller picks where it goes via flowPos
  // through insertBlock; we keep layout-relative positions.
  const cx = nodes.length > 0
    ? nodes.reduce((s, n) => s + n.position.x, 0) / nodes.length
    : 0;
  const cy = nodes.length > 0
    ? nodes.reduce((s, n) => s + n.position.y, 0) / nodes.length
    : 0;
  return {
    snapshot: {
      name: spec.meta.name,
      nodes,
      edges,
      centroid: { x: cx, y: cy },
    },
    warnings,
  };
}
