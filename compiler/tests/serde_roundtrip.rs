//! Verifies the `serde` feature derives behave end-to-end:
//! every public type that opts into derives can be serialized
//! to JSON and deserialized back without loss.
//!
//! This test only runs when the `serde` feature is enabled:
//!     cargo test --features serde --test serde_roundtrip
//!
//! B.3 groundwork: this proves the WASM bridge (B.4) can ship an
//! AST + diagnostics across the worker boundary as JSON.

#![cfg(feature = "serde")]

use solflow_compiler::{
    compile_source, parse_source, DiagnosticPhase, DiagnosticSeverity, SolDiagnostic, SourceSpan,
};

#[test]
fn parsed_program_roundtrips_through_json() {
    let source = r#"
        function add(a: int, b: int) -> int {
            return a + b;
        }
        function start() -> int {
            return add(1, 2);
        }
    "#;
    let parsed = parse_source(source);
    assert!(!parsed.has_errors(), "fixture should parse: {:#?}", parsed.diagnostics);
    let program = parsed.value.expect("program present");

    let json = serde_json::to_string(&program).expect("serialize program");
    let restored: solflow_compiler::parser::Program =
        serde_json::from_str(&json).expect("deserialize program");

    // Re-serialize the restored value; identical JSON proves
    // no data was lost in the round-trip. (Direct struct equality
    // isn't available because Ast doesn't derive PartialEq.)
    let json2 = serde_json::to_string(&restored).expect("re-serialize");
    assert_eq!(json, json2, "serialization must be stable under roundtrip");
}

#[test]
fn diagnostics_roundtrip_through_json() {
    let source = "function start() -> int { return undefined_var; }";
    let compiled = compile_source(source);
    assert!(compiled.has_errors(), "should produce an error");

    let json = serde_json::to_string(&compiled.diagnostics).expect("serialize diagnostics");
    let restored: Vec<SolDiagnostic> =
        serde_json::from_str(&json).expect("deserialize diagnostics");

    assert_eq!(compiled.diagnostics.len(), restored.len());
    for (before, after) in compiled.diagnostics.iter().zip(restored.iter()) {
        assert_eq!(before.severity, after.severity);
        assert_eq!(before.phase, after.phase);
        assert_eq!(before.code, after.code);
        assert_eq!(before.message, after.message);
    }
}

#[test]
fn source_span_roundtrips() {
    let span = SourceSpan::new(12, 34);
    let json = serde_json::to_string(&span).expect("serialize");
    let restored: SourceSpan = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(span, restored);
}

#[test]
fn enum_phases_serialize_as_human_strings() {
    // Serde's default enum representation for unit-variants is a
    // plain JSON string ("Lexer", "Parser", ...). This is what
    // the WASM bridge will see; pinning the format here so a
    // representation change doesn't silently break callers.
    let json = serde_json::to_string(&DiagnosticPhase::Analyzer).unwrap();
    assert_eq!(json, "\"Analyzer\"");

    let json = serde_json::to_string(&DiagnosticSeverity::Error).unwrap();
    assert_eq!(json, "\"Error\"");
}
