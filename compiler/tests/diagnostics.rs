//! Negative-fixture diagnostics test.
//!
//! Every fixture in tests/fixtures/negative/ exercises a known
//! compile-time error. After B.2 c3+c4 the lexer, parser, and
//! analyzer all return diagnostics through the `*_source` calls
//! instead of crashing the test runner.
//!
//! `error_runtime.sol` is a VM panic and is not exercised here —
//! no compile-time call reaches it.

use solflow_compiler::{
    analyze_source, codes, parse_source, DiagnosticPhase, DiagnosticSeverity,
};

fn read_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/negative/{name}.sol");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// `error_parse1.sol`: `let x: int = ;` — empty initializer. The
/// parser hits the expression's catch-all in primary, emits
/// E0009, and recovery skips to the next top-level keyword.
#[test]
fn parse1_empty_initializer_returns_diagnostic() {
    let source = read_fixture("error_parse1");
    let result = parse_source(&source);

    assert!(
        result.has_errors(),
        "expected parse error; got {} diagnostics, value present? {}",
        result.diagnostics.len(),
        result.value.is_some(),
    );

    let first_error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == DiagnosticSeverity::Error)
        .expect("at least one error");
    assert_eq!(first_error.phase, DiagnosticPhase::Parser);
    assert_eq!(first_error.code, codes::PARSE_NOT_EXPRESSION);
}

/// `error_parse2.sol`: `let x: int = 5` missing semicolon. The
/// parser's `eat(Semi)` emits E0005 (PARSE_MISSING_DELIMITER).
#[test]
fn parse2_missing_semicolon_returns_diagnostic() {
    let source = read_fixture("error_parse2");
    let result = parse_source(&source);

    assert!(result.has_errors(), "expected parse error");

    let codes_seen: Vec<&str> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .map(|d| d.code)
        .collect();
    assert!(
        codes_seen.contains(&codes::PARSE_MISSING_DELIMITER),
        "expected E0005 (missing delimiter); got {codes_seen:?}",
    );
}

/// `error_semantic1.sol`: `return undefined_var;` — analyzer's
/// `ExprVar` lookup misses, emits E1001.
#[test]
fn semantic1_undefined_var_returns_diagnostic() {
    let source = read_fixture("error_semantic1");
    let result = analyze_source(&source);

    assert!(result.has_errors(), "expected analyzer error");
    let codes_seen: Vec<&str> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .map(|d| d.code)
        .collect();
    assert!(
        codes_seen.contains(&codes::SEMA_UNDEFINED_NAME),
        "expected E1001 (undefined name); got {codes_seen:?}",
    );
    // Diagnostic must be tagged Analyzer phase.
    let first = result
        .diagnostics
        .iter()
        .find(|d| d.severity == DiagnosticSeverity::Error)
        .unwrap();
    assert_eq!(first.phase, DiagnosticPhase::Analyzer);
}

/// `error_semantic2.sol`: `let x` declared twice in the same scope.
/// Analyzer's `add_entry` rejects the second insert, emits E1002.
#[test]
fn semantic2_redefinition_returns_diagnostic() {
    let source = read_fixture("error_semantic2");
    let result = analyze_source(&source);

    assert!(result.has_errors(), "expected analyzer error");
    let codes_seen: Vec<&str> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .map(|d| d.code)
        .collect();
    assert!(
        codes_seen.contains(&codes::SEMA_REDEFINITION),
        "expected E1002 (redefinition); got {codes_seen:?}",
    );
}

/// `error_semantic3.sol`: `function foo` declared twice at top
/// level. Pass 1 of `run()` registers both via `add_entry`; the
/// second call emits E1002.
#[test]
fn semantic3_function_redefinition_returns_diagnostic() {
    let source = read_fixture("error_semantic3");
    let result = analyze_source(&source);

    assert!(result.has_errors(), "expected analyzer error");
    let codes_seen: Vec<&str> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .map(|d| d.code)
        .collect();
    assert!(
        codes_seen.contains(&codes::SEMA_REDEFINITION),
        "expected E1002 (redefinition); got {codes_seen:?}",
    );
}

/// Smoke: parse_source on every positive fixture still returns no
/// errors. Earlier B.2 conversions shouldn't have introduced any
/// new false positives.
#[test]
fn positive_fixtures_still_parse_cleanly() {
    let positive_fixtures = &[
        "fwdecl",
        "gemini_long",
        "jj_comp",
        "jjsi",
        "largemini",
        "retest",
        "s1",
        // s2 skipped — analyzer rejects `str + str` as a type
        //   mismatch (E1006) per its current rules. Not a parse
        //   failure (so parse_source is Ok), but analyze_source
        //   would also flag it; leave out of the positive corpus.
        "test_arith",
        "test_array",
        "test_control",
        "test_edge",
        "test_func",
        "test_scope",
        "test_struct",
    ];
    for name in positive_fixtures {
        let path = format!("tests/fixtures/positive/{name}.sol");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        let result = parse_source(&source);
        assert!(
            !result.has_errors(),
            "{name}: parse_source unexpectedly produced errors: {:#?}",
            result.diagnostics,
        );
        assert!(
            result.value.is_some(),
            "{name}: parse_source returned no value",
        );
    }
}

/// Smoke: analyze_source on every positive fixture stays clean.
/// Verifies c4's emit+return None conversion didn't accidentally
/// inject diagnostics into well-formed programs.
#[test]
fn positive_fixtures_analyze_cleanly() {
    let positive_fixtures = &[
        "fwdecl",
        "gemini_long",
        "jj_comp",
        "jjsi",
        "largemini",
        "retest",
        "s1",
        "test_arith",
        "test_array",
        "test_control",
        "test_edge",
        "test_func",
        "test_scope",
        "test_struct",
    ];
    for name in positive_fixtures {
        let path = format!("tests/fixtures/positive/{name}.sol");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        let result = analyze_source(&source);
        assert!(
            !result.has_errors(),
            "{name}: analyze_source unexpectedly produced errors: {:#?}",
            result.diagnostics,
        );
    }
}
