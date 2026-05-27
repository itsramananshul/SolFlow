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
}

/// Run a compiled program to completion (or to first runtime
/// error). The program is expected to end with `Inst::Call("start", 0)`
/// so execution naturally drains into the start function — the
/// compiler emits this trailing call automatically, so callers
/// can just pass `CompiledProgram.bytecode` directly.
///
/// Step limit: configurable via the second arg; pass `None` for
/// the VM's default (1M steps).
pub fn run_program(program: &[Inst], step_limit: Option<usize>) -> RunOutcome {
    let mut vm = VM::from(program);
    if let Some(n) = step_limit {
        vm.step_limit = n;
    }
    match vm.run() {
        Ok(return_value) => RunOutcome {
            return_value,
            output: vm.output,
            steps: vm.steps,
            error: None,
        },
        Err(e) => RunOutcome {
            return_value: 0,
            output: vm.output,
            steps: vm.steps,
            error: Some(e),
        },
    }
}
