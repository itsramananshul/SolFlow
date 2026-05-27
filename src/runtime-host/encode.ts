/**
 * Editor-side compile-for-controller helper (Phase C C.2 c61).
 *
 * Wraps the WASM bridge's `compile_for_wire_json` entry point.
 * Returns the standard `CompileEnvelope` shape but with `value`
 * carrying pre-encoded `bytecode` + `instruction_spans` bytes
 * ready to drop into a `WorkflowSubmission`.
 *
 * The format is opaque to the editor — it just forwards the
 * bytes verbatim to the controller. Both sides agree on
 * `solflow_host_spec::encode_bytecode` (JSON of `Vec<Inst>`)
 * as the wire format.
 */
import type { CompileEnvelope, Program } from '@/compiler/types';

type WasmModule = typeof import('../../compiler-wasm/pkg/solflow_compiler_wasm');

let modulePromise: Promise<WasmModule> | null = null;

function loadModule(): Promise<WasmModule> {
  if (!modulePromise) {
    modulePromise = import('../../compiler-wasm/pkg/solflow_compiler_wasm');
  }
  return modulePromise;
}

/** The `value` shape of the wire-encoding envelope. */
export interface CompiledForWire {
  program: Program;
  instruction_count: number;
  /** Wire-ready bytecode bytes. See WorkflowSubmission.bytecode. */
  bytecode: number[];
  /** Wire-ready instruction-spans sidecar. */
  instruction_spans: number[];
}

/**
 * Compile + wire-encode. Returns the envelope verbatim from the
 * WASM bridge. On compile failure `value` is null and `ok` is
 * false; diagnostics carry the reason. On success, the byte
 * arrays are ready for `POST /workflows` after wrapping in a
 * `WorkflowSubmission`.
 *
 * Runs on the main thread (explicit user action — Run with
 * controller-local mode). The worker channel only handles
 * parse/analyze for the hot edit path.
 */
export async function compileForController(
  source: string,
): Promise<CompileEnvelope<CompiledForWire>> {
  const mod = await loadModule();
  return JSON.parse(
    mod.compile_for_wire_json(source),
  ) as CompileEnvelope<CompiledForWire>;
}
