//! Native tests for the canonical SOL VM.
//!
//! These compile + run small SOL programs end-to-end via
//! `solflow_compiler::compile_source` + `solflow_runtime::run_program`,
//! exactly the path the WASM bridge uses. No browser needed.

use solflow_compiler::compile_source;
use solflow_runtime::{run_program, run_program_with, RunError, RunOptions};

fn run(source: &str) -> solflow_runtime::RunOutcome {
    let compiled = compile_source(source);
    let cp = compiled.value.unwrap_or_else(|| {
        panic!("compile failed: {:#?}", compiled.diagnostics);
    });
    run_program(&cp.bytecode, None)
}

fn run_traced(source: &str) -> solflow_runtime::RunOutcome {
    let compiled = compile_source(source);
    let cp = compiled.value.unwrap_or_else(|| {
        panic!("compile failed: {:#?}", compiled.diagnostics);
    });
    run_program_with(&cp.bytecode, RunOptions { step_limit: None, trace: true, ext_call_handler: None })
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

/// Phase C C.4 c76: with an `ExtCallHandler` installed, the VM
/// dispatches to the handler and pushes its returned value back
/// onto the stack instead of erroring with `ExtCallBlocked`.
#[test]
fn ext_call_with_handler_returns_value() {
    use solflow_compiler::bytecode::Inst;
    use solflow_compiler::parser::{Ast, Type};
    use solflow_runtime::{
        run_program_with, ExtCallContext, ExtCallError, ExtCallHandler,
        ExtCallValue, RunOptions,
    };
    use std::sync::{Arc, Mutex};

    /// Echo handler: returns the integer it received, scaled by 10.
    struct EchoHandler {
        seen: Mutex<Vec<(String, String, Vec<ExtCallValue>)>>,
    }
    impl ExtCallHandler for EchoHandler {
        fn handle(
            &self,
            ctx: ExtCallContext<'_>,
        ) -> Result<ExtCallValue, ExtCallError> {
            self.seen.lock().unwrap().push((
                ctx.function_name.to_string(),
                ctx.url.to_string(),
                ctx.args.to_vec(),
            ));
            match ctx.args.first() {
                Some(ExtCallValue::Int(n)) => Ok(ExtCallValue::Int(n * 10)),
                _ => Err(ExtCallError::failed(
                    "echo",
                    ctx.function_name,
                    "expected one int arg",
                )),
            }
        }
    }

    let handler = Arc::new(EchoHandler { seen: Mutex::new(Vec::new()) });
    let program = vec![
        Inst::PushConst(Ast::ExprInteger(7)),
        Inst::PushConst(Ast::ExprString("scale".to_string())),
        Inst::PushConst(Ast::ExprString("connector://echo".to_string())),
        Inst::ExtCall(vec![Type::Integer], Box::new(Type::Integer)),
        Inst::Ret,
    ];
    let opts = RunOptions {
        step_limit: None,
        trace: false,
        ext_call_handler: Some(handler.clone()),
    };
    let out = run_program_with(&program, opts);
    assert!(out.error.is_none(), "expected clean run, got {:?}", out.error);
    // Top-of-stack (return_value) should be 7 * 10 = 70.
    assert_eq!(out.return_value as i64, 70);
    let seen = handler.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "scale");
    assert_eq!(seen[0].1, "connector://echo");
    assert!(matches!(seen[0].2[0], ExtCallValue::Int(7)));
}

/// Phase C C.4 c76: a handler that returns `Err` surfaces as
/// `RunError::ExtCallFailed` (not the blocked variant).
#[test]
fn ext_call_handler_error_surfaces_as_extcall_failed() {
    use solflow_compiler::bytecode::Inst;
    use solflow_compiler::parser::{Ast, Type};
    use solflow_runtime::{
        run_program_with, ExtCallContext, ExtCallError, ExtCallHandler,
        ExtCallValue, RunOptions,
    };
    use std::sync::Arc;

    struct FailingHandler;
    impl ExtCallHandler for FailingHandler {
        fn handle(
            &self,
            ctx: ExtCallContext<'_>,
        ) -> Result<ExtCallValue, ExtCallError> {
            Err(ExtCallError::failed("test", ctx.function_name, "boom"))
        }
    }
    let program = vec![
        Inst::PushConst(Ast::ExprString("greet".to_string())),
        Inst::PushConst(Ast::ExprString("connector://test".to_string())),
        Inst::ExtCall(vec![], Box::new(Type::Integer)),
        Inst::Ret,
    ];
    let opts = RunOptions {
        step_limit: None,
        trace: false,
        ext_call_handler: Some(Arc::new(FailingHandler)),
    };
    let out = run_program_with(&program, opts);
    match out.error {
        Some(RunError::ExtCallFailed { connector, function_name, message }) => {
            assert_eq!(connector, "test");
            assert_eq!(function_name, "greet");
            assert!(message.contains("boom"));
        }
        other => panic!("expected ExtCallFailed; got {other:?}"),
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

// =============================================================
//  B.D c42 — execution trace + error inst_ptr
// =============================================================

#[test]
fn trace_is_empty_when_disabled() {
    let out = run("function start() -> int { return 42; }");
    assert!(out.error.is_none());
    // Default `run_program` doesn't enable tracing.
    assert!(out.trace.is_empty(), "trace should be off by default");
    assert!(!out.trace_truncated);
    assert!(out.error_inst_ptr.is_none());
}

#[test]
fn trace_records_inst_ptrs_when_enabled() {
    let out = run_traced(
        "function start() -> int { let x: int = 7; return x; }",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(
        !out.trace.is_empty(),
        "trace should be populated when enabled",
    );
    // Steps + trace entries are 1:1 modulo the synthesized end-
    // of-program step (steps counts that one; trace doesn't).
    // Looser invariant: trace is non-empty and bounded by steps.
    assert!(out.trace.len() <= out.steps);
    assert!(!out.trace_truncated);
}

#[test]
fn trace_truncates_at_default_cap_for_infinite_loop() {
    // Hits step_limit before the trace fills (default trace cap
    // 10k; step_limit 1M means we'd cap trace first).
    let source = "function start() -> int { while (1 == 1) { } return 0; }";
    let compiled = compile_source(source);
    let cp = compiled.value.expect("should compile");
    let out = run_program_with(
        &cp.bytecode,
        RunOptions { step_limit: Some(100_000), trace: true, ext_call_handler: None },
    );
    match out.error {
        Some(RunError::StepLimit { .. }) => {}
        other => panic!("expected StepLimit; got {other:?}"),
    }
    assert!(out.trace_truncated, "should have truncated at cap");
}

#[test]
fn error_inst_ptr_captured_on_runtime_error() {
    let out = run_traced("function start() -> int { return 10 / 0; }");
    match out.error {
        Some(RunError::DivByZero) => {}
        other => panic!("expected DivByZero; got {other:?}"),
    }
    assert!(
        out.error_inst_ptr.is_some(),
        "error_inst_ptr should be set on runtime error",
    );
    // The IntDiv instruction is somewhere in the middle of the
    // program; just assert it's a valid index, not the exact value.
    let ip = out.error_inst_ptr.unwrap();
    assert!(ip > 0, "error inst_ptr {ip} should be after some setup");
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
