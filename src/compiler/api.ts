/**
 * TypeScript wrapper around the WASM compiler bridge.
 *
 * Lazy-loads the wasm-pack `bundler`-target module the first time
 * any entrypoint is called so the WASM bytes don't block initial
 * page paint. Subsequent calls share the same instance.
 *
 * All three entrypoints are synchronous *with respect to compilation*
 * (the compiler itself is sync), but `async` overall because the
 * first call has to wait for WASM instantiation. Callers should
 * await every call uniformly.
 */
import type {
  AnalyzedProgramView,
  CompileEnvelope,
  CompiledProgramView,
  Program,
  RunEnvelope,
} from './types';

// Vite resolves this through the wasm-pack package.json in
// `compiler-wasm/pkg/` (committed at the repo root). The dynamic
// import keeps the WASM bytes out of the initial bundle.
type WasmModule = typeof import('../../compiler-wasm/pkg/solflow_compiler_wasm');

let modulePromise: Promise<WasmModule> | null = null;

function loadModule(): Promise<WasmModule> {
  if (!modulePromise) {
    modulePromise = import('../../compiler-wasm/pkg/solflow_compiler_wasm');
  }
  return modulePromise;
}

/** Parse + tokenize. The `value` field is the raw AST. */
export async function parseSource(
  source: string,
): Promise<CompileEnvelope<Program>> {
  const mod = await loadModule();
  return JSON.parse(mod.parse_source_json(source)) as CompileEnvelope<Program>;
}

/** Parse + analyze. The `value` field carries `{ program }`. */
export async function analyzeSource(
  source: string,
): Promise<CompileEnvelope<AnalyzedProgramView>> {
  const mod = await loadModule();
  return JSON.parse(
    mod.analyze_source_json(source),
  ) as CompileEnvelope<AnalyzedProgramView>;
}

/**
 * Parse + analyze + code-generate. The `value` field carries
 * `{ program, instruction_count }`. Use this for a "would compile"
 * indicator without committing to bytecode transport.
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
 */
export async function compilerVersion(): Promise<string> {
  const mod = await loadModule();
  return mod.version();
}

/**
 * Warm the WASM module up front, e.g. on app boot. Resolves once
 * the module is instantiated. Errors are swallowed — if WASM fails
 * to load, the first real call will surface the error in context.
 */
export function preloadCompiler(): Promise<void> {
  return loadModule()
    .then(() => undefined)
    .catch(() => undefined);
}
