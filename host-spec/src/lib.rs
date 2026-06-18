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
use solflow_compiler::bytecode::Inst;

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

/// Per-run lifecycle state.
///
/// Phase C C.6 (c89) extended this from the original 5 states
/// (Queued / Running / Succeeded / Failed / Cancelled) to a full
/// 9-state lifecycle. New variants are **additive** at the wire
/// level — older editor builds see them as unknown strings and
/// degrade gracefully (we render "Unknown" rather than crash).
///
/// State machine (terminals are sinks):
///
/// ```text
///   Queued     ─► Starting | Cancelled | Rejected
///   Starting   ─► Running  | Cancelled | Failed
///   Running    ─► Succeeded | Failed | Cancelling | TimedOut
///   Cancelling ─► Cancelled | Failed
/// ```
///
/// See `RunStatus::can_transition_to` for the enforcement helper
/// the controller uses; see `docs/dev/RUN_LIFECYCLE.md` for the
/// authoritative narrative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    /// Accepted by the controller; sitting in the pending queue.
    Queued,
    /// Dequeued by a worker; persisting `started_at` and emitting
    /// lifecycle events. VM not yet ticking. (Phase C C.6)
    Starting,
    /// The VM is executing instructions.
    Running,
    /// A cancel request landed; waiting for execution to wind
    /// down + connectors to release. Terminal `Cancelled`
    /// follows shortly. (Phase C C.6)
    Cancelling,
    /// Terminal — VM ran to completion without runtime error.
    Succeeded,
    /// Terminal — VM hit a runtime error (div-by-zero / OOB /
    /// ExtCallFailed / ResourceLimit / …).
    Failed,
    /// Terminal — user-initiated cancellation completed.
    Cancelled,
    /// Terminal — wall-clock budget exceeded. Distinguished from
    /// `Failed` so the editor can render distinct UX. (Phase C C.6)
    TimedOut,
    /// Terminal — controller refused to enqueue (e.g. queue full
    /// under reject-on-saturation policy). No VM execution
    /// attempted. (Phase C C.6)
    Rejected,
}

/// Error returned when a caller tries `RunStatus::transition_to`
/// with an invalid (from, to) pair. Used by the controller's
/// lifecycle helper to catch state-machine bugs at the source
/// rather than silently corrupting status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: RunStatus,
    pub to: RunStatus,
}

impl std::fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid run-status transition: {:?} → {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for InvalidTransition {}

impl RunStatus {
    /// Terminal states are sinks — no outgoing transitions.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::TimedOut
                | RunStatus::Rejected
        )
    }

    /// Whether `from → to` is a valid transition. Source of truth
    /// for the lifecycle state machine. Encoded once; controller
    /// + tests + future remote-controller code consult this.
    pub fn can_transition_to(self, next: RunStatus) -> bool {
        use RunStatus::*;
        match (self, next) {
            // Queued —
            (Queued, Starting) | (Queued, Cancelled) | (Queued, Rejected) => true,
            // Starting —
            (Starting, Running) | (Starting, Cancelled) | (Starting, Failed) => true,
            // Running —
            (Running, Succeeded)
            | (Running, Failed)
            | (Running, Cancelling)
            | (Running, TimedOut) => true,
            // Cancelling —
            (Cancelling, Cancelled) | (Cancelling, Failed) => true,
            // Anything else (including any-from-terminal, or
            // any-to-same-state) is rejected.
            _ => false,
        }
    }

    /// Disciplined transition entry point. Returns
    /// `Err(InvalidTransition)` if the move would violate the
    /// state machine. The controller's RunManager calls this on
    /// every status update so bugs surface here, not as silently
    /// corrupt persisted state.
    pub fn transition_to(self, next: RunStatus) -> Result<RunStatus, InvalidTransition> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(InvalidTransition { from: self, to: next })
        }
    }
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
    /// Phase C C.6 — `Queued → Starting`. The dispatcher
    /// dequeued the run; the VM hasn't ticked yet.
    Starting {
        run_id: RunId,
        seq: u64,
        ts: i64,
    },
    /// Phase C C.6 — a cancel request arrived; waiting for the
    /// VM to finish current step + connectors to release. A
    /// terminal `Cancelled` follows shortly.
    Cancelling {
        run_id: RunId,
        seq: u64,
        ts: i64,
    },
    /// Phase C C.6 — terminal: controller refused to enqueue
    /// (queue saturation under reject-on-saturation policy).
    Rejected {
        run_id: RunId,
        seq: u64,
        ts: i64,
        reason: String,
    },
    /// Phase C C.6 — terminal: wall-clock budget exhausted.
    /// Distinct from `Failed` so the editor renders distinct UX.
    TimedOut {
        run_id: RunId,
        seq: u64,
        ts: i64,
        wall_clock_secs: u64,
    },
}

impl RunEvent {
    /// Run id the event belongs to. Useful so persistence /
    /// broadcast layers don't have to pattern-match the variant.
    pub fn run_id(&self) -> &RunId {
        match self {
            RunEvent::Queued { run_id, .. }
            | RunEvent::Started { run_id, .. }
            | RunEvent::Print { run_id, .. }
            | RunEvent::ExtCallStarted { run_id, .. }
            | RunEvent::ExtCallCompleted { run_id, .. }
            | RunEvent::Diagnostic { run_id, .. }
            | RunEvent::Completed { run_id, .. }
            | RunEvent::Failed { run_id, .. }
            | RunEvent::Cancelled { run_id, .. }
            | RunEvent::Starting { run_id, .. }
            | RunEvent::Cancelling { run_id, .. }
            | RunEvent::Rejected { run_id, .. }
            | RunEvent::TimedOut { run_id, .. } => run_id,
        }
    }

    /// Monotonic seq the event was emitted at. The same value
    /// the SSE replay endpoint expects via `?after=N`.
    pub fn seq(&self) -> u64 {
        match self {
            RunEvent::Queued { seq, .. }
            | RunEvent::Started { seq, .. }
            | RunEvent::Print { seq, .. }
            | RunEvent::ExtCallStarted { seq, .. }
            | RunEvent::ExtCallCompleted { seq, .. }
            | RunEvent::Diagnostic { seq, .. }
            | RunEvent::Completed { seq, .. }
            | RunEvent::Failed { seq, .. }
            | RunEvent::Cancelled { seq, .. }
            | RunEvent::Starting { seq, .. }
            | RunEvent::Cancelling { seq, .. }
            | RunEvent::Rejected { seq, .. }
            | RunEvent::TimedOut { seq, .. } => *seq,
        }
    }

    /// Wall-clock timestamp (millis since UNIX epoch).
    pub fn ts(&self) -> i64 {
        match self {
            RunEvent::Queued { ts, .. }
            | RunEvent::Started { ts, .. }
            | RunEvent::Print { ts, .. }
            | RunEvent::ExtCallStarted { ts, .. }
            | RunEvent::ExtCallCompleted { ts, .. }
            | RunEvent::Diagnostic { ts, .. }
            | RunEvent::Completed { ts, .. }
            | RunEvent::Failed { ts, .. }
            | RunEvent::Cancelled { ts, .. }
            | RunEvent::Starting { ts, .. }
            | RunEvent::Cancelling { ts, .. }
            | RunEvent::Rejected { ts, .. }
            | RunEvent::TimedOut { ts, .. } => *ts,
        }
    }

    /// The serde tag — matches the persistence table's `kind`
    /// column and the SSE event name.
    pub fn kind(&self) -> &'static str {
        match self {
            RunEvent::Queued { .. } => "Queued",
            RunEvent::Started { .. } => "Started",
            RunEvent::Print { .. } => "Print",
            RunEvent::ExtCallStarted { .. } => "ExtCallStarted",
            RunEvent::ExtCallCompleted { .. } => "ExtCallCompleted",
            RunEvent::Diagnostic { .. } => "Diagnostic",
            RunEvent::Completed { .. } => "Completed",
            RunEvent::Failed { .. } => "Failed",
            RunEvent::Cancelled { .. } => "Cancelled",
            RunEvent::Starting { .. } => "Starting",
            RunEvent::Cancelling { .. } => "Cancelling",
            RunEvent::Rejected { .. } => "Rejected",
            RunEvent::TimedOut { .. } => "TimedOut",
        }
    }

    /// True for variants that end a run lifecycle. SSE handlers
    /// close the connection after sending one of these.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunEvent::Completed { .. }
                | RunEvent::Failed { .. }
                | RunEvent::Cancelled { .. }
                | RunEvent::Rejected { .. }
                | RunEvent::TimedOut { .. }
        )
    }
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
    /// Per-run resource cap exceeded. `resource` discriminates
    /// which cap fired — `"output_lines"` for the print buffer
    /// cap, `"events"` for the per-run event-log cap, etc.
    /// Phase C C.6. (Field is `resource` rather than `kind` to
    /// avoid clashing with the enum's serde tag.)
    ResourceLimit { resource: String, limit: u64 },
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
//  Diagnostics (locally owned wire types)
// =============================================================
//
// host-spec owns its wire types ("pure data + serde; no transport,
// no impl"). These mirror the shape the editor expects (and the
// compiler-wasm bridge emits) byte for byte, so the wire JSON and
// the editor's `runtime-host/types.ts` are unchanged — only the
// crate that defines the type moved.

/// Severity tier for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}

/// Which pipeline stage produced the diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticPhase {
    Lexer,
    Parser,
    Analyzer,
    Codegen,
    Runtime,
    Internal,
}

/// Secondary span pointing at related context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedSpan {
    pub span: SourceSpan,
    pub message: String,
}

/// A diagnostic produced by any pipeline stage. Field order matches
/// the natural display order and the wire contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolDiagnostic {
    pub severity: DiagnosticSeverity,
    pub phase: DiagnosticPhase,
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub related: Vec<RelatedSpan>,
    pub help: Option<String>,
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

// =============================================================
//  Bytecode wire encoding (C.2 c59)
// =============================================================
//
// `WorkflowSubmission.bytecode` is a `Vec<u8>` at the wire-type
// level. The format INSIDE that vec is opaque from the host-spec
// perspective, but in practice the editor + controller agree on
// JSON-encoded `Vec<Inst>`. JSON over bincode for C.2 because:
//   - debuggability (you can curl + jq a bytecode payload)
//   - simpler dep graph (serde_json is already in the workspace)
//   - bytecode size isn't a perf concern at C.2 scale
// Bincode is a future optimization if payload sizes matter.

/// Encode bytecode for the wire. Same format on both sides;
/// callers must use this helper rather than `serde_json::to_vec`
/// directly so any future format change (e.g. bincode) happens
/// in one place.
pub fn encode_bytecode(insts: &[Inst]) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(insts)
}

/// Decode bytecode from the wire. Inverse of `encode_bytecode`.
pub fn decode_bytecode(bytes: &[u8]) -> Result<Vec<Inst>, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// Encode the per-instruction span sidecar for the wire. Same
/// format invariant as `encode_bytecode`.
pub fn encode_instruction_spans(
    spans: &[Option<SourceSpan>],
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(spans)
}

/// Decode the per-instruction span sidecar. Inverse of
/// `encode_instruction_spans`.
pub fn decode_instruction_spans(
    bytes: &[u8],
) -> Result<Vec<Option<SourceSpan>>, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// `GET /healthz` response.
///
/// Phase C C.7 (c97) extended this with `name` + `auth_required` so
/// the editor can fingerprint a controller before connecting:
///
/// - `name` lets remote editors confirm they're talking to a
///   real solflow controller (and not, say, a random HTTPS service
///   on the same port). Constant for now (`"solflow-controller"`);
///   future forks can override.
/// - `auth_required` tells the editor whether a bearer token is
///   needed BEFORE the user tries (and gets a 401 on) their first
///   protected request. The healthz endpoint itself stays open in
///   every config so this probe always works.
///
/// New fields are additive at the wire level — older editor builds
/// see them as unknown JSON keys and ignore them. Older controller
/// builds without these fields deserialize fine because the TS
/// mirror marks them optional and the editor falls back to "assume
/// no auth, name unknown" semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub ok: bool,
    /// Full controller version string (e.g. crate version).
    pub controller_version: String,
    /// Host-spec major version this controller speaks. Editors
    /// reject connection on mismatch.
    pub host_spec_major: u32,
    /// Stable software name. Lets clients confirm they're talking
    /// to a SolFlow controller. Defaults to `"solflow-controller"`.
    /// Phase C C.7.
    #[serde(default = "default_controller_name")]
    pub name: String,
    /// Whether the controller requires `Authorization: Bearer …`
    /// on mutating endpoints. `/healthz` itself is always open.
    /// Phase C C.7.
    #[serde(default)]
    pub auth_required: bool,
}

fn default_controller_name() -> String {
    "solflow-controller".to_string()
}

/// Stable software identifier baked into `Health::name`. The
/// editor compares case-insensitively so forks that capitalize
/// differently still pass the fingerprint check.
pub const CONTROLLER_NAME: &str = "solflow-controller";

impl Default for Health {
    fn default() -> Self {
        Self {
            ok: true,
            controller_version: env!("CARGO_PKG_VERSION").to_string(),
            host_spec_major: HOST_SPEC_MAJOR,
            name: CONTROLLER_NAME.to_string(),
            auth_required: false,
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

    // =============================================================
    //  Phase C C.6 c89 — lifecycle + new variants
    // =============================================================

    #[test]
    fn run_status_terminal_classification() {
        // Non-terminals
        assert!(!RunStatus::Queued.is_terminal());
        assert!(!RunStatus::Starting.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
        assert!(!RunStatus::Cancelling.is_terminal());
        // Terminals
        assert!(RunStatus::Succeeded.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(RunStatus::TimedOut.is_terminal());
        assert!(RunStatus::Rejected.is_terminal());
    }

    #[test]
    fn run_status_valid_transitions() {
        use RunStatus::*;
        let valid: Vec<(RunStatus, RunStatus)> = vec![
            // Queued —
            (Queued, Starting), (Queued, Cancelled), (Queued, Rejected),
            // Starting —
            (Starting, Running), (Starting, Cancelled), (Starting, Failed),
            // Running —
            (Running, Succeeded), (Running, Failed),
            (Running, Cancelling), (Running, TimedOut),
            // Cancelling —
            (Cancelling, Cancelled), (Cancelling, Failed),
        ];
        for (from, to) in valid {
            assert!(
                from.can_transition_to(to),
                "{from:?} → {to:?} should be valid",
            );
            assert_eq!(from.transition_to(to).unwrap(), to);
        }
    }

    #[test]
    fn run_status_rejects_invalid_transitions() {
        use RunStatus::*;
        let invalid: Vec<(RunStatus, RunStatus)> = vec![
            // Queued shortcuts that skip Starting are illegal.
            (Queued, Running), (Queued, Succeeded), (Queued, Failed),
            // Backward transitions
            (Running, Queued), (Cancelling, Running),
            // Terminals are sinks.
            (Succeeded, Failed), (Failed, Running),
            (Cancelled, Running), (Rejected, Starting),
            (TimedOut, Running),
            // Self-transitions are not transitions.
            (Running, Running), (Queued, Queued),
            // Starting can't TimedOut directly (only Running can).
            (Starting, TimedOut),
        ];
        for (from, to) in invalid {
            assert!(
                !from.can_transition_to(to),
                "{from:?} → {to:?} should be invalid",
            );
            let err = from.transition_to(to).expect_err("must reject");
            assert_eq!(err.from, from);
            assert_eq!(err.to, to);
        }
    }

    #[test]
    fn run_status_new_variants_serde_bare_strings() {
        // Wire compat: plain-enum variants serialize as bare
        // JSON strings (matches the existing `Succeeded` shape).
        for (status, expected) in &[
            (RunStatus::Starting, r#""Starting""#),
            (RunStatus::Cancelling, r#""Cancelling""#),
            (RunStatus::TimedOut, r#""TimedOut""#),
            (RunStatus::Rejected, r#""Rejected""#),
        ] {
            let s = serde_json::to_string(status).unwrap();
            assert_eq!(&s, expected);
            // Round-trip.
            let back: RunStatus = serde_json::from_str(&s).unwrap();
            assert_eq!(&back, status);
        }
    }

    #[test]
    fn run_event_lifecycle_variants_round_trip() {
        let evts: Vec<RunEvent> = vec![
            RunEvent::Starting {
                run_id: "r".into(), seq: 0, ts: 1,
            },
            RunEvent::Cancelling {
                run_id: "r".into(), seq: 1, ts: 2,
            },
            RunEvent::Rejected {
                run_id: "r".into(), seq: 2, ts: 3,
                reason: "queue full".into(),
            },
            RunEvent::TimedOut {
                run_id: "r".into(), seq: 3, ts: 4,
                wall_clock_secs: 600,
            },
        ];
        for ev in &evts {
            let json = serde_json::to_string(ev).unwrap();
            let back: RunEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(ev.kind(), back.kind());
            assert_eq!(ev.seq(), back.seq());
            assert_eq!(ev.ts(), back.ts());
            assert_eq!(ev.run_id(), back.run_id());
        }
    }

    #[test]
    fn run_event_is_terminal_covers_new_variants() {
        let make = |run_id: &str| -> Vec<(RunEvent, bool)> {
            vec![
                (RunEvent::Queued { run_id: run_id.into(), seq: 0, ts: 0 }, false),
                (RunEvent::Starting { run_id: run_id.into(), seq: 0, ts: 0 }, false),
                (RunEvent::Cancelling { run_id: run_id.into(), seq: 0, ts: 0 }, false),
                (RunEvent::Rejected { run_id: run_id.into(), seq: 0, ts: 0, reason: "".into() }, true),
                (RunEvent::TimedOut { run_id: run_id.into(), seq: 0, ts: 0, wall_clock_secs: 0 }, true),
                (RunEvent::Cancelled { run_id: run_id.into(), seq: 0, ts: 0 }, true),
            ]
        };
        for (ev, want) in make("r") {
            assert_eq!(ev.is_terminal(), want, "kind={}", ev.kind());
        }
    }

    #[test]
    fn runtime_error_view_resource_limit_serde() {
        let e = RuntimeErrorView::ResourceLimit {
            resource: "output_lines".into(),
            limit: 1024,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains(r#""kind":"ResourceLimit""#));
        assert!(json.contains(r#""resource":"output_lines""#));
        assert!(json.contains(r#""limit":1024"#));
        let back: RuntimeErrorView = serde_json::from_str(&json).unwrap();
        match back {
            RuntimeErrorView::ResourceLimit { resource, limit } => {
                assert_eq!(resource, "output_lines");
                assert_eq!(limit, 1024);
            }
            other => panic!("expected ResourceLimit, got {other:?}"),
        }
    }

    #[test]
    fn bytecode_round_trips_through_wire_encoding() {
        // C.2 c59: editor compiles → encode_bytecode → wire →
        // decode_bytecode → controller runs. Roundtrip must
        // preserve exact instruction sequence.
        use solflow_compiler::bytecode::Inst;
        use solflow_compiler::parser::Ast;
        let bytecode = vec![
            Inst::PushConst(Ast::ExprInteger(42)),
            Inst::PushConst(Ast::ExprInteger(7)),
            Inst::IntAdd,
            Inst::Ret,
        ];
        let bytes = encode_bytecode(&bytecode).expect("encode");
        let restored = decode_bytecode(&bytes).expect("decode");
        // Inst doesn't derive PartialEq; re-encode + compare bytes.
        let bytes2 = encode_bytecode(&restored).expect("re-encode");
        assert_eq!(bytes, bytes2, "bytecode round-trip stable");
    }

    #[test]
    fn instruction_spans_round_trip_through_wire_encoding() {
        let spans: Vec<Option<SourceSpan>> = vec![
            Some(SourceSpan { start: 0, end: 10 }),
            None,
            Some(SourceSpan { start: 12, end: 20 }),
            None,
        ];
        let bytes = encode_instruction_spans(&spans).expect("encode");
        let restored = decode_instruction_spans(&bytes).expect("decode");
        assert_eq!(spans, restored);
    }

    #[test]
    fn health_defaults_populate_correctly() {
        let h = Health::default();
        assert!(h.ok);
        assert_eq!(h.host_spec_major, HOST_SPEC_MAJOR);
        assert!(!h.controller_version.is_empty());
        assert_eq!(h.name, CONTROLLER_NAME);
        assert!(!h.auth_required);
    }

    /// Phase C C.7 c97 — older controller builds may serialize a
    /// `Health` without `name` / `auth_required`. The deserializer
    /// must populate sensible defaults rather than fail, so editors
    /// can still negotiate with pre-C.7 controllers.
    #[test]
    fn health_back_compat_missing_optional_fields() {
        let legacy_json = r#"{"ok":true,"controller_version":"0.1.0","host_spec_major":0}"#;
        let h: Health = serde_json::from_str(legacy_json).expect("legacy parse");
        assert!(h.ok);
        assert_eq!(h.host_spec_major, 0);
        assert_eq!(h.name, CONTROLLER_NAME);
        assert!(!h.auth_required);
    }

    /// Phase C C.7 c97 — round-trip when all new fields are set.
    #[test]
    fn health_round_trip_includes_new_fields() {
        let h = Health {
            ok: true,
            controller_version: "9.9.9".into(),
            host_spec_major: HOST_SPEC_MAJOR,
            name: "solflow-controller".into(),
            auth_required: true,
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains(r#""auth_required":true"#));
        assert!(json.contains(r#""name":"solflow-controller""#));
        let back: Health = serde_json::from_str(&json).unwrap();
        assert!(back.auth_required);
        assert_eq!(back.name, "solflow-controller");
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
