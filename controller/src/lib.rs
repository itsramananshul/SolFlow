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

pub mod canonical_exec;
pub mod connector;
pub mod event_sink;
pub mod executor;
pub mod local;
pub mod persistence;
pub mod run_manager;
pub mod scheduler;
pub mod server;
pub mod tls;

pub use event_sink::{EventSink, PersistentEventSink, RunEventCtx};
pub use local::LocalController;
pub use persistence::SqlitePersistence;
pub use run_manager::{
    ActiveRunSummary, ConcurrencyMetrics, ConcurrencyPolicy, EnqueueOutcome,
    RunManager, RunManagerError, SaturationPolicy,
};
pub use scheduler::TokioScheduler;

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
    /// Phase C C.6 c95 — explicit saturation signal. Maps to
    /// HTTP 503 with `code: "queue_full"` so editors can render
    /// a "controller busy" UX distinct from generic 5xx.
    #[error("queue full: {current_depth}/{capacity}; retry shortly")]
    QueueFull { current_depth: usize, capacity: usize },
    /// Phase C C.7 c98 — request rejected by the auth middleware.
    /// Surfaces as HTTP 401 with a structured `code` so clients
    /// can render "controller requires token" distinctly from
    /// a real 4xx in the protected handler.
    #[error("unauthorized: {reason}")]
    Unauthorized { reason: &'static str },
}

// =============================================================
//  Phase C C.7 c98 — auth configuration
// =============================================================

/// Controller authentication policy.
///
/// `Disabled` is the default + the local-dev shape. `Bearer` adds
/// a single shared bearer token; every mutating endpoint requires
/// `Authorization: Bearer <token>` and rejects with 401 otherwise.
/// `/healthz` is ALWAYS open so editors can probe the controller
/// before sending credentials, and CORS preflight (`OPTIONS`)
/// stays open so browsers can establish cross-origin sessions.
///
/// This is intentionally NOT enterprise auth — no per-user
/// principals, no RBAC, no rotating tokens. It's the
/// "remote-controller safety scaffold" the roadmap names: enough
/// to keep a public HTTPS controller from being world-writable,
/// not enough to claim multi-tenant safety. Phase D is where the
/// full identity model lands.
#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// No authentication enforced. Default; matches pre-C.7 builds.
    Disabled,
    /// Single shared bearer token. Clients send
    /// `Authorization: Bearer <token>` on every protected endpoint.
    /// The token is compared in **constant time** to defeat
    /// timing-side-channel guessing.
    Bearer { token: String },
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::Disabled
    }
}

impl AuthConfig {
    /// Construct from an optional token. `None` (or `Some("")`)
    /// disables auth; any non-empty string enables it. Binaries
    /// call this with the value of `SOLFLOW_CONTROLLER_AUTH_TOKEN`
    /// so empty env → disabled, set env → enabled.
    pub fn from_env_token(token: Option<String>) -> Self {
        match token {
            Some(t) if !t.is_empty() => AuthConfig::Bearer { token: t },
            _ => AuthConfig::Disabled,
        }
    }

    /// True iff a token is required to call protected endpoints.
    pub fn is_required(&self) -> bool {
        matches!(self, AuthConfig::Bearer { .. })
    }

    /// Constant-time bearer-token comparison. Returns Ok(()) when
    /// the supplied `presented` matches the configured token, or
    /// `Err` describing whether the header was missing /
    /// malformed / mismatched. `Disabled` always returns Ok(()).
    ///
    /// We compare every byte regardless of mismatch position so
    /// the response latency leaks nothing about how close a guess
    /// got. Length difference still leaks (it can't be helped
    /// without padding the comparison), but bearer tokens are
    /// fixed-length per deployment.
    pub fn verify(&self, presented: Option<&str>) -> Result<(), AuthFailure> {
        match self {
            AuthConfig::Disabled => Ok(()),
            AuthConfig::Bearer { token } => {
                let raw = presented.ok_or(AuthFailure::Missing)?;
                let stripped = raw
                    .strip_prefix("Bearer ")
                    .or_else(|| raw.strip_prefix("bearer "))
                    .ok_or(AuthFailure::Malformed)?;
                if constant_time_eq(stripped.as_bytes(), token.as_bytes()) {
                    Ok(())
                } else {
                    Err(AuthFailure::Mismatch)
                }
            }
        }
    }
}

/// Reason an auth check failed. Mapped by the middleware into a
/// structured 401 + a discriminating `code` (`auth_missing` /
/// `auth_malformed` / `auth_mismatch`) so the editor can render
/// distinct UX per case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    /// No `Authorization` header on the request.
    Missing,
    /// Header present but doesn't start with `Bearer `.
    Malformed,
    /// Header present, scheme correct, but token doesn't match.
    Mismatch,
}

impl AuthFailure {
    pub fn code(self) -> &'static str {
        match self {
            AuthFailure::Missing => "auth_missing",
            AuthFailure::Malformed => "auth_malformed",
            AuthFailure::Mismatch => "auth_mismatch",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            AuthFailure::Missing => "missing Authorization header",
            AuthFailure::Malformed => "malformed Authorization header",
            AuthFailure::Mismatch => "bearer token mismatch",
        }
    }
}

/// Constant-time byte comparison. Stdlib lacks one and pulling a
/// crate for ~6 lines is more dependency than this needs.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
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
//  Connector framework
// =============================================================
//
// The C.1 stub trait that lived here is replaced by the richer
// surface in `crate::connector`. ExtCall lookup, structured
// errors, the URL parser, and the registry all live in that
// module; reference connectors (HTTP, etc.) live as submodules.

pub use connector::{
    Connector, ConnectorError, ConnectorInvocation, ConnectorMeta,
    ConnectorOutcome, ConnectorRegistry, InvocationPolicy,
    ParsedConnectorRef,
};

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

    // ----- Phase C C.7 c98 — AuthConfig -----

    #[test]
    fn auth_config_default_is_disabled() {
        assert!(matches!(AuthConfig::default(), AuthConfig::Disabled));
        assert!(!AuthConfig::default().is_required());
    }

    #[test]
    fn auth_config_from_env_token_handles_all_shapes() {
        assert!(!AuthConfig::from_env_token(None).is_required());
        assert!(!AuthConfig::from_env_token(Some(String::new())).is_required());
        assert!(AuthConfig::from_env_token(Some("s3cret".into())).is_required());
    }

    #[test]
    fn auth_disabled_accepts_anything() {
        let cfg = AuthConfig::Disabled;
        assert!(cfg.verify(None).is_ok());
        assert!(cfg.verify(Some("anything")).is_ok());
        assert!(cfg.verify(Some("Bearer bogus")).is_ok());
    }

    #[test]
    fn auth_bearer_verify_missing_malformed_mismatch_match() {
        let cfg = AuthConfig::Bearer { token: "abc123".into() };
        assert_eq!(cfg.verify(None), Err(AuthFailure::Missing));
        assert_eq!(
            cfg.verify(Some("Token abc123")),
            Err(AuthFailure::Malformed),
        );
        assert_eq!(
            cfg.verify(Some("Bearer wrong")),
            Err(AuthFailure::Mismatch),
        );
        assert!(cfg.verify(Some("Bearer abc123")).is_ok());
        // Case-insensitive scheme keyword.
        assert!(cfg.verify(Some("bearer abc123")).is_ok());
    }

    #[test]
    fn auth_failure_codes_and_reasons_distinct() {
        assert_ne!(
            AuthFailure::Missing.code(),
            AuthFailure::Mismatch.code(),
        );
        assert_ne!(
            AuthFailure::Malformed.code(),
            AuthFailure::Mismatch.code(),
        );
    }
}
