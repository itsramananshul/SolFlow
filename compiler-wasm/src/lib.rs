//! wasm-bindgen bridge over the canonical SOL crate (openprem-sol-v2).
//! Keeps SolFlow's stable JSON envelope contract (see src/compiler/types.ts).

use std::panic;
use std::sync::Once;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use openprem_sol_v2::ast::{Program, TopLevel};
use openprem_sol_v2::analysis::analyze_workflow;
use openprem_sol_v2::vm::{take_output, TraceEvent, TraceKind};
use openprem_sol_v2::{Compiler, Parser, StepResult, Value, WorkflowExecutor};

static PANIC_HOOK_INIT: Once = Once::new();
fn install_panic_hook() { PANIC_HOOK_INIT.call_once(|| console_error_panic_hook::set_once()); }

#[derive(Serialize)]
struct Span { start: usize, end: usize }

#[derive(Serialize)]
struct Diag {
    severity: &'static str,
    phase: &'static str,
    code: &'static str,
    message: String,
    span: Option<Span>,
    related: Vec<()>,
    help: Option<String>,
}
fn diag(severity: &'static str, phase: &'static str, code: &'static str, message: String) -> Diag {
    Diag { severity, phase, code, message, span: None, related: vec![], help: None }
}
fn err(phase: &'static str, code: &'static str, message: String) -> Diag { diag("Error", phase, code, message) }

#[derive(Serialize)]
struct Envelope<V: Serialize> { ok: bool, value: Option<V>, diagnostics: Vec<Diag> }

fn ice(m: &str) -> String {
    let e = m.replace('\\', "\\\\").replace('"', "\\\"");
    format!(r#"{{"ok":false,"value":null,"diagnostics":[{{"severity":"Error","phase":"Internal","code":"ICE0001","message":"{e}","span":null,"related":[],"help":null}}],"run":null}}"#)
}
fn ok_env<V: Serialize>(value: V) -> String {
    serde_json::to_string(&Envelope { ok: true, value: Some(value), diagnostics: vec![] }).unwrap_or_else(|e| ice(&e.to_string()))
}
fn err_env(d: Diag) -> String {
    serde_json::to_string(&Envelope::<()> { ok: false, value: None, diagnostics: vec![d] }).unwrap_or_else(|e| ice(&e.to_string()))
}
fn workflow_names(p: &Program) -> Vec<String> {
    p.items.iter().filter_map(|it| if let TopLevel::Workflow(w) = it { Some(w.name.clone()) } else { None }).collect()
}
fn guarded<F: FnOnce() -> String + std::panic::UnwindSafe>(f: F) -> String {
    install_panic_hook();
    panic::catch_unwind(f).unwrap_or_else(|_| ice("compiler panic"))
}

#[wasm_bindgen]
pub fn version() -> String { env!("CARGO_PKG_VERSION").to_string() }

#[wasm_bindgen]
pub fn parse_source_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || match Parser::new(&src).parse() {
        Ok(prog) => ok_env(prog), // value IS the Program (CompileEnvelope<Program>)
        Err(e) => err_env(err("Parser", "E_PARSE", e)),
    })
}

#[wasm_bindgen]
pub fn analyze_source_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || {
        let prog = match Parser::new(&src).parse() { Ok(p) => p, Err(e) => return err_env(err("Parser", "E_PARSE", e)) };
        #[derive(Serialize)]
        struct WfA { name: String, capabilities: Vec<String>, imported_modules: Vec<String> }
        #[derive(Serialize)]
        struct V { program: Program, workflows: Vec<WfA> }
        let mut wfs = vec![];
        for n in workflow_names(&prog) {
            if let Ok(a) = analyze_workflow(&src, &n) {
                wfs.push(WfA { name: a.workflow_name, capabilities: a.capabilities, imported_modules: a.imported_modules });
            }
        }
        ok_env(V { program: prog, workflows: wfs })
    })
}

#[wasm_bindgen]
pub fn compile_source_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || {
        let prog = match Parser::new(&src).parse() { Ok(p) => p, Err(e) => return err_env(err("Parser", "E_PARSE", e)) };
        match Compiler::new().compile(&prog) {
            Ok(chunk) => {
                #[derive(Serialize)]
                struct V { program: Program, instruction_count: usize }
                ok_env(V { program: prog, instruction_count: chunk.instructions.len() })
            }
            Err(e) => err_env(err("Codegen", "E_CODEGEN", e)),
        }
    })
}

#[wasm_bindgen]
pub fn format_source_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || match openprem_sol_v2::format_source(&src) {
        Ok(formatted) => {
            #[derive(Serialize)]
            struct V { source: String }
            ok_env(V { source: formatted })
        }
        Err(e) => err_env(err("Parser", "E_PARSE", e)),
    })
}

#[wasm_bindgen]
pub fn compile_for_wire_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || {
        let prog = match Parser::new(&src).parse() { Ok(p) => p, Err(e) => return err_env(err("Parser", "E_PARSE", e)) };
        match Compiler::new().compile(&prog) {
            Ok(chunk) => ok_env(chunk),
            Err(e) => err_env(err("Codegen", "E_CODEGEN", e)),
        }
    })
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum RtErr {
    ExtCallBlocked { function_name: String, url: String },
    StepLimit { limit: u64 },
}

/// One execution-trace step in the run envelope. `kind` is one of
/// `"stmt" | "call" | "return" | "error"`; `line` is the 1-based source line
/// the `span` starts on, for click-to-highlight in the UI.
#[derive(Serialize)]
struct TraceStep {
    step: u64,
    kind: &'static str,
    function: String,
    span: Option<Span>,
    line: Option<usize>,
    depth: usize,
    detail: Option<String>,
}

fn trace_kind_str(k: &TraceKind) -> &'static str {
    match k {
        TraceKind::Stmt => "stmt",
        TraceKind::Call => "call",
        TraceKind::Return => "return",
        TraceKind::Error => "error",
    }
}

/// 1-based line number that byte offset `off` falls on within `src`.
fn line_of(src: &str, off: usize) -> usize {
    1 + src.as_bytes().iter().take(off.min(src.len())).filter(|&&b| b == b'\n').count()
}

fn to_trace_steps(src: &str, events: &[TraceEvent]) -> Vec<TraceStep> {
    events
        .iter()
        .map(|e| {
            let span = e.span.map(|(start, end)| Span { start, end });
            let line = e.span.map(|(start, _)| line_of(src, start));
            TraceStep {
                step: e.step,
                kind: trace_kind_str(&e.kind),
                function: e.function.clone(),
                span,
                line,
                depth: e.depth,
                detail: e.detail.clone(),
            }
        })
        .collect()
}

#[wasm_bindgen]
pub fn run_source_json(source: &str) -> String {
    let src = source.to_string();
    guarded(move || {
        // parse + compile (for instruction_count and compile-stage diagnostics)
        let prog = match Parser::new(&src).parse() { Ok(p) => p, Err(e) => return run_fail(err("Parser", "E_PARSE", e)) };
        let instruction_count = match Compiler::new().compile(&prog) { Ok(c) => c.instructions.len(), Err(e) => return run_fail(err("Codegen", "E_CODEGEN", e)) };
        let name = match workflow_names(&prog).into_iter().next() { Some(n) => n, None => return run_fail(err("Analyzer", "E_NO_WORKFLOW", "no workflow declaration found".into())) };
        let mut exec = match WorkflowExecutor::new(&src, &name) { Ok(e) => e, Err(e) => return run_fail(err("Codegen", "E_CODEGEN", e)) };
        exec.enable_trace();

        let _ = take_output();
        let mut diagnostics: Vec<Diag> = vec![];
        let mut return_value = serde_json::Value::Null;
        let mut runtime_error: Option<RtErr> = None;
        const LIMIT: u64 = 1_000_000;
        let mut guard = 0u64;
        loop {
            guard += 1;
            if guard > 200_000 { runtime_error = Some(RtErr::StepLimit { limit: LIMIT }); break; }
            match exec.step(64) {
                Ok(StepResult::Completed(v)) => {
                    return_value = match v { Value::Int(i) => serde_json::json!(i), Value::Float(f) => serde_json::json!(f), _ => serde_json::Value::Null };
                    break;
                }
                Ok(StepResult::Yielded(_)) => continue,
                Ok(StepResult::RemoteCall { capability, .. }) => { runtime_error = Some(RtErr::ExtCallBlocked { function_name: capability, url: String::new() }); break; }
                Ok(StepResult::Failed(reason)) => { diagnostics.push(diag("Warning", "Runtime", "E_RUNTIME", reason)); break; }
                Err(e) => { diagnostics.push(diag("Warning", "Runtime", "E_RUNTIME", e)); break; }
            }
        }
        let raw = take_output();
        let output: Vec<String> = if raw.is_empty() { vec![] } else { raw.trim_end_matches('\n').split('\n').map(|s| s.to_string()).collect() };
        let steps = exec.save().step_count;

        // Real execution trace recorded by the VM as it ran.
        let trace = to_trace_steps(&src, exec.trace());
        let trace_truncated = exec.trace_truncated();
        // Point the run at the exact statement that failed, when one did:
        // the last Error event carries the failing statement's source span.
        let runtime_error_source_span = exec
            .trace()
            .iter()
            .rev()
            .find(|e| matches!(e.kind, TraceKind::Error))
            .and_then(|e| e.span)
            .map(|(start, end)| Span { start, end });

        #[derive(Serialize)]
        struct RunResult { return_value: serde_json::Value, output: Vec<String>, steps: u64, runtime_error: Option<RtErr>, runtime_error_source_span: Option<Span>, trace: Vec<TraceStep>, trace_truncated: bool }
        #[derive(Serialize)]
        struct RunEnvelope { ok: bool, value: Option<Ic>, diagnostics: Vec<Diag>, run: Option<RunResult> }
        #[derive(Serialize)]
        struct Ic { instruction_count: usize }

        let run = RunResult { return_value, output, steps, runtime_error, runtime_error_source_span, trace, trace_truncated };
        serde_json::to_string(&RunEnvelope { ok: true, value: Some(Ic { instruction_count }), diagnostics, run: Some(run) }).unwrap_or_else(|e| ice(&e.to_string()))
    })
}

// compile-stage failure shape for run_source_json: ok:false, run:null
fn run_fail(d: Diag) -> String {
    #[derive(Serialize)]
    struct RunEnvelope { ok: bool, value: Option<()>, diagnostics: Vec<Diag>, run: Option<()> }
    serde_json::to_string(&RunEnvelope { ok: false, value: None, diagnostics: vec![d], run: None }).unwrap_or_else(|e| ice(&e.to_string()))
}
