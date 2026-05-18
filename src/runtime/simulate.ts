/**
 * SolFlow Phase A — execution trace recorder.
 *
 * Runs the existing interpreter once, recording the order in which
 * statement-form nodes are entered/exited and which edges are traversed.
 * The simulation store then plays this trace back on the canvas at a
 * human-paced rate so the user can watch their workflow execute.
 *
 * Single-pass, deterministic given identical input; safe to call as
 * often as the user wants without re-running side effects on the canvas
 * (print output is captured but the canvas animation reads only the
 * event timeline, not the output buffer).
 */

import { run, type RunOptions, type RunResult } from './interpret';
import type { SolWorkflow } from '@/graph/schema';

export type StepEvent =
  | { type: 'enter'; id: string }
  | { type: 'exit'; id: string }
  | { type: 'edge'; id: string }
  | { type: 'error'; id: string; message: string };

export interface Trace {
  events: StepEvent[];
  result: RunResult;
}

export function recordTrace(workflow: SolWorkflow, opts?: RunOptions): Trace {
  const events: StepEvent[] = [];
  // Use a fresh array for each call; the interpreter walks synchronously.
  const result = run(
    workflow,
    {
      onNodeEnter: (id) => events.push({ type: 'enter', id }),
      onNodeExit: (id) => events.push({ type: 'exit', id }),
      onEdgeTraverse: (id) => events.push({ type: 'edge', id }),
      onError: (id, message) => events.push({ type: 'error', id, message }),
    },
    opts,
  );
  return { events, result };
}
