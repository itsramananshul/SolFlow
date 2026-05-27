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

export type RunStatus =
  | 'Queued'
  | 'Running'
  | 'Succeeded'
  | 'Failed'
  | 'Cancelled';

export interface RunCreated {
  run_id: RunId;
  status: RunStatus;
}

export interface RunOutput {
  return_value: number | null;
  output: string[];
  steps: number;
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
  | { kind: 'Timeout'; wall_clock_secs: number };

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

export interface Health {
  ok: boolean;
  controller_version: string;
  host_spec_major: number;
}

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
