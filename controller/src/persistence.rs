//! SQLite-backed persistence for the C.2 local controller.
//!
//! Implements the `Persistence` trait via `sqlx::SqlitePool`.
//! Schema migrations live in `migrations/` next to the crate
//! root and are applied on pool construction.
//!
//! Single-writer at the SQL level (SQLite's default); the C.2
//! controller serializes writes via the run-execution path, which
//! is fine for the local-first MVP. Multi-process / multi-node
//! storage is a C.7+ concern.

use crate::{ControllerError, ControllerResult, Persistence};
use async_trait::async_trait;
use solflow_host_spec::{
    RunEvent, RunOutput, RunRecord, RunStatus, RunTrigger,
    WorkflowId, RunId,
};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::path::Path;

/// Concrete persistence backend wrapping a sqlx connection pool.
#[derive(Clone)]
pub struct SqlitePersistence {
    pool: SqlitePool,
}

impl SqlitePersistence {
    /// Open (or create) a SQLite database at the given path + run
    /// migrations. The directory is NOT created; caller is
    /// responsible for ensuring parent dirs exist.
    pub async fn open(path: impl AsRef<Path>) -> ControllerResult<Self> {
        let path_str = path.as_ref().display().to_string();
        let url = format!("sqlite://{path_str}?mode=rwc");
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("open {path_str}: {e}"),
            })?;
        let p = Self { pool };
        p.migrate().await?;
        Ok(p)
    }

    /// In-memory SQLite for tests. Migrations applied on creation.
    pub async fn open_in_memory() -> ControllerResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // shared in-memory DB across connections is tricky; cap to 1
            .connect("sqlite::memory:")
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("open in-memory: {e}"),
            })?;
        let p = Self { pool };
        p.migrate().await?;
        Ok(p)
    }

    /// Apply embedded migrations.
    async fn migrate(&self) -> ControllerResult<()> {
        // C.2 ships a single migration. As more land we'll wire
        // sqlx's `migrate!` macro; for now this is plain SQL so
        // the migration is visible + reviewable.
        sqlx::query(include_str!("../migrations/0001_initial.sql"))
            .execute(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("migrate: {e}"),
            })?;
        Ok(())
    }

    /// Borrow the underlying pool. Lets the run-executor reach
    /// into the DB without going through the trait for bulk
    /// workflow + run reads.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Persistence for SqlitePersistence {
    async fn put_workflow(
        &self,
        id: &WorkflowId,
        bytecode: &[u8],
        spans: &[u8],
        meta_json: &str,
    ) -> ControllerResult<()> {
        // meta_json carries: name, description, content_hash, created_at.
        // We deserialize JSON here rather than expanding the trait
        // signature — keeps the C.1 trait stable.
        #[derive(serde::Deserialize)]
        struct Meta {
            name: String,
            #[serde(default)]
            description: Option<String>,
            content_hash: String,
            created_at: i64,
            #[serde(default)]
            source: Option<String>,
        }
        let m: Meta = serde_json::from_str(meta_json).map_err(|e| {
            ControllerError::Persistence {
                message: format!("decode workflow meta: {e}"),
            }
        })?;
        sqlx::query(
            "INSERT INTO workflows
                (id, content_hash, bytecode, instruction_spans,
                 source, name, description, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(id)
        .bind(&m.content_hash)
        .bind(bytecode)
        .bind(spans)
        .bind(m.source.as_deref())
        .bind(&m.name)
        .bind(m.description.as_deref())
        .bind(m.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("put_workflow: {e}"),
        })?;
        Ok(())
    }

    async fn get_workflow_bytecode(
        &self,
        id: &WorkflowId,
    ) -> ControllerResult<(Vec<u8>, Vec<u8>)> {
        let row = sqlx::query(
            "SELECT bytecode, instruction_spans
             FROM workflows WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("get_workflow: {e}"),
        })?
        .ok_or_else(|| ControllerError::WorkflowNotFound { id: id.clone() })?;
        let bc: Vec<u8> = row.get("bytecode");
        let sp: Vec<u8> = row.get("instruction_spans");
        Ok((bc, sp))
    }

    async fn put_run(&self, record: &RunRecord) -> ControllerResult<()> {
        let trigger_json = serde_json::to_string(&record.trigger)
            .expect("trigger serializes");
        let inputs_json = serde_json::to_string(&record.inputs)
            .expect("inputs serialize");
        let output_json = record
            .output
            .as_ref()
            .map(|o| serde_json::to_string(o).expect("output serializes"));
        let diagnostics_json = serde_json::to_string(&record.diagnostics)
            .expect("diagnostics serialize");
        sqlx::query(
            "INSERT INTO runs
                (id, workflow_id, status, trigger_json, inputs_json,
                 output_json, diagnostics_json, started_at,
                 completed_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                status           = excluded.status,
                output_json      = excluded.output_json,
                diagnostics_json = excluded.diagnostics_json,
                started_at       = excluded.started_at,
                completed_at     = excluded.completed_at",
        )
        .bind(&record.id)
        .bind(&record.workflow_id)
        .bind(status_to_str(record.status))
        .bind(&trigger_json)
        .bind(&inputs_json)
        .bind(output_json.as_deref())
        .bind(&diagnostics_json)
        .bind(record.started_at)
        .bind(record.completed_at)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("put_run: {e}"),
        })?;
        Ok(())
    }

    async fn get_run(&self, id: &RunId) -> ControllerResult<RunRecord> {
        let row = sqlx::query(
            "SELECT id, workflow_id, status, trigger_json,
                    inputs_json, output_json, diagnostics_json,
                    started_at, completed_at, created_at
             FROM runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("get_run: {e}"),
        })?
        .ok_or_else(|| ControllerError::RunNotFound { id: id.clone() })?;
        Ok(row_to_run_record(&row)?)
    }

    async fn append_event(&self, _event: &RunEvent) -> ControllerResult<()> {
        // C.2 doesn't persist events — that's C.5. Trait method
        // returns Ok so the rest of the controller can call it
        // unconditionally; C.5 wires real storage here.
        Ok(())
    }

    async fn list_events(
        &self,
        _run_id: &RunId,
        _after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>> {
        // Same — empty list until C.5 lands.
        Ok(Vec::new())
    }
}

// =============================================================
//  C.2 helpers (NOT part of the trait — internal to the
//  controller binary; not exposed to other crates yet).
// =============================================================

impl SqlitePersistence {
    /// Paginated run history for `GET /workflows/:id/runs`.
    pub async fn list_runs(
        &self,
        workflow_id: &WorkflowId,
        status: Option<RunStatus>,
        limit: Option<usize>,
    ) -> ControllerResult<Vec<RunRecord>> {
        let limit = limit.unwrap_or(100).min(1000) as i64;
        let rows = if let Some(s) = status {
            sqlx::query(
                "SELECT id, workflow_id, status, trigger_json,
                        inputs_json, output_json, diagnostics_json,
                        started_at, completed_at, created_at
                 FROM runs
                 WHERE workflow_id = ? AND status = ?
                 ORDER BY created_at DESC
                 LIMIT ?",
            )
            .bind(workflow_id)
            .bind(status_to_str(s))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, workflow_id, status, trigger_json,
                        inputs_json, output_json, diagnostics_json,
                        started_at, completed_at, created_at
                 FROM runs
                 WHERE workflow_id = ?
                 ORDER BY created_at DESC
                 LIMIT ?",
            )
            .bind(workflow_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| ControllerError::Persistence {
            message: format!("list_runs: {e}"),
        })?;
        rows.iter().map(row_to_run_record).collect()
    }

    /// Look up workflow name (used for HTTP-API responses).
    pub async fn get_workflow_name(
        &self,
        id: &WorkflowId,
    ) -> ControllerResult<String> {
        let row = sqlx::query("SELECT name FROM workflows WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("get_workflow_name: {e}"),
            })?
            .ok_or_else(|| ControllerError::WorkflowNotFound { id: id.clone() })?;
        Ok(row.get("name"))
    }
}

fn row_to_run_record(row: &sqlx::sqlite::SqliteRow) -> ControllerResult<RunRecord> {
    let status_s: String = row.get("status");
    let trigger_s: String = row.get("trigger_json");
    let inputs_s: String = row.get("inputs_json");
    let output_s: Option<String> = row.get("output_json");
    let diagnostics_s: String = row.get("diagnostics_json");
    Ok(RunRecord {
        id: row.get("id"),
        workflow_id: row.get("workflow_id"),
        status: str_to_status(&status_s).ok_or_else(|| {
            ControllerError::Persistence {
                message: format!("unknown status: {status_s}"),
            }
        })?,
        trigger: serde_json::from_str::<RunTrigger>(&trigger_s).map_err(|e| {
            ControllerError::Persistence {
                message: format!("decode trigger: {e}"),
            }
        })?,
        inputs: serde_json::from_str(&inputs_s).unwrap_or(serde_json::Value::Null),
        output: output_s
            .as_deref()
            .map(|s| serde_json::from_str::<RunOutput>(s))
            .transpose()
            .map_err(|e| ControllerError::Persistence {
                message: format!("decode output: {e}"),
            })?,
        diagnostics: serde_json::from_str(&diagnostics_s).unwrap_or_default(),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        created_at: row.get("created_at"),
    })
}

fn status_to_str(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Queued => "Queued",
        RunStatus::Running => "Running",
        RunStatus::Succeeded => "Succeeded",
        RunStatus::Failed => "Failed",
        RunStatus::Cancelled => "Cancelled",
    }
}

fn str_to_status(s: &str) -> Option<RunStatus> {
    match s {
        "Queued" => Some(RunStatus::Queued),
        "Running" => Some(RunStatus::Running),
        "Succeeded" => Some(RunStatus::Succeeded),
        "Failed" => Some(RunStatus::Failed),
        "Cancelled" => Some(RunStatus::Cancelled),
        _ => None,
    }
}

// =============================================================
//  Tests — verify persistence shape against in-memory SQLite
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_host_spec::{RunOutput, RunStatus, RunTrigger};

    fn sample_meta_json() -> String {
        serde_json::json!({
            "name": "Hello",
            "content_hash": "deadbeef",
            "created_at": 1_700_000_000_000_i64,
        })
        .to_string()
    }

    #[tokio::test]
    async fn open_in_memory_applies_migrations() {
        let p = SqlitePersistence::open_in_memory().await.expect("open");
        // Should be able to insert + read back a workflow.
        p.put_workflow(&"wf_t1".to_string(), b"bytecode", b"spans", &sample_meta_json())
            .await
            .expect("put");
        let (bc, sp) = p.get_workflow_bytecode(&"wf_t1".into()).await.expect("get");
        assert_eq!(bc, b"bytecode");
        assert_eq!(sp, b"spans");
    }

    #[tokio::test]
    async fn get_workflow_bytecode_missing_returns_not_found() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let err = p
            .get_workflow_bytecode(&"wf_missing".into())
            .await
            .expect_err("missing");
        assert!(matches!(err, ControllerError::WorkflowNotFound { .. }));
    }

    #[tokio::test]
    async fn put_run_then_get_round_trips() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_t2".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let record = RunRecord {
            id: "run_t2_1".into(),
            workflow_id: "wf_t2".into(),
            status: RunStatus::Succeeded,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: Some(RunOutput {
                return_value: Some(42),
                output: vec!["hi".into()],
                steps: 12,
            }),
            diagnostics: Vec::new(),
            created_at: 1_700_000_000_000,
            started_at: Some(1_700_000_000_001),
            completed_at: Some(1_700_000_000_002),
        };
        p.put_run(&record).await.unwrap();
        let got = p.get_run(&"run_t2_1".into()).await.unwrap();
        assert_eq!(got.id, record.id);
        assert_eq!(got.status, RunStatus::Succeeded);
        assert_eq!(got.output.unwrap().return_value, Some(42));
    }

    #[tokio::test]
    async fn put_run_upsert_updates_status() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_t3".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let mut record = RunRecord {
            id: "run_t3_1".into(),
            workflow_id: "wf_t3".into(),
            status: RunStatus::Queued,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: None,
            diagnostics: Vec::new(),
            created_at: 1_700_000_000_000,
            started_at: None,
            completed_at: None,
        };
        p.put_run(&record).await.unwrap();

        record.status = RunStatus::Succeeded;
        record.output = Some(RunOutput {
            return_value: Some(7),
            output: vec![],
            steps: 3,
        });
        record.completed_at = Some(1_700_000_000_500);
        p.put_run(&record).await.unwrap();

        let got = p.get_run(&"run_t3_1".into()).await.unwrap();
        assert_eq!(got.status, RunStatus::Succeeded);
        assert_eq!(got.output.unwrap().return_value, Some(7));
    }

    #[tokio::test]
    async fn list_runs_filters_by_status() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_t4".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        for i in 0..3 {
            let record = RunRecord {
                id: format!("run_t4_{i}"),
                workflow_id: "wf_t4".into(),
                status: if i == 0 { RunStatus::Succeeded } else { RunStatus::Failed },
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
                output: None,
                diagnostics: Vec::new(),
                created_at: 1_700_000_000_000 + i,
                started_at: None,
                completed_at: None,
            };
            p.put_run(&record).await.unwrap();
        }
        let failed = p
            .list_runs(&"wf_t4".into(), Some(RunStatus::Failed), Some(10))
            .await
            .unwrap();
        assert_eq!(failed.len(), 2);
        let all = p.list_runs(&"wf_t4".into(), None, Some(10)).await.unwrap();
        assert_eq!(all.len(), 3);
    }
}
