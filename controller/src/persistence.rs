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
    ScheduleId, ScheduleRecord, WorkflowId, RunId,
};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::path::Path;

/// Embedded migrations in execution order. Add new files to the
/// end — never re-order or delete entries (controllers in the
/// wild rely on each running exactly once per fresh DB).
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_initial", include_str!("../migrations/0001_initial.sql")),
    ("0002_schedules", include_str!("../migrations/0002_schedules.sql")),
    ("0003_run_events", include_str!("../migrations/0003_run_events.sql")),
    (
        "0004_lifecycle_expansion",
        include_str!("../migrations/0004_lifecycle_expansion.sql"),
    ),
];

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

    /// Apply embedded migrations in order. Each file is run via
    /// `execute_many` so multi-statement migrations (CREATE TABLE
    /// + CREATE INDEX in the same file) work without splitting.
    ///
    /// Migrations are idempotent (`CREATE TABLE IF NOT EXISTS`,
    /// `CREATE INDEX IF NOT EXISTS`), so re-running them on a
    /// populated DB is safe.
    async fn migrate(&self) -> ControllerResult<()> {
        for (name, sql) in MIGRATIONS {
            sqlx::raw_sql(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| ControllerError::Persistence {
                    message: format!("migrate {name}: {e}"),
                })?;
        }
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

    async fn append_event(&self, event: &RunEvent) -> ControllerResult<()> {
        let payload = serde_json::to_string(event).map_err(|e| {
            ControllerError::Persistence {
                message: format!("encode run_event: {e}"),
            }
        })?;
        sqlx::query(
            "INSERT INTO run_events (run_id, seq, ts, kind, payload_json)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(event.run_id())
        .bind(event.seq() as i64)
        .bind(event.ts())
        .bind(event.kind())
        .bind(&payload)
        .execute(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("append_event: {e}"),
        })?;
        Ok(())
    }

    async fn list_events(
        &self,
        run_id: &RunId,
        after_seq: u64,
    ) -> ControllerResult<Vec<RunEvent>> {
        // SQLite INTEGER is signed; clamp to i64::MAX so a sentinel
        // u64::MAX (used by callers as "everything in the future")
        // doesn't wrap to -1 and match every row.
        let after_seq_i: i64 = after_seq.min(i64::MAX as u64) as i64;
        let rows = sqlx::query(
            "SELECT payload_json
             FROM run_events
             WHERE run_id = ? AND seq > ?
             ORDER BY seq ASC",
        )
        .bind(run_id)
        .bind(after_seq_i)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("list_events: {e}"),
        })?;
        rows.iter()
            .map(|row| {
                let s: String = row.get("payload_json");
                serde_json::from_str::<RunEvent>(&s).map_err(|e| {
                    ControllerError::Persistence {
                        message: format!("decode run_event: {e}"),
                    }
                })
            })
            .collect()
    }

    // ---- Phase C C.3 — schedules ----

    async fn put_schedule(&self, record: &ScheduleRecord) -> ControllerResult<()> {
        let trigger_json = serde_json::to_string(&record.trigger)
            .expect("trigger serializes");
        sqlx::query(
            "INSERT INTO schedules
                (id, workflow_id, trigger_json, enabled,
                 next_fire_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                trigger_json = excluded.trigger_json,
                enabled      = excluded.enabled,
                next_fire_at = excluded.next_fire_at",
        )
        .bind(&record.id)
        .bind(&record.workflow_id)
        .bind(&trigger_json)
        .bind(if record.enabled { 1_i32 } else { 0 })
        .bind(record.next_fire_at)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("put_schedule: {e}"),
        })?;
        Ok(())
    }

    async fn get_schedule(&self, id: &ScheduleId) -> ControllerResult<ScheduleRecord> {
        let row = sqlx::query(SCHEDULE_SELECT_COLUMNS_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("get_schedule: {e}"),
            })?
            .ok_or_else(|| ControllerError::ScheduleNotFound { id: id.clone() })?;
        row_to_schedule_record(&row)
    }

    async fn delete_schedule(&self, id: &ScheduleId) -> ControllerResult<()> {
        sqlx::query("DELETE FROM schedules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("delete_schedule: {e}"),
            })?;
        Ok(())
    }

    async fn list_schedules_for_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> ControllerResult<Vec<ScheduleRecord>> {
        let rows = sqlx::query(SCHEDULE_SELECT_COLUMNS_BY_WORKFLOW)
            .bind(workflow_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("list_schedules_for_workflow: {e}"),
            })?;
        rows.iter().map(row_to_schedule_record).collect()
    }

    async fn list_due_timer_schedules(
        &self,
        now_ms: i64,
    ) -> ControllerResult<Vec<ScheduleRecord>> {
        let rows = sqlx::query(SCHEDULE_SELECT_DUE_TIMERS)
            .bind(now_ms)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("list_due_timer_schedules: {e}"),
            })?;
        rows.iter().map(row_to_schedule_record).collect()
    }

    async fn list_enabled_event_schedules(&self)
        -> ControllerResult<Vec<ScheduleRecord>>
    {
        let rows = sqlx::query(SCHEDULE_SELECT_ENABLED_EVENTS)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("list_enabled_event_schedules: {e}"),
            })?;
        rows.iter().map(row_to_schedule_record).collect()
    }

    async fn update_schedule_next_fire(
        &self,
        id: &ScheduleId,
        next_fire_at: Option<i64>,
    ) -> ControllerResult<()> {
        sqlx::query("UPDATE schedules SET next_fire_at = ? WHERE id = ?")
            .bind(next_fire_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("update_schedule_next_fire: {e}"),
            })?;
        Ok(())
    }

    async fn set_schedule_enabled(
        &self,
        id: &ScheduleId,
        enabled: bool,
    ) -> ControllerResult<()> {
        sqlx::query("UPDATE schedules SET enabled = ? WHERE id = ?")
            .bind(if enabled { 1_i32 } else { 0 })
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("set_schedule_enabled: {e}"),
            })?;
        Ok(())
    }
}

// =============================================================
//  Schedule SQL constants + row mapper (C.3)
// =============================================================

const SCHEDULE_SELECT_COLUMNS_BY_ID: &str =
    "SELECT id, workflow_id, trigger_json, enabled, next_fire_at, created_at
     FROM schedules WHERE id = ?";

const SCHEDULE_SELECT_COLUMNS_BY_WORKFLOW: &str =
    "SELECT id, workflow_id, trigger_json, enabled, next_fire_at, created_at
     FROM schedules
     WHERE workflow_id = ?
     ORDER BY created_at ASC";

const SCHEDULE_SELECT_DUE_TIMERS: &str =
    "SELECT id, workflow_id, trigger_json, enabled, next_fire_at, created_at
     FROM schedules
     WHERE enabled = 1
       AND next_fire_at IS NOT NULL
       AND next_fire_at <= ?
     ORDER BY next_fire_at ASC";

const SCHEDULE_SELECT_ENABLED_EVENTS: &str =
    "SELECT id, workflow_id, trigger_json, enabled, next_fire_at, created_at
     FROM schedules
     WHERE enabled = 1 AND next_fire_at IS NULL";

fn row_to_schedule_record(
    row: &sqlx::sqlite::SqliteRow,
) -> ControllerResult<ScheduleRecord> {
    let trigger_s: String = row.get("trigger_json");
    let enabled_i: i64 = row.get("enabled");
    Ok(ScheduleRecord {
        id: row.get("id"),
        workflow_id: row.get("workflow_id"),
        trigger: serde_json::from_str::<RunTrigger>(&trigger_s).map_err(|e| {
            ControllerError::Persistence {
                message: format!("decode schedule trigger: {e}"),
            }
        })?,
        enabled: enabled_i != 0,
        next_fire_at: row.get("next_fire_at"),
        created_at: row.get("created_at"),
    })
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

    // ---- Phase C C.6 c90 — orchestration helpers ----

    /// Flip a run's `cancel_requested` bit. RunManager calls this
    /// when a cancel arrives so the bit survives a controller
    /// restart — the boot-recovery sweep then skips re-enqueueing
    /// or finalizes the run as Cancelled.
    pub async fn set_cancel_requested(
        &self,
        run_id: &str,
        requested: bool,
    ) -> ControllerResult<()> {
        sqlx::query("UPDATE runs SET cancel_requested = ? WHERE id = ?")
            .bind(if requested { 1_i32 } else { 0 })
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("set_cancel_requested: {e}"),
            })?;
        Ok(())
    }

    pub async fn is_cancel_requested(&self, run_id: &str) -> ControllerResult<bool> {
        let row = sqlx::query("SELECT cancel_requested FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ControllerError::Persistence {
                message: format!("is_cancel_requested: {e}"),
            })?;
        Ok(match row {
            Some(r) => {
                let v: i64 = r.get("cancel_requested");
                v != 0
            }
            None => false,
        })
    }

    /// Rows the boot-recovery sweep should re-enqueue: any
    /// non-terminal status (a controller crash mid-run leaves
    /// them as `Starting` / `Running` / `Cancelling`; legitimate
    /// `Queued` rows that never got dispatched also qualify).
    /// Returns runs in `created_at ASC` so dispatch order matches
    /// submission order.
    pub async fn list_recoverable_runs(&self) -> ControllerResult<Vec<RunRecord>> {
        let rows = sqlx::query(
            "SELECT id, workflow_id, status, trigger_json,
                    inputs_json, output_json, diagnostics_json,
                    started_at, completed_at, created_at
             FROM runs
             WHERE status IN ('Queued','Starting','Running','Cancelling')
             ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("list_recoverable_runs: {e}"),
        })?;
        rows.iter().map(row_to_run_record).collect()
    }

    /// Reset every non-terminal run row to `Queued` so boot
    /// recovery can re-enqueue them. Called from RunManager
    /// startup. Sweep + enqueue happen separately so the
    /// caller can hold the queue lock briefly per item.
    pub async fn reset_non_terminal_to_queued(&self) -> ControllerResult<u64> {
        let res = sqlx::query(
            "UPDATE runs SET status = 'Queued',
                             started_at = NULL,
                             completed_at = NULL
             WHERE status IN ('Starting','Running','Cancelling')",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("reset_non_terminal_to_queued: {e}"),
        })?;
        Ok(res.rows_affected())
    }

    /// Next monotonic seq for a run's event log. Used when the
    /// RunManager needs to emit a post-execute event (e.g. cancel
    /// override) after the per-run RunEventCtx has dropped.
    pub async fn next_event_seq(&self, run_id: &str) -> ControllerResult<u64> {
        // Fetch the highest seq actually present for this run. We
        // select the last row rather than `MAX(seq)` because an
        // aggregate over an empty set returns SQL NULL, which the
        // sqlite driver decodes back to `0` — indistinguishable from
        // a real seq-0 row. Selecting a concrete row makes "no prior
        // events" return `None` (→ next seq 0) honestly.
        let row = sqlx::query(
            "SELECT seq FROM run_events WHERE run_id = ? ORDER BY seq DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("next_event_seq: {e}"),
        })?;
        let next = match row {
            Some(r) => {
                let last: i64 = r.try_get("seq").unwrap_or(-1);
                if last >= 0 {
                    (last as u64) + 1
                } else {
                    0
                }
            }
            None => 0,
        };
        Ok(next)
    }

    /// All persisted events for `run_id`, ASC by seq. Unlike the
    /// trait's `list_events(after_seq)` (strict `>`), this has
    /// no lower bound — used by the SSE endpoint when the client
    /// hasn't supplied `?after` and wants the complete log.
    pub async fn list_all_events(
        &self,
        run_id: &RunId,
    ) -> ControllerResult<Vec<RunEvent>> {
        let rows = sqlx::query(
            "SELECT payload_json
             FROM run_events
             WHERE run_id = ?
             ORDER BY seq ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ControllerError::Persistence {
            message: format!("list_all_events: {e}"),
        })?;
        rows.iter()
            .map(|row| {
                let s: String = row.get("payload_json");
                serde_json::from_str::<RunEvent>(&s).map_err(|e| {
                    ControllerError::Persistence {
                        message: format!("decode run_event: {e}"),
                    }
                })
            })
            .collect()
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
        RunStatus::Starting => "Starting",
        RunStatus::Running => "Running",
        RunStatus::Cancelling => "Cancelling",
        RunStatus::Succeeded => "Succeeded",
        RunStatus::Failed => "Failed",
        RunStatus::Cancelled => "Cancelled",
        RunStatus::TimedOut => "TimedOut",
        RunStatus::Rejected => "Rejected",
    }
}

fn str_to_status(s: &str) -> Option<RunStatus> {
    match s {
        "Queued" => Some(RunStatus::Queued),
        "Starting" => Some(RunStatus::Starting),
        "Running" => Some(RunStatus::Running),
        "Cancelling" => Some(RunStatus::Cancelling),
        "Succeeded" => Some(RunStatus::Succeeded),
        "Failed" => Some(RunStatus::Failed),
        "Cancelled" => Some(RunStatus::Cancelled),
        "TimedOut" => Some(RunStatus::TimedOut),
        "Rejected" => Some(RunStatus::Rejected),
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

    /// Phase C C.6 c89 — every new lifecycle variant round-
    /// trips through the table (CHECK constraint accepts them +
    /// status_to_str / str_to_status agree).
    #[tokio::test]
    async fn run_status_new_variants_round_trip_through_runs_table() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_c89".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let lifecycle_states = [
            RunStatus::Queued,
            RunStatus::Starting,
            RunStatus::Running,
            RunStatus::Cancelling,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::TimedOut,
            RunStatus::Rejected,
        ];
        for (i, status) in lifecycle_states.iter().enumerate() {
            let record = RunRecord {
                id: format!("run_c89_{i}"),
                workflow_id: "wf_c89".into(),
                status: *status,
                trigger: RunTrigger::Manual,
                inputs: serde_json::json!({}),
                output: None,
                diagnostics: Vec::new(),
                created_at: 1_700_000_000_000 + i as i64,
                started_at: None,
                completed_at: None,
            };
            p.put_run(&record).await.unwrap();
            let got = p.get_run(&record.id).await.unwrap();
            assert_eq!(got.status, *status, "status {status:?} didn't round-trip");
        }
    }

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
                trace: Vec::new(),
                trace_truncated: false,
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
            trace: Vec::new(),
            trace_truncated: false,
        });
        record.completed_at = Some(1_700_000_000_500);
        p.put_run(&record).await.unwrap();

        let got = p.get_run(&"run_t3_1".into()).await.unwrap();
        assert_eq!(got.status, RunStatus::Succeeded);
        assert_eq!(got.output.unwrap().return_value, Some(7));
    }

    // =============================================================
    //  Phase C C.3 — schedule round-trip tests
    // =============================================================

    fn timer_schedule(id: &str, workflow_id: &str, next: i64) -> ScheduleRecord {
        ScheduleRecord {
            id: id.into(),
            workflow_id: workflow_id.into(),
            trigger: RunTrigger::Timer {
                schedule_id: id.into(),
                cron: "*/5 * * * *".into(),
            },
            enabled: true,
            next_fire_at: Some(next),
            created_at: 1_700_000_000_000,
        }
    }

    fn event_schedule(id: &str, workflow_id: &str, path: &str) -> ScheduleRecord {
        ScheduleRecord {
            id: id.into(),
            workflow_id: workflow_id.into(),
            trigger: RunTrigger::Event { source: path.into() },
            enabled: true,
            next_fire_at: None,
            created_at: 1_700_000_000_000,
        }
    }

    #[tokio::test]
    async fn put_get_schedule_round_trips() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s1".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let s = timer_schedule("sch_s1_a", "wf_s1", 1_700_000_001_000);
        p.put_schedule(&s).await.unwrap();
        let got = p.get_schedule(&"sch_s1_a".to_string()).await.unwrap();
        assert_eq!(got.workflow_id, "wf_s1");
        assert_eq!(got.next_fire_at, Some(1_700_000_001_000));
        match got.trigger {
            RunTrigger::Timer { cron, .. } => assert_eq!(cron, "*/5 * * * *"),
            _ => panic!("expected Timer trigger"),
        }
    }

    #[tokio::test]
    async fn get_missing_schedule_returns_not_found() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let err = p.get_schedule(&"sch_nope".to_string()).await.expect_err("missing");
        assert!(matches!(err, ControllerError::ScheduleNotFound { .. }));
    }

    #[tokio::test]
    async fn list_due_timer_schedules_filters_by_now_and_enabled() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s2".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        // Three timers: one past-due, one future, one disabled.
        let past = timer_schedule("sch_past", "wf_s2", 1_700_000_000_500);
        let future = timer_schedule("sch_future", "wf_s2", 1_700_000_100_000);
        let mut disabled = timer_schedule("sch_disabled", "wf_s2", 1_700_000_000_500);
        disabled.enabled = false;
        let event = event_schedule("sch_event", "wf_s2", "webhook/x");
        p.put_schedule(&past).await.unwrap();
        p.put_schedule(&future).await.unwrap();
        p.put_schedule(&disabled).await.unwrap();
        p.put_schedule(&event).await.unwrap();

        let due = p.list_due_timer_schedules(1_700_000_001_000).await.unwrap();
        let ids: Vec<_> = due.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["sch_past"]);
    }

    #[tokio::test]
    async fn list_enabled_event_schedules_excludes_timers_and_disabled() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s3".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let event = event_schedule("sch_e1", "wf_s3", "deploy");
        let mut event_disabled = event_schedule("sch_e2", "wf_s3", "rollback");
        event_disabled.enabled = false;
        let timer = timer_schedule("sch_t1", "wf_s3", 1_700_000_100_000);
        p.put_schedule(&event).await.unwrap();
        p.put_schedule(&event_disabled).await.unwrap();
        p.put_schedule(&timer).await.unwrap();

        let got = p.list_enabled_event_schedules().await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "sch_e1");
    }

    #[tokio::test]
    async fn update_schedule_next_fire_advances_then_lists_due_again() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s4".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let s = timer_schedule("sch_adv", "wf_s4", 1_700_000_000_500);
        p.put_schedule(&s).await.unwrap();
        // Advance past now → no longer due at the same tick.
        p.update_schedule_next_fire(&"sch_adv".to_string(), Some(1_700_000_100_000))
            .await
            .unwrap();
        let due = p.list_due_timer_schedules(1_700_000_001_000).await.unwrap();
        assert!(due.is_empty(), "advanced schedule should not be due");
        let got = p.get_schedule(&"sch_adv".to_string()).await.unwrap();
        assert_eq!(got.next_fire_at, Some(1_700_000_100_000));
    }

    #[tokio::test]
    async fn set_schedule_enabled_pauses_and_resumes() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s5".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let s = timer_schedule("sch_pause", "wf_s5", 1_700_000_000_500);
        p.put_schedule(&s).await.unwrap();
        p.set_schedule_enabled(&"sch_pause".to_string(), false).await.unwrap();
        let due = p.list_due_timer_schedules(1_700_000_001_000).await.unwrap();
        assert!(due.is_empty());
        p.set_schedule_enabled(&"sch_pause".to_string(), true).await.unwrap();
        let due = p.list_due_timer_schedules(1_700_000_001_000).await.unwrap();
        assert_eq!(due.len(), 1);
    }

    // =============================================================
    //  Phase C C.5 — run_events
    // =============================================================

    use solflow_host_spec::{RunEvent as Ev, RuntimeErrorView};

    fn print_event(run_id: &str, seq: u64, text: &str) -> Ev {
        Ev::Print {
            run_id: run_id.into(),
            seq,
            ts: 1_700_000_000_000 + seq as i64,
            text: text.into(),
            source_span: None,
        }
    }

    #[tokio::test]
    async fn append_event_round_trips_and_orders_by_seq() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_e".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let record = RunRecord {
            id: "run_events_a".into(),
            workflow_id: "wf_e".into(),
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

        // Append out-of-order; list_events must still return ASC.
        p.append_event(&print_event("run_events_a", 2, "second")).await.unwrap();
        p.append_event(&print_event("run_events_a", 0, "first")).await.unwrap();
        p.append_event(&print_event("run_events_a", 1, "middle")).await.unwrap();
        // Different run; must not pollute the list.
        p.append_event(&print_event("run_other", 0, "other-run")).await.unwrap_or(()); // no FK so OK even if absent

        let events = p
            .list_events(&"run_events_a".to_string(), 0)
            .await
            .unwrap();
        // after_seq=0 is EXCLUSIVE per the architecture doc; expect seqs 1 + 2.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq(), 1);
        assert_eq!(events[1].seq(), 2);

        let all = p
            .list_events(&"run_events_a".to_string(), u64::MAX)
            .await
            .unwrap();
        assert!(all.is_empty(), "after=u64::MAX returns nothing");
    }

    #[tokio::test]
    async fn append_event_handles_every_variant_via_serde() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_v".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        let record = RunRecord {
            id: "run_v".into(),
            workflow_id: "wf_v".into(),
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

        let variants = vec![
            Ev::Queued { run_id: "run_v".into(), seq: 0, ts: 1 },
            Ev::Started { run_id: "run_v".into(), seq: 1, ts: 2 },
            Ev::Print {
                run_id: "run_v".into(), seq: 2, ts: 3,
                text: "hi".into(), source_span: None,
            },
            Ev::ExtCallStarted {
                run_id: "run_v".into(), seq: 3, ts: 4,
                connector: "http".into(), fn_name: "fetch".into(),
            },
            Ev::ExtCallCompleted {
                run_id: "run_v".into(), seq: 4, ts: 5,
                connector: "http".into(), fn_name: "fetch".into(), ok: true,
            },
            Ev::Completed {
                run_id: "run_v".into(), seq: 5, ts: 6,
                output: RunOutput {
                    return_value: Some(0),
                    output: vec![],
                    steps: 1,
                    trace: Vec::new(),
                    trace_truncated: false,
                },
            },
            Ev::Failed {
                run_id: "run_v".into(), seq: 6, ts: 7,
                error: RuntimeErrorView::DivByZero,
                source_span: None,
            },
            Ev::Cancelled { run_id: "run_v".into(), seq: 7, ts: 8 },
        ];
        for e in &variants {
            p.append_event(e).await.unwrap();
        }
        let got = p.list_events(&"run_v".to_string(), 0).await.unwrap();
        // after=0 excludes seq=0; so we expect 7 events back (seq 1..=7).
        assert_eq!(got.len(), 7);
        assert_eq!(got[0].kind(), "Started");
        assert_eq!(got.last().unwrap().kind(), "Cancelled");
    }

    #[tokio::test]
    async fn delete_schedule_removes_row() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        p.put_workflow(&"wf_s6".to_string(), b"bc", b"sp", &sample_meta_json())
            .await
            .unwrap();
        p.put_schedule(&timer_schedule("sch_del", "wf_s6", 1_700_000_000_500))
            .await
            .unwrap();
        p.delete_schedule(&"sch_del".to_string()).await.unwrap();
        let err = p.get_schedule(&"sch_del".to_string()).await.expect_err("gone");
        assert!(matches!(err, ControllerError::ScheduleNotFound { .. }));
        // Delete is no-op-safe on missing IDs.
        p.delete_schedule(&"sch_del".to_string()).await.unwrap();
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
