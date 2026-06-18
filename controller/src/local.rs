//! `LocalController` — C.2 reference implementation of the
//! `Controller` trait.
//!
//! Wraps `SqlitePersistence` and a fixed `RunPolicy`. Spawns
//! per-run tokio tasks via `executor::execute_run`. All persistence
//! through the same pool means run history survives controller
//! restarts.
//!
//! Limits documented in the architecture (architecture §10.2)
//! apply via the `RunPolicy` builder.

use crate::connector::{http::HttpConnector, ConnectorMeta, ConnectorRegistry};
use crate::event_sink::PersistentEventSink;
use crate::executor::{now_ms, RunPolicy};
use crate::run_manager::{ConcurrencyPolicy, RunManager};
use crate::scheduler::TokioScheduler;
use crate::{
    AuthConfig, Connector, Controller, ControllerError, ControllerResult,
    Persistence, SqlitePersistence,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use solflow_host_spec::{
    Health, RunCreated, RunEvent, RunId, RunRecord, RunRequest,
    RunStatus, ScheduleCreate, ScheduleId, ScheduleRecord, WorkflowId,
    WorkflowSubmission, WorkflowSubmissionResponse, CONTROLLER_NAME,
    HOST_SPEC_MAJOR,
};
use std::sync::Arc;

/// C.2 + C.3 + C.4 controller. Single-process, SQLite-backed,
/// auto-spawns the scheduler on construction.
///
/// The connector registry is part of the controller's identity:
/// every run + every scheduler-triggered run dispatches ExtCall
/// through this registry. `LocalController::new()` registers the
/// HTTP reference connector by default; add more via
/// `with_connector(...)`.
#[derive(Clone)]
pub struct LocalController {
    persistence: Arc<SqlitePersistence>,
    policy: RunPolicy,
    scheduler: TokioScheduler,
    connectors: ConnectorRegistry,
    /// Phase C C.5: every run emits RunEvents through this sink.
    /// The sink fan-outs to SQLite persistence (run_events table)
    /// AND to an in-process broadcast channel the SSE endpoint
    /// subscribes to.
    event_sink: PersistentEventSink,
    /// Phase C C.6 c90: real orchestration coordinator. Every
    /// run (manual + scheduler-triggered) goes through here so
    /// concurrency caps + saturation policy + cancellation are
    /// honored consistently.
    run_manager: RunManager,
    /// Cached concurrency policy so the HTTP layer can surface
    /// it without poking through the manager.
    concurrency: ConcurrencyPolicy,
    /// Phase C C.7 c98 — auth policy. `Disabled` by default;
    /// when set to `Bearer`, the HTTP middleware enforces the
    /// token and `Health::auth_required` flips to `true` so
    /// editors can probe before connecting.
    auth: AuthConfig,
}

impl LocalController {
    /// Construct a controller. Tests use this in isolation —
    /// the scheduler tick loop isn't running yet. Production
    /// (the binary) chains `.with_policy(...)` then
    /// `.start_scheduler()` so timer triggers fire.
    ///
    /// HTTP connector is registered by default; replace with
    /// `with_connector_registry()` if you need a custom set
    /// (e.g. tests using an allowlist-restricted HttpConnector).
    pub fn new(persistence: SqlitePersistence) -> Self {
        let policy = RunPolicy::default();
        let connectors = default_connector_registry();
        let event_sink = PersistentEventSink::new(persistence.clone());
        let concurrency = ConcurrencyPolicy::default();
        let run_manager = RunManager::new(
            persistence.clone(),
            policy,
            concurrency,
            connectors.clone(),
            Some(event_sink.clone()),
        );
        let scheduler = TokioScheduler::new(persistence.clone(), policy)
            .with_connectors(connectors.clone())
            .with_event_sink(event_sink.clone())
            .with_run_manager(run_manager.clone());
        Self {
            persistence: Arc::new(persistence),
            policy,
            scheduler,
            connectors,
            event_sink,
            run_manager,
            concurrency,
            auth: AuthConfig::default(),
        }
    }

    /// Phase C C.7 c98 — install an auth policy. Builder-style;
    /// chain before `start_scheduler()`. Passing `AuthConfig::
    /// Disabled` reverts to the default open-controller behavior.
    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = auth;
        self
    }

    /// Access the configured auth policy. The server router clones
    /// this into its auth middleware closure on construction.
    pub fn auth(&self) -> &AuthConfig {
        &self.auth
    }

    /// Replace the run policy. Builder-style; safe to chain
    /// before `start_scheduler()`. After the scheduler has
    /// started, policy changes don't take effect on the
    /// already-spawned tick loop — re-create the controller if
    /// you need that.
    pub fn with_policy(mut self, policy: RunPolicy) -> Self {
        self.policy = policy;
        // Rebuild RunManager so the new policy reaches every
        // future run. The previous manager's dispatcher is
        // orphaned (its mpsc sender is dropped via Self overwrite);
        // the loop exits naturally on the next recv() returning None.
        self.run_manager = RunManager::new(
            (*self.persistence).clone(),
            self.policy,
            self.concurrency,
            self.connectors.clone(),
            Some(self.event_sink.clone()),
        );
        self.scheduler = TokioScheduler::new(
            (*self.persistence).clone(),
            self.policy,
        )
        .with_connectors(self.connectors.clone())
        .with_event_sink(self.event_sink.clone())
        .with_run_manager(self.run_manager.clone());
        self
    }

    /// Phase C C.6 c90 — replace the concurrency policy.
    pub fn with_concurrency_policy(mut self, concurrency: ConcurrencyPolicy) -> Self {
        self.concurrency = concurrency;
        self.run_manager = RunManager::new(
            (*self.persistence).clone(),
            self.policy,
            self.concurrency,
            self.connectors.clone(),
            Some(self.event_sink.clone()),
        );
        self
    }

    /// Replace the connector registry. Useful for tests (register
    /// a mock connector) and for production deployments that want
    /// a non-default HTTP connector (e.g. with an allowlist).
    pub fn with_connector_registry(mut self, connectors: ConnectorRegistry) -> Self {
        self.connectors = connectors.clone();
        self.run_manager = RunManager::new(
            (*self.persistence).clone(),
            self.policy,
            self.concurrency,
            connectors.clone(),
            Some(self.event_sink.clone()),
        );
        self.scheduler =
            TokioScheduler::new((*self.persistence).clone(), self.policy)
                .with_connectors(connectors)
                .with_event_sink(self.event_sink.clone())
                .with_run_manager(self.run_manager.clone());
        self
    }

    /// Add a connector to the registry. Convenience: rebuilds
    /// the registry with the existing connectors plus the new one.
    pub fn with_connector(self, connector: Arc<dyn Connector>) -> Self {
        let mut builder = ConnectorRegistry::builder();
        for meta in self.connectors.list_meta() {
            // Re-register existing connectors via lookup
            // (registry stores Arc<dyn Connector>, list_meta only
            // exposes meta — but lookup gives the Arc).
            if let Ok(c) = self.connectors.lookup(&meta.name) {
                builder = builder.register(c);
            }
        }
        builder = builder.register(connector);
        self.with_connector_registry(builder.build())
    }

    /// Spawn the scheduler tick loop. Idempotent — calling
    /// twice on the same scheduler is a no-op (the second call
    /// returns a do-nothing JoinHandle). The binary calls this
    /// after wiring policy; tests skip it so they don't race
    /// against a background tick.
    pub fn start_scheduler(&self) -> tokio::task::JoinHandle<()> {
        self.scheduler.start()
    }

    /// Phase C C.6 c91 — boot recovery. Sweeps every run row
    /// the controller left in a non-terminal status (Queued,
    /// Starting, Running, Cancelling) and re-attaches it into
    /// the orchestration queue.
    ///
    /// Semantics:
    ///   - A `Queued` row never picked up before the crash is
    ///     simply re-dispatched.
    ///   - `Starting` / `Running` / `Cancelling` rows are
    ///     **at-least-once**: the workflow side-effects already
    ///     performed (ExtCalls fired) may execute again on
    ///     retry. The Phase C contract documents this; workflow
    ///     authors are responsible for idempotency.
    ///   - `cancel_requested = 1` survives the restart, so a
    ///     mid-cancel run finalizes as `Cancelled` on the first
    ///     dispatcher pass after reboot (the cancel check fires
    ///     before promotion to Starting).
    ///
    /// Returns the number of rows recovered. Idempotent — safe
    /// to call multiple times (subsequent calls see no
    /// non-terminal rows).
    pub async fn recover_runs(&self) -> ControllerResult<u64> {
        // Order matters: list first (so we capture pre-reset
        // IDs), then reset (so the run_manager.reattach pushes
        // records that already have status=Queued in the DB).
        let recoverable = self.persistence.list_recoverable_runs().await?;
        if recoverable.is_empty() {
            return Ok(0);
        }
        self.persistence.reset_non_terminal_to_queued().await?;
        let mut count: u64 = 0;
        for mut rec in recoverable {
            // Status was set by reset_non_terminal_to_queued;
            // mirror it in the in-memory record we hand to the
            // queue so the dispatcher's lifecycle assumptions
            // hold.
            rec.status = RunStatus::Queued;
            rec.started_at = None;
            rec.completed_at = None;
            match self.run_manager.reattach(rec).await {
                Ok(()) => count += 1,
                Err(e) => {
                    tracing::error!("recover_runs reattach failed: {e}");
                }
            }
        }
        tracing::info!("boot recovery re-enqueued {count} runs");
        Ok(count)
    }

    /// Expose persistence so the HTTP server (and tests) can
    /// run extra queries that aren't on the trait yet
    /// (history listing, schedule listing, etc.).
    pub fn persistence(&self) -> &SqlitePersistence {
        &self.persistence
    }

    /// Expose the scheduler for the HTTP layer (webhook ingress
    /// + schedule registration go through here).
    pub fn scheduler(&self) -> &TokioScheduler {
        &self.scheduler
    }

    /// Expose the connector registry — the HTTP layer's
    /// `GET /connectors` returns `list_meta()` from here.
    pub fn connectors(&self) -> &ConnectorRegistry {
        &self.connectors
    }

    /// List registered-connector metadata.
    pub fn list_connectors(&self) -> Vec<ConnectorMeta> {
        self.connectors.list_meta()
    }

    /// Expose the event sink so the HTTP layer's SSE endpoint
    /// can subscribe to the in-process broadcast (Phase C C.5).
    pub fn event_sink(&self) -> &PersistentEventSink {
        &self.event_sink
    }

    /// Phase C C.6 — expose the RunManager so the HTTP layer
    /// can serve `/runs/active` + `/controller/concurrency` and
    /// the scheduler (c91) can enqueue through it.
    pub fn run_manager(&self) -> &RunManager {
        &self.run_manager
    }

    /// Phase C C.7 — whether protected endpoints require a
    /// bearer token. Surfaced in `/healthz` so editors can probe
    /// a controller before connecting. Reads through the
    /// configured `AuthConfig` (c98).
    pub fn auth_required(&self) -> bool {
        self.auth.is_required()
    }
}

fn default_connector_registry() -> ConnectorRegistry {
    ConnectorRegistry::builder()
        .register(Arc::new(HttpConnector::default()) as Arc<dyn Connector>)
        .build()
}

#[async_trait]
impl Controller for LocalController {
    async fn health(&self) -> ControllerResult<Health> {
        Ok(Health {
            ok: true,
            controller_version: env!("CARGO_PKG_VERSION").to_string(),
            host_spec_major: HOST_SPEC_MAJOR,
            name: CONTROLLER_NAME.to_string(),
            // Phase C C.7 c98 will populate this from AuthConfig.
            // c97 keeps it `false` so existing test fixtures keep
            // their meaning ("no auth required").
            auth_required: self.auth_required(),
        })
    }

    async fn submit_workflow(
        &self,
        submission: WorkflowSubmission,
    ) -> ControllerResult<WorkflowSubmissionResponse> {
        if submission.bytecode.is_empty() {
            return Err(ControllerError::BytecodeInvalid {
                reason: "empty bytecode".into(),
            });
        }
        // Content hash for replay + audit. SHA-256 of the
        // bytecode bytes. Same workflow submitted twice gets a
        // new id (no de-dup here; that's a C.7 concern when
        // multi-tenant identity matters).
        let mut h = Sha256::new();
        h.update(&submission.bytecode);
        h.update(&submission.instruction_spans);
        let hash = hex::encode(h.finalize());

        let id = format!("wf_{}", uuid::Uuid::new_v4().simple());
        let meta = serde_json::json!({
            "name": submission.name,
            "description": submission.description,
            "content_hash": hash,
            "created_at": now_ms(),
            "source": submission.source,
        });
        self.persistence
            .put_workflow(
                &id,
                &submission.bytecode,
                &submission.instruction_spans,
                &meta.to_string(),
            )
            .await?;
        Ok(WorkflowSubmissionResponse {
            workflow_id: id,
            content_hash: hash,
        })
    }

    async fn create_run(&self, request: RunRequest) -> ControllerResult<RunCreated> {
        // Verify workflow exists before we accept the run.
        let _ = self
            .persistence
            .get_workflow_bytecode(&request.workflow_id)
            .await?;

        let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let record = RunRecord {
            id: run_id.clone(),
            workflow_id: request.workflow_id,
            status: RunStatus::Queued,
            trigger: request.trigger,
            inputs: request.inputs,
            output: None,
            diagnostics: Vec::new(),
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
        };

        // Phase C C.6 c90 — delegate to RunManager. It persists,
        // applies concurrency policy, and spawns through the
        // worker pool. The Controller trait's create_run returns
        // a RunCreated regardless of enqueue outcome; for
        // Rejected we still return the run id (client sees
        // terminal Rejected when it polls).
        let outcome = self.run_manager.enqueue(record).await.map_err(|e| match e {
            crate::run_manager::RunManagerError::Persistence(ctrl_err) => ctrl_err,
            other => ControllerError::Persistence {
                message: other.to_string(),
            },
        })?;
        let (returned_id, status) = match outcome {
            crate::run_manager::EnqueueOutcome::Accepted { run_id } => {
                (run_id, RunStatus::Queued)
            }
            crate::run_manager::EnqueueOutcome::Rejected { run_id, reason: _ } => {
                (run_id, RunStatus::Rejected)
            }
            crate::run_manager::EnqueueOutcome::QueueFull {
                current_depth,
                capacity,
            } => {
                return Err(ControllerError::QueueFull {
                    current_depth,
                    capacity,
                });
            }
        };
        Ok(RunCreated {
            run_id: returned_id,
            status,
        })
    }

    async fn get_run(&self, run_id: &RunId) -> ControllerResult<RunRecord> {
        self.persistence.get_run(run_id).await
    }

    async fn cancel_run(&self, run_id: &RunId) -> ControllerResult<()> {
        // Phase C C.6 c90 — real cancellation via RunManager.
        // RunManager handles the three cases (active / queued /
        // already-terminal) and returns a bool indicating whether
        // anything was cancelled. Terminal-already returns Ok(())
        // (idempotent); other RunManager errors map to
        // RunNotFound / Persistence.
        match self.run_manager.cancel(run_id).await {
            Ok(_) => Ok(()),
            Err(crate::run_manager::RunManagerError::RunNotFound(id)) => {
                Err(ControllerError::RunNotFound { id })
            }
            Err(crate::run_manager::RunManagerError::Persistence(e)) => Err(e),
            Err(other) => Err(ControllerError::Persistence {
                message: other.to_string(),
            }),
        }
    }

    async fn list_runs(
        &self,
        workflow_id: &WorkflowId,
        status: Option<RunStatus>,
        limit: Option<usize>,
    ) -> ControllerResult<Vec<RunRecord>> {
        self.persistence.list_runs(workflow_id, status, limit).await
    }

    async fn list_events(
        &self,
        _run_id: &RunId,
        _after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>> {
        // C.5 ships event storage. Returns empty list until then
        // so clients can call this without seeing NotImplemented
        // errors — `[]` means "no events recorded yet" which is
        // technically true.
        Ok(Vec::new())
    }

    async fn create_schedule(
        &self,
        workflow_id: &WorkflowId,
        create: ScheduleCreate,
    ) -> ControllerResult<ScheduleRecord> {
        // Build a fresh ScheduleRecord from the request; the
        // scheduler fills in id + next_fire_at + created_at.
        let record = ScheduleRecord {
            id: String::new(),
            workflow_id: workflow_id.clone(),
            trigger: create.trigger,
            enabled: create.enabled,
            next_fire_at: None,
            created_at: 0,
        };
        self.scheduler.register(record).await
    }
}

// =============================================================
//  Non-trait schedule helpers exposed to the HTTP layer (C.3)
// =============================================================

impl LocalController {
    /// `GET /workflows/:id/schedules`
    pub async fn list_schedules_for_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> ControllerResult<Vec<ScheduleRecord>> {
        self.persistence
            .list_schedules_for_workflow(workflow_id)
            .await
    }

    /// `GET /schedules/:id`
    pub async fn get_schedule(&self, id: &ScheduleId) -> ControllerResult<ScheduleRecord> {
        self.persistence.get_schedule(id).await
    }

    /// `DELETE /schedules/:id`. Idempotent.
    pub async fn cancel_schedule(&self, id: &ScheduleId) -> ControllerResult<()> {
        self.scheduler.cancel(id).await
    }

    /// `PATCH /schedules/:id` with `{ enabled }`. Returns the
    /// updated record so the client can refresh its UI.
    pub async fn set_schedule_enabled(
        &self,
        id: &ScheduleId,
        enabled: bool,
    ) -> ControllerResult<ScheduleRecord> {
        self.persistence.set_schedule_enabled(id, enabled).await?;
        // If we just enabled a Timer schedule whose next_fire_at
        // is in the past (the disable-then-enable cycle leaves
        // the old next_fire_at), the tick loop will fire it
        // immediately. That matches a user's expectation when
        // they hit Enable: it'll fire as soon as it can.
        self.get_schedule(id).await
    }

    /// `POST /events/*path`. Returns the first created run.
    pub async fn ingress_event(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> ControllerResult<RunRecord> {
        self.scheduler.ingress_event(path, body).await
    }
}

// =============================================================
//  Tests — end-to-end (submit → create_run → poll → verify)
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_host_spec::RunTrigger;
    use std::time::Duration;
    use tokio::time::sleep;

    async fn run_clean_workflow_through_local_controller(source: &str) -> RunRecord {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);

        // The editor submits the workflow source as raw UTF-8 bytes;
        // the controller reads it back and runs it through the
        // canonical VM. Mirror that exactly here.
        let bytecode = source.as_bytes().to_vec();
        let spans = b"[]".to_vec();
        let submission = WorkflowSubmission {
            name: "test".into(),
            description: None,
            bytecode,
            instruction_spans: spans,
            source: Some(source.to_string()),
        };
        let resp = controller.submit_workflow(submission).await.unwrap();
        let req = RunRequest {
            workflow_id: resp.workflow_id,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
        };
        let created = controller.create_run(req).await.unwrap();

        // Poll for completion. Default policy gives 600s wall-
        // clock; tiny test programs finish in <10ms but we loop
        // for safety.
        for _ in 0..50 {
            sleep(Duration::from_millis(20)).await;
            let r = controller.get_run(&created.run_id).await.unwrap();
            if r.status == RunStatus::Succeeded || r.status == RunStatus::Failed {
                return r;
            }
        }
        panic!("run didn't complete in time");
    }

    #[tokio::test]
    async fn submit_create_poll_hello_world() {
        let r = run_clean_workflow_through_local_controller(
            r#"workflow "start" {
                 print("hello");
                 print("world");
                 return 0;
               }"#,
        )
        .await;
        assert_eq!(r.status, RunStatus::Succeeded);
        let out = r.output.unwrap();
        assert_eq!(out.return_value, Some(0));
        assert_eq!(out.output, vec!["hello".to_string(), "world".to_string()]);
    }

    /// Phase C C.6 c90 — cancel_run is real now. An unknown run
    /// id returns `RunNotFound`; the cancel-an-existing-run path
    /// is covered end-to-end in run_manager::tests.
    #[tokio::test]
    async fn cancel_run_unknown_id_returns_run_not_found() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        let err = controller
            .cancel_run(&"run_does_not_exist".into())
            .await
            .expect_err("unknown run");
        assert!(matches!(err, ControllerError::RunNotFound { .. }));
    }

    #[tokio::test]
    async fn list_events_returns_empty_in_c2() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        let events = controller
            .list_events(&"run_xxx".into(), 0)
            .await
            .unwrap();
        assert!(events.is_empty(), "C.2: list_events returns [] until C.5");
    }

    #[tokio::test]
    async fn submit_empty_bytecode_rejected() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        let err = controller
            .submit_workflow(WorkflowSubmission {
                name: "empty".into(),
                description: None,
                bytecode: vec![],
                instruction_spans: vec![],
                source: None,
            })
            .await
            .expect_err("rejected");
        assert!(matches!(err, ControllerError::BytecodeInvalid { .. }));
    }

    #[tokio::test]
    async fn create_run_unknown_workflow_returns_not_found() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        let err = controller
            .create_run(RunRequest {
                workflow_id: "wf_nonexistent".into(),
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
            })
            .await
            .expect_err("not found");
        assert!(matches!(err, ControllerError::WorkflowNotFound { .. }));
    }

    // =============================================================
    //  Phase C C.4 c77 — end-to-end ExtCall through HTTP connector
    // =============================================================

    /// End-to-end ExtCall through an HTTP connector via the canonical
    /// engine: `canonical_exec` resolves the Action's module to a
    /// registered connector endpoint and POSTs to it for real.
    ///
    /// Ignored by default — it configures the connector registry via
    /// the process-global `SOLFLOW_CONNECTORS` env var, which is unsafe
    /// to set while the rest of the suite runs in parallel. Run it
    /// explicitly:
    ///   cargo test -p solflow_controller \
    ///     ext_call_runs_through_http_connector_end_to_end -- --ignored --test-threads=1
    #[tokio::test]
    #[ignore = "sets process-global SOLFLOW_CONNECTORS; run with --ignored --test-threads=1"]
    async fn ext_call_runs_through_http_connector_end_to_end() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(42_i64))
            .mount(&server)
            .await;

        // Register the `scale` module's connector endpoint the way the
        // controller resolves them (canonical_exec::load_connectors).
        unsafe {
            std::env::set_var(
                "SOLFLOW_CONNECTORS",
                serde_json::json!({ "scale": server.uri() }).to_string(),
            );
        }

        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        // Canonical source: a workflow that calls the `scale` capability
        // and returns its result. Submitted as raw source bytes, exactly
        // as the editor submits.
        let source = r#"workflow "main" { return scale.run({ "value": 21 }); }"#;
        let submission = WorkflowSubmission {
            name: "extcall-e2e".into(),
            description: None,
            bytecode: source.as_bytes().to_vec(),
            instruction_spans: b"[]".to_vec(),
            source: Some(source.to_string()),
        };
        let resp = controller.submit_workflow(submission).await.unwrap();
        let created = controller
            .create_run(RunRequest {
                workflow_id: resp.workflow_id,
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Poll until terminal.
        for _ in 0..200 {
            sleep(Duration::from_millis(20)).await;
            let r = controller.get_run(&created.run_id).await.unwrap();
            if r.status == RunStatus::Succeeded || r.status == RunStatus::Failed {
                unsafe { std::env::remove_var("SOLFLOW_CONNECTORS") };
                assert_eq!(
                    r.status,
                    RunStatus::Succeeded,
                    "expected Succeeded; got record={r:?}",
                );
                let out = r.output.unwrap();
                assert_eq!(
                    out.return_value,
                    Some(42),
                    "controller-side ExtCall round-trip returned wrong value",
                );
                return;
            }
        }
        unsafe { std::env::remove_var("SOLFLOW_CONNECTORS") };
        panic!("end-to-end ExtCall run didn't complete in time");
    }

    #[tokio::test]
    async fn ext_call_unknown_connector_fails_with_extcall_failed() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        // A capability whose module has no registered connector must
        // fail clearly — canonical_exec blocks it (ExtCallBlocked).
        let source = r#"workflow "main" { return nope.run({}); }"#;
        let submission = WorkflowSubmission {
            name: "extcall-unknown".into(),
            description: None,
            bytecode: source.as_bytes().to_vec(),
            instruction_spans: b"[]".to_vec(),
            source: Some(source.to_string()),
        };
        let resp = controller.submit_workflow(submission).await.unwrap();
        let created = controller
            .create_run(RunRequest {
                workflow_id: resp.workflow_id,
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
            })
            .await
            .unwrap();
        for _ in 0..50 {
            sleep(Duration::from_millis(20)).await;
            let r = controller.get_run(&created.run_id).await.unwrap();
            if r.status == RunStatus::Failed {
                return; // The reason line goes into output for C.4
            }
            if r.status == RunStatus::Succeeded {
                panic!("expected failure for unknown connector, got success");
            }
        }
        panic!("run didn't fail in time for unknown connector");
    }

}
