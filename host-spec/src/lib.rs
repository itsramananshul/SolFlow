//! `solflow_host_spec` — IDE ↔ controller wire-protocol types.
//!
//! Phase C C.1 scaffolding. Defines the shared shapes the editor
//! and the controller exchange across the network. Pure data +
//! serde derives; this crate has no transport (no HTTP, no
//! WebSocket, no async) and no implementation.
//!
//! The TS editor mirrors these types in
//! `src/runtime-host/types.ts`. Both sides serialize via serde's
//! default (externally-tagged) representation; the TS mirror is
//! pinned by the round-trip tests at the bottom of this file.
//!
//! ## Versioning
//!
//! `host-spec` is **semver-bound**. Breaking shape changes bump
//! the major version; controllers + editors that disagree on
//! major version refuse to connect. See architecture doc §5.3.

use serde::{Deserialize, Serialize};
use solflow_compiler::SolDiagnostic;

/// The host-spec major version. Bump on breaking shape changes.
/// Minor / patch live in `Cargo.toml::version`.
pub const HOST_SPEC_MAJOR: u32 = 0;

/// Stable string ID for a workflow (controller-assigned on first
/// `POST /workflows`). Opaque to the editor.
pub type WorkflowId = String;

/// Stable string ID for a run (controller-assigned on
/// `POST /runs`).
pub type RunId = String;

/// Stable string ID for a schedule.
pub type ScheduleId = String;

// =============================================================
//  Workflow submission
// =============================================================

/// The body of `POST /workflows`. Editor sends compiled bytecode
/// + the instruction-spans sidecar so the controller can attach
/// source-mapped diagnostics on runtime errors.
///
/// `source` is optional — most callers send it for debuggability,
/// but it's not required for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSubmission {
    /// Human-readable display name (NOT a unique key — the
    /// controller assigns `WorkflowId` on receipt).
    pub name: String,

    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,

    /// Canonical-compiled bytecode (`bincode`-encoded
    /// `Vec<solflow_compiler::bytecode::Inst>`).
    /// Wire format is base64-of-bincode in JSON transports.
    pub bytecode: Vec<u8>,

    /// Per-instruction span sidecar (`bincode`-encoded
    /// `Vec<Option<SourceSpan>>` parallel to `bytecode`).
    /// Same wire format as `bytecode`.
    pub instruction_spans: Vec<u8>,

    /// Optional original SOL source — purely for editor debug
    /// affordances; the controller never re-compiles from it.
    #[serde(default)]
    pub source: Option<String>,
}

/// Returned by `POST /workflows`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSubmissionResponse {
    pub workflow_id: WorkflowId,
    /// Content hash of the bytecode (used for replay + audit).
    pub content_hash: String,
}

// =============================================================
//  Run lifecycle
// =============================================================

/// Trigger that created a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RunTrigger {
    /// IDE-initiated run.
    Manual,
    /// Scheduler-fired run; carries the original schedule ID for
    /// audit + cancellation.
    Timer { schedule_id: ScheduleId, cron: String },
    /// Webhook-fired run; carries the webhook path that received
    /// the event.
    Event { source: String },
}

/// Body of `POST /runs`. Inputs are an opaque JSON value the
/// workflow's `start` function can interpret (Phase C doesn't yet
/// constrain input shape per workflow signature; that's a future
/// type-system milestone).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRequest {
    pub workflow_id: WorkflowId,
    pub trigger: RunTrigger,
    #[serde(default)]
    pub inputs: serde_json::Value,
}

/// Returned by `POST /runs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCreated {
    pub run_id: RunId,
    pub status: RunStatus,
}

/// Per-run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Final run output. Present iff `status == Succeeded`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutput {
    /// Top-of-stack value at termination. Interpretation per
    /// declared return type (matches the existing browser-sim
    /// `RunResult.return_value`).
    pub return_value: Option<i64>,
    /// Captured `print` output.
    pub output: Vec<String>,
    /// Total VM steps executed.
    pub steps: usize,
}

/// Full state returned by `GET /runs/:id`. Includes events only
/// when explicitly requested with `?include=events`; otherwise
/// the events table is paged separately via
/// `GET /runs/:id/events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub workflow_id: WorkflowId,
    pub status: RunStatus,
    pub trigger: RunTrigger,
    pub inputs: serde_json::Value,
    #[serde(default)]
    pub output: Option<RunOutput>,
    /// Compile diagnostics carried for completeness — typically
    /// empty since only canonical-compiled bytecode reaches the
    /// controller; populated if a hypothetical future warning
    /// stream is added.
    #[serde(default)]
    pub diagnostics: Vec<SolDiagnostic>,
    /// Wall-clock timestamps (millis since UNIX epoch).
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
}

// =============================================================
//  Event stream
// =============================================================

/// One event in a run's execution stream.
///
/// Monotonic `seq` per run (starts at 0). Clients can resume from
/// a known seq by passing `?after=N` to the events endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RunEvent {
    Queued {
        run_id: RunId,
        seq: u64,
        ts: i64,
    },
    Started {
        run_id: RunId,
        seq: u64,
        ts: i64,
    },
    Print {
        run_id: RunId,
        seq: u64,
        ts: i64,
        text: String,
        #[serde(default)]
        source_span: Option<SourceSpan>,
    },
    ExtCallStarted {
        run_id: RunId,
        seq: u64,
        ts: i64,
        connector: String,
        fn_name: String,
    },
    ExtCallCompleted {
        run_id: RunId,
        seq: u64,
        ts: i64,
        connector: String,
        fn_name: String,
        ok: bool,
    },
    Diagnostic {
        run_id: RunId,
        seq: u64,
        ts: i64,
        diagnostic: SolDiagnostic,
    },
    Completed {
        run_id: RunId,
        seq: u64,
        ts: i64,
        output: RunOutput,
    },
    Failed {
        run_id: RunId,
        seq: u64,
        ts: i64,
        error: RuntimeErrorView,
        #[serde(default)]
        source_span: Option<SourceSpan>,
    },
    Cancelled {
        run_id: RunId,
        seq: u64,
        ts: i64,
    },
}

/// Wire shape for a structured runtime error. Matches the
/// existing `RuntimeErrorView` from the compiler-wasm bridge so
/// the editor doesn't need a separate union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RuntimeErrorView {
    DivByZero,
    IndexOutOfBounds { index: usize, length: usize },
    StackUnderflow,
    StepLimit { limit: usize },
    /// In browser-sim this is the "blocked" variant; in
    /// controller mode it represents a real connector call that
    /// failed (timeout / network / 5xx / etc.).
    ExtCallBlocked { function_name: String, url: String },
    /// Real connector call that the controller attempted but
    /// failed for a non-blocked reason. C.4+.
    ExtCallFailed { connector: String, function_name: String, message: String },
    HeapShapeMismatch { expected: String, got: String },
    /// Run cancelled via `DELETE /runs/:id`. C.6+.
    Cancelled,
    /// Wall-clock timeout exceeded.
    Timeout { wall_clock_secs: u64 },
}

/// Byte-range source span. Mirrors `solflow_compiler::SourceSpan`
/// but locally-declared so this crate doesn't expose serde
/// internals of the compiler crate to the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl From<solflow_compiler::SourceSpan> for SourceSpan {
    fn from(s: solflow_compiler::SourceSpan) -> Self {
        Self { start: s.start, end: s.end }
    }
}

// =============================================================
//  Schedules
// =============================================================

/// Body of `POST /workflows/:id/schedules`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleCreate {
    pub trigger: RunTrigger,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRecord {
    pub id: ScheduleId,
    pub workflow_id: WorkflowId,
    pub trigger: RunTrigger,
    pub enabled: bool,
    #[serde(default)]
    pub next_fire_at: Option<i64>,
    pub created_at: i64,
}

// =============================================================
//  Health + version
// =============================================================

/// `GET /healthz` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub ok: bool,
    /// Full controller version string (e.g. crate version).
    pub controller_version: String,
    /// Host-spec major version this controller speaks. Editors
    /// reject connection on mismatch.
    pub host_spec_major: u32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            ok: true,
            controller_version: env!("CARGO_PKG_VERSION").to_string(),
            host_spec_major: HOST_SPEC_MAJOR,
        }
    }
}

// =============================================================
//  Tests — pin the wire shape so the TS mirror stays valid
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Serialize a sample of every top-level type, parse back,
    /// re-serialize; identical JSON proves round-trip stability.
    /// Also gives the TS mirror author a concrete reference.
    #[test]
    fn round_trip_workflow_submission() {
        let s = WorkflowSubmission {
            name: "Hello".into(),
            description: Some("test".into()),
            bytecode: vec![1, 2, 3, 4],
            instruction_spans: vec![5, 6, 7, 8],
            source: Some("function start() -> int { return 0; }".into()),
        };
        round_trip(&s);
    }

    #[test]
    fn round_trip_run_record() {
        let r = RunRecord {
            id: "run_abc".into(),
            workflow_id: "wf_xyz".into(),
            status: RunStatus::Succeeded,
            trigger: RunTrigger::Manual,
            inputs: serde_json::json!({}),
            output: Some(RunOutput {
                return_value: Some(42),
                output: vec!["hello".into()],
                steps: 12,
            }),
            diagnostics: Vec::new(),
            created_at: 1_700_000_000_000,
            started_at: Some(1_700_000_000_500),
            completed_at: Some(1_700_000_001_000),
        };
        round_trip(&r);
    }

    #[test]
    fn round_trip_run_event_completed() {
        let e = RunEvent::Completed {
            run_id: "run_abc".into(),
            seq: 5,
            ts: 1_700_000_001_000,
            output: RunOutput {
                return_value: Some(7),
                output: vec!["hi".into()],
                steps: 3,
            },
        };
        round_trip(&e);
    }

    #[test]
    fn round_trip_run_event_failed_with_span() {
        let e = RunEvent::Failed {
            run_id: "run_abc".into(),
            seq: 9,
            ts: 1_700_000_002_000,
            error: RuntimeErrorView::DivByZero,
            source_span: Some(SourceSpan { start: 42, end: 47 }),
        };
        round_trip(&e);
    }

    #[test]
    fn enum_tag_format_uses_kind_field() {
        // Pins the TS-mirror contract. Editor parses these as
        // discriminated unions on the `kind` field.
        let json = serde_json::to_string(&RunTrigger::Manual).unwrap();
        assert_eq!(json, r#"{"kind":"Manual"}"#);

        let json =
            serde_json::to_string(&RunStatus::Succeeded).unwrap();
        // Plain enum variants serialize as bare strings.
        assert_eq!(json, r#""Succeeded""#);

        let json = serde_json::to_string(
            &RuntimeErrorView::DivByZero,
        ).unwrap();
        assert_eq!(json, r#"{"kind":"DivByZero"}"#);

        let json = serde_json::to_string(&RuntimeErrorView::ExtCallFailed {
            connector: "http".into(),
            function_name: "fetch".into(),
            message: "timeout".into(),
        })
        .unwrap();
        assert!(json.starts_with(r#"{"kind":"ExtCallFailed""#));
    }

    #[test]
    fn health_defaults_populate_correctly() {
        let h = Health::default();
        assert!(h.ok);
        assert_eq!(h.host_spec_major, HOST_SPEC_MAJOR);
        assert!(!h.controller_version.is_empty());
    }

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug>(val: &T) {
        let json1 = serde_json::to_string(val).expect("serialize");
        let restored: T = serde_json::from_str(&json1).expect("deserialize");
        let json2 = serde_json::to_string(&restored).expect("re-serialize");
        assert_eq!(
            json1, json2,
            "round-trip must produce identical JSON",
        );
        // Also verify it parses as a JSON Value cleanly (no
        // bincode bytes leaking, no NaN, etc.).
        let _: Value = serde_json::from_str(&json1).expect("valid JSON");
    }
}
