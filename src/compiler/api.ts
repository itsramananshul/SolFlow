/**
 * TypeScript wrapper around the WASM compiler bridge.
 *
 * Two execution channels (B.D c41):
 *
 *   - Worker (hot path): parseSource + analyzeSource. These run
 *     on every keystroke during edit-mode (250ms debounced). A
 *     dedicated Web Worker holds its own WASM instance and serves
 *     requests over postMessage so a slow parse on a long file
 *     can't freeze the UI.
 *
 *   - Main thread (explicit actions): compileSource + runSource.
 *     These are user-triggered (button clicks). Latency is
 *     observable but tolerable; running them on the main thread
 *     keeps the cancellation / progress UX simpler.
 *
 * Both channels lazy-load — neither pays its WASM cost until the
 * first call.
 */
import CompilerWorker from './worker.ts?worker';
import type { WorkerRequest, WorkerResponse } from './worker';
import type {
  AnalyzedProgramView,
  CompileEnvelope,
  CompiledProgramView,
  Program,
  RunEnvelope,
} from './types';

// =============================================================
//  Main-thread WASM module (compile / run / version)
// =============================================================

type WasmModule = typeof import('../../compiler-wasm/pkg/solflow_compiler_wasm');

let modulePromise: Promise<WasmModule> | null = null;

function loadModule(): Promise<WasmModule> {
  if (!modulePromise) {
    modulePromise = import('../../compiler-wasm/pkg/solflow_compiler_wasm');
  }
  return modulePromise;
}

// =============================================================
//  Worker channel (parse / analyze hot path)
// =============================================================

interface PendingRequest {
  resolve: (result: string) => void;
  reject: (error: Error) => void;
}

let worker: Worker | null = null;
const pending = new Map<number, PendingRequest>();
let nextRequestId = 1;

function ensureWorker(): Worker {
  if (worker) return worker;
  worker = new CompilerWorker();
  worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
    const handler = pending.get(e.data.id);
    if (!handler) return; // stale or already settled
    pending.delete(e.data.id);
    if (e.data.ok) handler.resolve(e.data.result);
    else handler.reject(new Error(e.data.error));
  };
  worker.onerror = (e) => {
    // A worker error (e.g. WASM instantiation failure) leaves
    // every pending request hanging forever. Reject them all.
    // The caller's promise rejection is the natural surface;
    // subsequent calls will spawn a fresh worker.
    const err = new Error(`compiler worker error: ${e.message}`);
    for (const handler of pending.values()) handler.reject(err);
    pending.clear();
    worker = null;
  };
  return worker;
}

function workerRequest(
  kind: WorkerRequest['kind'],
  source: string,
): Promise<string> {
  const w = ensureWorker();
  const id = nextRequestId++;
  return new Promise<string>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    const req: WorkerRequest = { id, kind, source };
    w.postMessage(req);
  });
}

// =============================================================
//  Public API
// =============================================================

/**
 * Parse + tokenize. Runs in the compiler Web Worker.
 *
 * The `value` field of the returned envelope is the raw AST. On
 * parse failure, `value` is null and `diagnostics` contains the
 * lexer/parser errors.
 */
export async function parseSource(
  source: string,
): Promise<CompileEnvelope<Program>> {
  const json = await workerRequest('parse', source);
  return JSON.parse(json) as CompileEnvelope<Program>;
}

/**
 * Parse + analyze. Runs in the compiler Web Worker.
 *
 * The `value` field carries `{ program }` (the symbol table is
 * deliberately dropped from the envelope to keep transport size
 * small; consumers that need it can call `compileSource` and
 * re-run analysis themselves).
 */
export async function analyzeSource(
  source: string,
): Promise<CompileEnvelope<AnalyzedProgramView>> {
  const json = await workerRequest('analyze', source);
  return JSON.parse(json) as CompileEnvelope<AnalyzedProgramView>;
}

/**
 * Parse + analyze + code-generate. Runs on the main thread —
 * explicit user action; worker overhead not justified.
 *
 * The `value` field carries `{ program, instruction_count }`.
 * Use this for a "would compile" indicator without committing
 * to bytecode transport.
 */
export async function compileSource(
  source: string,
): Promise<CompileEnvelope<CompiledProgramView>> {
  const mod = await loadModule();
  return JSON.parse(
    mod.compile_source_json(source),
  ) as CompileEnvelope<CompiledProgramView>;
}

/**
 * Compile + run a SOL source via the canonical SOL VM (B.10).
 * Runs on the main thread — explicit user action; worker
 * overhead not justified.
 *
 * The single entry point for canonical-semantics simulation in
 * SolFlow. Returns:
 *   - compile diagnostics (always populated)
 *   - `run: null` when compile failed
 *   - `run: { return_value, output, steps, runtime_error }`
 *     when execution was attempted
 *
 * External calls (`ext function ... at <url>`) are intentionally
 * blocked — `run.runtime_error` will be `{ kind: 'ExtCallBlocked' }`
 * when the program reaches one. The editor surfaces this honestly
 * rather than faking a successful HTTP roundtrip.
 *
 * Infinite loops are bounded by a step limit (default 1M) on the
 * Rust side; surfaces as `{ kind: 'StepLimit' }`.
 */
export async function runSource(source: string): Promise<RunEnvelope> {
  const mod = await loadModule();
  return JSON.parse(mod.run_source_json(source)) as RunEnvelope;
}

/**
 * Version stamp of the loaded WASM module. Useful for diagnostics
 * banners ("editor expected vX, loaded vY"). Currently pinned to
 * `compiler-wasm/Cargo.toml::version`.
 *
 * Reads from the main-thread module — version is constant, so
 * either instance has the same value.
 */
export async function compilerVersion(): Promise<string> {
  const mod = await loadModule();
  return mod.version();
}

/**
 * Warm BOTH WASM instances (main + worker) up front, e.g. on
 * app boot. Resolves once both are instantiated. Errors are
 * swallowed — if WASM fails to load, the first real call will
 * surface the error in context.
 */
export function preloadCompiler(): Promise<void> {
  const main = loadModule()
    .then(() => undefined)
    .catch(() => undefined);
  // Sending a no-op "parse" with an empty source forces worker
  // boot + WASM instantiation in the background.
  const wk = workerRequest('parse', '')
    .then(() => undefined)
    .catch(() => undefined);
  return Promise.all([main, wk]).then(() => undefined);
}
