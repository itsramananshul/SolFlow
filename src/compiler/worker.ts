/// <reference lib="webworker" />
/**
 * Compiler Web Worker — runs the WASM compiler off the main thread
 * for the hot path (parse/analyze, called on every keystroke during
 * edit-mode debounce).
 *
 * Why ONLY parse + analyze:
 *   - These two run on every keystroke (250ms debounced); they
 *     dominate the latency budget. A slow parse on a long file
 *     would otherwise freeze the UI.
 *   - `compileSource` and `runSource` are explicit user actions
 *     (button clicks). Their latency is observable but tolerable;
 *     moving them to the worker would mean restructuring
 *     cancellation + progress UX for no measured win.
 *
 * One worker instance per page. Holds its own WASM instantiation
 * (separate ~3MB heap from the main thread's instance, when one
 * exists). Caller-side request/response correlation by monotonic
 * `id` field on every message.
 */

import {
  parse_source_json,
  analyze_source_json,
} from '../../compiler-wasm/pkg/solflow_compiler_wasm';

export type WorkerRequest = {
  id: number;
  kind: 'parse' | 'analyze';
  source: string;
};

export type WorkerResponse =
  | { id: number; ok: true; result: string }
  | { id: number; ok: false; error: string };

// `self` in a Worker is the DedicatedWorkerGlobalScope.
const ctx = self as unknown as DedicatedWorkerGlobalScope;

ctx.onmessage = (e: MessageEvent<WorkerRequest>) => {
  const { id, kind, source } = e.data;
  try {
    let result: string;
    switch (kind) {
      case 'parse':
        result = parse_source_json(source);
        break;
      case 'analyze':
        result = analyze_source_json(source);
        break;
    }
    const response: WorkerResponse = { id, ok: true, result };
    ctx.postMessage(response);
  } catch (e) {
    const response: WorkerResponse = {
      id,
      ok: false,
      error: e instanceof Error ? `${e.name}: ${e.message}` : String(e),
    };
    ctx.postMessage(response);
  }
};
