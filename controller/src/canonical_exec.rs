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
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::runtime::Handle;
use tracing::info;

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
    handle: Handle,
) -> CanonicalOutcome {
    // Registered connectors: module name -> base HTTP URL, read from
    // the SOLFLOW_CONNECTORS env (a JSON object). External Actions
    // whose module is registered execute for real; the rest stay
    // honestly blocked.
    let connectors = load_connectors();
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
            Ok(StepResult::RemoteCall { capability, params }) => {
                // External Action. Resolve the module to a registered
                // connector endpoint and execute it for real. A module
                // with no registered endpoint stays honestly blocked.
                let (module, func) = split_capability(&capability);
                // Resolve the module to an endpoint; fall back to a "*"
                // wildcard connector that catches every Action regardless
                // of module name (handy for demos / a single gateway).
                let endpoint = connectors.get(&module).or_else(|| connectors.get("*"));
                match endpoint {
                    Some(base_url) => {
                        info!("action {capability}: calling connector {base_url}");
                        match invoke_connector(&handle, base_url, &module, &func, &params) {
                            Ok(result) => {
                                if let Err(e) = exec.resolve_remote_call(&capability, result) {
                                    error = Some(classify(e));
                                    break;
                                }
                                // resumed — keep stepping the workflow
                            }
                            Err(message) => {
                                error = Some(RuntimeErrorView::ExtCallFailed {
                                    connector: module,
                                    function_name: func,
                                    message,
                                });
                                break;
                            }
                        }
                    }
                    None => {
                        info!(
                            "action {capability}: no connector registered (module `{module}`, no `*` fallback) — blocked"
                        );
                        error = Some(RuntimeErrorView::ExtCallBlocked {
                            function_name: capability,
                            url: String::new(),
                        });
                        break;
                    }
                }
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


/// Read the connector registry from the `SOLFLOW_CONNECTORS` env var.
/// Format: a JSON object mapping a module name to its base HTTP URL,
/// e.g. `{"weather_station":"http://127.0.0.1:8088"}`. Missing /
/// empty / malformed yields an empty registry (all Actions block).
fn load_connectors() -> HashMap<String, String> {
    match std::env::var("SOLFLOW_CONNECTORS") {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => HashMap::new(),
    }
}

/// Split a capability into (module, function). Handles both the
/// `module::func` namespace form and the `module.func` member form.
fn split_capability(cap: &str) -> (String, String) {
    if let Some(i) = cap.rfind("::") {
        (cap[..i].to_string(), cap[i + 2..].to_string())
    } else if let Some(i) = cap.rfind('.') {
        (cap[..i].to_string(), cap[i + 1..].to_string())
    } else {
        (cap.to_string(), String::new())
    }
}

/// Invoke a connector endpoint over HTTP and return the response as a
/// SOL `Value`. POSTs `{"function": <func>, "params": <params-json>}`
/// to the module's base URL and parses the JSON response. Runs on the
/// async runtime via `handle.block_on` (we are on a blocking thread).
fn invoke_connector(
    handle: &Handle,
    base_url: &str,
    module: &str,
    func: &str,
    params: &Value,
) -> Result<Value, String> {
    let body = serde_json::json!({
        "module": module,
        "function": func,
        "params": value_to_json(params),
    });
    let url = base_url.to_string();
    let resp: serde_json::Value = handle.block_on(async move {
        let client = reqwest::Client::new();
        let r = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("connector request failed: {e}"))?;
        let status = r.status();
        if !status.is_success() {
            return Err(format!("connector returned HTTP {}", status.as_u16()));
        }
        r.json::<serde_json::Value>()
            .await
            .map_err(|e| format!("connector response was not JSON: {e}"))
    })?;
    Ok(json_to_value(resp))
}

/// Convert a SOL `Value` to plain JSON for a connector request body.
fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) => J::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        Value::Char(c) => J::String(c.to_string()),
        Value::Str(s) => J::String(s.clone()),
        Value::Array(a) => J::Array(a.iter().map(value_to_json).collect()),
        Value::Struct(m) => {
            J::Object(m.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect())
        }
        Value::Enum(e, var) => J::String(format!("{e}::{var}")),
        Value::Unit => J::Null,
        Value::Module(p) => J::String(p.clone()),
        Value::RemoteRef { id, .. } => J::String(id.clone()),
    }
}

/// Convert a connector's plain JSON response back into a SOL `Value`.
fn json_to_value(j: serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::Unit,
        J::Bool(b) => Value::Bool(b),
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Unit
            }
        }
        J::String(s) => Value::Str(s),
        J::Array(a) => Value::Array(a.into_iter().map(json_to_value).collect()),
        J::Object(m) => Value::Struct(m.into_iter().map(|(k, v)| (k, json_to_value(v))).collect()),
    }
}
