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

use crate::{Persistence, SqlitePersistence};
use solflow_host_spec::{
    decode_bytecode, RunOutput, RunRecord, RunStatus,
};
use solflow_runtime::{run_program_with, RunOptions};
use std::time::Duration;

/// Configurable run-execution policy applied by the controller.
/// MVP defaults match the architecture doc §10.2 numbers.
#[derive(Debug, Clone, Copy)]
pub struct RunPolicy {
    pub step_limit: usize,
    pub wall_clock_timeout: Duration,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            step_limit: 10_000_000,
            wall_clock_timeout: Duration::from_secs(600),
        }
    }
}

/// Execute a run synchronously (with its own internal timeout).
/// Caller spawns this on a tokio task. Persists the final
/// RunRecord state through `persistence`.
pub async fn execute_run(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    policy: RunPolicy,
) {
    // Mark Running + persist before the VM starts so callers
    // polling GET /runs/:id see the transition.
    record.status = RunStatus::Running;
    record.started_at = Some(now_ms());
    if let Err(e) = persistence.put_run(&record).await {
        // If persistence is broken we can't even record the
        // run; log + bail. Tracing only — caller already
        // got their HTTP 202.
        tracing::error!("execute_run persistence put_run (Running) failed: {e}");
        return;
    }

    // Load + decode bytecode.
    let (bc_bytes, _spans_bytes) =
        match persistence.get_workflow_bytecode(&record.workflow_id).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("execute_run get_workflow_bytecode failed: {e}");
                finalize_failed(persistence, record, format!("{e}")).await;
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
            )
            .await;
            return;
        }
    };

    // Run through the canonical VM. Bounded by tokio timeout so
    // wall-clock limits actually fire (the VM's own step_limit
    // bounds CPU but not real time).
    //
    // run_program_with is synchronous; wrap in spawn_blocking so
    // the tokio runtime stays responsive to other tasks.
    let opts = RunOptions {
        step_limit: Some(policy.step_limit),
        trace: false, // C.5 will enable + persist trace via events.
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
            )
            .await;
            return;
        }
    };

    // Translate VM outcome to RunRecord.
    record.completed_at = Some(now_ms());
    if let Some(_err) = outcome.error {
        record.status = RunStatus::Failed;
        // C.2: we capture the runtime-error message on the
        // record. Structured runtime errors land in events in C.5.
        record.output = Some(RunOutput {
            return_value: None,
            output: outcome.output.clone(),
            steps: outcome.steps,
        });
    } else {
        record.status = RunStatus::Succeeded;
        record.output = Some(RunOutput {
            return_value: Some(outcome.return_value as i64),
            output: outcome.output,
            steps: outcome.steps,
        });
    }
    if let Err(e) = persistence.put_run(&record).await {
        tracing::error!("execute_run persistence put_run (final) failed: {e}");
    }
}

async fn finalize_failed(
    persistence: SqlitePersistence,
    mut record: RunRecord,
    reason: String,
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
        execute_run(p.clone(), record, RunPolicy::default()).await;
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
        execute_run(p.clone(), record, RunPolicy::default()).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Failed);
        assert!(got.output.unwrap().return_value.is_none());
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
        };
        execute_run(p.clone(), record, policy).await;
        let got = p.get_run(&run_id).await.unwrap();
        assert_eq!(got.status, RunStatus::Failed);
    }
}
