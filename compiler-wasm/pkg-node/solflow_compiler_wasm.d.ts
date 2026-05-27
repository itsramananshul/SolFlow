/* tslint:disable */
/* eslint-disable */

export function analyze_source_json(source: string): string;

export function compile_for_wire_json(source: string): string;

export function compile_source_json(source: string): string;

/**
 * Tokenize + parse the given SOL source.
 */
export function parse_source_json(source: string): string;

/**
 * Compile + run a SOL source via the canonical VM.
 *
 * Envelope shape (extends the standard parse/analyze envelope):
 *   {
 *     ok: boolean,                  // compile-stage clean
 *     value: { instruction_count }, // present iff compile clean
 *     diagnostics: SolDiagnostic[], // compile diagnostics
 *     run: {                        // null iff compile failed
 *       return_value: i64 | null,
 *       output: string[],
 *       steps: number,
 *       runtime_error: RuntimeErrorView | null,
 *     } | null,
 *   }
 *
 * `ok` reflects compile-stage success only — `run.runtime_error`
 * may be non-null even when `ok: true`. The TS side typically
 * renders both layers (compile + runtime) independently.
 */
export function run_source_json(source: string): string;

/**
 * Version stamp the JS side can read to detect when it's loaded
 * an older WASM than the one it expected. Pinned to the crate
 * version in Cargo.toml.
 */
export function version(): string;
