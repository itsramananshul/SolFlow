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

/// Pin the AST JSON shape so the TS importer's typed mirror in
/// `src/compiler/ast.ts` can't silently drift. If this test breaks,
/// either the Rust AST changed (in which case update ast.ts in the
/// same commit) or serde's representation flipped (in which case
/// the breakage is the alarm).
///
/// We test a representative slice — every variant that matters for
/// the importer — and assert specific substrings rather than the
/// full JSON. That keeps the test stable under inconsequential
/// changes (e.g. HashMap reordering) while still locking the bits
/// the TS side parses.
#[test]
fn ast_json_shape_locked_for_importer() {
    let source = r#"
        struct Point { x: int }
        enum Status { Active }
        function start() -> int {
            let p: int = 0;
            if (p == 0) { print("z"); }
            while (p < 10) { p = p + 1; }
            for x in [1, 2, 3] { print(x); }
            return p;
        }
    "#;
    let parsed = parse_source(source);
    assert!(!parsed.has_errors(), "fixture should parse cleanly");
    let json = serde_json::to_string(&parsed.value).expect("serialize");

    // Top-level declaration variants — externally tagged objects.
    assert!(json.contains(r#""DeclStruct""#), "DeclStruct shape");
    assert!(json.contains(r#""DeclEnum""#), "DeclEnum shape");
    assert!(json.contains(r#""DeclFunc""#), "DeclFunc shape");

    // Type union — primitives serialize as plain strings.
    assert!(json.contains(r#""Integer""#), "Integer type");

    // Statements + control flow.
    assert!(json.contains(r#""Block""#), "Block wrapper");
    assert!(json.contains(r#""StmtIf""#), "StmtIf shape");
    assert!(json.contains(r#""StmtWhile""#), "StmtWhile shape");
    assert!(json.contains(r#""StmtFor""#), "StmtFor shape");

    // Expressions.
    assert!(json.contains(r#""ExprBinary""#), "ExprBinary shape");
    assert!(json.contains(r#""ExprVar""#), "ExprVar shape");
    assert!(json.contains(r#""ExprInteger""#), "ExprInteger shape");
    assert!(json.contains(r#""ExprFuncCall""#), "ExprFuncCall shape");
    assert!(json.contains(r#""ExprArrayInit""#), "ExprArrayInit shape");
    assert!(json.contains(r#""ExprReturn""#), "ExprReturn shape");
    assert!(json.contains(r#""DeclVar""#), "DeclVar shape");

    // Op token names — the TS BinOpToken / UnaryOpToken unions
    // depend on these exact strings.
    assert!(json.contains(r#""op":"Plus""#), "Plus op token");
    assert!(json.contains(r#""op":"EqEq""#), "EqEq op token");
    assert!(json.contains(r#""op":"LessThan""#), "LessThan op token");
    // Assignment encoded as ExprBinary { op: "Eq" } — the importer
    // explicitly distinguishes this from EqEq.
    assert!(json.contains(r#""op":"Eq""#), "Eq op token (assignment)");

    // `params` of DeclFunc is Vec<(String, Type)> → array of pairs.
    // Verify the pair shape rather than the variable order serde
    // chose for HashMap keys.
    assert!(
        json.contains(r#""params":[]"#) || json.contains(r#""params":[["#),
        "params shape (array of pairs)",
    );
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
