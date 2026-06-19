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

use openprem_sol_v2::vm::{take_output, TraceKind};
use openprem_sol_v2::{Parser, StepResult, TopLevel, Value, WorkflowExecutor};
use solflow_host_spec::{RuntimeErrorView, SourceSpan, TraceStep};
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

/// Cap on trace events persisted in a run record. The VM bounds its
/// in-memory trace at 50k, but the controller persists each run's trace
/// in SQLite and returns it on every `GET /runs/:id`, so a tight loop's
/// trace would bloat storage and responses. We keep the first
/// `CONTROLLER_TRACE_CAP` events (enough to show the run's structure and
/// helper boundaries) and set `trace_truncated`. The failing statement is
/// surfaced separately via `error_span`, so a truncated trace still points
/// at the error.
const CONTROLLER_TRACE_CAP: usize = 2_000;

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
    /// Real execution trace recorded by the VM, in order. Mirrors the
    /// browser-sim `run.trace[]` so both run targets render identically.
    pub trace: Vec<TraceStep>,
    /// Whether the trace hit its cap and stopped recording.
    pub trace_truncated: bool,
    /// Source span of the failing statement, when the run errored.
    pub error_span: Option<SourceSpan>,
}

/// Default per-call connector timeout when none is configured.
const DEFAULT_CONNECTOR_TIMEOUT_MS: u64 = 30_000;

/// The providers `run_canonical` resolves external Actions against, plus
/// the per-call HTTP timeout. Built from the environment in production
/// (`ProviderConfig::from_env`) and injected directly in tests.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Module name -> connector base URL. A `"*"` key is a wildcard that
    /// catches every Action regardless of module.
    pub endpoints: HashMap<String, String>,
    /// Hard per connector-call HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

impl ProviderConfig {
    /// Read providers from `SOLFLOW_CONNECTORS` and the per-call timeout
    /// from `SOLFLOW_CONNECTOR_TIMEOUT_MS` (default 30s).
    pub fn from_env() -> Self {
        let timeout_ms = std::env::var("SOLFLOW_CONNECTOR_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|ms| *ms > 0)
            .unwrap_or(DEFAULT_CONNECTOR_TIMEOUT_MS);
        Self { endpoints: load_connectors(), timeout_ms }
    }

    fn resolve(&self, module: &str) -> Option<&String> {
        self.endpoints.get(module).or_else(|| self.endpoints.get("*"))
    }
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
    providers: &ProviderConfig,
    inputs: &serde_json::Value,
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
                trace: Vec::new(),
                trace_truncated: false,
                error_span: None,
            };
        }
    };
    exec.enable_trace();
    // Bind the run's inputs as `payload` so the workflow's event-data
    // references resolve. Empty/null inputs bind nothing (an unprovided
    // payload then fails clearly at the reference site).
    if !inputs.is_null() {
        exec.bind_input("payload", json_to_value(inputs.clone()));
    }

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
                let endpoint = providers.resolve(&module);
                match endpoint {
                    Some(base_url) => {
                        info!("action {capability}: calling connector {base_url}");
                        match invoke_connector(
                            &handle, base_url, &module, &func, &params, providers.timeout_ms,
                        ) {
                            Ok(result) => {
                                if let Err(e) = exec.resolve_remote_call(&capability, result) {
                                    exec.trace_ext_error(format!(
                                        "provider '{module}' returned a value '{func}' could not accept: {e}"
                                    ));
                                    error = Some(classify(e));
                                    break;
                                }
                                // resumed — keep stepping the workflow
                            }
                            Err(message) => {
                                // Tie the provider failure to its call site
                                // in the trace, then surface it structurally.
                                exec.trace_ext_error(format!(
                                    "provider '{module}' failed calling '{func}': {message}"
                                ));
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
                            "action {capability}: no provider registered (module `{module}`, no `*` fallback) — blocked"
                        );
                        // Record a clear, source mapped error at the call site:
                        // which module/function had no provider.
                        exec.trace_ext_error(format!(
                            "no provider registered for module '{module}' (capability '{capability}'); register a connector or switch to a target that has one"
                        ));
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

    // Map the VM trace onto the wire shape (with 1-based source lines)
    // and surface the failing statement's span when the run errored.
    let full_trace = exec.trace();
    let mut trace = to_trace_steps(source, full_trace);
    let trace_truncated = exec.trace_truncated() || trace.len() > CONTROLLER_TRACE_CAP;
    trace.truncate(CONTROLLER_TRACE_CAP);
    let error_span = exec
        .trace()
        .iter()
        .rev()
        .find(|e| matches!(e.kind, TraceKind::Error))
        .and_then(|e| e.span)
        .map(|(start, end)| SourceSpan { start, end });

    CanonicalOutcome {
        return_value: if error.is_some() { None } else { return_value },
        output: drain_output(),
        steps: total_steps,
        error,
        trace,
        trace_truncated,
        error_span,
    }
}

/// 1-based line number that byte offset `off` falls on within `src`.
fn line_of(src: &str, off: usize) -> usize {
    1 + src.as_bytes().iter().take(off.min(src.len())).filter(|&&b| b == b'\n').count()
}

/// Map the VM's trace events onto the wire `TraceStep` shape, computing
/// the 1-based source line each span starts on for click-to-highlight.
fn to_trace_steps(src: &str, events: &[openprem_sol_v2::TraceEvent]) -> Vec<TraceStep> {
    events
        .iter()
        .map(|e| TraceStep {
            step: e.step,
            kind: match e.kind {
                TraceKind::Stmt => "stmt",
                TraceKind::Call => "call",
                TraceKind::Return => "return",
                TraceKind::ExtCall => "extcall",
                TraceKind::ExtResult => "extresult",
                TraceKind::Error => "error",
            }
            .to_string(),
            function: e.function.clone(),
            span: e.span.map(|(start, end)| SourceSpan { start, end }),
            line: e.span.map(|(start, _)| line_of(src, start)),
            depth: e.depth,
            detail: e.detail.clone(),
        })
        .collect()
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

/// The providers the controller will actually resolve `call(...)` against,
/// for `GET /providers`. Reads the same `SOLFLOW_CONNECTORS` registry the
/// execution path uses, so the listing reflects reality. Sorted by module.
pub fn provider_list() -> Vec<solflow_host_spec::ProviderInfo> {
    let mut out: Vec<solflow_host_spec::ProviderInfo> = load_connectors()
        .into_iter()
        .map(|(module, url)| solflow_host_spec::ProviderInfo { module, url })
        .collect();
    out.sort_by(|a, b| a.module.cmp(&b.module));
    out
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
    timeout_ms: u64,
) -> Result<Value, String> {
    let body = serde_json::json!({
        "module": module,
        "function": func,
        "params": value_to_json(params),
    });
    let url = base_url.to_string();
    let resp: serde_json::Value = handle.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| format!("connector client init failed: {e}"))?;
        let r = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    format!("connector timed out after {timeout_ms}ms")
                } else {
                    format!("connector request failed: {e}")
                }
            })?;
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

#[cfg(test)]
mod provider_tests {
    use super::*;
    use axum::{routing::post, Router};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// What a mock connector should do for one scenario.
    #[derive(Clone, Copy)]
    enum Mock {
        Ok,       // 200 + JSON 42
        Error500, // 500
        NotJson,  // 200 + text/plain
        Slow,     // sleeps 2s then 200 + JSON
    }

    /// Spawn a mock connector on an ephemeral port; return its base URL.
    async fn spawn_mock(kind: Mock) -> String {
        let app = Router::new().route(
            "/",
            post(move |_body: String| async move {
                match kind {
                    Mock::Ok => axum::response::Response::builder()
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from("42"))
                        .unwrap(),
                    Mock::Error500 => axum::response::Response::builder()
                        .status(500)
                        .body(axum::body::Body::from("boom"))
                        .unwrap(),
                    Mock::NotJson => axum::response::Response::builder()
                        .header("content-type", "text/plain")
                        .body(axum::body::Body::from("not json at all"))
                        .unwrap(),
                    Mock::Slow => {
                        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                        axum::response::Response::builder()
                            .body(axum::body::Body::from("1"))
                            .unwrap()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        // Suppress the unused `_body` binding warning is unnecessary; just go.
        format!("http://{addr}")
    }

    fn cfg(module: &str, url: &str, timeout_ms: u64) -> ProviderConfig {
        let mut endpoints = HashMap::new();
        endpoints.insert(module.to_string(), url.to_string());
        ProviderConfig { endpoints, timeout_ms }
    }

    /// Run a workflow that makes one external call through `providers`.
    async fn run_with(providers: ProviderConfig, src: &str) -> CanonicalOutcome {
        run_with_inputs(providers, src, serde_json::Value::Null).await
    }

    async fn run_with_inputs(
        providers: ProviderConfig,
        src: &str,
        inputs: serde_json::Value,
    ) -> CanonicalOutcome {
        let handle = tokio::runtime::Handle::current();
        let src = src.to_string();
        tokio::task::spawn_blocking(move || {
            run_canonical(
                &src,
                "start",
                1_000_000,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                handle,
                &providers,
                &inputs,
            )
        })
        .await
        .unwrap()
    }

    const CALL_SRC: &str = r#"
        workflow "start" {
            let r: int = call("demo.add", { a: 20, b: 22 });
            print(r);
            return r;
        }
    "#;

    const PAYLOAD_SRC: &str =
        r#"workflow "start" { let t: int = payload.total; print(t); return t; }"#;

    #[tokio::test]
    async fn payload_injected_from_run_inputs() {
        let providers = cfg("none", "http://127.0.0.1:1", 5_000);
        // No inputs -> the payload reference fails clearly.
        let miss = run_with_inputs(providers.clone(), PAYLOAD_SRC, serde_json::Value::Null).await;
        assert!(miss.error.is_some(), "expected a missing-payload failure");
        // Inputs bound as payload -> resolves and returns the value.
        let ok = run_with_inputs(providers, PAYLOAD_SRC, serde_json::json!({ "total": 1200 })).await;
        assert!(ok.error.is_none(), "expected success, got {:?}", ok.error);
        assert_eq!(ok.return_value, Some(1200));
    }

    #[tokio::test]
    async fn provider_success_runs_end_to_end_and_traces_extcall() {
        let url = spawn_mock(Mock::Ok).await;
        let out = run_with(cfg("demo", &url, 5_000), CALL_SRC).await;
        assert!(out.error.is_none(), "expected success, got {:?}", out.error);
        assert_eq!(out.return_value, Some(42));
        // Trace shows the external call AND its result, both source mapped.
        let call = out.trace.iter().find(|s| s.kind == "extcall").expect("extcall");
        assert_eq!(call.detail.as_deref(), Some("demo.add"));
        assert!(call.line.is_some(), "ext call must carry a source line");
        assert!(out.trace.iter().any(|s| s.kind == "extresult"));
    }

    #[tokio::test]
    async fn missing_provider_blocks_clearly_with_source_line() {
        // No provider for "demo".
        let out = run_with(cfg("other", "http://127.0.0.1:1", 5_000), CALL_SRC).await;
        match out.error {
            Some(RuntimeErrorView::ExtCallBlocked { ref function_name, .. }) => {
                assert!(function_name.contains("demo"), "names the capability: {function_name}");
            }
            other => panic!("expected ExtCallBlocked, got {other:?}"),
        }
        // A source mapped error step points at the failing call.
        let err = out.trace.iter().find(|s| s.kind == "error").expect("error step");
        assert!(err.detail.as_deref().unwrap_or("").contains("no provider"));
        assert!(err.line.is_some(), "blocked error must carry a source line");
        assert!(out.error_span.is_some(), "error span surfaced for the UI");
    }

    #[tokio::test]
    async fn provider_http_error_fails_clearly() {
        let url = spawn_mock(Mock::Error500).await;
        let out = run_with(cfg("demo", &url, 5_000), CALL_SRC).await;
        match out.error {
            Some(RuntimeErrorView::ExtCallFailed { ref message, .. }) => {
                assert!(message.contains("500"), "carries the status: {message}");
            }
            other => panic!("expected ExtCallFailed, got {other:?}"),
        }
        assert!(out.trace.iter().any(|s| s.kind == "error"));
    }

    #[tokio::test]
    async fn provider_invalid_response_fails_clearly() {
        let url = spawn_mock(Mock::NotJson).await;
        let out = run_with(cfg("demo", &url, 5_000), CALL_SRC).await;
        match out.error {
            Some(RuntimeErrorView::ExtCallFailed { ref message, .. }) => {
                assert!(message.contains("not JSON"), "explains the decode failure: {message}");
            }
            other => panic!("expected ExtCallFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn provider_timeout_fails_clearly() {
        let url = spawn_mock(Mock::Slow).await;
        // Short per-call timeout; the mock sleeps 2s.
        let out = run_with(cfg("demo", &url, 200), CALL_SRC).await;
        match out.error {
            Some(RuntimeErrorView::ExtCallFailed { ref message, .. }) => {
                assert!(message.contains("timed out"), "reports a timeout: {message}");
            }
            other => panic!("expected ExtCallFailed (timeout), got {other:?}"),
        }
    }
}
