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
    decode_bytecode, RunEvent, RunOutput, RunRecord, RunStatus, RuntimeErrorView,
};
use solflow_runtime::{
    run_program_with, ExtCallContext, ExtCallError, ExtCallHandler,
    ExtCallHandlerArc, ExtCallValue, PrintCallback, RunError, RunOptions,
};
use std::sync::atomic::Ordering;
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
pub async fn execute_run(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    policy: RunPolicy,
    connectors: Option<ConnectorRegistry>,
    event_sink: Option<Arc<dyn EventSink>>,
) {
    // Per-run event context (shared with the VM print hook + the
    // ExtCallHandler so all three sources of events share the
    // same monotonic seq counter).
    let ctx = event_sink
        .as_ref()
        .map(|s| Arc::new(RunEventCtx::new(record.id.clone(), s.clone())));

    if let Some(c) = &ctx {
        c.emit(queued_event(c)).await;
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

    // Load + decode bytecode + per-instruction span sidecar.
    let (bc_bytes, spans_bytes) =
        match persistence.get_workflow_bytecode(&record.workflow_id).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("execute_run get_workflow_bytecode failed: {e}");
                finalize_failed(persistence, record, format!("{e}"), ctx.clone()).await;
                return;
            }
        };
    let bytecode = match decode_bytecode(&bc_bytes) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("execute_run decode_bytecode failed: {e}");
            finalize_failed(
                persistence,
                record,
                format!("bytecode decode failed: {e}"),
                ctx.clone(),
            )
            .await;
            return;
        }
    };
    // Spans are best-effort — a missing/malformed sidecar means
    // events don't get click-to-source affordances, not that the
    // run fails. Decode into a sharable Arc so the print
    // callback (sync, blocking thread) can look up by inst_ptr.
    let spans: Arc<Vec<Option<solflow_host_spec::SourceSpan>>> = Arc::new(
        solflow_host_spec::decode_instruction_spans(&spans_bytes).unwrap_or_default(),
    );

    // Build VM options. PrintCallback + ExtCallHandler both
    // emit events through the shared ctx so the seq stream
    // stays monotonic across sources.
    let print_callback: Option<PrintCallback> = ctx.as_ref().map(|c| {
        let (seq, sink, handle, run_id) = c.split_for_print();
        let spans = spans.clone();
        Arc::new(move |line: &str, inst_ptr: usize| {
            // Look up the source span for the Print instruction
            // so the editor can render click-to-source on each
            // print row.
            let source_span = spans
                .get(inst_ptr)
                .and_then(|s| s.as_ref().copied());
            let event = RunEvent::Print {
                run_id: run_id.clone(),
                seq: seq.fetch_add(1, Ordering::Relaxed),
                ts: now_ms(),
                text: line.to_string(),
                source_span,
            };
            let sink_clone = sink.clone();
            handle.spawn(async move { sink_clone.emit(event).await });
        }) as PrintCallback
    });

    let opts = RunOptions {
        step_limit: Some(policy.step_limit),
        trace: false, // C.5 trace streaming via events lands here too eventually
        ext_call_handler: connectors.map(|registry| {
            Arc::new(ControllerExtCallHandler {
                registry,
                tokio_handle: tokio::runtime::Handle::current(),
                ctx: ctx.clone(),
            }) as ExtCallHandlerArc
        }),
        print_callback,
        // Phase C C.6 c89 — cancellation is plumbed end-to-end
        // by the RunManager in c91. For now the executor passes
        // `None` so behavior matches C.5.
        cancel_callback: None,
        max_output_lines: Some(policy.max_output_lines),
        max_events_per_run: Some(policy.max_events_per_run),
    };
    let bytecode_for_task = bytecode.clone();
    let vm_future = tokio::task::spawn_blocking(move || {
        run_program_with(&bytecode_for_task, opts)
    });
    let outcome = match tokio::time::timeout(policy.wall_clock_timeout, vm_future).await {
        Ok(Ok(o)) => o,
        Ok(Err(join_err)) => {
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
        Err(_elapsed) => {
            // Wall-clock timeout — VM didn't finish in time.
            finalize_failed(
                persistence,
                record,
                format!(
                    "wall-clock timeout: {}s",
                    policy.wall_clock_timeout.as_secs()
                ),
                ctx.clone(),
            )
            .await;
            return;
        }
    };

    // Translate VM outcome to RunRecord.
    record.completed_at = Some(now_ms());
    let vm_error = outcome.error.clone();
    let final_output = RunOutput {
        return_value: if vm_error.is_some() {
            None
        } else {
            Some(outcome.return_value as i64)
        },
        output: outcome.output.clone(),
        steps: outcome.steps,
    };
    if vm_error.is_some() {
        record.status = RunStatus::Failed;
    } else {
        record.status = RunStatus::Succeeded;
    }
    record.output = Some(final_output.clone());
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("execute_run persistence put_run (final) failed: {e}");
    }

    // Emit terminal event AFTER the final RunRecord write so any
    // subscriber that immediately fetches the record on Completed
    // sees the final state.
    if let Some(c) = &ctx {
        match vm_error {
            Some(err) => {
                c.emit(failed_event(c, run_error_to_view(&err), None)).await;
            }
            None => c.emit(completed_event(c, final_output)).await,
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
        let invocation = crate::connector::ConnectorInvocation {
            fn_name: ctx.function_name.to_string(),
            url_params: parsed.params,
            args: args_json,
            policy: connector.meta().default_policy,
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
        execute_run(p.clone(), record, RunPolicy::default(), None, None).await;
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
        execute_run(p.clone(), record, RunPolicy::default(), None, None).await;
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
        execute_run(p.clone(), record, policy, None, None).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Failed);
    }
}
