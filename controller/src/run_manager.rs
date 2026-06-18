//! Run orchestration coordinator — Phase C C.6 c90.
//!
//! `RunManager` owns the full lifecycle of every run on the
//! controller. It replaces the direct `tokio::spawn(execute_run)`
//! calls that lived in `LocalController` + `TokioScheduler` so
//! every code path that wants to execute a workflow goes through
//! one orchestration surface that:
//!
//!   - persists each run in a bounded FIFO queue
//!   - dispatches to a worker pool capped at
//!     `ConcurrencyPolicy::max_concurrent_runs`
//!   - enforces saturation policy (Queue vs Reject) when the
//!     queue is full
//!   - tracks active runs in an in-memory registry keyed by
//!     run id so cancellation can target them
//!   - threads a per-run `cancel_flag: Arc<AtomicBool>` into
//!     `execute_run` so the VM's cancel callback + the ExtCall
//!     handler can both observe a single source of truth
//!   - promotes a `Failed` terminal to `Cancelled` when the
//!     cancel flag was set during execution (covers the case
//!     where a cancel arrives while the VM is mid-ExtCall —
//!     the handler returns Failed, but the run was conceptually
//!     cancelled)
//!
//! Scheduler integration lands in c91; HTTP cancel endpoint in
//! c92. c90's surface is enough for LocalController.create_run
//! to delegate the entire spawn + lifecycle to one place.

use crate::connector::ConnectorRegistry;
use crate::event_sink::PersistentEventSink;
use crate::executor::{execute_run, now_ms, RunPolicy};
use crate::{EventSink, Persistence, SqlitePersistence};
use serde::{Deserialize, Serialize};
use solflow_host_spec::{
    InvalidTransition, RunEvent, RunId, RunRecord, RunStatus, WorkflowId,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::{mpsc, Semaphore};

// =============================================================
//  Policy + outcomes
// =============================================================

/// How the controller behaves when the queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaturationPolicy {
    /// Backpressure: return `EnqueueOutcome::QueueFull`. Caller
    /// (HTTP layer / scheduler) decides whether to retry or
    /// surface 503. Default.
    Queue,
    /// Reject the run synchronously with terminal status
    /// `Rejected`. Persists the record + emits `RunEvent::Rejected`
    /// so downstream observers see the refusal in the event log.
    Reject,
}

/// Controller-wide orchestration policy.
#[derive(Debug, Clone, Copy)]
pub struct ConcurrencyPolicy {
    /// Cap on concurrently-executing runs. Default 8.
    pub max_concurrent_runs: usize,
    /// Cap on queued-but-not-yet-running runs. Default 64.
    /// Used together with `on_saturation` to either reject or
    /// 503 once full.
    pub max_queued_runs: usize,
    pub on_saturation: SaturationPolicy,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_runs: 8,
            max_queued_runs: 64,
            on_saturation: SaturationPolicy::Queue,
        }
    }
}

/// Result of `RunManager::enqueue`. The HTTP layer maps these
/// to status codes: Accepted = 202, Rejected = 503 + body,
/// QueueFull = 503 + body. Callers tell them apart so
/// reject-on-saturation can be observable distinctly from
/// queue-full-backpressure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnqueueOutcome {
    /// Accepted into the queue (possibly already dispatched).
    Accepted { run_id: RunId },
    /// Reject-on-saturation refused this run; a terminal
    /// `Rejected` record was persisted.
    Rejected { run_id: RunId, reason: String },
    /// Queue-on-saturation can't accept right now.
    QueueFull { current_depth: usize, capacity: usize },
}

/// Snapshot of a single active run (read-only). The cancel flag
/// stays internal — callers cancel via `RunManager::cancel(id)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveRunSummary {
    pub run_id: RunId,
    pub workflow_id: WorkflowId,
    /// ms since epoch when the worker picked up the run.
    pub dispatched_at: i64,
}

/// Per-controller concurrency snapshot. Editor `/controller/concurrency`
/// endpoint returns this (c92).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyMetrics {
    pub max_concurrent_runs: usize,
    pub max_queued_runs: usize,
    pub active_runs: usize,
    pub queued_runs: usize,
    pub saturation_policy: SaturationPolicy,
}

#[derive(Debug, Error)]
pub enum RunManagerError {
    #[error("invalid lifecycle transition: {0}")]
    InvalidTransition(#[from] InvalidTransition),
    #[error("persistence error: {0}")]
    Persistence(#[from] crate::ControllerError),
    #[error("run not found: {0}")]
    RunNotFound(RunId),
    /// Internal channel send failure — shouldn't happen in
    /// practice (dispatcher task lives as long as the manager).
    #[error("queue send failed: {0}")]
    QueueSendFailed(String),
}

// =============================================================
//  Internal types
// =============================================================

struct QueuedRun {
    record: RunRecord,
    cancel_flag: Arc<AtomicBool>,
}

struct ActiveRunEntry {
    workflow_id: WorkflowId,
    cancel_flag: Arc<AtomicBool>,
    dispatched_at: i64,
}

// =============================================================
//  RunManager
// =============================================================

#[derive(Clone)]
pub struct RunManager {
    persistence: SqlitePersistence,
    policy: RunPolicy,
    concurrency: ConcurrencyPolicy,
    connectors: ConnectorRegistry,
    event_sink: Option<PersistentEventSink>,
    tx: mpsc::Sender<QueuedRun>,
    queue_depth: Arc<AtomicUsize>,
    active: Arc<Mutex<HashMap<RunId, Arc<ActiveRunEntry>>>>,
    permits: Arc<Semaphore>,
}

impl RunManager {
    /// Construct + immediately spawn the dispatcher loop on the
    /// current tokio runtime. Callers must be inside a tokio
    /// context; `LocalController::new()` is.
    pub fn new(
        persistence: SqlitePersistence,
        policy: RunPolicy,
        concurrency: ConcurrencyPolicy,
        connectors: ConnectorRegistry,
        event_sink: Option<PersistentEventSink>,
    ) -> Self {
        // Channel capacity = max_queued_runs so the mpsc backs the
        // queue directly (no separate VecDeque needed). queue_depth
        // is bumped by enqueue + decremented by the dispatcher so
        // metrics stay accurate without polling mpsc internals.
        let cap = concurrency.max_queued_runs.max(1);
        let (tx, rx) = mpsc::channel::<QueuedRun>(cap);
        let permits = Arc::new(Semaphore::new(concurrency.max_concurrent_runs));
        let me = Self {
            persistence,
            policy,
            concurrency,
            connectors,
            event_sink,
            tx,
            queue_depth: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(Mutex::new(HashMap::new())),
            permits,
        };
        let dispatcher = me.clone();
        tokio::spawn(async move { dispatcher.dispatcher_loop(rx).await });
        me
    }

    /// Enqueue a fresh run. The caller supplies a complete
    /// `RunRecord` (id minted upstream; status is overwritten to
    /// `Queued`). Returns the enqueue outcome so callers can map
    /// it to 202 / 503 / etc.
    pub async fn enqueue(
        &self,
        mut record: RunRecord,
    ) -> Result<EnqueueOutcome, RunManagerError> {
        let current_depth = self.queue_depth.load(Ordering::Relaxed);
        let at_capacity = current_depth >= self.concurrency.max_queued_runs;
        if at_capacity {
            match self.concurrency.on_saturation {
                SaturationPolicy::Reject => {
                    return self.finalize_rejected(record, current_depth).await;
                }
                SaturationPolicy::Queue => {
                    return Ok(EnqueueOutcome::QueueFull {
                        current_depth,
                        capacity: self.concurrency.max_queued_runs,
                    });
                }
            }
        }
        // Persist as Queued. Status invariant: enqueue always
        // produces a row with status = Queued, regardless of
        // what the caller put on the record.
        record.status = RunStatus::Queued;
        record.started_at = None;
        record.completed_at = None;
        self.persistence.put_run(&record).await?;

        // Emit Queued event so the SSE stream replays consistently.
        if let Some(sink) = &self.event_sink {
            let seq = self
                .persistence
                .next_event_seq(&record.id)
                .await
                .unwrap_or(0);
            sink.emit(RunEvent::Queued {
                run_id: record.id.clone(),
                seq,
                ts: now_ms(),
            })
            .await;
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let run_id = record.id.clone();

        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = self
            .tx
            .send(QueuedRun {
                record,
                cancel_flag,
            })
            .await
        {
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);
            return Err(RunManagerError::QueueSendFailed(e.to_string()));
        }
        Ok(EnqueueOutcome::Accepted { run_id })
    }

    /// Re-attach a recovered run (Phase C C.6 c91 boot-recovery).
    /// The caller has already:
    ///   1. queried `list_recoverable_runs`
    ///   2. reset its status to `Queued` via
    ///      `reset_non_terminal_to_queued`
    ///   3. decided whether to honor a sticky `cancel_requested`
    ///      bit
    ///
    /// `reattach` pushes the run into the dispatcher channel
    /// WITHOUT re-persisting (status is already Queued) and
    /// WITHOUT emitting a duplicate `Queued` event (one was
    /// emitted at original submission and stays in the
    /// event log for replay).
    pub async fn reattach(&self, record: RunRecord) -> Result<(), RunManagerError> {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        // queue_depth is a non-blocking bump; if the mpsc is
        // full the send below errors — recovery surfaces the
        // error to the caller so it can decide (typically: log
        // + retry on next boot).
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = self
            .tx
            .send(QueuedRun {
                record,
                cancel_flag,
            })
            .await
        {
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);
            return Err(RunManagerError::QueueSendFailed(e.to_string()));
        }
        Ok(())
    }

    /// Cancel a run.
    ///
    /// Three cases:
    ///   - **Active** — flip the in-memory cancel flag; the VM
    ///     polls it and aborts with `RunError::Cancelled`. Also
    ///     persist `cancel_requested = 1` so a controller crash
    ///     before the worker finalizes is recoverable.
    ///   - **Queued (not yet dispatched)** — persist
    ///     `cancel_requested = 1`; the dispatcher checks the bit
    ///     before promoting the run to Starting and finalizes
    ///     it as Cancelled without ever running the VM.
    ///   - **Terminal** — no-op; idempotent.
    ///
    /// Returns `Ok(true)` if the cancel took effect (or was
    /// already in-flight); `Ok(false)` for already-terminal runs.
    pub async fn cancel(&self, run_id: &RunId) -> Result<bool, RunManagerError> {
        // Active path first — cheapest + most common.
        let active_entry = {
            let guard = self.active.lock().expect("active mutex");
            guard.get(run_id).cloned()
        };
        if let Some(entry) = active_entry {
            entry.cancel_flag.store(true, Ordering::Relaxed);
            // Persist the bit so a restart mid-cancel preserves
            // intent. Best-effort: log if persistence trips.
            let _ = self
                .persistence
                .set_cancel_requested(run_id, true)
                .await;
            // Emit a Cancelling lifecycle event so SSE
            // subscribers see the in-flight transition.
            if let Some(sink) = &self.event_sink {
                let seq = self
                    .persistence
                    .next_event_seq(run_id)
                    .await
                    .unwrap_or(0);
                sink.emit(RunEvent::Cancelling {
                    run_id: run_id.clone(),
                    seq,
                    ts: now_ms(),
                })
                .await;
            }
            return Ok(true);
        }

        // Not active — consult persistence.
        let rec = match self.persistence.get_run(run_id).await {
            Ok(r) => r,
            Err(_) => return Err(RunManagerError::RunNotFound(run_id.clone())),
        };
        if rec.status.is_terminal() {
            return Ok(false);
        }
        // Queued (or stale Starting/Running from a crashed
        // controller). Mark the bit; dispatcher will skip.
        self.persistence.set_cancel_requested(run_id, true).await?;
        Ok(true)
    }

    /// Read-only snapshot of running runs.
    pub fn list_active(&self) -> Vec<ActiveRunSummary> {
        let guard = self.active.lock().expect("active mutex");
        guard
            .iter()
            .map(|(id, e)| ActiveRunSummary {
                run_id: id.clone(),
                workflow_id: e.workflow_id.clone(),
                dispatched_at: e.dispatched_at,
            })
            .collect()
    }

    pub fn metrics(&self) -> ConcurrencyMetrics {
        ConcurrencyMetrics {
            max_concurrent_runs: self.concurrency.max_concurrent_runs,
            max_queued_runs: self.concurrency.max_queued_runs,
            active_runs: self.active.lock().expect("active mutex").len(),
            queued_runs: self.queue_depth.load(Ordering::Relaxed),
            saturation_policy: self.concurrency.on_saturation,
        }
    }

    // ---- internals ----

    async fn dispatcher_loop(self, mut rx: mpsc::Receiver<QueuedRun>) {
        while let Some(queued) = rx.recv().await {
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);

            // Coalesce in-flight cancel with persisted bit so a
            // cancel arriving between enqueue + dispatch lands.
            let cancel_in_db = self
                .persistence
                .is_cancel_requested(&queued.record.id)
                .await
                .unwrap_or(false);
            let cancelled = queued.cancel_flag.load(Ordering::Relaxed) || cancel_in_db;
            if cancelled {
                self.finalize_cancelled_queued(queued.record).await;
                continue;
            }

            // Gate by concurrency. permits.acquire_owned waits
            // here if max_concurrent_runs are already in flight.
            let permit = match self.permits.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break, // semaphore closed → shut down
            };

            let me = self.clone();
            tokio::spawn(async move {
                me.execute_one(queued).await;
                drop(permit);
            });
        }
    }

    /// Run one queued workflow end-to-end. Promotes the record
    /// to Starting → Running (inside execute_run) → terminal,
    /// registering + unregistering the active entry around the
    /// VM call. Post-execution checks the cancel flag and, when
    /// set, promotes the terminal to `Cancelled` so the lifecycle
    /// matches user intent.
    async fn execute_one(self, queued: QueuedRun) {
        let run_id = queued.record.id.clone();
        let workflow_id = queued.record.workflow_id.clone();

        // Cancel could have arrived between the dispatcher's
        // pre-acquire check and now (we sat on the permit queue
        // waiting). Re-check before promoting to Starting so a
        // cancel during permit-wait still finalizes as Cancelled
        // without ever running the VM.
        let cancel_in_db = self
            .persistence
            .is_cancel_requested(&run_id)
            .await
            .unwrap_or(false);
        if queued.cancel_flag.load(Ordering::Relaxed) || cancel_in_db {
            self.finalize_cancelled_queued(queued.record).await;
            return;
        }

        // Transition Queued → Starting + persist + emit.
        if let Err(e) = self
            .transition_emit(&run_id, RunStatus::Queued, RunStatus::Starting)
            .await
        {
            tracing::error!(
                "Queued → Starting failed for {}: {e}; aborting",
                run_id
            );
            return;
        }

        // Register active entry. Cancel flag is shared with the
        // VM cancel callback + ExtCall handler in execute_run.
        let dispatched_at = now_ms();
        let entry = Arc::new(ActiveRunEntry {
            workflow_id: workflow_id.clone(),
            cancel_flag: queued.cancel_flag.clone(),
            dispatched_at,
        });
        self.active
            .lock()
            .expect("active mutex")
            .insert(run_id.clone(), entry);

        // Hand off to execute_run. It owns the Running →
        // terminal transitions (kept there to preserve the
        // event ordering it already emits — Started after
        // persistence flush, Completed/Failed after final
        // RunRecord write).
        let event_sink: Option<Arc<dyn EventSink>> = self
            .event_sink
            .clone()
            .map(|s| Arc::new(s) as Arc<dyn EventSink>);
        let connectors = Some(self.connectors.clone());
        let cancel_flag = queued.cancel_flag.clone();
        execute_run(
            self.persistence.clone(),
            queued.record,
            self.policy,
            connectors,
            event_sink,
            Some(cancel_flag.clone()),
        )
        .await;

        // Unregister + reconcile terminal status.
        self.active.lock().expect("active mutex").remove(&run_id);
        self.reconcile_post_execution(&run_id, &cancel_flag).await;
    }

    /// If user cancel arrived while the VM was inside an ExtCall
    /// (or any path that produced a `Failed` / `Succeeded`
    /// terminal), promote the terminal to `Cancelled` so the
    /// final RunRecord reflects user intent. Also clears the
    /// cancel_requested bit so a subsequent re-run of the same
    /// record starts clean.
    ///
    /// Phase C C.6 c94 — TimedOut is NOT promoted to Cancelled.
    /// The executor sets the persisted status to TimedOut when
    /// wall-clock fires (using its own internal timeout flag,
    /// distinct from user cancel_flag). If both timeout AND
    /// user cancel fired, the user cancel wins for clarity:
    /// the user explicitly asked, and TimedOut would otherwise
    /// occlude that intent.
    async fn reconcile_post_execution(
        &self,
        run_id: &RunId,
        cancel_flag: &Arc<AtomicBool>,
    ) {
        if !cancel_flag.load(Ordering::Relaxed) {
            // No user cancel was requested; nothing to reconcile.
            // Clear cancel_requested defensively (a stale bit
            // from a prior run with the same id would otherwise
            // re-trigger).
            let _ = self.persistence.set_cancel_requested(run_id, false).await;
            return;
        }
        let rec = match self.persistence.get_run(run_id).await {
            Ok(r) => r,
            Err(_) => return,
        };
        // Promote any non-Cancelled terminal to Cancelled when
        // user cancel was set. TimedOut + Succeeded + Failed all
        // get overridden — user intent is the source of truth.
        let needs_override = matches!(
            rec.status,
            RunStatus::Failed
                | RunStatus::Succeeded
                | RunStatus::TimedOut
        );
        if needs_override {
            let mut overridden = rec.clone();
            overridden.status = RunStatus::Cancelled;
            if let Err(e) = self.persistence.put_run(&overridden).await {
                tracing::error!("cancel override put_run failed: {e}");
                return;
            }
            if let Some(sink) = &self.event_sink {
                let seq = self
                    .persistence
                    .next_event_seq(run_id)
                    .await
                    .unwrap_or(0);
                sink.emit(RunEvent::Cancelled {
                    run_id: run_id.clone(),
                    seq,
                    ts: now_ms(),
                })
                .await;
            }
        }
        let _ = self.persistence.set_cancel_requested(run_id, false).await;
    }

    /// Finalize a run that was cancelled while still queued (no
    /// VM execution). Persists Cancelled directly + emits a
    /// single Cancelled event. Skips the Starting/Cancelling
    /// intermediates since we never ran.
    async fn finalize_cancelled_queued(&self, mut record: RunRecord) {
        // Lifecycle: Queued → Cancelled directly (a valid
        // transition per c89's state machine).
        record.status = RunStatus::Cancelled;
        record.completed_at = Some(now_ms());
        if let Err(e) = self.persistence.put_run(&record).await {
            tracing::error!("finalize_cancelled_queued put_run failed: {e}");
            return;
        }
        let _ = self
            .persistence
            .set_cancel_requested(&record.id, false)
            .await;
        if let Some(sink) = &self.event_sink {
            let seq = self
                .persistence
                .next_event_seq(&record.id)
                .await
                .unwrap_or(0);
            sink.emit(RunEvent::Cancelled {
                run_id: record.id,
                seq,
                ts: now_ms(),
            })
            .await;
        }
    }

    /// Finalize a rejected run (saturation = Reject).
    async fn finalize_rejected(
        &self,
        mut record: RunRecord,
        current_depth: usize,
    ) -> Result<EnqueueOutcome, RunManagerError> {
        let reason = format!(
            "queue full ({current_depth}/{cap}) under reject policy",
            cap = self.concurrency.max_queued_runs,
        );
        record.status = RunStatus::Rejected;
        record.completed_at = Some(now_ms());
        self.persistence.put_run(&record).await?;
        if let Some(sink) = &self.event_sink {
            let seq = self
                .persistence
                .next_event_seq(&record.id)
                .await
                .unwrap_or(0);
            sink.emit(RunEvent::Rejected {
                run_id: record.id.clone(),
                seq,
                ts: now_ms(),
                reason: reason.clone(),
            })
            .await;
        }
        Ok(EnqueueOutcome::Rejected {
            run_id: record.id,
            reason,
        })
    }

    /// Persist a lifecycle transition + emit the matching event.
    /// Currently handles only the Queued→Starting case the
    /// dispatcher needs; other transitions stay inside
    /// execute_run for now (their event ordering is tied to
    /// per-event payloads only execute_run can build).
    async fn transition_emit(
        &self,
        run_id: &RunId,
        from: RunStatus,
        to: RunStatus,
    ) -> Result<(), RunManagerError> {
        let _ = from.transition_to(to)?;
        let mut rec = self
            .persistence
            .get_run(run_id)
            .await
            .map_err(|_| RunManagerError::RunNotFound(run_id.clone()))?;
        // Defensive: if the persisted status doesn't match
        // `from`, the lifecycle drifted. Log + skip the event
        // (don't corrupt status). This protects against double
        // dispatch in the (very unlikely) duplicate-message
        // case.
        if rec.status != from {
            tracing::warn!(
                "transition_emit skip {run_id}: db={:?} expected={from:?}",
                rec.status
            );
            return Ok(());
        }
        rec.status = to;
        if to == RunStatus::Starting {
            rec.started_at = Some(now_ms());
        }
        self.persistence.put_run(&rec).await?;
        if let Some(sink) = &self.event_sink {
            let seq = self.persistence.next_event_seq(run_id).await.unwrap_or(0);
            let ev = match to {
                RunStatus::Starting => RunEvent::Starting {
                    run_id: run_id.clone(),
                    seq,
                    ts: now_ms(),
                },
                _ => return Ok(()),
            };
            sink.emit(ev).await;
        }
        Ok(())
    }
}

// =============================================================
//  Tests
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_sink::PersistentEventSink;
    use solflow_host_spec::RunTrigger;
    use std::time::Duration;
    use tokio::time::sleep;

    async fn fresh_workflow(p: &SqlitePersistence) -> WorkflowId {
        fresh_workflow_from(
            p,
            r#"workflow "start" { print("hi"); return 0; }"#,
        )
        .await
    }

    /// Compile + persist a workflow with a custom SOL source so
    /// tests can install slow programs the cancel path can race.
    async fn fresh_workflow_from(p: &SqlitePersistence, source: &str) -> WorkflowId {
        let bytecode = source.as_bytes().to_vec();
        let spans = b"[]".to_vec();
        let id = format!("wf_{}", uuid::Uuid::new_v4().simple());
        let meta = serde_json::json!({
            "name": "test",
            "content_hash": "test",
            "created_at": now_ms(),
        });
        p.put_workflow(&id, &bytecode, &spans, &meta.to_string())
            .await
            .unwrap();
        id
    }

    fn mk_record(workflow_id: &str) -> RunRecord {
        RunRecord {
            id: format!("run_{}", uuid::Uuid::new_v4().simple()),
            workflow_id: workflow_id.into(),
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        }
    }

    fn registry_default() -> ConnectorRegistry {
        use crate::connector::http::HttpConnector;
        use crate::Connector;
        ConnectorRegistry::builder()
            .register(Arc::new(HttpConnector::default()) as Arc<dyn Connector>)
            .build()
    }

    async fn build_manager(
        concurrency: ConcurrencyPolicy,
    ) -> (RunManager, SqlitePersistence) {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let sink = PersistentEventSink::new(p.clone());
        let mgr = RunManager::new(
            p.clone(),
            RunPolicy::default(),
            concurrency,
            registry_default(),
            Some(sink),
        );
        (mgr, p)
    }

    /// Poll until `run_id` reaches `status` or 4 s elapses.
    async fn wait_for_status(
        p: &SqlitePersistence,
        run_id: &RunId,
        status: RunStatus,
    ) -> bool {
        for _ in 0..200 {
            if let Ok(rec) = p.get_run(run_id).await {
                if rec.status == status {
                    return true;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
        false
    }

    /// Poll until `run_id` is in any terminal status.
    async fn wait_for_terminal(
        p: &SqlitePersistence,
        run_id: &RunId,
    ) -> Option<RunStatus> {
        for _ in 0..200 {
            if let Ok(rec) = p.get_run(run_id).await {
                if rec.status.is_terminal() {
                    return Some(rec.status);
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
        None
    }

    #[tokio::test]
    async fn enqueue_accepts_under_capacity_and_runs_to_completion() {
        let (mgr, p) = build_manager(ConcurrencyPolicy::default()).await;
        let wf = fresh_workflow(&p).await;
        let rec = mk_record(&wf);
        let run_id = rec.id.clone();
        let outcome = mgr.enqueue(rec).await.unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Accepted { .. }));
        assert!(
            wait_for_status(&p, &run_id, RunStatus::Succeeded).await,
            "run never reached Succeeded",
        );
    }

    #[tokio::test]
    async fn enqueue_rejects_when_policy_is_reject_and_queue_is_full() {
        let policy = ConcurrencyPolicy {
            max_concurrent_runs: 1,
            max_queued_runs: 1,
            on_saturation: SaturationPolicy::Reject,
        };
        let (mgr, p) = build_manager(policy).await;
        let wf = fresh_workflow(&p).await;
        // First enqueue fills the single queue slot. Worker may
        // or may not have started it yet; either way the second
        // should reject because queue_depth was 1.
        let first = mgr.enqueue(mk_record(&wf)).await.unwrap();
        assert!(matches!(first, EnqueueOutcome::Accepted { .. }));
        // Quickly hammer enqueues until one rejects (the worker
        // may drain the queue between calls under fast scheduling).
        let mut rejected_seen = false;
        for _ in 0..50 {
            match mgr.enqueue(mk_record(&wf)).await.unwrap() {
                EnqueueOutcome::Rejected { .. } => {
                    rejected_seen = true;
                    break;
                }
                _ => continue,
            }
        }
        assert!(rejected_seen, "expected a Rejected outcome under reject policy");
    }

    #[tokio::test]
    async fn enqueue_returns_queue_full_when_policy_is_queue_and_full() {
        let policy = ConcurrencyPolicy {
            max_concurrent_runs: 1,
            max_queued_runs: 1,
            on_saturation: SaturationPolicy::Queue,
        };
        let (mgr, p) = build_manager(policy).await;
        let wf = fresh_workflow(&p).await;
        // Spam enqueues; at least one should return QueueFull.
        let mut full_seen = false;
        for _ in 0..50 {
            match mgr.enqueue(mk_record(&wf)).await.unwrap() {
                EnqueueOutcome::QueueFull { .. } => {
                    full_seen = true;
                    break;
                }
                _ => continue,
            }
        }
        assert!(full_seen, "expected a QueueFull outcome under queue policy");
    }

    #[tokio::test]
    async fn cancel_active_run_terminates_as_cancelled() {
        // Slow workflow: ~50k step counting loop. Plenty of time
        // for cancel to land while the VM is mid-loop. The VM's
        // cancel callback then fires RunError::Cancelled which
        // execute_run records as Failed; reconcile_post_execution
        // promotes it to Cancelled because cancel_flag was set.
        let policy = ConcurrencyPolicy {
            max_concurrent_runs: 1,
            max_queued_runs: 16,
            on_saturation: SaturationPolicy::Queue,
        };
        let (mgr, p) = build_manager(policy).await;
        let wf = fresh_workflow_from(
            &p,
            // ~1M loop iterations × ~9 VM steps each ≈ 9M
            // total steps — runs hundreds of ms even in release.
            // Comfortably longer than the cancel-arrival latency.
            r#"
                workflow "start" {
                    let i: int = 0;
                    while (i < 1000000) {
                        i = i + 1;
                    }
                    return i;
                }
            "#,
        )
        .await;
        let rec = mk_record(&wf);
        let run_id = rec.id.clone();
        mgr.enqueue(rec).await.unwrap();
        // Give the dispatcher a moment to start the run.
        sleep(Duration::from_millis(30)).await;
        let cancelled = mgr.cancel(&run_id).await.unwrap();
        assert!(cancelled, "cancel should succeed on an active run");
        // execute_run records Failed (RunError::Cancelled maps to
        // it), then reconcile_post_execution promotes to Cancelled.
        // Poll specifically for Cancelled so the test waits past
        // the transient Failed state.
        assert!(
            wait_for_status(&p, &run_id, RunStatus::Cancelled).await,
            "active-run cancel should land on Cancelled (eventually)",
        );
    }

    #[tokio::test]
    async fn cancel_queued_run_finalizes_as_cancelled_before_dispatch() {
        // Pin the worker with a blocker run; the second run sits
        // in the queue while we cancel it. The dispatcher's
        // pre-dispatch check sees cancel_requested + finalizes
        // it as Cancelled without ever running the VM.
        let policy = ConcurrencyPolicy {
            max_concurrent_runs: 1,
            max_queued_runs: 16,
            on_saturation: SaturationPolicy::Queue,
        };
        let (mgr, p) = build_manager(policy).await;
        let blocker_wf = fresh_workflow_from(
            &p,
            // Blocker pins the only worker for ~200ms so the
            // second run sits in the queue / permit-wait while
            // we cancel it.
            r#"
                workflow "start" {
                    let i: int = 0;
                    while (i < 1000000) {
                        i = i + 1;
                    }
                    return i;
                }
            "#,
        )
        .await;
        // Submit the blocker — fills the only worker.
        let _ = mgr.enqueue(mk_record(&blocker_wf)).await.unwrap();
        // Brief pause so the dispatcher picks up the blocker.
        sleep(Duration::from_millis(20)).await;
        // Now this run is queued behind the blocker.
        let target_wf = fresh_workflow(&p).await;
        let queued = mk_record(&target_wf);
        let queued_id = queued.id.clone();
        mgr.enqueue(queued).await.unwrap();
        // Cancel it while it's still queued.
        assert!(mgr.cancel(&queued_id).await.unwrap());
        // It should land on Cancelled before the blocker finishes.
        let final_status = wait_for_terminal(&p, &queued_id).await;
        assert_eq!(
            final_status,
            Some(RunStatus::Cancelled),
            "queued-run cancel should land on Cancelled, got {final_status:?}",
        );
    }

    #[tokio::test]
    async fn cancel_unknown_run_is_run_not_found() {
        let (mgr, _) = build_manager(ConcurrencyPolicy::default()).await;
        let err = mgr.cancel(&"run_does_not_exist".to_string()).await.unwrap_err();
        assert!(matches!(err, RunManagerError::RunNotFound(_)));
    }

    /// Phase C C.6 c91 — boot recovery. Persist a run row
    /// directly in a non-terminal state (simulating a crashed
    /// controller), then construct a fresh RunManager + invoke
    /// the recovery sweep. Verify the run actually executes +
    /// reaches Succeeded.
    #[tokio::test]
    async fn reattach_recovers_orphaned_runs_to_terminal() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = fresh_workflow(&p).await;
        // Simulate a crash mid-run: a row with status=Running.
        let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let orphan = RunRecord {
            id: run_id.clone(),
            workflow_id: wf,
            status: RunStatus::Running,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: Some(now_ms()),
            completed_at: None,
        };
        p.put_run(&orphan).await.unwrap();
        // Fresh manager (no prior in-memory state).
        let sink = PersistentEventSink::new(p.clone());
        let mgr = RunManager::new(
            p.clone(),
            RunPolicy::default(),
            ConcurrencyPolicy::default(),
            registry_default(),
            Some(sink),
        );
        // Boot recovery: reset + reattach.
        let recovered = p.list_recoverable_runs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        let reset = p.reset_non_terminal_to_queued().await.unwrap();
        assert_eq!(reset, 1);
        for mut rec in recovered {
            rec.status = RunStatus::Queued;
            rec.started_at = None;
            rec.completed_at = None;
            mgr.reattach(rec).await.unwrap();
        }
        // Recovery success criterion: orphan run reaches a
        // terminal status. The exact terminal isn't critical
        // for c91 (clean workflow → Succeeded).
        assert!(
            wait_for_status(&p, &run_id, RunStatus::Succeeded).await,
            "recovered run never reached Succeeded",
        );
    }

    /// Phase C C.6 c91 — boot recovery honors the sticky
    /// `cancel_requested` bit: an orphan that was being
    /// cancelled when the controller crashed is finalized as
    /// Cancelled on the first dispatcher pass after reboot.
    #[tokio::test]
    async fn reattach_with_cancel_requested_finalizes_cancelled() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = fresh_workflow(&p).await;
        let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let orphan = RunRecord {
            id: run_id.clone(),
            workflow_id: wf,
            status: RunStatus::Cancelling,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: Some(now_ms()),
            completed_at: None,
        };
        p.put_run(&orphan).await.unwrap();
        // Mark the cancel intent that survived the crash.
        p.set_cancel_requested(&run_id, true).await.unwrap();

        let sink = PersistentEventSink::new(p.clone());
        let mgr = RunManager::new(
            p.clone(),
            RunPolicy::default(),
            ConcurrencyPolicy::default(),
            registry_default(),
            Some(sink),
        );
        // Boot recovery.
        let recovered = p.list_recoverable_runs().await.unwrap();
        p.reset_non_terminal_to_queued().await.unwrap();
        for mut rec in recovered {
            rec.status = RunStatus::Queued;
            rec.started_at = None;
            rec.completed_at = None;
            mgr.reattach(rec).await.unwrap();
        }
        assert!(
            wait_for_status(&p, &run_id, RunStatus::Cancelled).await,
            "recovered run with cancel_requested should land Cancelled",
        );
    }

    #[tokio::test]
    async fn metrics_reports_caps_and_current_depth() {
        let policy = ConcurrencyPolicy {
            max_concurrent_runs: 4,
            max_queued_runs: 12,
            on_saturation: SaturationPolicy::Queue,
        };
        let (mgr, _) = build_manager(policy).await;
        let m = mgr.metrics();
        assert_eq!(m.max_concurrent_runs, 4);
        assert_eq!(m.max_queued_runs, 12);
        assert_eq!(m.saturation_policy, SaturationPolicy::Queue);
    }
}
