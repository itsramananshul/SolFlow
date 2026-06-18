//! Run-execution path. C.2 c62.
//!
//! Spawns a tokio task per run; loads bytecode from persistence,
//! runs through the canonical `solflow_runtime::VM`, persists the
//! final `RunRecord`.
//!
//! Wall-clock timeout via `tokio::time::timeout`. Step limit via
//! `RunOptions::step_limit`. ExtCall stays blocked until C.4 ships
//! the connector framework — until then `ExtCallBlocked` is what
//! workflows see.
//!
//! Cancellation (C.6): not yet implemented; `DELETE /runs/:id`
//! returns `NotImplemented` for now.

use crate::connector::ConnectorRegistry;
use crate::event_sink::{
    completed_event, failed_event, queued_event, started_event,
    EventSink, RunEventCtx,
};
use crate::{Persistence, SqlitePersistence};
use solflow_host_spec::{
    RunEvent, RunOutput, RunRecord, RunStatus, RuntimeErrorView,
};
// NOTE: the legacy `solflow_runtime` VM is no longer used to execute
// runs (the canonical openprem-sol-v2 VM is, via `canonical_exec`).
// These imports remain only for the still-present (dead) connector
// ExtCall bridge below, kept for the upcoming capability wiring.
#[allow(unused_imports)]
use solflow_runtime::{
    ExtCallContext, ExtCallError, ExtCallHandler, ExtCallHandlerArc, ExtCallValue, RunError,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configurable run-execution policy applied by the controller.
/// MVP defaults match the architecture doc §10.2 numbers.
///
/// Phase C C.6 c89 adds `max_output_lines` + `max_events_per_run`
/// as resource caps. The VM enforces `max_output_lines` directly;
/// `max_events_per_run` is reserved for the RunManager in c91.
#[derive(Debug, Clone, Copy)]
pub struct RunPolicy {
    pub step_limit: usize,
    pub wall_clock_timeout: Duration,
    /// Cap on `RunOutput::output` lines. Exceeding it surfaces
    /// as `RunError::ResourceLimit { resource: "output_lines",
    /// ... }`. Default 100k lines — large enough for any
    /// reasonable run, small enough to fence runaway loops.
    pub max_output_lines: u64,
    /// Cap on total `RunEvent` entries persisted for one run.
    /// RunManager honors this in c91; the VM doesn't see it.
    /// Default 1M events. Defensive ceiling against an
    /// adversarial workflow that logs a million prints.
    pub max_events_per_run: u64,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            step_limit: 10_000_000,
            wall_clock_timeout: Duration::from_secs(600),
            max_output_lines: 100_000,
            max_events_per_run: 1_000_000,
        }
    }
}

/// Execute a run synchronously (with its own internal timeout).
/// Caller spawns this on a tokio task. Persists the final
/// RunRecord state through `persistence`.
///
/// Phase C C.4 (c76): accepts an optional `ConnectorRegistry`.
/// Phase C C.5 (c82): accepts an optional `EventSink` — when
/// `Some`, emits Queued / Started / Print / ExtCallStarted /
/// ExtCallCompleted / Completed / Failed events the SSE endpoint
/// can stream to clients in real time.
/// Phase C C.6 (c90): accepts an optional `cancel_flag` shared
/// with the RunManager. When set, the VM's cancel callback fires
/// `RunError::Cancelled` between instructions and the
/// connector handler short-circuits before invoking the next
/// connector attempt.
pub async fn execute_run(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    policy: RunPolicy,
    connectors: Option<ConnectorRegistry>,
    event_sink: Option<Arc<dyn EventSink>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) {
    // Per-run event context (shared with the VM print hook + the
    // ExtCallHandler so all three sources of events share the
    // same monotonic seq counter). c94 — install the per-run
    // event-log cap so a runaway workflow can't spam the SSE
    // stream or grow the run_events table without bound.
    // Seed the per-run event seq counter so execute_run's events
    // continue *after* the `Queued` event the RunManager already
    // wrote (it emits Queued at seq 0 on enqueue). Without this the
    // two emitters both start at 0 and collide on UNIQUE(run_id,
    // seq). `start_seq == 0` means no prior events (e.g. execute_run
    // driven directly in a unit test), so we still emit our own
    // Queued; otherwise the RunManager owns it and we skip it.
    let start_seq = if event_sink.is_some() {
        persistence
            .next_event_seq(&record.id)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    let ctx = event_sink.as_ref().map(|s| {
        Arc::new(
            RunEventCtx::new(record.id.clone(), s.clone())
                .with_start_seq(start_seq)
                .with_max_events(Some(policy.max_events_per_run)),
        )
    });

    if start_seq == 0 {
        if let Some(c) = &ctx {
            c.emit(queued_event(c)).await;
        }
    }

    // Mark Running + persist before the VM starts so callers
    // polling GET /runs/:id see the transition.
    record.status = RunStatus::Running;
    record.started_at = Some(now_ms());
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("execute_run persistence put_run (Running) failed: {e}");
        return;
    }
    if let Some(c) = &ctx {
        c.emit(started_event(c)).await;
    }

    // Phase C/D — canonical execution. The editor submits SOL
    // *source* (stored in the workflow's blob); the controller
    // compiles + runs it through the canonical openprem-sol-v2 VM
    // so production runs share the exact semantics of the browser
    // sim. No client-side bytecode, no cross-crate format coupling.
    let (bc_bytes, _spans_bytes) =
        match persistence.get_workflow_bytecode(&record.workflow_id).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("execute_run get_workflow_bytecode failed: {e}");
                finalize_failed(persistence, record, format!("{e}"), ctx.clone()).await;
                return;
            }
        };
    let source = String::from_utf8_lossy(&bc_bytes).into_owned();
    let workflow_name = match crate::canonical_exec::first_workflow_name(&source) {
        Some(n) => n,
        None => {
            finalize_failed(
                persistence,
                record,
                "no workflow declaration found in submitted source".into(),
                ctx.clone(),
            )
            .await;
            return;
        }
    };

    // Phase C C.6 c90 — user cancel flag (set via DELETE /runs/:id).
    // Phase C C.6 c94 — internal timeout flag distinct from user
    // cancel: when wall-clock fires we set this so the VM exits at
    // its next cancel poll, but reconcile can tell user
    // cancellation apart from timeout.
    let user_cancel = cancel_flag
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let timeout_flag = Arc::new(AtomicBool::new(false));
    // Connectors aren't bound into the canonical capability model
    // yet; external Actions surface as ExtCallBlocked (the same
    // honest signal as the browser sim). Consume the param so it
    // doesn't read as unused.
    let _ = connectors;
    let step_limit = policy.step_limit as u64;
    let run_source = source.clone();
    let run_name = workflow_name.clone();
    let task_cancel = user_cancel.clone();
    let task_timeout = timeout_flag.clone();
    let mut vm_handle = tokio::task::spawn_blocking(move || {
        crate::canonical_exec::run_canonical(
            &run_source,
            &run_name,
            step_limit,
            task_cancel,
            task_timeout,
        )
    });
    // Race the VM against the wall-clock budget. On timeout: flip
    // the timeout flag so the VM exits at its next cancel poll +
    // wait for it to drain (with a small grace window so a
    // pathological VM doesn't leak the blocking thread forever).
    let mut timed_out = false;
    let outcome_res = {
        let timeout_sleep = tokio::time::sleep(policy.wall_clock_timeout);
        tokio::pin!(timeout_sleep);
        loop {
            tokio::select! {
                biased;
                res = &mut vm_handle => break res,
                _ = &mut timeout_sleep, if !timed_out => {
                    timed_out = true;
                    timeout_flag.store(true, Ordering::Relaxed);
                    // Loop again: wait for the VM to honor cancel.
                    // Grace window (5s) before we abandon the
                    // blocking thread + synthesize a TimedOut
                    // outcome.
                }
            }
            if timed_out {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    &mut vm_handle,
                )
                .await
                {
                    Ok(res) => break res,
                    Err(_) => {
                        tracing::error!(
                            "VM didn't honor cancel within 5s grace; abandoning task for run {}",
                            record.id,
                        );
                        finalize_timed_out(
                            persistence,
                            record,
                            policy.wall_clock_timeout.as_secs(),
                            ctx.clone(),
                        )
                        .await;
                        return;
                    }
                }
            }
        }
    };
    let outcome = match outcome_res {
        Ok(o) => o,
        Err(join_err) => {
            tracing::error!("execute_run vm task panicked: {join_err}");
            finalize_failed(
                persistence,
                record,
                format!("VM task panicked: {join_err}"),
                ctx.clone(),
            )
            .await;
            return;
        }
    };

    // Translate VM outcome to RunRecord. When timed_out, the VM's
    // error is Cancelled (we set the flag); promote to TimedOut.
    record.completed_at = Some(now_ms());
    let vm_error = outcome.error.clone();
    let final_output = RunOutput {
        return_value: if vm_error.is_some() {
            None
        } else {
            outcome.return_value
        },
        output: outcome.output.clone(),
        steps: outcome.steps as usize,
    };
    record.status = if timed_out {
        RunStatus::TimedOut
    } else if vm_error.is_some() {
        RunStatus::Failed
    } else {
        RunStatus::Succeeded
    };
    record.output = Some(final_output.clone());
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("execute_run persistence put_run (final) failed: {e}");
    }

    // Emit terminal event AFTER the final RunRecord write so any
    // subscriber that immediately fetches the record on the
    // terminal event sees the final state.
    if let Some(c) = &ctx {
        if timed_out {
            c.emit(RunEvent::TimedOut {
                run_id: c.run_id.clone(),
                seq: c.next_seq(),
                ts: now_ms(),
                wall_clock_secs: policy.wall_clock_timeout.as_secs(),
            })
            .await;
        } else {
            match vm_error {
                Some(err) => {
                    c.emit(failed_event(c, err, None)).await;
                }
                None => c.emit(completed_event(c, final_output)).await,
            }
        }
    }
}

/// Map the runtime's `RunError` to the wire-stable
/// `RuntimeErrorView` used in run events. Mirrors the mapping in
/// compiler-wasm's RunResultView so the editor's discriminator
/// stays uniform across browser-sim + controller paths.
fn run_error_to_view(e: &RunError) -> RuntimeErrorView {
    match e {
        RunError::DivByZero => RuntimeErrorView::DivByZero,
        RunError::IndexOutOfBounds { index, length } => {
            RuntimeErrorView::IndexOutOfBounds { index: *index, length: *length }
        }
        RunError::StackUnderflow => RuntimeErrorView::StackUnderflow,
        RunError::StepLimit { limit } => RuntimeErrorView::StepLimit { limit: *limit },
        RunError::ExtCallBlocked { function_name, url } => {
            RuntimeErrorView::ExtCallBlocked {
                function_name: function_name.clone(),
                url: url.clone(),
            }
        }
        RunError::ExtCallFailed { connector, function_name, message } => {
            RuntimeErrorView::ExtCallFailed {
                connector: connector.clone(),
                function_name: function_name.clone(),
                message: message.clone(),
            }
        }
        RunError::HeapShapeMismatch { expected, got } => {
            RuntimeErrorView::HeapShapeMismatch {
                expected: (*expected).to_string(),
                got: (*got).to_string(),
            }
        }
        RunError::Cancelled => RuntimeErrorView::Cancelled,
        RunError::ResourceLimit { resource, limit } => {
            RuntimeErrorView::ResourceLimit {
                resource: (*resource).to_string(),
                limit: *limit,
            }
        }
    }
}

/// Phase C C.6 c94 — synthesize a TimedOut terminal when the
/// VM ignored its cancel hook for too long after the wall-clock
/// fired. Sets the persisted status + emits one TimedOut event;
/// the orphaned spawn_blocking thread will eventually exit on
/// its own (step_limit at worst).
async fn finalize_timed_out(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    wall_clock_secs: u64,
    ctx: Option<Arc<RunEventCtx>>,
) {
    record.status = RunStatus::TimedOut;
    record.completed_at = Some(now_ms());
    record.output = Some(RunOutput {
        return_value: None,
        output: vec![format!(
            "[controller] wall-clock timeout: {wall_clock_secs}s (VM did not honor cancel)",
        )],
        steps: 0,
    });
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("finalize_timed_out persistence put_run failed: {e}");
    }
    if let Some(c) = ctx {
        c.emit(RunEvent::TimedOut {
            run_id: c.run_id.clone(),
            seq: c.next_seq(),
            ts: now_ms(),
            wall_clock_secs,
        })
        .await;
    }
}

async fn finalize_failed(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    reason: String,
    ctx: Option<Arc<RunEventCtx>>,
) {
    record.status = RunStatus::Failed;
    record.completed_at = Some(now_ms());
    record.output = Some(RunOutput {
        return_value: None,
        output: vec![format!("[controller] {reason}")],
        steps: 0,
    });
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("finalize_failed persistence put_run failed: {e}");
    }
    if let Some(c) = ctx {
        // Use a synthetic ExtCallFailed-like view for controller-
        // pre-VM errors; downstream renderers handle the unified
        // shape. We use ExtCallFailed since that variant already
        // carries a free-form message.
        c.emit(failed_event(
            &c,
            RuntimeErrorView::ExtCallFailed {
                connector: "(controller)".into(),
                function_name: "(pre-vm)".into(),
                message: reason,
            },
            None,
        ))
        .await;
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// =============================================================
//  ExtCall handler — bridges the synchronous VM to async connectors
// =============================================================

/// Concrete `ExtCallHandler` the controller installs into the VM
/// when a `ConnectorRegistry` is configured (Phase C C.4 c76).
///
/// The VM runs on the spawn_blocking thread; the connector is
/// async. We bridge by holding the runtime `Handle` captured at
/// `execute_run` time and calling `Handle::block_on(...)` to wait
/// for the connector future. That's safe because:
///
///   1. The blocking thread is dedicated to this VM run; nothing
///      else is parked on it.
///   2. The tokio runtime has worker threads available (we use
///      `rt-multi-thread`), so block_on won't deadlock waiting
///      for the only scheduler thread.
struct ControllerExtCallHandler {
    registry: ConnectorRegistry,
    tokio_handle: tokio::runtime::Handle,
    /// Optional event ctx; when present, emit ExtCallStarted +
    /// ExtCallCompleted around every dispatch. The seq counter
    /// is shared with the rest of execute_run via Arc.
    ctx: Option<Arc<RunEventCtx>>,
    /// Phase C C.6 c90 — user cancel flag from
    /// `DELETE /runs/:id`. Short-circuits the connector with
    /// Failed; reconcile promotes to Cancelled.
    cancel_flag: Option<Arc<AtomicBool>>,
    /// Phase C C.6 c94 — wall-clock timeout flag. When set,
    /// the handler aborts in-flight connector work the same
    /// way as cancel_flag; the executor promotes the terminal
    /// to TimedOut.
    timeout_flag: Option<Arc<AtomicBool>>,
}

impl ExtCallHandler for ControllerExtCallHandler {
    fn handle(
        &self,
        ctx: ExtCallContext<'_>,
    ) -> Result<ExtCallValue, ExtCallError> {
        // Parse the URL up front. Failures are connector-class
        // errors, not runtime panics.
        let parsed = crate::connector::parse_connector_url(ctx.url).map_err(|e| {
            ExtCallError::failed(
                "(unresolved)",
                ctx.function_name,
                format!("invalid connector URL `{}`: {e}", ctx.url),
            )
        })?;
        let connector = self.registry.lookup(&parsed.name).map_err(|e| {
            ExtCallError::failed("(unresolved)", ctx.function_name, e.to_string())
        })?;

        // Phase C C.6 c90/c94 — short-circuit if the run was
        // cancelled OR timed out while the VM was preparing
        // this ExtCall. Returning Failed here lets execute_run
        // see the abort; the post-VM mapping promotes to
        // Cancelled (user cancel) or TimedOut (wall-clock).
        let cancelled = self
            .cancel_flag
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Relaxed));
        let timed_out = self
            .timeout_flag
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Relaxed));
        if cancelled || timed_out {
            return Err(ExtCallError::failed(
                parsed.name.clone(),
                ctx.function_name,
                if cancelled {
                    "run cancelled before connector dispatch"
                } else {
                    "run timed out before connector dispatch"
                },
            ));
        }

        // Emit ExtCallStarted (fire-and-forget so the VM doesn't
        // pace on persistence latency).
        if let Some(ec) = &self.ctx {
            let event = RunEvent::ExtCallStarted {
                run_id: ec.run_id.clone(),
                seq: ec.next_seq(),
                ts: now_ms(),
                connector: parsed.name.clone(),
                fn_name: ctx.function_name.to_string(),
            };
            ec.spawn_emit(event);
        }

        // Marshal args + return-type hint into the invocation
        // payload. C.4: positional primitive args become a JSON
        // array (`[arg0, arg1, ...]`). The HTTP connector then
        // uses that array as the body / object args as query
        // params per its docs. Connectors that want named args
        // can read invocation.fn_name / url_params instead.
        let args_json = serde_json::Value::Array(
            ctx.args.iter().map(extcall_value_to_json).collect(),
        );
        // Phase C C.6 c94 — combined user-cancel + timeout flag
        // for the connector to race against in-flight I/O.
        // Build a small atomic that mirrors either bit; the
        // HttpConnector polls this between retries + via
        // select! during one attempt so a slow request doesn't
        // pin the run.
        let combined_flag = match (&self.cancel_flag, &self.timeout_flag) {
            (Some(c), Some(t)) => {
                let combined = Arc::new(AtomicBool::new(false));
                let cc = c.clone();
                let tt = t.clone();
                let cb = combined.clone();
                // Spawn a tiny watcher that flips `combined`
                // when either source flips. Watch period 50ms —
                // tighter than typical HTTP latency, loose
                // enough to be cheap.
                self.tokio_handle.spawn(async move {
                    loop {
                        if cc.load(Ordering::Relaxed)
                            || tt.load(Ordering::Relaxed)
                        {
                            cb.store(true, Ordering::Relaxed);
                            return;
                        }
                        // Self-shutdown when the combined flag
                        // is already set OR when the executor's
                        // arcs are dropped — Arc::strong_count
                        // catches the latter.
                        if Arc::strong_count(&cc) == 1
                            && Arc::strong_count(&tt) == 1
                        {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                });
                Some(combined)
            }
            _ => None,
        };
        let invocation = crate::connector::ConnectorInvocation {
            fn_name: ctx.function_name.to_string(),
            url_params: parsed.params,
            args: args_json,
            policy: connector.meta().default_policy,
            cancel_flag: combined_flag,
        };

        // Block on the async invocation from this blocking thread.
        let outcome_result = self.tokio_handle.block_on(connector.invoke(invocation));

        // Emit ExtCallCompleted with ok=true/false BEFORE we
        // propagate the error so the event stream stays
        // chronological even on failure.
        if let Some(ec) = &self.ctx {
            let event = RunEvent::ExtCallCompleted {
                run_id: ec.run_id.clone(),
                seq: ec.next_seq(),
                ts: now_ms(),
                connector: parsed.name.clone(),
                fn_name: ctx.function_name.to_string(),
                ok: outcome_result.is_ok(),
            };
            ec.spawn_emit(event);
        }

        let outcome = outcome_result.map_err(|e| {
            ExtCallError::failed(parsed.name.clone(), ctx.function_name, e.to_string())
        })?;

        // Decode the JSON-shaped outcome value back into the
        // SOL return type the VM is expecting.
        json_to_extcall_value(&outcome.value, ctx.ret_type, &parsed.name, ctx.function_name)
    }
}

fn extcall_value_to_json(v: &ExtCallValue) -> serde_json::Value {
    match v {
        ExtCallValue::Int(n) => serde_json::Value::from(*n),
        ExtCallValue::Float(f) => {
            // Non-finite floats can't go through JSON cleanly.
            // Map NaN / Inf to null so we never panic; the
            // connector will see an explicit null.
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        ExtCallValue::Bool(b) => serde_json::Value::from(*b),
        ExtCallValue::String(s) => serde_json::Value::from(s.clone()),
        ExtCallValue::Void => serde_json::Value::Null,
    }
}

fn json_to_extcall_value(
    v: &serde_json::Value,
    expected: solflow_runtime::ExtCallType,
    connector: &str,
    fn_name: &str,
) -> Result<ExtCallValue, ExtCallError> {
    use solflow_runtime::ExtCallType as T;
    match expected {
        T::Void => Ok(ExtCallValue::Void),
        T::Int => v.as_i64().map(ExtCallValue::Int).ok_or_else(|| {
            ExtCallError::failed(
                connector,
                fn_name,
                format!("expected integer return, got {v}"),
            )
        }),
        T::Float => v.as_f64().map(ExtCallValue::Float).ok_or_else(|| {
            ExtCallError::failed(
                connector,
                fn_name,
                format!("expected float return, got {v}"),
            )
        }),
        T::Bool => v.as_bool().map(ExtCallValue::Bool).ok_or_else(|| {
            ExtCallError::failed(
                connector,
                fn_name,
                format!("expected bool return, got {v}"),
            )
        }),
        T::String => match v {
            // String connectors typically return a JSON string;
            // accept any JSON value and stringify non-strings so
            // a `-> str` ext function never errors on shape.
            serde_json::Value::String(s) => Ok(ExtCallValue::String(s.clone())),
            other => Ok(ExtCallValue::String(other.to_string())),
        },
    }
}

// =============================================================
//  Tests — end-to-end execute_run against a real in-memory DB
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_compiler::compile_source;
    use solflow_host_spec::{encode_bytecode, encode_instruction_spans, RunTrigger};

    /// Helper: compile + persist a workflow, return its id.
    async fn submit_test_workflow(p: &SqlitePersistence, source: &str) -> String {
        let compiled = compile_source(source);
        let cp = compiled.value.expect("compile clean");
        let bytecode = encode_bytecode(&cp.bytecode).unwrap();
        let host_spans: Vec<Option<solflow_host_spec::SourceSpan>> = cp
            .instruction_spans
            .iter()
            .map(|s| s.map(Into::into))
            .collect();
        let spans = encode_instruction_spans(&host_spans).unwrap();
        let id = format!("wf_test_{}", uuid::Uuid::new_v4());
        let meta = serde_json::json!({
            "name": "test",
            "content_hash": "test-hash",
            "created_at": now_ms(),
        });
        p.put_workflow(&id, &bytecode, &spans, &meta.to_string())
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn execute_run_clean_program_succeeds() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            "function start() -> int { print(\"hi\"); return 42; }",
        )
        .await;
        let record = RunRecord {
            id: format!("run_{}", uuid::Uuid::new_v4()),
            workflow_id: wf,
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };
        let run_id = record.id.clone();
        execute_run(p.clone(), record, RunPolicy::default(), None, None, None).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Succeeded);
        let out = got.output.unwrap();
        assert_eq!(out.return_value, Some(42));
        assert_eq!(out.output, vec!["hi".to_string()]);
    }

    #[tokio::test]
    async fn execute_run_div_by_zero_fails() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            "function start() -> int { return 10 / 0; }",
        )
        .await;
        let record = RunRecord {
            id: format!("run_{}", uuid::Uuid::new_v4()),
            workflow_id: wf,
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };
        let run_id = record.id.clone();
        execute_run(p.clone(), record, RunPolicy::default(), None, None, None).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Failed);
        assert!(got.output.unwrap().return_value.is_none());
    }

    /// Phase C C.5 c82 — execute_run with an event sink installed
    /// fans the lifecycle out as RunEvents AND streams Print
    /// events as the VM emits them.
    #[tokio::test]
    async fn execute_run_emits_events_through_sink() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            r#"function start() -> int {
                 print("alpha");
                 print("beta");
                 return 7;
               }"#,
        )
        .await;
        let run_id = format!("run_{}", uuid::Uuid::new_v4());
        let record = RunRecord {
            id: run_id.clone(),
            workflow_id: wf,
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };
        let sink = crate::event_sink::CapturingEventSink::default();
        let sink_arc: Arc<dyn crate::EventSink> = Arc::new(sink.clone());
        execute_run(
            p.clone(),
            record,
            RunPolicy::default(),
            None,
            Some(sink_arc),
            None,
        )
        .await;

        // Allow fire-and-forget print emits to drain.
        for _ in 0..50 {
            if sink.events.lock().await.len() >= 5 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let events = sink.events.lock().await.clone();
        let kinds: Vec<&'static str> = events.iter().map(|e| e.kind()).collect();
        // Order: Queued, Started, then 2 Prints (possibly interleaved-
        // ordered after Started but before Completed), then Completed.
        assert_eq!(kinds.first(), Some(&"Queued"));
        assert!(kinds.contains(&"Started"));
        assert_eq!(kinds.iter().filter(|k| **k == "Print").count(), 2);
        assert_eq!(kinds.last(), Some(&"Completed"));

        // Seqs are monotonic (allocated by the shared atomic).
        let seqs: Vec<u64> = events.iter().map(|e| e.seq()).collect();
        for w in seqs.windows(2) {
            assert!(w[0] < w[1], "seq must strictly increase: {seqs:?}");
        }
    }

    /// Phase C C.6 c94 — wall-clock timeout lands as TimedOut
    /// (not Failed) when the VM's loop runs past the policy
    /// budget. The combined cancel-callback fires; the VM exits
    /// with Cancelled; the executor promotes to TimedOut.
    #[tokio::test]
    async fn execute_run_wall_clock_timeout_lands_timed_out() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            // Long counting loop — plenty more than 100ms of
            // VM steps.
            r#"
                function start() -> int {
                    let i: int = 0;
                    while (i < 5000000) {
                        i = i + 1;
                    }
                    return i;
                }
            "#,
        )
        .await;
        let record = RunRecord {
            id: format!("run_{}", uuid::Uuid::new_v4()),
            workflow_id: wf,
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };
        let policy = RunPolicy {
            step_limit: 1_000_000_000,
            // Short wall-clock so the test runs fast — guaranteed
            // shorter than the workflow's natural completion time.
            wall_clock_timeout: std::time::Duration::from_millis(100),
            max_output_lines: 100_000,
            max_events_per_run: 1_000_000,
        };
        let run_id = record.id.clone();
        execute_run(p.clone(), record, policy, None, None, None).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(
            got.status,
            RunStatus::TimedOut,
            "wall-clock timeout should land TimedOut",
        );
    }

    /// Phase C C.6 c94 — the per-run event cap fires a single
    /// terminal ResourceLimit event and suppresses subsequent
    /// emits. Verified at the sink level so we don't have to
    /// race a real VM.
    #[tokio::test]
    async fn event_cap_emits_resource_limit_marker_and_drops_overflow() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let meta = serde_json::json!({
            "name": "cap",
            "content_hash": "h",
            "created_at": now_ms(),
        });
        p.put_workflow(&"wf_cap".to_string(), b"bc", b"sp", &meta.to_string())
            .await
            .unwrap();
        let record = RunRecord {
            id: "run_cap".into(),
            workflow_id: "wf_cap".into(),
            status: RunStatus::Running,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: 0,
            started_at: None,
            completed_at: None,
        };
        p.put_run(&record).await.unwrap();

        let sink = crate::event_sink::CapturingEventSink::default();
        let sink_arc: Arc<dyn crate::EventSink> = Arc::new(sink.clone());
        let ctx = crate::event_sink::RunEventCtx::new(
            "run_cap".into(),
            sink_arc.clone(),
        )
        .with_max_events(Some(3));

        // Emit 5 events through the same ctx. Production
        // emitters allocate `seq` via `ctx.next_seq()` so cap
        // tracking works; mimic that pattern here.
        for i in 0..5 {
            let seq = ctx.next_seq();
            ctx.emit(RunEvent::Print {
                run_id: "run_cap".into(),
                seq,
                ts: 100 + i,
                text: format!("line {i}"),
                source_span: None,
            })
            .await;
        }

        let events = sink.events.lock().await.clone();
        // Cap semantics: "at most N events total." With cap=3,
        // we get 2 Prints (seqs 0, 1) before the cap fires at
        // seq=2, then 1 ResourceLimit marker (seq=3). Subsequent
        // emits are silently dropped via the cap_breached flag.
        // Total persisted = 3.
        assert_eq!(events.len(), 3, "expected 2 prints + 1 marker, got {events:#?}");
        let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
        assert_eq!(kinds, vec!["Print", "Print", "Failed"]);
        // The marker carries the cap as the limit field.
        match &events[2] {
            RunEvent::Failed {
                error: solflow_host_spec::RuntimeErrorView::ResourceLimit { resource, limit },
                ..
            } => {
                assert_eq!(resource, "events");
                assert_eq!(*limit, 3);
            }
            other => panic!("expected ResourceLimit marker, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_run_step_limit_enforced() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            "function start() -> int { while (1 == 1) { } return 0; }",
        )
        .await;
        let record = RunRecord {
            id: format!("run_{}", uuid::Uuid::new_v4()),
            workflow_id: wf,
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };
        let run_id = record.id.clone();
        let policy = RunPolicy {
            step_limit: 1000, // tiny limit for the test
            wall_clock_timeout: Duration::from_secs(10),
            max_output_lines: 100_000,
            max_events_per_run: 1_000_000,
        };
        execute_run(p.clone(), record, policy, None, None, None).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Failed);
    }
}
