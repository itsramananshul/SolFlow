/**
 * TypeScript shape of the compiler bridge's JSON envelope.
 *
 * These types mirror the Rust types in `compiler/src/diagnostic.rs`
 * and the envelope wrapper in `compiler-wasm/src/lib.rs`. The
 * pinning test `envelope_uses_human_readable_enum_strings` in the
 * compiler-wasm crate guarantees the string-literal unions below
 * stay in sync with what the bridge actually emits.
 */
export type DiagnosticSeverity = 'Error' | 'Warning' | 'Note';

export type DiagnosticPhase =
  | 'Lexer'
  | 'Parser'
  | 'Analyzer'
  | 'Codegen'
  | 'Runtime'
  | 'Internal';

export interface SourceSpan {
  start: number;
  end: number;
}

/** What an execution-trace step records. */
export type TraceStepKind = 'stmt' | 'call' | 'return' | 'error';

/**
 * One step of a real execution trace recorded by the VM as it ran.
 * Emitted identically by Browser Simulation (WASM bridge) and the
 * Local Controller, so the Trace tab renders both run targets the same.
 */
export interface TraceStep {
  /** Monotonic index of this step in the trace (0-based). */
  step: number;
  /** Step kind: a statement, a helper call, a return, or an error. */
  kind: TraceStepKind;
  /** Workflow or helper function executing at this step. */
  function: string;
  /** Byte span into the source this step maps to, when known. */
  span: SourceSpan | null;
  /** 1-based source line the span starts on, for click-to-highlight. */
  line: number | null;
  /** Call depth (0 = workflow body, 1 = inside a helper, ...). */
  depth: number;
  /** Callee name for `call`; error message for `error`. */
  detail: string | null;
}

export interface RelatedSpan {
  span: SourceSpan;
  message: string;
}

export interface SolDiagnostic {
  severity: DiagnosticSeverity;
  phase: DiagnosticPhase;
  /** Stable error code, e.g. `"E1001"`, `"E0009"`, `"ICE0001"`. */
  code: string;
  message: string;
  span: SourceSpan | null;
  related: RelatedSpan[];
  help: string | null;
}

/**
 * Stable envelope shape returned by every compiler-wasm entry point.
 * `T` varies per entry point — see {@link CompilerApi}.
 */
export interface CompileEnvelope<T> {
  /** True iff `diagnostics` contains no `Error`-severity entries. */
  ok: boolean;
  /** Present on success; null when a fatal error short-circuited. */
  value: T | null;
  diagnostics: SolDiagnostic[];
}

/**
 * AST shape — fully typed mirror of the canonical Rust
 * `ast::Program` (crate `openprem-sol-v2`). See `./ast.ts` for the
 * variant union; the shapes are confirmed against the bridge's
 * `parse_source_json` output.
 */
import type { Program } from './ast';
export type {
  Program,
  TopLevel,
  Stmt,
  Expr,
  SolType,
  BinOp,
  UnaryOp,
} from './ast';

export interface AnalyzedProgramView {
  program: Program;
}

export interface CompiledProgramView {
  program: Program;
  instruction_count: number;
}

// ----------------------------------------------------------------
//  B.10 — canonical-VM execution
// ----------------------------------------------------------------

/**
 * Structured runtime error from the canonical SOL VM. Discriminated
 * union; `kind` is the tag the Rust side serializes via
 * `#[serde(tag = "kind")]`.
 */
export type RuntimeError =
  | { kind: 'DivByZero' }
  | { kind: 'IndexOutOfBounds'; index: number; length: number }
  | { kind: 'StackUnderflow' }
  | { kind: 'StepLimit'; limit: number }
  | { kind: 'ExtCallBlocked'; function_name: string; url: string }
  | { kind: 'ExtCallFailed'; connector: string; function_name: string; message: string }
  | { kind: 'HeapShapeMismatch'; expected: string; got: string }
  // Phase C C.6 c89 — browser-sim doesn't trigger these today,
  // but the wire shape stays uniform with the controller's
  // runtime-error view so editor code can switch on `kind`
  // exhaustively.
  | { kind: 'Cancelled' }
  | { kind: 'ResourceLimit'; resource: string; limit: number };

/**
 * The `run` field of the {@link RunEnvelope}. Null when compile
 * failed; non-null when execution was attempted (even if it ended
 * in a runtime error).
 */
export interface RunResult {
  /** Top-of-stack value at termination. Null on runtime error. */
  return_value: number | null;
  /** Captured `print` output, in canonical order. */
  output: string[];
  /** Number of VM steps executed. */
  steps: number;
  /** Structured runtime error if execution didn't complete cleanly. */
  runtime_error: RuntimeError | null;
  /**
   * Source span of the instruction that produced `runtime_error`,
   * when one was captured (B.D c42). Lets the UI scroll the
   * source pane to the failure site. Null when execution
   * completed cleanly or the failing instruction's bytecode
   * didn't have a span (rare).
   */
  runtime_error_source_span: SourceSpan | null;
  /**
   * Real execution trace recorded by the VM as it ran, in order.
   * One entry per executed statement plus helper call/return and
   * error events, each carrying the function, source span/line, and
   * call depth. Never empty for a real run.
   */
  trace: TraceStep[];
  /**
   * True when the trace cap was hit and recording stopped. The UI
   * surfaces "execution trace truncated" so users know the list
   * isn't the full history.
   */
  trace_truncated: boolean;
}

/**
 * The envelope `run_source_json` returns. `ok` reflects
 * compile-stage success; `run.runtime_error` may still be
 * non-null on `ok: true`.
 */
export interface RunEnvelope {
  ok: boolean;
  value: { instruction_count: number } | null;
  diagnostics: SolDiagnostic[];
  run: RunResult | null;
}
