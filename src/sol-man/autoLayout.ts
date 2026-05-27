/**
 * Auto-layout for Sol Man-generated graphs.
 *
 * v2 — subtree-width-aware. Walks the control flow from the entry,
 * computes each subtree's horizontal span recursively, then places
 * nodes so that branches with deep / wide "then" arms don't collide
 * with their "else" siblings.
 *
 * The previous BFS placement assumed every subtree was exactly
 * `COL_WIDTH` wide, which produced collisions whenever a branch arm
 * had children that themselves branched. The new algorithm:
 *
 *   1. Walk the spec once to compute each node's `subtreeWidth` —
 *      the maximum horizontal extent its descendants occupy, in
 *      column units. A leaf is 1; a node with multiple children is
 *      the sum of its children's widths (or 1, whichever is larger).
 *   2. Place each node at a y proportional to its depth. For multi-
 *      out nodes (branch / while / forEach), allocate horizontal
 *      space per child equal to that child's subtree width and
 *      center the spread under the parent.
 *
 * Single-out nodes still walk straight down. Reconvergence at the
 * `after` port is treated as a separate subtree of the branch/loop —
 * so the next-after-branch chain doesn't get squeezed between the
 * spread arms.
 *
 * Returns a Map<llmNodeId, {x, y}> in LAYOUT space (origin (0,0) is
 * the entry). Callers add a flow-space origin offset before writing
 * positions onto the real GraphNodes.
 */

import type { GeneratedEdge, GeneratedGraphSpec } from './types';

const ROW_HEIGHT = 140;
const COL_WIDTH = 280;

interface OutsByPort {
  [fromPort: string]: GeneratedEdge[];
}

export function autoLayout(spec: GeneratedGraphSpec): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>();
  if (spec.nodes.length === 0) return positions;

  // Index outgoing control edges by source node + port. Data edges
  // don't structure the visual chain — they wire values between
  // existing layout positions.
  const outsByNode = new Map<string, OutsByPort>();
  for (const e of spec.edges) {
    if (e.kind && e.kind !== 'control') continue;
    const fromPort = e.fromPort ?? 'next';
    const byPort = outsByNode.get(e.from) ?? {};
    if (!byPort[fromPort]) byPort[fromPort] = [];
    byPort[fromPort].push(e);
    outsByNode.set(e.from, byPort);
  }

  const entry = spec.nodes.find((n) => n.kind === 'trigger') ?? spec.nodes[0];
  if (!entry) return positions;

  // -----------------------------------------------------------
  //  Pass 1 — compute subtree widths in column units.
  // -----------------------------------------------------------
  // A node's width is determined by its successors:
  //   - leaf (no outgoing control)               → 1
  //   - single successor                         → successor's width
  //   - branch (then/else, optional after)       → max(sum of arm widths, after width)
  //   - while/forEach (body, optional after)     → max(body width, after width)
  //
  // We tolerate cycles (forEach body loops back) by memoizing and
  // returning 1 when a node is re-entered mid-walk.

  const widthCache = new Map<string, number>();
  const visiting = new Set<string>();

  function subtreeWidth(nodeId: string): number {
    const cached = widthCache.get(nodeId);
    if (cached !== undefined) return cached;
    if (visiting.has(nodeId)) return 1; // cycle guard
    visiting.add(nodeId);

    const node = spec.nodes.find((n) => n.id === nodeId);
    if (!node) {
      visiting.delete(nodeId);
      widthCache.set(nodeId, 1);
      return 1;
    }

    const outs = outsByNode.get(nodeId) ?? {};
    let result: number;

    if (node.kind === 'branch') {
      // then + else stack horizontally; after sits below the join,
      // not as a parallel arm — so its width competes with the sum
      // rather than adding to it.
      const thenW = sumChildren(outs.then);
      const elseW = sumChildren(outs.else);
      const afterW = sumChildren(outs.after);
      const armSum = Math.max(thenW + elseW, 1);
      result = Math.max(armSum, afterW || 1);
    } else if (node.kind === 'while' || node.kind === 'forEach') {
      const bodyW = sumChildren(outs.body);
      const afterW = sumChildren(outs.after);
      // body loops back to the loop node, so its measured width is
      // its own subtree's. after is downstream.
      result = Math.max(bodyW || 1, afterW || 1);
    } else {
      // Single-out (or no-out) nodes follow `next`.
      const nextW = sumChildren(outs.next);
      result = nextW || 1;
    }

    visiting.delete(nodeId);
    widthCache.set(nodeId, result);
    return result;
  }

  function sumChildren(edges: GeneratedEdge[] | undefined): number {
    if (!edges || edges.length === 0) return 0;
    let total = 0;
    for (const e of edges) total += subtreeWidth(e.to);
    return total;
  }

  // Precompute widths for every node reachable from the entry. (This
  // populates widthCache for the placement pass.)
  subtreeWidth(entry.id);

  // -----------------------------------------------------------
  //  Pass 2 — place nodes given the computed widths.
  // -----------------------------------------------------------
  // The placement walk centers each node at the midpoint of its
  // allocated horizontal slot. For multi-out nodes, children get
  // a slot equal to their subtree width × COL_WIDTH; for single-out
  // nodes the child inherits the parent's slot.
  //
  // `placedSet` prevents re-placement when control-flow cycles back
  // (forEach body looping to the loop entry).

  const placedSet = new Set<string>();

  /**
   * Place `nodeId` at the center of a slot of width `slotW` (in
   * column units) whose left edge sits at `slotLeft` (in flow-space
   * pixels). Returns immediately if the node was already placed
   * (cycle guard).
   */
  function place(nodeId: string, slotLeft: number, slotW: number, depth: number): void {
    if (placedSet.has(nodeId)) return;
    placedSet.add(nodeId);

    const cx = slotLeft + (slotW * COL_WIDTH) / 2;
    // Position is the node's top-left, not its center. The node
    // footprint estimate matches the renderer's approximate
    // 220×60 box (see SolNode.vue defaults).
    positions.set(nodeId, {
      x: Math.round(cx - 110),
      y: depth * ROW_HEIGHT,
    });

    const node = spec.nodes.find((n) => n.id === nodeId);
    if (!node) return;

    const outs = outsByNode.get(nodeId) ?? {};

    if (node.kind === 'branch') {
      // then + else stack side-by-side beneath the branch. after
      // (if present) lands BELOW both arms — at the depth of the
      // longest arm + 1 — and inherits the branch's center column.
      const thenEdges = outs.then ?? [];
      const elseEdges = outs.else ?? [];
      const afterEdges = outs.after ?? [];
      const thenW = thenEdges.reduce((s, e) => s + (widthCache.get(e.to) ?? 1), 0) || 1;
      const elseW = elseEdges.reduce((s, e) => s + (widthCache.get(e.to) ?? 1), 0) || 1;
      const armsTotalW = thenW + elseW;
      // Center the arms under the branch.
      const armsLeft = slotLeft + ((slotW - armsTotalW) * COL_WIDTH) / 2;
      // Place then-arm children.
      let cursor = armsLeft;
      for (const e of thenEdges) {
        const w = widthCache.get(e.to) ?? 1;
        place(e.to, cursor, w, depth + 1);
        cursor += w * COL_WIDTH;
      }
      // Place else-arm children.
      for (const e of elseEdges) {
        const w = widthCache.get(e.to) ?? 1;
        place(e.to, cursor, w, depth + 1);
        cursor += w * COL_WIDTH;
      }
      // After branch — depth shifts to clear both arms.
      const armDepth = depth + 1 + maxChainDepth(thenEdges.concat(elseEdges));
      for (const e of afterEdges) {
        const w = widthCache.get(e.to) ?? 1;
        const cw = w;
        const left = slotLeft + ((slotW - cw) * COL_WIDTH) / 2;
        place(e.to, left, cw, armDepth);
      }
      return;
    }

    if (node.kind === 'while' || node.kind === 'forEach') {
      const bodyEdges = outs.body ?? [];
      const afterEdges = outs.after ?? [];
      // body sits directly below the loop node (cycle is fine —
      // placedSet prevents re-placement when body loops back).
      for (const e of bodyEdges) {
        const w = widthCache.get(e.to) ?? 1;
        const left = slotLeft + ((slotW - w) * COL_WIDTH) / 2;
        place(e.to, left, w, depth + 1);
      }
      // after lands below the body chain.
      const afterDepth = depth + 1 + maxChainDepth(bodyEdges);
      for (const e of afterEdges) {
        const w = widthCache.get(e.to) ?? 1;
        const left = slotLeft + ((slotW - w) * COL_WIDTH) / 2;
        place(e.to, left, w, afterDepth);
      }
      return;
    }

    // Single-out (or no-out). next child inherits the parent's slot.
    const nextEdges = outs.next ?? [];
    for (const e of nextEdges) {
      const w = widthCache.get(e.to) ?? 1;
      // Single-out children walk straight down at the parent's
      // column center, not spread.
      const cw = w;
      const left = slotLeft + ((slotW - cw) * COL_WIDTH) / 2;
      place(e.to, left, cw, depth + 1);
    }
  }

  /**
   * Approximate the deepest control-chain length reachable from any
   * of the given edges. Used to push "after" subtrees below their
   * branch/loop arms instead of letting them overlap.
   *
   * `visited` carries through recursion to break cycles cheaply.
   */
  function maxChainDepth(edges: GeneratedEdge[], visited = new Set<string>()): number {
    if (edges.length === 0) return 0;
    let best = 0;
    for (const e of edges) {
      if (visited.has(e.to)) continue;
      visited.add(e.to);
      const outs = outsByNode.get(e.to) ?? {};
      const childEdges: GeneratedEdge[] = [];
      for (const port of Object.keys(outs)) {
        // For chain-depth purposes, the loop body counts but "after"
        // does not (it's separate). Same for branch arms vs after.
        if (port === 'after') continue;
        childEdges.push(...outs[port]);
      }
      const childDepth = 1 + maxChainDepth(childEdges, visited);
      if (childDepth > best) best = childDepth;
      visited.delete(e.to);
    }
    return best;
  }

  // Kick off placement at the entry. Entry's slot is the entire
  // subtree's width × COL_WIDTH, centered at x = 0 in LAYOUT space.
  const entryWidth = widthCache.get(entry.id) ?? 1;
  place(entry.id, -((entryWidth * COL_WIDTH) / 2), entryWidth, 0);

  // -----------------------------------------------------------
  //  Orphans (no path from entry) — park them off to the right
  //  so they stay visible and the user can rewire them manually.
  // -----------------------------------------------------------
  // Find the rightmost placed x so orphans don't overlap the main
  // graph regardless of subtree shape.
  let rightEdge = 0;
  for (const { x } of positions.values()) {
    if (x > rightEdge) rightEdge = x;
  }
  const orphanX = rightEdge + COL_WIDTH;
  let orphanY = 0;
  for (const n of spec.nodes) {
    if (!positions.has(n.id)) {
      positions.set(n.id, { x: orphanX, y: orphanY });
      orphanY += ROW_HEIGHT;
    }
  }

  return positions;
}
