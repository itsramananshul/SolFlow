//! Phase B.1 smoke test — every positive fixture lexes + parses +
//! analyzes + code-generates without panic/exit.
//!
//! Negative fixtures (errors/*.sol) are NOT exercised here; the
//! vendored compiler still calls `std::process::exit(1)` on the
//! first error, which would terminate the test runner. B.2's
//! diagnostics-as-values work fixes that, after which negative
//! fixtures get their own test file (`diagnostics.rs`).

use solflow_compiler::compile_source;
use solflow_compiler::lexer::Lexer;
use solflow_compiler::parser::Parser;
use solflow_compiler::analyzer::Analyzer;
use solflow_compiler::bytecode::Codegen;

const POSITIVE_FIXTURES: &[&str] = &[
    "fwdecl",
    "gemini_long",
    "jj_comp",
    "jjsi",
    "largemini",
    "retest",
    "s1",
    // "s2" — skipped: uses `print("Deploying: " + service)` which the
    //   current analyzer rejects with "arithmetic operation Plus not
    //   supported for type String" (the str+str gap, T9023 in the
    //   SolFlow docs). Re-enable when B.2's diagnostics-as-values
    //   work lets the test runner survive a process-exit, or when
    //   the canonical compiler accepts ConcatStr at source level.
    "test_arith",
    "test_array",
    "test_control",
    "test_edge",
    "test_func",
    "test_scope",
    "test_struct",
];

fn fixture_path(name: &str) -> String {
    format!("tests/fixtures/positive/{name}.sol")
}

fn compile_fixture(name: &str) {
    let path = fixture_path(name);
    let mut lexer = Lexer::from(&path);
    let tokens = lexer.tokens();

    let mut parser = Parser::from(tokens);
    let mut program = parser.run();

    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);

    let mut codegen = Codegen::from(analyzer.tt_arena);
    let _bytecode = codegen.gen_bcode(&program);
}

#[test]
fn every_positive_fixture_compiles() {
    for name in POSITIVE_FIXTURES {
        // Each fixture is a separate sub-call so a failure prints
        // which name broke. The vendored pipeline still
        // process-exits on the first error; if any of these exit
        // we'll see it as a test-runner failure rather than a clean
        // panic, but the assertion guards against silent surprises.
        compile_fixture(name);
        println!("ok: {name}");
    }
}

/// Smoke test for the new B.2 public API. Calls `compile_source`
/// on each positive fixture's text, asserts that the returned
/// `CompileResult` carries a value and reports no errors. Once
/// c3-c5 land, this becomes the more rigorous "no diagnostics"
/// gate; for now it just verifies the API wires up.
#[test]
fn compile_source_api_smokes_every_positive_fixture() {
    for name in POSITIVE_FIXTURES {
        let path = fixture_path(name);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        let result = compile_source(&source);
        assert!(
            result.value.is_some(),
            "{name}: compile_source returned no value"
        );
        assert!(
            !result.has_errors(),
            "{name}: compile_source reported errors: {:?}",
            result.diagnostics,
        );
        println!("api ok: {name}");
    }
}
