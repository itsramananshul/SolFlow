//! Timer + event triggers (Phase C C.3 c68).
//!
//! `TokioScheduler` owns one tokio task that ticks every
//! `tick_interval` (default 1s) and asks persistence for
//! `list_due_timer_schedules(now_ms)`. For each due schedule:
//!
//!   1. mint a `RunRecord` with `trigger = RunTrigger::Timer { ... }`
//!   2. persist it as `Queued`
//!   3. spawn `execute_run`
//!   4. advance the schedule's `next_fire_at` from its cron expr
//!
//! Webhook ingress fires synchronously from `ingress_event(path,
//! body)` — finds enabled `Event { source }` schedules whose
//! source matches the path, mints + persists + spawns a run for
//! each match, returns the first one (the trait surface allows
//! one record; multi-match is supported on the DB side but the
//! HTTP layer returns the first as the response).
//!
//! ## Design notes
//!
//! - The scheduler is held by `LocalController`; both share the
//!   same `SqlitePersistence` clone so persistence writes from
//!   manual runs + scheduled runs see consistent state.
//! - Cron computation uses the `cron` crate's standard 7-field
//!   parser (`sec min hour dom mon dow year`) — we accept the
//!   familiar 5-field form (min hour dom mon dow) by prefixing
//!   `0 ` for seconds + appending ` *` for year, matching how
//!   common cron schedules are written.
//! - Cancellation of a registered schedule is just `DELETE`;
//!   the next tick won't see it. No need to bus-cancel the loop.
//! - The tick loop never panics — every error is logged via
//!   `tracing::error!` and the loop continues. A broken cron
//!   string is logged once per occurrence; the schedule's
//!   `next_fire_at` is cleared so it stops triggering until the
//!   user fixes it.

use crate::executor::{execute_run, now_ms, RunPolicy};
use crate::{ControllerError, ControllerResult, Persistence, SqlitePersistence};
use chrono::{TimeZone, Utc};
use cron::Schedule as CronSchedule;
use solflow_host_spec::{
    RunRecord, RunStatus, RunTrigger, ScheduleId, ScheduleRecord, WorkflowId,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// Default tick interval. 1s is fast enough that a `* * * * *`
/// cron fires within a second of the minute boundary, slow
/// enough that idle controllers don't burn CPU.
const DEFAULT_TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Scheduler bound to a controller's persistence + run policy.
/// Clone-cheap (everything inside is `Arc`).
#[derive(Clone)]
pub struct TokioScheduler {
    persistence: SqlitePersistence,
    policy: RunPolicy,
    tick_interval: Duration,
    /// Token set when `start()` has been called so subsequent
    /// calls don't spawn duplicate loops.
    started: Arc<std::sync::atomic::AtomicBool>,
}

impl TokioScheduler {
    pub fn new(persistence: SqlitePersistence, policy: RunPolicy) -> Self {
        Self {
            persistence,
            policy,
            tick_interval: DEFAULT_TICK_INTERVAL,
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Override the tick interval (tests use this to fire many
    /// ticks per second; production sticks with the default).
    pub fn with_tick_interval(mut self, tick: Duration) -> Self {
        self.tick_interval = tick;
        self
    }

    /// Spawn the background tick loop. Returns the JoinHandle so
    /// callers can `.abort()` it on shutdown. Idempotent — a
    /// second call after the first returns a do-nothing handle.
    pub fn start(&self) -> JoinHandle<()> {
        if self
            .started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            // Already running. Return a no-op handle so the
            // caller's lifecycle stays uniform.
            return tokio::spawn(async {});
        }
        let me = self.clone();
        tokio::spawn(async move { me.tick_loop().await })
    }

    async fn tick_loop(self) {
        let mut tick = tokio::time::interval(self.tick_interval);
        // Fire the first tick immediately so a freshly-registered
        // due timer fires within the first second.
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = self.tick_once().await {
                tracing::error!("scheduler tick failed: {e}");
            }
        }
    }

    async fn tick_once(&self) -> ControllerResult<()> {
        let now = now_ms();
        let due = self.persistence.list_due_timer_schedules(now).await?;
        for sched in due {
            if let Err(e) = self.fire_timer_schedule(sched, now).await {
                tracing::error!("fire_timer_schedule failed: {e}");
            }
        }
        Ok(())
    }

    /// Build + persist + spawn a run for a single due Timer schedule,
    /// then advance its `next_fire_at` from the cron expression. If
    /// the cron expression doesn't parse, the schedule's next_fire
    /// is cleared (stop firing) and an error is logged — the user
    /// must update or delete the schedule to recover.
    async fn fire_timer_schedule(
        &self,
        sched: ScheduleRecord,
        now: i64,
    ) -> ControllerResult<()> {
        let cron_expr = match &sched.trigger {
            RunTrigger::Timer { cron, .. } => cron.clone(),
            _ => {
                tracing::warn!(
                    "non-Timer schedule {} surfaced from list_due_timer_schedules; skipping",
                    sched.id
                );
                return Ok(());
            }
        };
        // Verify workflow still exists before we mint a run for it.
        // If the workflow was deleted, log + clear the next-fire
        // so this stops triggering.
        if self
            .persistence
            .get_workflow_bytecode(&sched.workflow_id)
            .await
            .is_err()
        {
            tracing::warn!(
                "scheduled workflow {} not found; disabling schedule {}",
                sched.workflow_id,
                sched.id
            );
            let _ = self
                .persistence
                .set_schedule_enabled(&sched.id, false)
                .await;
            return Ok(());
        }
        self.spawn_run_for(
            &sched.workflow_id,
            RunTrigger::Timer {
                schedule_id: sched.id.clone(),
                cron: cron_expr.clone(),
            },
            serde_json::json!({}),
            now,
        )
        .await?;
        // Advance next_fire_at to strictly after `now`. If parsing
        // fails, clear next_fire_at so the schedule stops firing.
        match cron_next_after_ms(&cron_expr, now) {
            Ok(next) => {
                self.persistence
                    .update_schedule_next_fire(&sched.id, Some(next))
                    .await?;
            }
            Err(e) => {
                tracing::error!(
                    "schedule {}: invalid cron expression \"{}\": {}",
                    sched.id,
                    cron_expr,
                    e
                );
                self.persistence
                    .update_schedule_next_fire(&sched.id, None)
                    .await?;
            }
        }
        Ok(())
    }

    /// Mint a `Queued` `RunRecord`, persist it, and spawn
    /// `execute_run` on a tokio task. Returns the persisted
    /// record so callers can echo it to clients (webhook handler).
    async fn spawn_run_for(
        &self,
        workflow_id: &WorkflowId,
        trigger: RunTrigger,
        inputs: serde_json::Value,
        now: i64,
    ) -> ControllerResult<RunRecord> {
        let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let record = RunRecord {
            id: run_id,
            workflow_id: workflow_id.clone(),
            status: RunStatus::Queued,
            trigger,
            inputs,
            output: None,
            diagnostics: Vec::new(),
            created_at: now,
            started_at: None,
            completed_at: None,
        };
        self.persistence.put_run(&record).await?;
        let p = self.persistence.clone();
        let r = record.clone();
        let policy = self.policy;
        tokio::spawn(async move {
            execute_run(p, r, policy, None).await;
        });
        Ok(record)
    }

    /// Register a new schedule. Mints an `id` when the input's id
    /// is empty (typical when the editor POSTs a fresh schedule).
    /// Computes `next_fire_at` for Timer triggers; Event triggers
    /// get `None`. Persists + returns the populated record.
    pub async fn register(
        &self,
        mut record: ScheduleRecord,
    ) -> ControllerResult<ScheduleRecord> {
        if record.id.is_empty() {
            record.id = format!("sch_{}", uuid::Uuid::new_v4().simple());
        }
        if record.created_at == 0 {
            record.created_at = now_ms();
        }
        // Verify the workflow exists so we fail fast instead of
        // letting the tick loop discover it later.
        self.persistence
            .get_workflow_bytecode(&record.workflow_id)
            .await?;
        // Set next_fire_at based on trigger kind.
        record.next_fire_at = match &record.trigger {
            RunTrigger::Timer { cron, .. } => Some(
                cron_next_after_ms(cron, now_ms()).map_err(|e| {
                    ControllerError::BytecodeInvalid {
                        reason: format!("invalid cron \"{cron}\": {e}"),
                    }
                })?,
            ),
            RunTrigger::Event { .. } => None,
            RunTrigger::Manual => {
                return Err(ControllerError::BytecodeInvalid {
                    reason: "schedules require Timer or Event triggers, not Manual".into(),
                });
            }
        };
        self.persistence.put_schedule(&record).await?;
        Ok(record)
    }

    /// Delete a schedule. The tick loop simply won't see it next
    /// tick. Idempotent.
    pub async fn cancel(&self, id: &ScheduleId) -> ControllerResult<()> {
        self.persistence.delete_schedule(id).await
    }

    /// Webhook ingress. Finds enabled `Event` schedules whose
    /// `source` equals `path` and spawns runs for each. Returns
    /// the FIRST created run so the HTTP layer can echo it; if
    /// no schedule matches, returns `ScheduleNotFound`.
    pub async fn ingress_event(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> ControllerResult<RunRecord> {
        let candidates = self.persistence.list_enabled_event_schedules().await?;
        let mut matched = Vec::new();
        for sched in candidates {
            if let RunTrigger::Event { source } = &sched.trigger {
                if source == path {
                    matched.push(sched);
                }
            }
        }
        if matched.is_empty() {
            return Err(ControllerError::ScheduleNotFound {
                id: format!("no enabled Event schedule for path \"{path}\""),
            });
        }
        let now = now_ms();
        let mut created: Option<RunRecord> = None;
        for sched in matched {
            // Each match gets its own run, with the schedule's
            // source in the trigger and the body as inputs.
            let trigger = RunTrigger::Event { source: path.to_string() };
            match self
                .spawn_run_for(&sched.workflow_id, trigger, body.clone(), now)
                .await
            {
                Ok(rec) => {
                    if created.is_none() {
                        created = Some(rec);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "ingress_event: failed to spawn run for schedule {}: {}",
                        sched.id,
                        e
                    );
                }
            }
        }
        created.ok_or_else(|| ControllerError::ScheduleNotFound {
            id: format!("matched schedules but all failed to spawn for \"{path}\""),
        })
    }
}

// =============================================================
//  Cron helpers
// =============================================================

/// Compute the next firing time strictly after `after_ms`, in
/// milliseconds since UNIX epoch.
///
/// Accepts the conventional 5-field cron (`min hour dom mon dow`)
/// or the 6/7-field forms the `cron` crate expects natively. We
/// adapt 5-field to 7-field by prefixing `"0 "` (seconds) and
/// appending `" *"` (year) so `*/5 * * * *` works as a user
/// reasonably expects.
pub fn cron_next_after_ms(expr: &str, after_ms: i64) -> Result<i64, String> {
    let normalized = normalize_cron(expr);
    let sched = CronSchedule::from_str(&normalized).map_err(|e| e.to_string())?;
    let after = Utc
        .timestamp_millis_opt(after_ms)
        .single()
        .ok_or_else(|| format!("invalid timestamp: {after_ms}"))?;
    let next = sched
        .after(&after)
        .next()
        .ok_or_else(|| "cron expression has no future firing time".to_string())?;
    Ok(next.timestamp_millis())
}

fn normalize_cron(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    match parts.len() {
        // Already a 6/7-field form — pass through.
        6 | 7 => expr.to_string(),
        // 5-field (min hour dom mon dow): seconds=0, year=*.
        5 => format!("0 {expr} *"),
        // Otherwise let the cron crate report the actual error.
        _ => expr.to_string(),
    }
}

// =============================================================
//  Tests
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solflow_compiler::compile_source;
    use solflow_host_spec::encode_bytecode;

    /// Helper: persist a minimal workflow for tests so register()
    /// can verify it exists.
    async fn submit_clean_workflow(p: &SqlitePersistence) -> String {
        let cp = compile_source("function start() -> int { print(\"tick\"); return 0; }")
            .value
            .expect("clean compile");
        let id = format!("wf_{}", uuid::Uuid::new_v4().simple());
        let bc = encode_bytecode(&cp.bytecode).unwrap();
        let spans = serde_json::to_vec::<Vec<()>>(&vec![]).unwrap();
        let meta = serde_json::json!({
            "name": "scheduler-test",
            "content_hash": "x",
            "created_at": now_ms(),
        });
        p.put_workflow(&id, &bc, &spans, &meta.to_string())
            .await
            .unwrap();
        id
    }

    #[test]
    fn cron_5_field_normalizes_and_parses() {
        // `*/5 * * * *` → fires every 5 minutes.
        let now = 1_700_000_000_000_i64;
        let next = cron_next_after_ms("*/5 * * * *", now).expect("ok");
        // Next firing time must be strictly after `now` AND within
        // 5 minutes + 1s of slack.
        assert!(next > now);
        assert!(next - now <= 5 * 60 * 1000 + 1_000);
    }

    #[test]
    fn cron_invalid_returns_error() {
        let err = cron_next_after_ms("not a cron", 1_700_000_000_000)
            .expect_err("invalid");
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn register_timer_persists_next_fire_at() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        let rec = ScheduleRecord {
            id: String::new(),
            workflow_id: wf,
            trigger: RunTrigger::Timer {
                schedule_id: String::new(),
                cron: "*/5 * * * *".into(),
            },
            enabled: true,
            next_fire_at: None,
            created_at: 0,
        };
        let registered = sched.register(rec).await.unwrap();
        assert!(registered.id.starts_with("sch_"));
        assert!(registered.next_fire_at.is_some());
        // Round-trips through persistence.
        let from_db = p.get_schedule(&registered.id).await.unwrap();
        assert_eq!(from_db.next_fire_at, registered.next_fire_at);
    }

    #[tokio::test]
    async fn register_event_has_no_next_fire() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        let rec = ScheduleRecord {
            id: String::new(),
            workflow_id: wf,
            trigger: RunTrigger::Event { source: "deploy".into() },
            enabled: true,
            next_fire_at: Some(999),
            created_at: 0,
        };
        let registered = sched.register(rec).await.unwrap();
        assert_eq!(registered.next_fire_at, None);
    }

    #[tokio::test]
    async fn register_manual_trigger_rejected() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        let rec = ScheduleRecord {
            id: String::new(),
            workflow_id: wf,
            trigger: RunTrigger::Manual,
            enabled: true,
            next_fire_at: None,
            created_at: 0,
        };
        let err = sched.register(rec).await.expect_err("rejected");
        assert!(matches!(err, ControllerError::BytecodeInvalid { .. }));
    }

    #[tokio::test]
    async fn cancel_deletes_schedule() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        let rec = sched
            .register(ScheduleRecord {
                id: String::new(),
                workflow_id: wf,
                trigger: RunTrigger::Event { source: "x".into() },
                enabled: true,
                next_fire_at: None,
                created_at: 0,
            })
            .await
            .unwrap();
        sched.cancel(&rec.id).await.unwrap();
        let err = p.get_schedule(&rec.id).await.expect_err("gone");
        assert!(matches!(err, ControllerError::ScheduleNotFound { .. }));
    }

    #[tokio::test]
    async fn ingress_event_creates_run_for_matching_path() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        sched
            .register(ScheduleRecord {
                id: String::new(),
                workflow_id: wf,
                trigger: RunTrigger::Event { source: "deploy".into() },
                enabled: true,
                next_fire_at: None,
                created_at: 0,
            })
            .await
            .unwrap();
        let body = serde_json::json!({ "ref": "main" });
        let rec = sched.ingress_event("deploy", body.clone()).await.unwrap();
        assert!(rec.id.starts_with("run_"));
        assert_eq!(rec.inputs, body);
        match rec.trigger {
            RunTrigger::Event { source } => assert_eq!(source, "deploy"),
            _ => panic!("expected Event trigger"),
        }
    }

    #[tokio::test]
    async fn ingress_event_unmatched_path_returns_not_found() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        let err = sched
            .ingress_event("nowhere", serde_json::json!({}))
            .await
            .expect_err("no match");
        assert!(matches!(err, ControllerError::ScheduleNotFound { .. }));
    }

    /// End-to-end tick: register a timer with next_fire_at already
    /// in the past; tick_once should fire it (create a run) and
    /// advance next_fire_at past `now`.
    #[tokio::test]
    async fn tick_once_fires_overdue_timer_and_advances_next_fire() {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let wf = submit_clean_workflow(&p).await;
        let sched = TokioScheduler::new(p.clone(), RunPolicy::default());
        // Hand-craft a schedule that's already overdue. Use a
        // cron that fires every minute so the next fire after
        // "now" is bounded by ~60s of slack.
        let id = "sch_overdue".to_string();
        let now = now_ms();
        let s = ScheduleRecord {
            id: id.clone(),
            workflow_id: wf,
            trigger: RunTrigger::Timer {
                schedule_id: id.clone(),
                cron: "* * * * *".into(),
            },
            enabled: true,
            next_fire_at: Some(now - 1_000), // 1s ago
            created_at: now,
        };
        p.put_schedule(&s).await.unwrap();

        sched.tick_once().await.unwrap();

        // Schedule's next_fire_at must have advanced past `now`.
        let updated = p.get_schedule(&id).await.unwrap();
        assert!(updated.next_fire_at.unwrap() > now);
    }
}
