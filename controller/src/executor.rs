//! Run-execution path. C.2 c62.
//!
//! Spawns a tokio task per run; loads the workflow source from
//! persistence, runs it through the canonical `openprem-sol-v2` VM
//! (via `canonical_exec`), persists the final `RunRecord`.
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
    // External Actions are executed by canonical_exec via HTTP
    // connectors registered through the SOLFLOW_CONNECTORS env, so
    // the legacy ConnectorRegistry isn't used on this path. Consume
    // it so it doesn't read as unused.
    let _ = connectors;
    let step_limit = policy.step_limit as u64;
    let run_source = source.clone();
    let run_name = workflow_name.clone();
    let task_cancel = user_cancel.clone();
    let task_timeout = timeout_flag.clone();
    // Captured on the async side so the blocking VM thread can drive
    // connector HTTP calls via `Handle::block_on`.
    let rt_handle = tokio::runtime::Handle::current();
    let mut vm_handle = tokio::task::spawn_blocking(move || {
        crate::canonical_exec::run_canonical(
            &run_source,
            &run_name,
            step_limit,
            task_cancel,
            task_timeout,
            rt_handle,
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
        // Surface each captured print line as a Print event before the
        // terminal event so SSE subscribers see program output. The
        // lines were collected by canonical_exec's print buffer and are
        // already part of `final_output`; emitting them here changes no
        // VM semantics, it just streams the already-captured output onto
        // the event channel ahead of the terminal event.
        for line in &final_output.output {
            let seq = c.next_seq();
            c.emit(RunEvent::Print {
                run_id: c.run_id.clone(),
                seq,
                ts: now_ms(),
                text: line.clone(),
                source_span: None,
            })
            .await;
        }
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
//  Tests — end-to-end execute_run against a real in-memory DB
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_host_spec::RunTrigger;

    /// Helper: persist a workflow from its canonical SOL source,
    /// carried as raw UTF-8 bytes exactly as the editor submits it
    /// (the controller reads the source back out of the bytecode
    /// blob and runs it through the canonical VM).
    async fn submit_test_workflow(p: &SqlitePersistence, source: &str) -> String {
        let bytecode = source.as_bytes().to_vec();
        let spans = b"[]".to_vec();
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
            "workflow \"start\" { print(\"hi\"); return 42; }",
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
    async fn execute_run_user_function_calls_succeed() {
        // The controller runs the same canonical openprem-sol-v2 VM as
        // Browser Simulation, so user-defined helper functions (with
        // nested calls and values returned across calls) must execute here
        // too. This is the controller-side parity guard for helper calls.
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let src = r#"
            fn label(n: int) { print("value:"); print(to_str(n)); }
            fn add(a: int, b: int) <- int { return a + b; }
            workflow "start" {
                let s: int = add(20, 22);
                label(s);
                return s;
            }
        "#;
        let wf = submit_test_workflow(&p, src).await;
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
        assert_eq!(got.status, RunStatus::Succeeded, "record={got:?}");
        let out = got.output.unwrap();
        assert_eq!(out.return_value, Some(42));
        assert_eq!(out.output, vec!["value:".to_string(), "42".to_string()]);
    }

    #[tokio::test]
    async fn execute_run_div_by_zero_fails() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_test_workflow(
            &p,
            "workflow \"start\" { return 10 / 0; }",
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
            r#"workflow "start" {
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
                workflow "start" {
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
        // A non-terminating loop whose body runs a statement each
        // iteration. The canonical VM bounds `step()` by statements
        // executed, so a bodied loop accumulates steps and trips the
        // policy step_limit (an empty-body loop is caught by the
        // wall-clock path instead, exercised by the timeout test).
        let wf = submit_test_workflow(
            &p,
            "workflow \"start\" { let i: int = 0; while (1 == 1) { i = i + 1; } return i; }",
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
