//! Negative-fixture diagnostics test.
//!
//! Every fixture in tests/fixtures/negative/ exercises a known
//! compile-time error. After B.2 c3 (lexer + parser conversion)
//! the parse-failing fixtures must return diagnostics through
//! `parse_source` instead of crashing the test runner.
//!
//! Semantic-error fixtures (`error_semantic*.sol`) are NOT yet
//! covered — they still trigger the analyzer's `process::exit(1)`
//! sites which land in c4. Likewise `error_runtime.sol` is a VM
//! panic that no compile-time call exercises.

use solflow_compiler::{
    codes, parse_source, DiagnosticPhase, DiagnosticSeverity,
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

/// Smoke: parse_source on every positive fixture still returns no
/// errors. The B.2 c3 conversion shouldn't have introduced any new
/// false positives.
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
        // s2 skipped — analyzer-side str+str rejection; not a parse
        //   failure (so parse_source returns Ok), but we'd still
        //   want it green once c4 lands. Hold for now.
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
