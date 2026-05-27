//! wasm-bindgen bridge for `solflow_compiler`.
//!
//! Exports three entry points that mirror the Rust library API
//! one-for-one, but return JSON strings so the JS↔WASM boundary
//! is stable and doesn't depend on wasm-bindgen's evolving
//! `JsValue` serialization story:
//!
//!   parse_source_json(source)    -> envelope JSON; value is Program
//!   analyze_source_json(source)  -> envelope JSON; value is { program }
//!   compile_source_json(source)  -> envelope JSON; value is { program, instruction_count }
//!
//! Envelope shape (stable contract for the TS side):
//!
//!   {
//!     "ok": boolean,                  // true iff no error-severity diagnostics
//!     "value": <T> | null,            // shape varies per entry point
//!     "diagnostics": SolDiagnostic[]
//!   }
//!
//! Panic handling: every entry point installs the browser-console
//! panic hook on first call and wraps the body in `catch_unwind`.
//! If the compiler panics, the bridge returns a single ICE
//! diagnostic envelope rather than letting the WASM instance abort.

use std::panic;
use std::sync::Once;

use serde::Serialize;
use solflow_compiler::{
    analyze_source, codes, compile_source, parse_source, AnalyzedProgram, CompiledProgram,
    DiagnosticPhase, DiagnosticSeverity, SolDiagnostic, SourceSpan,
};
use solflow_runtime::{run_program_with, RunError, RunOptions};
use wasm_bindgen::prelude::*;

static PANIC_HOOK_INIT: Once = Once::new();

fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        console_error_panic_hook::set_once();
    });
}

/// One stable envelope shape for every entry point.
#[derive(Serialize)]
struct Envelope<'a, V: Serialize> {
    ok: bool,
    value: Option<&'a V>,
    diagnostics: &'a [SolDiagnostic],
}

fn envelope_json<V: Serialize>(value: Option<&V>, diagnostics: &[SolDiagnostic]) -> String {
    let ok = !diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error);
    let env = Envelope { ok, value, diagnostics };
    serde_json::to_string(&env).unwrap_or_else(|e| ice_envelope(&format!("serialize: {e}")))
}

/// Last-resort envelope used when serde itself fails or a panic
/// crosses the boundary. Hand-written so it can't itself fail.
fn ice_envelope(message: &str) -> String {
    let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"ok":false,"value":null,"diagnostics":[{{"severity":"Error","phase":"Internal","code":"ICE0001","message":"{escaped}","span":null,"related":[],"help":"this is a bug in solflow_compiler; please report it"}}]}}"#
    )
}

/// Panic-catching wrapper for every WASM entry point. Each entry
/// point composes its own envelope JSON inside `f`; if `f` panics,
/// we return an ICE envelope so the JS side always gets valid JSON.
fn safe<F>(f: F) -> String
where
    F: FnOnce() -> String + std::panic::UnwindSafe,
{
    install_panic_hook();
    match panic::catch_unwind(f) {
        Ok(json) => json,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "compiler panic (no extractable message)".to_string()
            };
            // Synthetic ICE so the editor can render this exactly like
            // any other compiler diagnostic.
            let ice = SolDiagnostic {
                severity: DiagnosticSeverity::Error,
                phase: DiagnosticPhase::Internal,
                code: codes::ICE_UNHANDLED_AST.to_string(),
                message: format!("compiler panicked across the WASM boundary: {msg}"),
                span: None,
                related: Vec::new(),
                help: Some(
                    "this is a bug in solflow_compiler; please report it with the source that triggered it"
                        .to_string(),
                ),
            };
            envelope_json::<()>(None, &[ice])
        }
    }
}

/// Tokenize + parse the given SOL source.
#[wasm_bindgen]
pub fn parse_source_json(source: &str) -> String {
    safe(|| {
        let cr = parse_source(source);
        envelope_json(cr.value.as_ref(), &cr.diagnostics)
    })
}

/// Tokenize + parse + analyze. The `tt_arena` is dropped from the
/// returned value for now — the editor doesn't need symbol tables
/// yet and serializing them inflates the payload. Re-add when
/// hover/symbol-info actually needs them.
#[derive(Serialize)]
struct AnalyzedProgramView<'a> {
    program: &'a solflow_compiler::parser::Program,
}

#[wasm_bindgen]
pub fn analyze_source_json(source: &str) -> String {
    safe(|| {
        let cr = analyze_source(source);
        let view = cr
            .value
            .as_ref()
            .map(|AnalyzedProgram { program, .. }| AnalyzedProgramView { program });
        envelope_json(view.as_ref(), &cr.diagnostics)
    })
}

/// Tokenize + parse + analyze + code-generate. Returns the program +
/// the emitted bytecode size. Full `Inst` list isn't yet serde-derived
/// (see AST_SERDE_NOTES.md); the count is enough for the editor's
/// "this would compile" indicator without committing to bytecode
/// transport.
#[derive(Serialize)]
struct CompiledProgramView<'a> {
    program: &'a solflow_compiler::parser::Program,
    instruction_count: usize,
}

#[wasm_bindgen]
pub fn compile_source_json(source: &str) -> String {
    safe(|| {
        let cr = compile_source(source);
        let view = cr.value.as_ref().map(
            |CompiledProgram { program, bytecode, .. }| CompiledProgramView {
                program,
                instruction_count: bytecode.len(),
            },
        );
        envelope_json(view.as_ref(), &cr.diagnostics)
    })
}

/// Version stamp the JS side can read to detect when it's loaded
/// an older WASM than the one it expected. Pinned to the crate
/// version in Cargo.toml.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// =============================================================
//  Phase C C.2 — wire-ready compile for controller submission
// =============================================================
//
// `compile_for_wire_json(source)` returns an envelope whose value
// contains wire-encoded `bytecode` and `instruction_spans` as
// `Vec<u8>` (serialized as JSON number-arrays). The editor pipes
// these directly into `WorkflowSubmission`.
//
// The encoding is `serde_json::to_vec` of the bytecode (matching
// `solflow_host_spec::encode_bytecode`) — by keeping the bytes in
// the same shape the host-spec helper produces, the editor and
// controller agree on the wire format without either side knowing
// the inner JSON structure of `Inst`.

#[derive(Serialize)]
struct CompiledForWireView<'a> {
    program: &'a solflow_compiler::parser::Program,
    instruction_count: usize,
    /// `serde_json::to_vec(&bytecode)` — opaque bytes the editor
    /// forwards verbatim to `POST /workflows`.
    bytecode: Vec<u8>,
    /// `serde_json::to_vec(&instruction_spans)` — same contract.
    instruction_spans: Vec<u8>,
}

#[wasm_bindgen]
pub fn compile_for_wire_json(source: &str) -> String {
    safe(|| {
        let cr = compile_source(source);
        let view = cr.value.as_ref().and_then(
            |CompiledProgram { program, bytecode, instruction_spans, .. }| {
                let bc = serde_json::to_vec(bytecode).ok()?;
                let sp = serde_json::to_vec(instruction_spans).ok()?;
                Some(CompiledForWireView {
                    program,
                    instruction_count: bytecode.len(),
                    bytecode: bc,
                    instruction_spans: sp,
                })
            },
        );
        envelope_json(view.as_ref(), &cr.diagnostics)
    })
}

// =============================================================
//  B.10 — canonical VM execution
// =============================================================
//
// `run_source_json(source)` is the canonical-simulation entry
// point. Compiles the source through the standard pipeline; if
// any compile errors fire, returns them WITHOUT executing.
// Otherwise runs the bytecode through `solflow_runtime::VM` and
// returns the captured output + return value + any runtime error.
//
// The envelope shape extends the existing parse/analyze/compile
// envelope with a `run` field carrying the execution result. This
// way the TS side can branch on `run !== null` to know whether
// execution was attempted vs. short-circuited by compile errors.

/// Per-run result, mirrored from `solflow_runtime::RunOutcome`
/// with the runtime error structured for serde.
#[derive(Serialize)]
struct RunResultView<'a> {
    /// Top-of-stack value at termination. The TS side interprets
    /// per declared return type (raw u64; int = `value as i64`,
    /// float = `f64::from_bits(value)`, bool = `value != 0`, etc.).
    /// `null` when execution didn't complete (compile error or
    /// runtime error before any return).
    return_value: Option<i64>,
    /// Captured `print` output lines, in canonical order.
    output: &'a [String],
    /// Number of VM steps executed before termination.
    steps: usize,
    /// Structured runtime error (div-by-zero, OOB, step limit,
    /// ExtCall blocked, heap shape mismatch, stack underflow).
    /// Null on clean termination.
    runtime_error: Option<RuntimeErrorView>,
    /// Approximate source span of the offending instruction when
    /// `runtime_error` is non-null AND the bytecode at that
    /// inst_ptr had a span (most do). Lets the editor scroll the
    /// source pane to the failure site on error.
    runtime_error_source_span: Option<SourceSpan>,
    /// Executed-source-range trace (B.D c42). One entry per
    /// observable source position the VM visited, in order.
    /// Adjacent equal spans are de-duplicated (a 100-step inner
    /// loop produces one trace entry, not 100). Empty when
    /// tracing wasn't enabled by the caller.
    trace: Vec<SourceSpan>,
    /// True when the underlying VM's trace cap was hit. The UI
    /// renders "execution trace truncated" so the user knows the
    /// list isn't complete.
    trace_truncated: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum RuntimeErrorView {
    DivByZero,
    IndexOutOfBounds { index: usize, length: usize },
    StackUnderflow,
    StepLimit { limit: usize },
    ExtCallBlocked { function_name: String, url: String },
    ExtCallFailed { connector: String, function_name: String, message: String },
    HeapShapeMismatch { expected: String, got: String },
}

impl From<&RunError> for RuntimeErrorView {
    fn from(e: &RunError) -> Self {
        match e {
            RunError::DivByZero => RuntimeErrorView::DivByZero,
            RunError::IndexOutOfBounds { index, length } => {
                RuntimeErrorView::IndexOutOfBounds { index: *index, length: *length }
            }
            RunError::StackUnderflow => RuntimeErrorView::StackUnderflow,
            RunError::StepLimit { limit } => RuntimeErrorView::StepLimit { limit: *limit },
            RunError::ExtCallBlocked { function_name, url } => {
                RuntimeErrorView::ExtCallBlocked {
                    function_name: function_name.clone(),
                    url: url.clone(),
                }
            }
            RunError::ExtCallFailed { connector, function_name, message } => {
                RuntimeErrorView::ExtCallFailed {
                    connector: connector.clone(),
                    function_name: function_name.clone(),
                    message: message.clone(),
                }
            }
            RunError::HeapShapeMismatch { expected, got } => {
                RuntimeErrorView::HeapShapeMismatch {
                    expected: (*expected).to_string(),
                    got: (*got).to_string(),
                }
            }
        }
    }
}

/// Compile + run a SOL source via the canonical VM.
///
/// Envelope shape (extends the standard parse/analyze envelope):
///   {
///     ok: boolean,                  // compile-stage clean
///     value: { instruction_count }, // present iff compile clean
///     diagnostics: SolDiagnostic[], // compile diagnostics
///     run: {                        // null iff compile failed
///       return_value: i64 | null,
///       output: string[],
///       steps: number,
///       runtime_error: RuntimeErrorView | null,
///     } | null,
///   }
///
/// `ok` reflects compile-stage success only — `run.runtime_error`
/// may be non-null even when `ok: true`. The TS side typically
/// renders both layers (compile + runtime) independently.
#[wasm_bindgen]
pub fn run_source_json(source: &str) -> String {
    safe(|| {
        let cr = compile_source(source);
        // Pull diagnostics out first so we can keep referencing them
        // after we partially-move `cr.value`. `has_errors` is derivable
        // from the diagnostics directly.
        let diagnostics = cr.diagnostics;
        let has_errors = diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error);
        let CompiledProgram { program, bytecode, instruction_spans, .. } = match cr.value {
            Some(v) => v,
            None => {
                // Compile failed — surface diagnostics, skip
                // execution. `run` is null so the TS side knows
                // we never tried.
                #[derive(Serialize)]
                struct CompileFailEnvelope<'a> {
                    ok: bool,
                    value: Option<()>,
                    diagnostics: &'a [SolDiagnostic],
                    run: Option<()>,
                }
                let env = CompileFailEnvelope {
                    ok: false,
                    value: None,
                    diagnostics: &diagnostics,
                    run: None,
                };
                return serde_json::to_string(&env).unwrap_or_else(|e| {
                    ice_envelope(&format!("serialize compile-fail: {e}"))
                });
            }
        };

        // Compile clean — run with tracing enabled so the editor
        // can render the execution trace panel (B.D c42). Trace
        // overhead is one usize push per step; bounded by the
        // runtime's default cap (10k entries) so a runaway loop
        // doesn't blow memory.
        let outcome = run_program_with(
            &bytecode,
            RunOptions { step_limit: None, trace: true, ext_call_handler: None },
        );

        #[derive(Serialize)]
        struct CompileOkEnvelope<'a> {
            ok: bool,
            value: CompiledView,
            diagnostics: &'a [SolDiagnostic],
            run: RunResultView<'a>,
        }
        #[derive(Serialize)]
        struct CompiledView {
            instruction_count: usize,
        }

        // Avoid dead-code from the `program` field of CompiledProgram
        // (we only need its bytecode for execution; the AST is not
        // surfaced here — run_source_json is for executing, callers
        // use compile_source_json when they want the AST too).
        let _ = program;

        // Map inst_ptr → source span for the runtime error site
        // (if any) and for each trace step. The span lookup is
        // guarded against pathological inst_ptrs.
        let span_for = |ip: usize| -> Option<SourceSpan> {
            instruction_spans.get(ip).copied().flatten()
        };

        let runtime_err_view = outcome.error.as_ref().map(RuntimeErrorView::from);
        let runtime_err_span = outcome.error_inst_ptr.and_then(span_for);

        // Build trace as de-duplicated source-span list. The VM
        // produces one entry per executed inst_ptr; many adjacent
        // ips share spans (e.g. arithmetic chain inside one
        // expression), so the deduped list is much shorter and
        // much more readable as a UX surface.
        let mut trace_spans: Vec<SourceSpan> = Vec::new();
        let mut last_pushed: Option<SourceSpan> = None;
        for &ip in &outcome.trace {
            if let Some(span) = span_for(ip) {
                if Some(span) != last_pushed {
                    trace_spans.push(span);
                    last_pushed = Some(span);
                }
            }
        }

        let env = CompileOkEnvelope {
            ok: !has_errors,
            value: CompiledView { instruction_count: bytecode.len() },
            diagnostics: &diagnostics,
            run: RunResultView {
                return_value: if outcome.error.is_none() {
                    Some(outcome.return_value as i64)
                } else {
                    None
                },
                output: &outcome.output,
                steps: outcome.steps,
                runtime_error: runtime_err_view,
                runtime_error_source_span: runtime_err_span,
                trace: trace_spans,
                trace_truncated: outcome.trace_truncated,
            },
        };
        serde_json::to_string(&env)
            .unwrap_or_else(|e| ice_envelope(&format!("serialize run envelope: {e}")))
    })
}

// =============================================================
//  Integration tests
// =============================================================
//
// These run as a plain Rust test binary (cargo test -p
// solflow_compiler_wasm) — no browser involved. They exercise the
// same JSON wrappers the WASM bridge exports, so we get fast
// feedback when the envelope shape changes or panic-isolation
// breaks.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn must_parse_envelope(json: &str) -> Value {
        serde_json::from_str(json).expect("envelope must be valid JSON")
    }

    #[test]
    fn parse_valid_returns_ok_envelope() {
        let json = parse_source_json("function start() -> int { return 0; }");
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true);
        assert!(v["value"].is_array(), "value should be the Program array");
        assert_eq!(v["diagnostics"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn parse_broken_returns_error_envelope() {
        let json = parse_source_json("function start() -> int { return 0 }"); // missing semi
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], false);
        let diags = v["diagnostics"].as_array().unwrap();
        assert!(!diags.is_empty(), "expected at least one diagnostic");
        let codes: Vec<&str> = diags.iter().filter_map(|d| d["code"].as_str()).collect();
        assert!(
            codes.iter().any(|c| c.starts_with("E0")),
            "expected a parse-stage E0xxx code; got {codes:?}",
        );
    }

    #[test]
    fn analyze_undefined_var_returns_e1001() {
        let json =
            analyze_source_json("function start() -> int { return undefined_var; }");
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], false);
        let codes: Vec<&str> = v["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["code"].as_str())
            .collect();
        assert!(codes.contains(&"E1001"), "expected E1001 in {codes:?}");
    }

    #[test]
    fn compile_clean_program_reports_instruction_count() {
        let json = compile_source_json("function start() -> int { return 42; }");
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true);
        let count = v["value"]["instruction_count"]
            .as_u64()
            .expect("instruction_count present");
        assert!(count > 0, "non-empty program should have >0 bytecode");
    }

    #[test]
    fn version_returns_crate_version() {
        let v = version();
        assert!(!v.is_empty());
        assert!(v.contains('.'));
    }

    #[test]
    fn run_source_emits_canonical_output_for_clean_program() {
        let json = run_source_json(
            r#"function start() -> int { print("hi"); print(42); return 7; }"#,
        );
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true, "compile clean: {json}");
        assert_eq!(v["run"]["return_value"], 7);
        assert_eq!(v["run"]["output"][0], "hi");
        assert_eq!(v["run"]["output"][1], "42");
        assert!(v["run"]["runtime_error"].is_null());
        let steps = v["run"]["steps"].as_u64().expect("steps present");
        assert!(steps > 0);
    }

    #[test]
    fn run_source_short_circuits_on_compile_error() {
        // Missing semicolon — parser error, no execution attempted.
        let json = run_source_json("function start() -> int { return 0 }");
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], false);
        // `run` must be null when compile fails — TS side branches
        // on this to know whether execution was attempted.
        assert!(v["run"].is_null(), "run should be null when compile fails");
    }

    #[test]
    fn run_source_surfaces_div_by_zero_as_structured_error() {
        let json = run_source_json(
            "function start() -> int { return 10 / 0; }",
        );
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true, "compile clean (runtime-only failure)");
        assert!(v["run"]["return_value"].is_null());
        assert_eq!(v["run"]["runtime_error"]["kind"], "DivByZero");
    }

    #[test]
    fn run_source_surfaces_execution_trace_when_program_runs(
    ) {
        // B.D c42: every successful run includes a de-duplicated
        // source-span execution trace.
        let json = run_source_json(
            "function start() -> int { let x: int = 1; return x; }",
        );
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true, "{json}");
        let trace = v["run"]["trace"]
            .as_array()
            .expect("trace should be an array");
        assert!(!trace.is_empty(), "trace should have at least one entry");
        // Each entry shape: { start, end }
        let first = &trace[0];
        assert!(first["start"].is_number());
        assert!(first["end"].is_number());
        assert_eq!(v["run"]["trace_truncated"], false);
    }

    #[test]
    fn run_source_attaches_source_span_to_runtime_error() {
        // B.D c42: when a runtime error fires, the offending
        // instruction's source span is surfaced so the editor can
        // scroll the source pane to the failure site.
        let json = run_source_json(
            "function start() -> int { return 10 / 0; }",
        );
        let v = must_parse_envelope(&json);
        assert_eq!(v["run"]["runtime_error"]["kind"], "DivByZero");
        let span = &v["run"]["runtime_error_source_span"];
        assert!(
            span.is_object(),
            "runtime_error_source_span should be populated; got {span}",
        );
        assert!(span["start"].is_number());
        assert!(span["end"].is_number());
    }

    #[test]
    fn run_source_surfaces_step_limit_for_infinite_loop() {
        // This relies on the runtime's 1M default; small infinite
        // loop still hits the limit fast enough for tests.
        let json = run_source_json(
            "function start() -> int { while (1 == 1) { } return 0; }",
        );
        let v = must_parse_envelope(&json);
        assert_eq!(v["ok"], true);
        assert_eq!(v["run"]["runtime_error"]["kind"], "StepLimit");
    }

    #[test]
    fn envelope_uses_human_readable_enum_strings() {
        // Pins the contract the TS side relies on. Changing this
        // breaks the TS DiagnosticPhase / DiagnosticSeverity unions.
        let json =
            analyze_source_json("function start() -> int { return undefined_var; }");
        assert!(
            json.contains(r#""phase":"Analyzer""#),
            "expected human-string phase in {json}",
        );
        assert!(
            json.contains(r#""severity":"Error""#),
            "expected human-string severity in {json}",
        );
    }
}
