/**
 * Reusable-block schema + built-in patterns.
 *
 * A "block" is a packaged snapshot of a node selection — the same
 * shape the internal clipboard uses, plus a name/description and an
 * origin tag. Two kinds:
 *
 *   user    — produced by "Save selection as block" and persisted to
 *             localStorage via blocks.store.ts.
 *   builtin — small reusable patterns shipped in this file as factory
 *             functions. Fresh ids on every build() so multiple
 *             insertions don't collide.
 *
 * Insertion is handled by graph.insertBlock(block, flowPos) which
 * routes through the same _insertSnapshot helper as paste / multi-
 * select duplicate. Single code path, no special-casing.
 */

import { nanoid } from 'nanoid';

import { createNode, type WorkflowCtx } from './factory';
import type { GraphEdge, GraphNode, NodeData } from './schema';

export interface SavedBlock {
  id: string;
  name: string;
  description: string;
  origin: 'user' | 'builtin';
  /** Stable identifier for built-ins; absent on user blocks. */
  patternId?: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
  centroid: { x: number; y: number };
  createdAt: string;
}

/** Empty ctx used when building built-in patterns — they reference
 *  only primitive types so don't need user struct/enum/function refs. */
const EMPTY_CTX: WorkflowCtx = { structs: [], enums: [], functions: [] };

function ctl(from: GraphNode, fromPort: string, to: GraphNode, toPort: string): GraphEdge {
  return {
    id: nanoid(8),
    source: { node: from.id, port: fromPort },
    target: { node: to.id, port: toPort },
    kind: 'control',
  };
}

function setInline(n: GraphNode, portId: string, text: string) {
  n.expressions = { ...(n.expressions ?? {}), [portId]: text };
}

/** Compute centroid of an array of nodes — used for paste-at math. */
function centroidOf(nodes: GraphNode[]): { x: number; y: number } {
  if (nodes.length === 0) return { x: 0, y: 0 };
  const x = nodes.reduce((s, n) => s + n.position.x, 0) / nodes.length;
  const y = nodes.reduce((s, n) => s + n.position.y, 0) / nodes.length;
  return { x, y };
}

// =============================================================
//  Built-in patterns
// =============================================================

interface BuiltinPatternDef {
  patternId: string;
  name: string;
  description: string;
  build(): SavedBlock;
}

const PATTERNS: BuiltinPatternDef[] = [
  {
    patternId: 'retry-counter',
    name: 'Retry counter',
    description: 'Try up to 3 times, incrementing the counter on each pass.',
    build(): SavedBlock {
      const letAttempts = createNode(
        'let',
        { x: 0, y: 0 },
        EMPTY_CTX,
        { kind: 'let', varName: 'attempts', varType: { kind: 'int' } } as Partial<NodeData>,
      );
      setInline(letAttempts, 'value', '0');

      const whileNode = createNode('while', { x: 0, y: 120 }, EMPTY_CTX);
      setInline(whileNode, 'cond', 'attempts < 3');

      const assignNode = createNode(
        'assign',
        { x: 280, y: 120 },
        EMPTY_CTX,
        { kind: 'assign', varName: 'attempts' } as Partial<NodeData>,
      );
      setInline(assignNode, 'value', 'attempts + 1');

      const nodes = [letAttempts, whileNode, assignNode];
      const edges = [
        ctl(letAttempts, 'next', whileNode, 'prev'),
        ctl(whileNode, 'body', assignNode, 'prev'),
      ];
      return {
        id: nanoid(8),
        name: 'Retry counter',
        description: 'Try up to 3 times, incrementing the counter on each pass.',
        origin: 'builtin',
        patternId: 'retry-counter',
        nodes,
        edges,
        centroid: centroidOf(nodes),
        createdAt: new Date().toISOString(),
      };
    },
  },

  {
    patternId: 'validation-gate',
    name: 'Validation gate',
    description:
      'If a condition fails, return early. Otherwise execution continues.',
    build(): SavedBlock {
      const branch = createNode(
        'branch',
        { x: 0, y: 0 },
        EMPTY_CTX,
        { kind: 'branch', hasElse: true } as Partial<NodeData>,
      );
      setInline(branch, 'cond', '/* your condition */');

      const print = createNode('print', { x: -180, y: 140 }, EMPTY_CTX);
      setInline(print, 'value', '"validation failed"');

      const ret = createNode(
        'return',
        { x: -180, y: 260 },
        EMPTY_CTX,
        { kind: 'return', hasValue: false } as Partial<NodeData>,
      );

      const nodes = [branch, print, ret];
      const edges = [
        ctl(branch, 'else', print, 'prev'),
        ctl(print, 'next', ret, 'prev'),
      ];
      return {
        id: nanoid(8),
        name: 'Validation gate',
        description:
          'If a condition fails, return early. Otherwise execution continues.',
        origin: 'builtin',
        patternId: 'validation-gate',
        nodes,
        edges,
        centroid: centroidOf(nodes),
        createdAt: new Date().toISOString(),
      };
    },
  },

  {
    patternId: 'logging-wrapper',
    name: 'Logging wrapper',
    description: 'Print "starting" before your work, "done" after.',
    build(): SavedBlock {
      const printStart = createNode('print', { x: 0, y: 0 }, EMPTY_CTX);
      setInline(printStart, 'value', '"starting"');
      const printEnd = createNode('print', { x: 0, y: 160 }, EMPTY_CTX);
      setInline(printEnd, 'value', '"done"');

      const nodes = [printStart, printEnd];
      // No internal control edge — the user wires their work between
      // these two by connecting printStart.next → ... → printEnd.prev.
      return {
        id: nanoid(8),
        name: 'Logging wrapper',
        description: 'Print "starting" before your work, "done" after.',
        origin: 'builtin',
        patternId: 'logging-wrapper',
        nodes,
        edges: [],
        centroid: centroidOf(nodes),
        createdAt: new Date().toISOString(),
      };
    },
  },

  {
    patternId: 'payload-extract',
    name: 'Extract from payload',
    description:
      'Pull a value out of the trigger payload into a named variable.',
    build(): SavedBlock {
      const letPick = createNode(
        'let',
        { x: 0, y: 0 },
        EMPTY_CTX,
        { kind: 'let', varName: 'value', varType: { kind: 'any' } } as Partial<NodeData>,
      );
      setInline(letPick, 'value', 'payload.value');
      return {
        id: nanoid(8),
        name: 'Extract from payload',
        description:
          'Pull a value out of the trigger payload into a named variable.',
        origin: 'builtin',
        patternId: 'payload-extract',
        nodes: [letPick],
        edges: [],
        centroid: centroidOf([letPick]),
        createdAt: new Date().toISOString(),
      };
    },
  },
];

export function listBuiltinPatterns(): BuiltinPatternDef[] {
  return PATTERNS;
}

export function buildBuiltinPattern(patternId: string): SavedBlock | null {
  const p = PATTERNS.find((x) => x.patternId === patternId);
  return p ? p.build() : null;
}
