/**
 * Span → graph-node lookup helpers (B.D c43).
 *
 * The AST→graph importer attaches `meta.sourceSpan` on every
 * node whose source AST carried one. These helpers let the
 * editor map an execution-trace span (or runtime error span)
 * back to the graph node that produced it.
 *
 * Honest semantics: when no enclosing node has a span that
 * contains the query, returns null. The editor renders
 * "no graph mapping" rather than synthesizing a placeholder.
 */

import type { SolWorkflow, GraphNode, FunctionGraph } from './schema';

/**
 * Find the graph node whose `meta.sourceSpan` contains the given
 * source byte range. Returns the function + node, or null.
 *
 * When multiple nodes' spans contain the query (e.g. a `while`
 * node's span contains all its body nodes' spans), the
 * **smallest-containing** node wins — most specific match.
 */
export function findNodeForSpan(
  workflow: SolWorkflow,
  span: { start: number; end: number },
): { fn: FunctionGraph; node: GraphNode } | null {
  let best: { fn: FunctionGraph; node: GraphNode; len: number } | null = null;
  for (const fn of workflow.functions) {
    for (const node of fn.nodes) {
      const ns = node.meta?.sourceSpan;
      if (!ns) continue;
      if (ns.start <= span.start && ns.end >= span.end) {
        const len = ns.end - ns.start;
        if (!best || len < best.len) {
          best = { fn, node, len };
        }
      }
    }
  }
  if (!best) return null;
  return { fn: best.fn, node: best.node };
}
