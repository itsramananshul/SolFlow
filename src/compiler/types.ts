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
 * AST shape — fully typed mirror of the Rust `parser::Program`.
 * See `./ast.ts` for the variant union. Pinned by a serde-snapshot
 * test in `compiler/tests/serde_roundtrip.rs` so the contract
 * can't silently drift.
 */
import type { Program } from './ast';
export type { Program, Ast, SolType, BinOpToken, UnaryOpToken } from './ast';

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
  | { kind: 'HeapShapeMismatch'; expected: string; got: string };

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
