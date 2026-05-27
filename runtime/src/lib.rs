//! `solflow_runtime` — the canonical SOL bytecode VM, browser-safe.
//!
//! See `UPSTREAM.md` for provenance and the catalog of edits
//! relative to the upstream sibling workspace.

pub mod error;
pub mod vm;

pub use error::RunError;
pub use vm::{HeapObject, VM};

use solflow_compiler::bytecode::Inst;

/// One-shot run result. Captures everything the editor needs to
/// render a canonical-simulation execution: stdout buffer, the
/// top-of-stack value at program exit, any structured runtime
/// error, and the executed-instruction count.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Top-of-stack value at program termination. The
    /// interpretation (int / float-bits / heap-ref) depends on
    /// the program's declared return type; the editor side
    /// decides how to render it.
    pub return_value: u64,
    /// Lines captured from `print` instructions, in order.
    pub output: Vec<String>,
    /// Number of `step()` calls made before termination.
    pub steps: usize,
    /// Structured runtime error if execution didn't complete
    /// successfully. When present, `return_value` is 0.
    pub error: Option<RunError>,
    /// When tracing was enabled at `with_trace=true`, the inst_ptr
    /// of each executed instruction in execution order. Empty
    /// when tracing was disabled (default).
    pub trace: Vec<usize>,
    /// True when `trace` hit its cap and recording stopped early.
    /// Surfaces "trace truncated at N steps" in the editor.
    pub trace_truncated: bool,
    /// When `error` is `Some`, the inst_ptr of the offending
    /// instruction. The bridge uses this to attach a source span
    /// to the runtime error so the editor can scroll to the
    /// failure site. `None` when run completed without error or
    /// when capturing failed (shouldn't happen in practice).
    pub error_inst_ptr: Option<usize>,
}

/// Options for `run_program`. Built explicitly rather than as
/// positional args so future additions don't break callers.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Override the VM's default `step_limit` (1M). `None` keeps
    /// the default.
    pub step_limit: Option<usize>,
    /// When `true`, the VM records `trace` + `error_inst_ptr`.
    /// Off by default — the editor's normal "Run" path enables
    /// it to populate the execution-trace UI, while plain
    /// `cargo test` paths leave it off for speed.
    pub trace: bool,
}

/// Run a compiled program to completion (or to first runtime
/// error). The program is expected to end with `Inst::Call("start", 0)`
/// so execution naturally drains into the start function — the
/// compiler emits this trailing call automatically, so callers
/// can just pass `CompiledProgram.bytecode` directly.
pub fn run_program(program: &[Inst], step_limit: Option<usize>) -> RunOutcome {
    run_program_with(program, RunOptions { step_limit, trace: false })
}

/// Run a compiled program with explicit options (B.D2 c42).
/// Use `run_program` for the simple no-trace path.
pub fn run_program_with(program: &[Inst], opts: RunOptions) -> RunOutcome {
    let mut vm = VM::from(program);
    if let Some(n) = opts.step_limit {
        vm.step_limit = n;
    }
    if opts.trace {
        vm = vm.with_trace(None);
    }
    match vm.run() {
        Ok(return_value) => RunOutcome {
            return_value,
            output: vm.output,
            steps: vm.steps,
            error: None,
            trace: vm.trace,
            trace_truncated: vm.trace_truncated,
            error_inst_ptr: None,
        },
        Err(e) => RunOutcome {
            return_value: 0,
            output: vm.output,
            steps: vm.steps,
            error: Some(e),
            trace: vm.trace,
            trace_truncated: vm.trace_truncated,
            error_inst_ptr: vm.error_inst_ptr,
        },
    }
}
