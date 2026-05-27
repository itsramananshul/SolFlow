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
 * AST shape — opaque on the TS side for now. The editor only needs
 * to display diagnostics; reading individual AST nodes is a B.6+
 * concern once hover/symbol-info lands. We type it as `unknown` to
 * keep callers honest about not depending on undocumented structure.
 */
export type Program = unknown;

export interface AnalyzedProgramView {
  program: Program;
}

export interface CompiledProgramView {
  program: Program;
  instruction_count: number;
}
