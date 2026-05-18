/**
 * Auto-layout for Sol Man-generated graphs.
 *
 * Walks the control flow from the entry (trigger or first node) and
 * places nodes in a top-to-bottom chain. Multi-out nodes (branch,
 * while, forEach) spread their children horizontally so both arms
 * stay visible without overlap.
 *
 * Returns a Map<llmNodeId, {x,y}> in LAYOUT space (origin (0,0) is
 * the entry). Callers add a flow-space origin offset before writing
 * positions onto the real GraphNodes.
 *
 * Phase A: deliberately simple. No edge-routing optimization, no
 * compaction, no symmetry. Good enough to render a clean fresh graph
 * the user can rearrange via the existing alignment tools.
 */

import type { GeneratedEdge, GeneratedGraphSpec } from './types';

const ROW_HEIGHT = 140;
const COL_WIDTH = 280;

export function autoLayout(spec: GeneratedGraphSpec): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>();

  // Outgoing adjacency, control edges only — data edges don't structure
  // the visual chain.
  const outs = new Map<string, GeneratedEdge[]>();
  for (const e of spec.edges) {
    if (e.kind && e.kind !== 'control') continue;
    if (!outs.has(e.from)) outs.set(e.from, []);
    outs.get(e.from)!.push(e);
  }

  const entry =
    spec.nodes.find((n) => n.kind === 'trigger') ?? spec.nodes[0];
  if (!entry) return positions;

  // BFS from the entry. We tolerate cycles (forEach's body→back loops)
  // by skipping already-visited nodes.
  const visited = new Set<string>();
  const queue: { id: string; x: number; y: number }[] = [
    { id: entry.id, x: 0, y: 0 },
  ];

  while (queue.length > 0) {
    const { id, x, y } = queue.shift()!;
    if (visited.has(id)) continue;
    visited.add(id);
    positions.set(id, { x, y });

    const outEdges = outs.get(id) ?? [];
    if (outEdges.length === 1) {
      queue.push({ id: outEdges[0].to, x, y: y + ROW_HEIGHT });
    } else if (outEdges.length > 1) {
      // Branches / loops: spread children horizontally so neither arm
      // collides with the other. Center the spread under the parent.
      const totalWidth = (outEdges.length - 1) * COL_WIDTH;
      const startX = x - totalWidth / 2;
      outEdges.forEach((e, i) => {
        queue.push({
          id: e.to,
          x: startX + i * COL_WIDTH,
          y: y + ROW_HEIGHT,
        });
      });
    }
  }

  // Orphans (no path from entry) — park them off to the right so they
  // stay visible and the user can rewire them manually.
  let orphanY = 0;
  for (const n of spec.nodes) {
    if (!positions.has(n.id)) {
      positions.set(n.id, { x: COL_WIDTH * 4, y: orphanY });
      orphanY += ROW_HEIGHT;
    }
  }

  return positions;
}
