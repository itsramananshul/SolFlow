//! Canonical SOL execution for the controller.
//!
//! The editor submits SOL *source* (carried in the workflow's
//! stored blob); the controller compiles and runs it through the
//! canonical `openprem-sol-v2` VM. This is the single execution
//! path: production controller runs share the exact semantics of
//! the browser sim, and there is no cross-crate bytecode-format
//! coupling between the editor's WASM compiler and the controller.
//!
//! The canonical VM is a pull-based stepper: `WorkflowExecutor`
//! compiles the source, then `step(budget)` runs up to `budget`
//! statements and returns a `StepResult`:
//!   - `Completed(value)` — the workflow returned.
//!   - `Yielded(n)`       — ran `n` statements, keep going.
//!   - `RemoteCall { .. }` — hit an external Action.
//!   - `Failed(msg)`      — runtime error.
//! `print` output accumulates in a thread-local buffer drained
//! with `take_output()` after the run.

use openprem_sol_v2::vm::take_output;
use openprem_sol_v2::{Parser, StepResult, TopLevel, Value, WorkflowExecutor};
use solflow_host_spec::RuntimeErrorView;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Statements executed per `step()` call before we re-check the
/// cancel / timeout flags. Small enough that cancel latency stays
/// sub-millisecond, large enough that the per-call overhead is
/// negligible for compute-heavy workflows.
const STEP_BUDGET: u64 = 10_000;

/// Outcome of a canonical run, shaped so `execute_run` can map it
/// straight onto `RunOutput` + `RunStatus`.
pub struct CanonicalOutcome {
    /// Return value, narrowed to `i64` (Int / Bool). Non-integer
    /// returns surface as `None`, matching the browser sim's
    /// `RunResult.return_value` contract.
    pub return_value: Option<i64>,
    /// Captured `print` lines, in order.
    pub output: Vec<String>,
    /// Total statements executed (approximate; counts `Yielded`).
    pub steps: u64,
    /// `Some` iff the run did not complete cleanly.
    pub error: Option<RuntimeErrorView>,
}

/// The entry workflow name to run: the first `workflow "name" {}`
/// declaration in the source. `None` when the source has no
/// workflow (a pure library) or fails to parse.
pub fn first_workflow_name(source: &str) -> Option<String> {
    let prog = Parser::new(source).parse().ok()?;
    prog.items.iter().find_map(|it| {
        if let TopLevel::Workflow(w) = it {
            Some(w.name.clone())
        } else {
            None
        }
    })
}

/// Run `workflow_name` from `source` to completion (or error),
/// polling `user_cancel` / `timeout` between step batches. Runs
/// synchronously; the caller drives it on a blocking thread.
pub fn run_canonical(
    source: &str,
    workflow_name: &str,
    step_limit: u64,
    user_cancel: Arc<AtomicBool>,
    timeout: Arc<AtomicBool>,
) -> CanonicalOutcome {
    // Clear any print residue left on this pooled blocking thread
    // by a previous run (the output buffer is thread-local).
    let _ = take_output();

    let mut exec = match WorkflowExecutor::new(source, workflow_name) {
        Ok(e) => e,
        Err(message) => {
            return CanonicalOutcome {
                return_value: None,
                output: drain_output(),
                steps: 0,
                error: Some(RuntimeErrorView::ExtCallFailed {
                    connector: "(controller)".into(),
                    function_name: "(compile)".into(),
                    message,
                }),
            };
        }
    };

    let mut total_steps: u64 = 0;
    let mut error: Option<RuntimeErrorView> = None;
    let mut return_value: Option<i64> = None;

    loop {
        if user_cancel.load(Ordering::Relaxed) || timeout.load(Ordering::Relaxed) {
            error = Some(RuntimeErrorView::Cancelled);
            break;
        }
        match exec.step(STEP_BUDGET) {
            Ok(StepResult::Completed(value)) => {
                return_value = value_to_return(&value);
                break;
            }
            Ok(StepResult::Yielded(n)) => {
                total_steps = total_steps.saturating_add(n);
                if total_steps >= step_limit {
                    error = Some(RuntimeErrorView::StepLimit {
                        limit: step_limit as usize,
                    });
                    break;
                }
            }
            Ok(StepResult::RemoteCall { capability, .. }) => {
                // External Action. No connector is bound to the
                // canonical capability model on this local path,
                // so we surface the same honest "blocked" signal
                // the browser sim does rather than fake a result.
                error = Some(RuntimeErrorView::ExtCallBlocked {
                    function_name: capability,
                    url: String::new(),
                });
                break;
            }
            Ok(StepResult::Failed(message)) => {
                error = Some(classify(message));
                break;
            }
            Err(message) => {
                error = Some(classify(message));
                break;
            }
        }
    }

    CanonicalOutcome {
        return_value: if error.is_some() { None } else { return_value },
        output: drain_output(),
        steps: total_steps,
        error,
    }
}

/// Drain the thread-local print buffer into lines.
fn drain_output() -> Vec<String> {
    let buf = take_output();
    if buf.is_empty() {
        return Vec::new();
    }
    buf.lines().map(|s| s.to_string()).collect()
}

/// Narrow a return `Value` to the wire's `Option<i64>`. Int and
/// Bool map through; everything else (Float, Str, Struct, Unit)
/// is `None`, matching the browser sim.
fn value_to_return(v: &Value) -> Option<i64> {
    match v {
        Value::Int(i) => Some(*i),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

/// Best-effort classification of a VM error string into the
/// wire-stable `RuntimeErrorView`.
fn classify(message: String) -> RuntimeErrorView {
    let l = message.to_lowercase();
    if l.contains("divide by zero") || l.contains("division by zero") || l.contains("div by zero") {
        RuntimeErrorView::DivByZero
    } else if l.contains("stack underflow") {
        RuntimeErrorView::StackUnderflow
    } else {
        RuntimeErrorView::ExtCallFailed {
            connector: "(vm)".into(),
            function_name: "(runtime)".into(),
            message,
        }
    }
}
