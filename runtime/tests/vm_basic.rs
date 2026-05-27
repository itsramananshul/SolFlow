//! Native tests for the canonical SOL VM.
//!
//! These compile + run small SOL programs end-to-end via
//! `solflow_compiler::compile_source` + `solflow_runtime::run_program`,
//! exactly the path the WASM bridge uses. No browser needed.

use solflow_compiler::compile_source;
use solflow_runtime::{run_program, RunError};

fn run(source: &str) -> solflow_runtime::RunOutcome {
    let compiled = compile_source(source);
    let cp = compiled.value.unwrap_or_else(|| {
        panic!("compile failed: {:#?}", compiled.diagnostics);
    });
    run_program(&cp.bytecode, None)
}

#[test]
fn return_integer_literal() {
    let out = run("function start() -> int { return 42; }");
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.return_value, 42);
    assert!(out.output.is_empty());
    assert!(out.steps > 0);
}

#[test]
fn arithmetic() {
    let out = run("function start() -> int { return ((10 + 5) * 2) - 3; }");
    assert!(out.error.is_none(), "{:?}", out.error);
    // (10+5)*2 - 3 = 27
    assert_eq!(out.return_value as i64, 27);
}

#[test]
fn print_integer() {
    let out = run(
        "function start() -> int { print(7); print(11); return 0; }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.output, vec!["7".to_string(), "11".to_string()]);
}

#[test]
fn print_string() {
    let out = run(
        r#"function start() -> int { print("hello"); print("world"); return 0; }"#,
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(
        out.output,
        vec!["hello".to_string(), "world".to_string()],
    );
}

#[test]
fn branch_true_path() {
    let out = run(
        "function start() -> int { if (1 == 1) { return 100; } return 200; }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.return_value as i64, 100);
}

#[test]
fn branch_false_path() {
    let out = run(
        "function start() -> int { if (1 == 2) { return 100; } return 200; }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.return_value as i64, 200);
}

#[test]
fn while_loop_counts() {
    let out = run(
        "function start() -> int { let x: int = 0; while (x < 5) { x = x + 1; } return x; }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.return_value as i64, 5);
}

#[test]
fn function_call_returns_value() {
    let out = run(
        "function add(a: int, b: int) -> int { return a + b; }
         function start() -> int { return add(3, 4); }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.return_value as i64, 7);
}

#[test]
fn division_by_zero_is_structured_error() {
    let out = run(
        "function start() -> int { return 10 / 0; }",
    );
    match out.error {
        Some(RunError::DivByZero) => {}
        other => panic!("expected DivByZero; got {other:?}"),
    }
    // Steps stopped before the loop counter ran away.
    assert!(out.steps < 100);
}

#[test]
fn step_limit_enforced_on_infinite_loop() {
    let source = "function start() -> int { while (1 == 1) { } return 0; }";
    let compiled = compile_source(source);
    let cp = compiled.value.expect("should compile");
    let out = run_program(&cp.bytecode, Some(1_000));
    match out.error {
        Some(RunError::StepLimit { limit }) => assert_eq!(limit, 1_000),
        other => panic!("expected StepLimit; got {other:?}"),
    }
}

#[test]
fn ext_call_is_blocked_not_panicked() {
    // ext function declared but no endpoint configured —
    // compiler refuses to emit bytecode for this case (E0051),
    // so we can't reach ExtCall via compile_source. The blocked
    // behavior is exercised directly:
    //
    //   - SerializeRequest is happy in-browser (pure JSON)
    //   - ExtCall would attempt network; should block
    //
    // We construct minimal hand-crafted bytecode that hits ExtCall:
    use solflow_compiler::bytecode::Inst;
    use solflow_compiler::parser::{Ast, Type};
    let program = vec![
        Inst::PushConst(Ast::ExprString("my_fn".to_string())),
        Inst::PushConst(Ast::ExprString("http://example.com".to_string())),
        Inst::ExtCall(vec![], Box::new(Type::Integer)),
    ];
    let out = run_program(&program, None);
    match out.error {
        Some(RunError::ExtCallBlocked { function_name, url }) => {
            assert_eq!(function_name, "my_fn");
            assert_eq!(url, "http://example.com");
        }
        other => panic!("expected ExtCallBlocked; got {other:?}"),
    }
}

/// B.11 c32 regression: GetField on an OOB index used to panic
/// (uncaught), bringing down the WASM boundary as an ICE. After
/// the hardening sweep it produces a structured runtime error.
#[test]
fn field_index_out_of_bounds_is_structured_error() {
    use solflow_compiler::bytecode::Inst;
    use solflow_compiler::parser::Ast;
    // Hand-crafted bytecode: allocate a 2-field struct, ask for
    // field 99. Skip codegen entirely so we have direct control.
    let program = vec![
        Inst::PushConst(Ast::ExprInteger(10)),  // field 0 value
        Inst::PushConst(Ast::ExprInteger(20)),  // field 1 value
        Inst::NewStruct(2),                     // -> struct ref
        Inst::GetField(99),                     // OOB
    ];
    let out = run_program(&program, None);
    match out.error {
        Some(RunError::IndexOutOfBounds { index, length }) => {
            assert_eq!(index, 99);
            assert_eq!(length, 2);
        }
        other => panic!("expected IndexOutOfBounds; got {other:?}"),
    }
}

#[test]
fn set_field_index_out_of_bounds_is_structured_error() {
    use solflow_compiler::bytecode::Inst;
    use solflow_compiler::parser::Ast;
    // SetField pops struct_ref first (top), then value. So the
    // stack just before SetField must be `[..., value, struct_ref]`:
    //   push 99               -> [99]                  (value)
    //   push 10/20 NewStruct  -> [99, refS]            (struct on top)
    //   SetField(50)          -> idx 50 of 2-field struct = OOB
    let program = vec![
        Inst::PushConst(Ast::ExprInteger(99)),  // value to set
        Inst::PushConst(Ast::ExprInteger(10)),  // field 0 init
        Inst::PushConst(Ast::ExprInteger(20)),  // field 1 init
        Inst::NewStruct(2),                     // pops 20, 10; pushes refS
        Inst::SetField(50),                     // pops refS, then 99
    ];
    let out = run_program(&program, None);
    match out.error {
        Some(RunError::IndexOutOfBounds { index, length }) => {
            assert_eq!(index, 50);
            assert_eq!(length, 2);
        }
        other => panic!("expected IndexOutOfBounds; got {other:?}"),
    }
}

#[test]
fn cross_function_call_with_print() {
    let out = run(
        r#"function greet(who: str) -> int {
            print(who);
            return 0;
        }
        function start() -> int {
            greet("alice");
            greet("bob");
            return 0;
        }"#,
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(
        out.output,
        vec!["alice".to_string(), "bob".to_string()],
    );
}
