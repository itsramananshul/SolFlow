/**
 * TypeScript mirror of `host-spec/src/lib.rs`.
 *
 * The IDE ↔ controller wire-protocol shapes (Phase C C.1). Pinned
 * by the round-trip + tag-format tests in the Rust crate; the
 * editor parses controller responses via these types.
 *
 * Implementation note: this is a one-way mirror. Anytime the Rust
 * types change, this file must be updated in lockstep. The Rust
 * crate's `enum_tag_format_uses_kind_field` test is the canonical
 * specification of how discriminated unions are tagged.
 *
 * C.1 scaffolding only — no transport / no fetch / no WebSocket
 * client. C.2 ships the actual editor → controller client.
 */

/** Current host-spec major version. Editors + controllers
 *  refuse to connect on mismatch. */
export const HOST_SPEC_MAJOR = 0;

export type WorkflowId = string;
export type RunId = string;
export type ScheduleId = string;

// =============================================================
//  Workflow submission
// =============================================================

export interface WorkflowSubmission {
  name: string;
  description?: string;
  /**
   * Wire-encoded bytecode bytes. Comes from the WASM compiler's
   * `compile_for_wire_json` entry point, which JSON-encodes
   * `Vec<Inst>` via `serde_json::to_vec` to match
   * `solflow_host_spec::encode_bytecode`. Transported as a JSON
   * number-array since the outer `WorkflowSubmission` itself is
   * JSON — `Vec<u8>` serializes as `[u8, u8, …]` by default.
   */
  bytecode: number[];
  /**
   * Wire-encoded per-instruction span sidecar. Same contract
   * as `bytecode`. Parallel to the instruction stream so the
   * controller can attach source-mapped runtime errors.
   */
  instruction_spans: number[];
  /** Optional original SOL source — purely for debug. */
  source?: string;
}

export interface WorkflowSubmissionResponse {
  workflow_id: WorkflowId;
  content_hash: string;
}

// =============================================================
//  Source spans
// =============================================================

export interface SourceSpan {
  start: number;
  end: number;
}

// =============================================================
//  Triggers / Runs
// =============================================================

export type RunTrigger =
  | { kind: 'Manual' }
  | { kind: 'Timer'; schedule_id: ScheduleId; cron: string }
  | { kind: 'Event'; source: string };

export interface RunRequest {
  workflow_id: WorkflowId;
  trigger: RunTrigger;
  inputs?: unknown;
}

/**
 * Per-run lifecycle state. Phase C C.6 (c89) extended this from
 * the original 5 states to 9. New variants are additive — older
 * editor builds see them as unknown strings and degrade
 * gracefully. See `docs/dev/RUN_LIFECYCLE.md`.
 */
export type RunStatus =
  | 'Queued'
  | 'Starting'    // dequeued, persisting; VM not yet ticking (C.6)
  | 'Running'
  | 'Cancelling'  // cancel requested, winding down (C.6)
  | 'Succeeded'
  | 'Failed'
  | 'Cancelled'
  | 'TimedOut'    // wall-clock budget exhausted (C.6)
  | 'Rejected';   // controller refused to enqueue (C.6)

export interface RunCreated {
  run_id: RunId;
  status: RunStatus;
}

/** What an execution-trace step records (mirrors the wire `TraceStep`). */
export type TraceStepKind = 'stmt' | 'call' | 'return' | 'error';

/**
 * One step of a real execution trace recorded by the controller's VM.
 * Identical in shape to the browser-sim trace so the Trace tab renders
 * both run targets the same way.
 */
export interface TraceStep {
  step: number;
  kind: TraceStepKind;
  function: string;
  span: SourceSpan | null;
  line: number | null;
  depth: number;
  detail: string | null;
}

export interface RunOutput {
  return_value: number | null;
  output: string[];
  steps: number;
  /** Real execution trace recorded by the VM, in order. */
  trace?: TraceStep[];
  /** True when the controller's trace cap was hit. */
  trace_truncated?: boolean;
}

/**
 * Mirror of `solflow_compiler::SolDiagnostic`. Already defined
 * in `src/compiler/types.ts`; re-imported here to keep
 * runtime-host self-contained.
 */
export interface SolDiagnostic {
  severity: 'Error' | 'Warning' | 'Note';
  phase: 'Lexer' | 'Parser' | 'Analyzer' | 'Codegen' | 'Runtime' | 'Internal';
  code: string;
  message: string;
  span: SourceSpan | null;
  related: Array<{ span: SourceSpan; message: string }>;
  help: string | null;
}

export interface RunRecord {
  id: RunId;
  workflow_id: WorkflowId;
  status: RunStatus;
  trigger: RunTrigger;
  inputs: unknown;
  output?: RunOutput;
  diagnostics: SolDiagnostic[];
  created_at: number;     // ms since UNIX epoch
  started_at?: number;
  completed_at?: number;
}

// =============================================================
//  Runtime errors
// =============================================================

export type RuntimeErrorView =
  | { kind: 'DivByZero' }
  | { kind: 'IndexOutOfBounds'; index: number; length: number }
  | { kind: 'StackUnderflow' }
  | { kind: 'StepLimit'; limit: number }
  | { kind: 'ExtCallBlocked'; function_name: string; url: string }
  | { kind: 'ExtCallFailed'; connector: string; function_name: string; message: string }
  | { kind: 'HeapShapeMismatch'; expected: string; got: string }
  | { kind: 'Cancelled' }
  | { kind: 'Timeout'; wall_clock_secs: number }
  /** Phase C C.6 — per-run resource cap exceeded.
   *  `resource` is one of "output_lines" / "events" / … */
  | { kind: 'ResourceLimit'; resource: string; limit: number };

// =============================================================
//  Event stream
// =============================================================

export type RunEvent =
  | { kind: 'Queued'; run_id: RunId; seq: number; ts: number }
  | { kind: 'Started'; run_id: RunId; seq: number; ts: number }
  | {
      kind: 'Print';
      run_id: RunId;
      seq: number;
      ts: number;
      text: string;
      source_span?: SourceSpan;
    }
  | {
      kind: 'ExtCallStarted';
      run_id: RunId;
      seq: number;
      ts: number;
      connector: string;
      fn_name: string;
    }
  | {
      kind: 'ExtCallCompleted';
      run_id: RunId;
      seq: number;
      ts: number;
      connector: string;
      fn_name: string;
      ok: boolean;
    }
  | {
      kind: 'Diagnostic';
      run_id: RunId;
      seq: number;
      ts: number;
      diagnostic: SolDiagnostic;
    }
  | {
      kind: 'Completed';
      run_id: RunId;
      seq: number;
      ts: number;
      output: RunOutput;
    }
  | {
      kind: 'Failed';
      run_id: RunId;
      seq: number;
      ts: number;
      error: RuntimeErrorView;
      source_span?: SourceSpan;
    }
  | {
      kind: 'Cancelled';
      run_id: RunId;
      seq: number;
      ts: number;
    }
  // Phase C C.6 — lifecycle expansion.
  | {
      kind: 'Starting';
      run_id: RunId;
      seq: number;
      ts: number;
    }
  | {
      kind: 'Cancelling';
      run_id: RunId;
      seq: number;
      ts: number;
    }
  | {
      kind: 'Rejected';
      run_id: RunId;
      seq: number;
      ts: number;
      reason: string;
    }
  | {
      kind: 'TimedOut';
      run_id: RunId;
      seq: number;
      ts: number;
      wall_clock_secs: number;
    };

// =============================================================
//  Schedules
// =============================================================

export interface ScheduleCreate {
  trigger: RunTrigger;
  enabled?: boolean;
}

export interface ScheduleRecord {
  id: ScheduleId;
  workflow_id: WorkflowId;
  trigger: RunTrigger;
  enabled: boolean;
  next_fire_at?: number;
  created_at: number;
}

// =============================================================
//  Health
// =============================================================

/**
 * `GET /healthz` payload. Phase C C.7 (c97) added `name` +
 * `auth_required` so editors can fingerprint + capability-probe
 * a controller before negotiating a connection.
 *
 * Both new fields are **optional in the wire shape** — older
 * controllers don't include them; the editor falls back to
 * "no auth" + "unknown name" semantics in that case.
 */
export interface Health {
  ok: boolean;
  controller_version: string;
  host_spec_major: number;
  /** Software identifier. Stable string `"solflow-controller"`
   *  on canonical builds; absent on pre-C.7 controllers. */
  name?: string;
  /** Whether mutating endpoints require `Authorization: Bearer …`.
   *  Absent on pre-C.7 controllers — treated as `false`. */
  auth_required?: boolean;
}

/** Stable controller-name fingerprint. Editor checks
 *  `Health.name` against this case-insensitively. */
export const CONTROLLER_NAME = 'solflow-controller';

// =============================================================
//  Connectors (Phase C C.4)
// =============================================================

/** Conservative per-call execution policy mirroring
 *  `solflow_controller::InvocationPolicy`. */
export interface InvocationPolicy {
  timeout_ms: number;
  retry_attempts: number;
  backoff_base_ms: number;
  max_response_bytes: number;
}

/** Per-connector metadata returned by `GET /connectors`. */
export interface ConnectorMeta {
  name: string;
  description: string;
  version: string;
  default_policy: InvocationPolicy;
}

// =============================================================
//  Orchestration introspection (Phase C C.6)
// =============================================================

/** Snapshot of one in-flight run from `GET /runs/active`. */
export interface ActiveRunSummary {
  run_id: RunId;
  workflow_id: WorkflowId;
  /** ms since UNIX epoch when the worker dequeued the run. */
  dispatched_at: number;
}

/** Saturation policy mirrored from
 *  `solflow_controller::run_manager::SaturationPolicy`. */
export type SaturationPolicy = 'Queue' | 'Reject';

/** Controller-wide concurrency snapshot from
 *  `GET /controller/concurrency`. */
export interface ConcurrencyMetrics {
  max_concurrent_runs: number;
  max_queued_runs: number;
  active_runs: number;
  queued_runs: number;
  saturation_policy: SaturationPolicy;
}
