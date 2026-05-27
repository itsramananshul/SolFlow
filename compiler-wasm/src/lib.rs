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
    DiagnosticPhase, DiagnosticSeverity, SolDiagnostic,
};
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
