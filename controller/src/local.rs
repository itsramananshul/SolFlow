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

use crate::executor::{execute_run, now_ms, RunPolicy};
use crate::scheduler::TokioScheduler;
use crate::{Controller, ControllerError, ControllerResult, Persistence, SqlitePersistence};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use solflow_host_spec::{
    Health, RunCreated, RunEvent, RunId, RunRecord, RunRequest,
    RunStatus, ScheduleCreate, ScheduleId, ScheduleRecord, WorkflowId,
    WorkflowSubmission, WorkflowSubmissionResponse, HOST_SPEC_MAJOR,
};
use std::sync::Arc;

/// C.2 + C.3 controller. Single-process, SQLite-backed,
/// auto-spawns the scheduler on construction.
#[derive(Clone)]
pub struct LocalController {
    persistence: Arc<SqlitePersistence>,
    policy: RunPolicy,
    scheduler: TokioScheduler,
}

impl LocalController {
    /// Construct a controller. Tests use this in isolation —
    /// the scheduler tick loop isn't running yet. Production
    /// (the binary) chains `.with_policy(...)` then
    /// `.start_scheduler()` so timer triggers fire.
    pub fn new(persistence: SqlitePersistence) -> Self {
        let policy = RunPolicy::default();
        let scheduler = TokioScheduler::new(persistence.clone(), policy);
        Self {
            persistence: Arc::new(persistence),
            policy,
            scheduler,
        }
    }

    /// Replace the run policy. Builder-style; safe to chain
    /// before `start_scheduler()`. After the scheduler has
    /// started, policy changes don't take effect on the
    /// already-spawned tick loop — re-create the controller if
    /// you need that.
    pub fn with_policy(mut self, policy: RunPolicy) -> Self {
        self.policy = policy;
        self.scheduler = TokioScheduler::new(
            (*self.persistence).clone(),
            self.policy,
        );
        self
    }

    /// Spawn the scheduler tick loop. Idempotent — calling
    /// twice on the same scheduler is a no-op (the second call
    /// returns a do-nothing JoinHandle). The binary calls this
    /// after wiring policy; tests skip it so they don't race
    /// against a background tick.
    pub fn start_scheduler(&self) -> tokio::task::JoinHandle<()> {
        self.scheduler.start()
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
}

#[async_trait]
impl Controller for LocalController {
    async fn health(&self) -> ControllerResult<Health> {
        Ok(Health {
            ok: true,
            controller_version: env!("CARGO_PKG_VERSION").to_string(),
            host_spec_major: HOST_SPEC_MAJOR,
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
        self.persistence.put_run(&record).await?;

        // Spawn execution in the background. Caller's response
        // returns immediately; client polls GET /runs/:id for
        // completion. (Event stream lands in C.5.)
        let p = (*self.persistence).clone();
        let r = record.clone();
        let policy = self.policy;
        tokio::spawn(async move {
            execute_run(p, r, policy, None).await;
        });

        Ok(RunCreated {
            run_id,
            status: RunStatus::Queued,
        })
    }

    async fn get_run(&self, run_id: &RunId) -> ControllerResult<RunRecord> {
        self.persistence.get_run(run_id).await
    }

    async fn cancel_run(&self, _run_id: &RunId) -> ControllerResult<()> {
        // C.6 ships real cancellation. C.2 returns NotImplemented
        // explicitly so callers don't see a silent success.
        Err(ControllerError::NotImplemented {
            what: "cancel_run lands in C.6",
        })
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
    use solflow_compiler::compile_source;
    use solflow_host_spec::{encode_bytecode, encode_instruction_spans, RunTrigger};
    use std::time::Duration;
    use tokio::time::sleep;

    async fn run_clean_workflow_through_local_controller(source: &str) -> RunRecord {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);

        let compiled = compile_source(source);
        let cp = compiled.value.expect("compile clean");
        let bytecode = encode_bytecode(&cp.bytecode).unwrap();
        let host_spans: Vec<Option<solflow_host_spec::SourceSpan>> = cp
            .instruction_spans
            .iter()
            .map(|s| s.map(Into::into))
            .collect();
        let spans = encode_instruction_spans(&host_spans).unwrap();
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
            r#"function start() -> int {
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

    #[tokio::test]
    async fn cancel_run_returns_not_implemented_in_c2() {
        let persistence = SqlitePersistence::open_in_memory().await.unwrap();
        let controller = LocalController::new(persistence);
        let err = controller
            .cancel_run(&"run_xxx".into())
            .await
            .expect_err("c.2 stub");
        assert!(matches!(err, ControllerError::NotImplemented { .. }));
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
}
