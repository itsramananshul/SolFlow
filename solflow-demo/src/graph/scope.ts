/**
 * SolFlow Phase A — scope walking.
 *
 * Given a function graph and a node that needs to reference a variable
 * (varGet, assign, fieldSet), return the set of `let`-declared variables
 * reachable along control-flow edges from any incoming control edge
 * back to the function entry.
 *
 * Phase A: a conservative approximation. We walk backwards along control
 * edges through `let` nodes, function parameters, and forEach iterator
 * outputs. Branches/loops aren't subjected to per-arm scope isolation
 * here — that's a Phase B refinement.
 */

import type { FunctionGraph, GraphEdge, GraphNode, SolType } from './schema';

export interface ScopeBinding {
  name: string;
  type: SolType;
  source: 'param' | 'let' | 'forEach-iter';
  nodeId?: string;
}

export function bindingsInScope(
  fn: FunctionGraph,
  forNodeId: string,
): ScopeBinding[] {
  const bindings: ScopeBinding[] = [];

  // Function parameters are always in scope.
  for (const p of fn.params) {
    bindings.push({ name: p.name, type: p.type, source: 'param' });
  }

  // Build an inverse edge index — for each node, who points at its control-in.
  const incoming: Record<string, GraphEdge[]> = {};
  for (const e of fn.edges) {
    if (e.kind !== 'control') continue;
    incoming[e.target.node] = incoming[e.target.node] ?? [];
    incoming[e.target.node].push(e);
  }

  // BFS backwards from this node, collecting let/forEach declarations seen along the way.
  const visited = new Set<string>();
  const queue: string[] = [forNodeId];

  const nodeMap: Record<string, GraphNode> = {};
  for (const n of fn.nodes) nodeMap[n.id] = n;

  while (queue.length > 0) {
    const id = queue.shift()!;
    if (visited.has(id)) continue;
    visited.add(id);

    const n = nodeMap[id];
    if (!n) continue;

    // If THIS node declares something (and it's not the originating node), include it.
    if (n.id !== forNodeId) {
      if (n.data.kind === 'let') {
        bindings.push({
          name: n.data.varName,
          type: n.data.varType,
          source: 'let',
          nodeId: n.id,
        });
      }
      if (n.data.kind === 'forEach') {
        bindings.push({
          name: n.data.iteratorName,
          type: n.data.iteratorType,
          source: 'forEach-iter',
          nodeId: n.id,
        });
      }
    }

    const ins = incoming[id] ?? [];
    for (const e of ins) {
      queue.push(e.source.node);
    }
  }

  // Dedupe by name — innermost wins (closest to forNodeId, which we visited first).
  const seen = new Set<string>();
  const out: ScopeBinding[] = [];
  for (const b of bindings) {
    if (seen.has(b.name)) continue;
    seen.add(b.name);
    out.push(b);
  }

  return out;
}
