//! `solflow_controller` — orchestration runtime.
//!
//! **Phase C C.2 — local controller MVP.** Ships:
//!   - the trait surface from C.1
//!   - `SqlitePersistence` (real SQLite-backed `Persistence` impl)
//!   - `LocalController` (real `Controller` using SQLite +
//!     `solflow_runtime`)
//!   - binary target `solflow-controller` (`src/bin/server.rs`)
//!     hosting the HTTP API
//!
//! `StubController` is retained for trait-shape tests; production
//! consumers construct `LocalController::new(...)`.
//!
//! Pending milestones:
//!   C.3 — TokioScheduler (timer + webhook triggers)
//!   C.4 — Connector framework + HTTP reference impl
//!   C.5 — Event log + WebSocket event stream
//!   C.6 — Multi-run management
//!   C.7 — Remote (TLS) controller mode
//!   C.8 — Stabilization + release
//!
//! Documentation: see
//! [`docs/dev/PHASE_C_ARCHITECTURE.md`](../../docs/dev/PHASE_C_ARCHITECTURE.md)
//! for the canonical design and
//! [`docs/dev/PHASE_C_ROADMAP.md`](../../docs/dev/PHASE_C_ROADMAP.md)
//! for the milestone delivery plan.

#![allow(async_fn_in_trait)] // C.1: traits are scaffolding; the
                              // async_trait wrapper lands when
                              // reference impls do.

pub mod executor;
pub mod local;
pub mod persistence;
pub mod server;

pub use local::LocalController;
pub use persistence::SqlitePersistence;

use async_trait::async_trait;
use solflow_host_spec::{
    Health, RunCreated, RunEvent, RunId, RunRecord, RunRequest,
    RunStatus, ScheduleCreate, ScheduleId, ScheduleRecord,
    WorkflowId, WorkflowSubmission, WorkflowSubmissionResponse,
};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

/// Errors a controller can return to the caller. Wire-stable
/// for HTTP error responses in C.2.
#[derive(Debug, Error)]
pub enum ControllerError {
    #[error("workflow not found: {id}")]
    WorkflowNotFound { id: WorkflowId },
    #[error("run not found: {id}")]
    RunNotFound { id: RunId },
    #[error("schedule not found: {id}")]
    ScheduleNotFound { id: ScheduleId },
    #[error("bytecode validation failed: {reason}")]
    BytecodeInvalid { reason: String },
    /// Returned by every stub method in this crate. Goes away as
    /// reference impls land in C.2+.
    #[error("not implemented (Phase C.1 scaffolding only): {what}")]
    NotImplemented { what: &'static str },
    /// Persistence backend failures (SQLite errors, etc.). C.2+.
    #[error("persistence error: {message}")]
    Persistence { message: String },
    /// Connector dispatch failures (timeout, auth, etc.). C.4+.
    #[error("connector error: {connector}: {message}")]
    Connector { connector: String, message: String },
}

/// Result alias shared across trait surfaces.
pub type ControllerResult<T> = Result<T, ControllerError>;

// =============================================================
//  Core trait — what a controller does
// =============================================================

/// The IDE↔controller contract from a controller's perspective.
/// Reference impl lands in C.2 as `LocalController`.
///
/// Methods correspond to the HTTP endpoints documented in
/// `PHASE_C_ARCHITECTURE.md` §5.
#[async_trait]
pub trait Controller: Send + Sync {
    /// `GET /healthz`
    async fn health(&self) -> ControllerResult<Health>;

    /// `POST /workflows`
    async fn submit_workflow(
        &self,
        submission: WorkflowSubmission,
    ) -> ControllerResult<WorkflowSubmissionResponse>;

    /// `POST /runs`
    async fn create_run(&self, request: RunRequest) -> ControllerResult<RunCreated>;

    /// `GET /runs/:id`
    async fn get_run(&self, run_id: &RunId) -> ControllerResult<RunRecord>;

    /// `DELETE /runs/:id` — best-effort cancellation.
    async fn cancel_run(&self, run_id: &RunId) -> ControllerResult<()>;

    /// `GET /workflows/:id/runs` — paginated history.
    async fn list_runs(
        &self,
        workflow_id: &WorkflowId,
        status: Option<RunStatus>,
        limit: Option<usize>,
    ) -> ControllerResult<Vec<RunRecord>>;

    /// `GET /runs/:id/events?after=N`
    async fn list_events(
        &self,
        run_id: &RunId,
        after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>>;

    /// `POST /workflows/:id/schedules` (C.3)
    async fn create_schedule(
        &self,
        workflow_id: &WorkflowId,
        create: ScheduleCreate,
    ) -> ControllerResult<ScheduleRecord>;
}

// =============================================================
//  Connector trait — how ExtCall reaches the outside world
// =============================================================

/// A controller-side dispatcher for `Inst::ExtCall`. Reference
/// impl `HttpConnector` lands in C.4.
///
/// Credentials live inside connector configuration on the
/// controller; they are NEVER transmitted to the editor.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Stable connector name. Maps to the URL prefix
    /// `connector://<name>?...` in bytecode-emitted ExtCall URLs.
    fn name(&self) -> &str;

    /// Invoke the named function with serialized args. Result is
    /// serialized back to a JSON value the VM marshals to its
    /// declared return type.
    async fn call(
        &self,
        fn_name: &str,
        args: serde_json::Value,
    ) -> ControllerResult<serde_json::Value>;
}

// =============================================================
//  Persistence trait — storage backend abstraction
// =============================================================

/// Storage abstraction so C.2 can ship SQLite while C.7+ can
/// swap to Postgres without changing the rest of the controller.
#[async_trait]
pub trait Persistence: Send + Sync {
    async fn put_workflow(
        &self,
        id: &WorkflowId,
        bytecode: &[u8],
        spans: &[u8],
        meta_json: &str,
    ) -> ControllerResult<()>;

    async fn get_workflow_bytecode(
        &self,
        id: &WorkflowId,
    ) -> ControllerResult<(Vec<u8>, Vec<u8>)>;

    async fn put_run(&self, record: &RunRecord) -> ControllerResult<()>;

    async fn get_run(&self, id: &RunId) -> ControllerResult<RunRecord>;

    async fn append_event(&self, event: &RunEvent) -> ControllerResult<()>;

    async fn list_events(
        &self,
        run_id: &RunId,
        after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>>;

    // ---- Phase C C.3 — schedules ----

    /// Insert a new schedule. Upserts on `id` so the scheduler's
    /// "advance next_fire_at" path can re-use this rather than
    /// having a separate method.
    async fn put_schedule(&self, record: &ScheduleRecord) -> ControllerResult<()>;

    /// Fetch a single schedule. Returns `ScheduleNotFound` if absent.
    async fn get_schedule(&self, id: &ScheduleId) -> ControllerResult<ScheduleRecord>;

    /// Hard-delete a schedule. No-op if the id doesn't exist.
    async fn delete_schedule(&self, id: &ScheduleId) -> ControllerResult<()>;

    /// All schedules registered against `workflow_id`. Empty when
    /// the workflow has none.
    async fn list_schedules_for_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> ControllerResult<Vec<ScheduleRecord>>;

    /// Timer-trigger schedules whose `next_fire_at <= now_ms` and
    /// are enabled. Used by the scheduler tick to pick up due
    /// triggers. Excludes Event triggers (those fire via webhook
    /// ingress, never the timer loop).
    async fn list_due_timer_schedules(
        &self,
        now_ms: i64,
    ) -> ControllerResult<Vec<ScheduleRecord>>;

    /// All enabled Event-trigger schedules. The webhook handler
    /// filters this list in-memory by `RunTrigger::Event { source }`
    /// matching the request path — paths are rare enough that we
    /// don't bother with a path-indexed query yet.
    async fn list_enabled_event_schedules(&self)
        -> ControllerResult<Vec<ScheduleRecord>>;

    /// Update an existing schedule's `next_fire_at` after the
    /// scheduler advances it past a tick. `None` clears it
    /// (used when an Event-trigger schedule is registered, since
    /// those don't carry a next-fire time).
    async fn update_schedule_next_fire(
        &self,
        id: &ScheduleId,
        next_fire_at: Option<i64>,
    ) -> ControllerResult<()>;

    /// Toggle the enabled bit. Editor "pause schedule" affordance.
    async fn set_schedule_enabled(
        &self,
        id: &ScheduleId,
        enabled: bool,
    ) -> ControllerResult<()>;
}

// =============================================================
//  Scheduler trait — fires timer/event triggers
// =============================================================

/// Trigger-fire dispatcher. Reference impl `TokioScheduler`
/// lands in C.3.
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// Register a schedule. Returns the scheduled-record with
    /// `next_fire_at` populated.
    async fn register(&self, record: ScheduleRecord) -> ControllerResult<ScheduleRecord>;

    /// Cancel + remove a schedule.
    async fn cancel(&self, id: &ScheduleId) -> ControllerResult<()>;

    /// Webhook ingress — POST to `/events/:path` lands here.
    /// Returns the created `RunRecord` so the caller can echo
    /// the run id back to the webhook sender.
    async fn ingress_event(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> ControllerResult<RunRecord>;
}

// =============================================================
//  Stub Controller — used by tests, NEVER in production
// =============================================================

/// `StubController` returns `ControllerError::NotImplemented` for
/// most operations. C.1 ships this so the editor's
/// ControllerSettingsModal can compile + render against a real
/// trait object; C.2 replaces it with `LocalController`.
///
/// In-memory only. No persistence. No threading.
pub struct StubController {
    workflows: Mutex<HashMap<WorkflowId, WorkflowSubmission>>,
    next_id: Mutex<u64>,
}

impl Default for StubController {
    fn default() -> Self {
        Self {
            workflows: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }
}

impl StubController {
    pub fn new() -> Self {
        Self::default()
    }

    fn mint_id(&self, prefix: &str) -> String {
        let mut n = self.next_id.lock().expect("not poisoned");
        let id = format!("{prefix}_{:06}", *n);
        *n += 1;
        id
    }
}

#[async_trait]
impl Controller for StubController {
    async fn health(&self) -> ControllerResult<Health> {
        Ok(Health::default())
    }

    async fn submit_workflow(
        &self,
        submission: WorkflowSubmission,
    ) -> ControllerResult<WorkflowSubmissionResponse> {
        // Minimum bytecode-shape sanity gate. Real validation
        // happens in C.2.
        if submission.bytecode.is_empty() {
            return Err(ControllerError::BytecodeInvalid {
                reason: "empty bytecode".into(),
            });
        }
        let id = self.mint_id("wf");
        self.workflows
            .lock()
            .expect("not poisoned")
            .insert(id.clone(), submission);
        Ok(WorkflowSubmissionResponse {
            workflow_id: id,
            content_hash: "stub-no-hash".into(),
        })
    }

    async fn create_run(&self, _request: RunRequest) -> ControllerResult<RunCreated> {
        Err(ControllerError::NotImplemented {
            what: "create_run lands in C.2",
        })
    }

    async fn get_run(&self, _run_id: &RunId) -> ControllerResult<RunRecord> {
        Err(ControllerError::NotImplemented {
            what: "get_run lands in C.2",
        })
    }

    async fn cancel_run(&self, _run_id: &RunId) -> ControllerResult<()> {
        Err(ControllerError::NotImplemented {
            what: "cancel_run lands in C.6",
        })
    }

    async fn list_runs(
        &self,
        _workflow_id: &WorkflowId,
        _status: Option<RunStatus>,
        _limit: Option<usize>,
    ) -> ControllerResult<Vec<RunRecord>> {
        Err(ControllerError::NotImplemented {
            what: "list_runs lands in C.2",
        })
    }

    async fn list_events(
        &self,
        _run_id: &RunId,
        _after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>> {
        Err(ControllerError::NotImplemented {
            what: "list_events lands in C.5",
        })
    }

    async fn create_schedule(
        &self,
        _workflow_id: &WorkflowId,
        _create: ScheduleCreate,
    ) -> ControllerResult<ScheduleRecord> {
        Err(ControllerError::NotImplemented {
            what: "create_schedule lands in C.3",
        })
    }
}

// =============================================================
//  Tests — verify trait shapes + stub behavior
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_host_spec::RunTrigger;

    #[tokio::test]
    async fn stub_health_returns_default() {
        let c = StubController::new();
        let h = c.health().await.expect("ok");
        assert!(h.ok);
        assert!(h.host_spec_major < 100);
    }

    #[tokio::test]
    async fn stub_submit_workflow_assigns_id() {
        let c = StubController::new();
        let r = c
            .submit_workflow(WorkflowSubmission {
                name: "test".into(),
                description: None,
                bytecode: vec![1, 2, 3],
                instruction_spans: vec![],
                source: None,
            })
            .await
            .expect("ok");
        assert!(r.workflow_id.starts_with("wf_"));
    }

    #[tokio::test]
    async fn stub_rejects_empty_bytecode() {
        let c = StubController::new();
        let err = c
            .submit_workflow(WorkflowSubmission {
                name: "empty".into(),
                description: None,
                bytecode: vec![],
                instruction_spans: vec![],
                source: None,
            })
            .await
            .expect_err("should reject");
        assert!(matches!(
            err,
            ControllerError::BytecodeInvalid { .. }
        ));
    }

    #[tokio::test]
    async fn stub_create_run_returns_not_implemented() {
        let c = StubController::new();
        let err = c
            .create_run(RunRequest {
                workflow_id: "wf_000001".into(),
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
            })
            .await
            .expect_err("stub returns NotImplemented");
        assert!(matches!(
            err,
            ControllerError::NotImplemented { what: "create_run lands in C.2" }
        ));
    }

    // Marker tests — verify trait object construction compiles.
    // If any trait grows / changes a method signature, these
    // type-checks fail at compile time.
    fn _assert_controller_object_safe() {
        let _c: Box<dyn Controller> = Box::new(StubController::new());
    }
}
