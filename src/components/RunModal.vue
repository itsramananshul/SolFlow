<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useSimulationStore } from '@/stores/simulation.store';
import { useUIStore } from '@/stores/ui.store';
import { useControllerStore } from '@/stores/controller.store';
import { useControllerRunHistoryStore } from '@/stores/controller-run-history.store';
import { recordTrace, type Trace } from '@/runtime/simulate';
import { runSource } from '@/compiler/api';
import {
  ControllerClientErr,
  type ControllerClientError,
} from '@/runtime-host/client';
import { isOpremClientError } from '@/runtime-host/opremClient';
import type { RunEventStreamHandle } from '@/runtime-host/event-stream';
import type {
  RunEnvelope,
  RuntimeError,
  SolDiagnostic,
  SourceSpan,
} from '@/compiler/types';
import type { RunEvent, RunRecord, RunStatus } from '@/runtime-host/types';
import { findNodeForSpan } from '@/graph/nodeLookup';

const graph = useGraphStore();
const sim = useSimulationStore();
const ui = useUIStore();
const controller = useControllerStore();
const runHistory = useControllerRunHistoryStore();

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

// --- B.10: canonical SOL VM execution (browser-sim mode) ----
//
// `runEnvelope` holds the result of `compile(emitted) + run` via
// the in-browser WASM VM. Output, return value, and runtime_error
// come from here — canonical SOL semantics, not the JS approx.
const runEnvelope = ref<RunEnvelope | null>(null);

// --- Phase C C.2 c63: controller-local execution mode -------
//
// When `mode === 'controller-local'`, `execute()` compiles for
// wire, POSTs to the controller, polls until terminal, and the
// rendered output below pulls from the resulting RunRecord.

// The run target lives in the controller store (browser-sim | local |
// cloud) so it persists and the Controller Settings modal stays in
// sync. Most of this component only needs the binary "browser vs
// controller" split, so we derive a local `mode` from it and leave the
// existing branches intact.
type ExecutionMode = 'browser-sim' | 'controller-local';
const mode = computed<ExecutionMode>(() =>
  controller.runTarget === 'browser-sim' ? 'browser-sim' : 'controller-local',
);

type ControllerRunState =
  | { kind: 'idle' }
  | { kind: 'compiling' }
  | { kind: 'compile_failed'; diagnostics: SolDiagnostic[] }
  | { kind: 'submitting' }
  | {
      kind: 'running';
      workflowId: string;
      runId: string;
      record: RunRecord;
      startedAt: number;
    }
  | {
      kind: 'done';
      workflowId: string;
      runId: string;
      record: RunRecord;
      durationMs: number;
    }
  | {
      kind: 'controller_error';
      phase: 'submit' | 'create' | 'poll';
      error: ControllerClientError;
      workflowId?: string;
      runId?: string;
    };

const controllerRun = ref<ControllerRunState>({ kind: 'idle' });

/** Live RunEvents streamed from the controller via SSE (c85).
 *  Cleared at the start of every controller-local run; populated
 *  as Queued / Started / Print / ExtCall* / Completed / Failed
 *  events arrive. */
const liveEvents = ref<RunEvent[]>([]);

/** Abort signal for the active controller run (poll loop). */
let controllerAbort: AbortController | null = null;
/** Handle for the active SSE event stream — closed on modal
 *  close, mode-switch, or terminal event. */
let liveStreamHandle: RunEventStreamHandle | null = null;

function closeLiveStream() {
  if (liveStreamHandle && !liveStreamHandle.isDone) {
    liveStreamHandle.close();
  }
  liveStreamHandle = null;
}

type RunTargetId = 'browser-sim' | 'local' | 'cloud';

interface RunTargetOption {
  id: RunTargetId;
  label: string;
  /** A URL is configured (or none is needed, for browser-sim). */
  configured: boolean;
  /** Live health: the controller answered /healthz. */
  connected: boolean;
  /** A health probe is in flight. */
  connecting: boolean;
  title: string;
}

/** The three run targets shown in the header, with live status pulled
 *  from the controller store. */
const runTargets = computed<RunTargetOption[]>(() => [
  {
    id: 'browser-sim',
    label: 'Browser Simulation',
    configured: true,
    connected: true,
    connecting: false,
    title: 'Run in this browser via the canonical SOL VM (WASM). External Actions are blocked.',
  },
  {
    id: 'local',
    label: 'Local Controller',
    configured: controller.localUrl.trim().length > 0,
    connected: controller.localConn.kind === 'connected',
    connecting: controller.localConn.kind === 'connecting',
    title: `Run on a controller on this machine (${controller.localUrl || 'set a URL in Controller Settings'}).`,
  },
  {
    id: 'cloud',
    label: 'Cloud Controller',
    configured: controller.cloudUrl.trim().length > 0,
    connected: controller.cloudConn.kind === 'connected',
    connecting: controller.cloudConn.kind === 'connecting',
    title:
      controller.cloudUrl.trim().length > 0
        ? `Run on the hosted HTTPS controller (${controller.cloudUrl}).`
        : 'Set a cloud controller URL in Controller Settings to enable.',
  },
]);

function selectRunTarget(id: RunTargetId) {
  controller.setRunTarget(id);
}

// --- Legacy JS-trace path (canvas playback only) -----------
//
// We still record a JS interpreter trace so the canvas can
// animate node-by-node playback. That animation is APPROXIMATE
// (per SIMULATOR_PARITY.md). The modal's text output panel now
// uses the canonical envelope above; the canvas playback is
// labeled "approximate animation" so users know not to trust
// its per-node timing as semantics.
const trace = ref<Trace | null>(null);

const isRunning = ref(false);

const tabs = ['output', 'live', 'trace', 'sol'] as const;
type Tab = (typeof tabs)[number];
const activeTab = ref<Tab>('output');

function tabLabel(t: Tab): string {
  if (t === 'output') return 'Output';
  if (t === 'live') return 'Live';
  if (t === 'trace') return 'Trace';
  return 'Generated SOL';
}

async function execute() {
  isRunning.value = true;
  runEnvelope.value = null;
  trace.value = null;
  controllerRun.value = { kind: 'idle' };
  if (controllerAbort) {
    controllerAbort.abort();
    controllerAbort = null;
  }
  // Defer to next tick so the UI shows "Running…" before WASM kicks in.
  await new Promise((r) => setTimeout(r, 0));
  try {
    if (mode.value === 'browser-sim') {
      // Canonical run (the authoritative output) in-browser.
      runEnvelope.value = await runSource(graph.emitted.source);
      // Legacy JS trace for canvas animation only.
      trace.value = recordTrace(graph.workflow);
      if (trace.value) sim.play(trace.value, { workflow: graph.workflow });
    } else {
      await executeControllerLocal();
    }
  } finally {
    isRunning.value = false;
  }
}

async function executeControllerLocal() {
  controllerAbort = new AbortController();
  const signal = controllerAbort.signal;

  // The controller compiles + runs canonical SOL *source* itself
  // through the openprem-sol-v2 VM (the same engine as the browser
  // sim). We carry the source in the submission's opaque bytecode
  // blob, so there is no client-side bytecode and no cross-crate
  // bytecode-format coupling: register the workflow (POST /workflows),
  // start a run (POST /runs), then poll GET /runs/:id to completion.
  controllerRun.value = { kind: 'submitting' };
  liveEvents.value = [];
  const submitStart = Date.now();
  const workflowName = entryWorkflowName();
  const client = controller.getClient();
  const source = graph.emitted.source;
  try {
    const enc = new TextEncoder();
    const wf = await client.submitWorkflow(
      {
        name: workflowName,
        bytecode: Array.from(enc.encode(source)),
        instruction_spans: Array.from(enc.encode('[]')),
        source,
      },
      { signal, timeoutMs: 10_000 },
    );
    const created = await client.createRun(
      { workflow_id: wf.workflow_id, trigger: { kind: 'Manual' } },
      { signal, timeoutMs: 10_000 },
    );
    controllerRun.value = {
      kind: 'running',
      workflowId: wf.workflow_id,
      runId: created.run_id,
      record: {
        id: created.run_id,
        workflow_id: wf.workflow_id,
        status: created.status,
        trigger: { kind: 'Manual' },
        inputs: {},
        diagnostics: [],
        created_at: submitStart,
        started_at: submitStart,
      },
      startedAt: submitStart,
    };
    const record = await client.pollRun(created.run_id, {
      signal,
      intervalMs: 200,
      overallTimeoutMs: 60_000,
    });
    const durationMs = Date.now() - submitStart;
    controllerRun.value = {
      kind: 'done',
      workflowId: wf.workflow_id,
      runId: created.run_id,
      record,
      durationMs,
    };
    runHistory.record({
      controllerUrl: controller.url,
      workflowId: wf.workflow_id,
      runId: created.run_id,
      workflowName,
      status: record.status,
      durationMs,
      submittedAt: submitStart,
    });
  } catch (e) {
    const phase: 'submit' | 'create' | 'poll' =
      controllerRun.value.kind === 'running' ? 'poll' : 'submit';
    const error: ControllerClientError =
      e instanceof ControllerClientErr
        ? e.payload
        : { kind: 'network', message: e instanceof Error ? e.message : String(e) };
    const ids =
      controllerRun.value.kind === 'running'
        ? { workflowId: controllerRun.value.workflowId, runId: controllerRun.value.runId }
        : {};
    controllerRun.value = { kind: 'controller_error', phase, error, ...ids };
  } finally {
    controllerAbort = null;
  }
}

/** The runnable workflow name to submit to the controller — the entry
 *  `workflow`, falling back to the first function. */
function entryWorkflowName(): string {
  const wf =
    graph.workflow.functions.find((f) => f.isWorkflow)
    ?? graph.workflow.functions[0];
  return wf?.name ?? 'main';
}

/** Map an opremClient error onto the modal's existing
 *  ControllerClientError display union. */
function opremErrorToControllerError(
  p: import('@/runtime-host/opremClient').OpremClientError,
): ControllerClientError {
  switch (p.kind) {
    case 'timeout':
      return { kind: 'timeout', message: p.message, timeoutMs: p.timeoutMs };
    case 'http':
      return { kind: 'http', status: p.status, message: p.message };
    case 'decode':
      return { kind: 'decode', message: p.message };
    case 'invalid_url':
      return { kind: 'invalid_url', reason: 'unparseable', message: p.message };
    case 'aborted':
      return { kind: 'aborted', message: p.message };
    case 'network':
    default:
      return { kind: 'network', message: p.message };
  }
}

// Phase C C.6 c93 — Cancel button availability + handler.

/** True when we're in controller-local mode AND a run is
 *  currently in flight (not yet terminal). */
const canCancel = computed(() => {
  if (mode.value !== 'controller-local') return false;
  const c = controllerRun.value;
  return c.kind === 'submitting' || c.kind === 'running';
});

const cancelInFlight = ref(false);

async function onCancelRun() {
  const c = controllerRun.value;
  if (c.kind !== 'running' && c.kind !== 'submitting') return;
  // submitting state may or may not have a runId yet — read it
  // from the running variant; submitting has none.
  const runId = c.kind === 'running' ? c.runId : null;
  if (!runId) return;
  cancelInFlight.value = true;
  try {
    await controller.getClient().cancelRun(runId, { timeoutMs: 5_000 });
    // The SSE stream will deliver the Cancelled lifecycle event
    // and pollRun will resolve with status=Cancelled.
  } catch (e) {
    if (e instanceof ControllerClientErr) {
      controllerRun.value = {
        kind: 'controller_error',
        phase: 'poll',
        error: e.payload,
        workflowId: c.kind === 'running' ? c.workflowId : undefined,
        runId,
      };
    } else {
      controllerRun.value = {
        kind: 'controller_error',
        phase: 'poll',
        error: {
          kind: 'network',
          message: e instanceof Error ? e.message : String(e),
        },
      };
    }
  } finally {
    cancelInFlight.value = false;
  }
}

/** Display name for the workflow submission. C.2 doesn't yet have
 *  workflow names in the graph store, so we synthesize one based
 *  on the active function. */
function workflowDisplayName(): string {
  const fn = graph.workflow.functions.find((f) => f.id === graph.activeFunctionId)
    ?? graph.workflow.functions[0];
  return fn ? `editor:${fn.name}` : 'editor:workflow';
}

// c64: recent controller runs for the connected URL. Hidden when
// browser-sim mode or no controller URL set.
const recentRuns = computed(() => {
  if (mode.value !== 'controller-local' || !controller.url) return [];
  return runHistory.listFor(controller.url);
});

/** Re-fetch a historical run by id and pop it into the displayed
 *  state — same UX as if it just completed. */
async function reopenRun(workflowId: string, runId: string) {
  controllerRun.value = { kind: 'submitting' }; // borrow the spinner
  isRunning.value = true;
  try {
    const client = controller.getClient();
    const r = await client.getRun(runId, { timeoutMs: 5_000 });
    controllerRun.value = {
      kind: 'done',
      workflowId,
      runId,
      record: r,
      durationMs:
        r.completed_at !== undefined && r.started_at !== undefined
          ? r.completed_at - r.started_at
          : 0,
    };
    runHistory.update(controller.url, runId, {
      status: r.status,
      durationMs:
        r.completed_at !== undefined && r.started_at !== undefined
          ? r.completed_at - r.started_at
          : null,
    });
  } catch (e) {
    if (e instanceof ControllerClientErr) {
      controllerRun.value = {
        kind: 'controller_error',
        phase: 'poll',
        error: e.payload,
        workflowId,
        runId,
      };
    } else {
      controllerRun.value = {
        kind: 'controller_error',
        phase: 'poll',
        error: { kind: 'network', message: e instanceof Error ? e.message : String(e) },
      };
    }
  } finally {
    isRunning.value = false;
  }
}

/** Jump-to-source/canvas helper for a `RunEvent.Print` /
 *  `RunEvent.Failed` source_span (C.5 c85). Reuses the same
 *  spanning machinery the browser-sim Trace tab already uses. */
function jumpFromEvent(span: SourceSpan) {
  const source = graph.emitted.source;
  const { line } = lineColAt(source, span.start);
  const match = findNodeForSpan(graph.workflow, span);
  if (match) {
    jumpToNode(match.fn.name, match.node.id);
  } else {
    focusSourceLine(line);
  }
}

function relativeTimestamp(ms: number): string {
  const now = Date.now();
  const diff = Math.max(0, now - ms);
  if (diff < 60_000) return `${Math.round(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.round(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.round(diff / 3_600_000)}h ago`;
  return `${Math.round(diff / 86_400_000)}d ago`;
}

// ---- Derived display state ----
//
// Both modes converge on the same shapes so the rest of the
// template doesn't need to branch. The browser-sim path reads
// from `runEnvelope`; the controller-local path reads from
// `controllerRun` and synthesizes a `RunResult`-shaped object
// the existing UI rendering can consume unchanged.

/**
 * True iff a result exists to render. The template uses this in
 * the same spot it previously checked `runEnvelope`.
 */
const hasResult = computed(() => {
  if (mode.value === 'browser-sim') return runEnvelope.value !== null;
  const c = controllerRun.value;
  return c.kind !== 'idle' && c.kind !== 'compiling' && c.kind !== 'submitting';
});

const compileFailed = computed(() => {
  if (mode.value === 'browser-sim') {
    return runEnvelope.value !== null && !runEnvelope.value.ok;
  }
  return controllerRun.value.kind === 'compile_failed';
});

const compileDiagnostics = computed<SolDiagnostic[]>(() => {
  if (mode.value === 'browser-sim') {
    // Only the compile-stage diagnostics belong in the "compile failed"
    // list. Runtime-phase diagnostics are surfaced separately below.
    return (runEnvelope.value?.diagnostics ?? []).filter((d) => d.phase !== 'Runtime');
  }
  const c = controllerRun.value;
  return c.kind === 'compile_failed' ? c.diagnostics : [];
});

// Browser-sim runtime diagnostics. The VM reports some failures (e.g.
// "function 'x' not found", a step-limit hit) as Warning/Runtime
// diagnostics rather than a structured `runtime_error`. These are real
// failures and must be shown, not swallowed as "ran with no output".
const browserRuntimeDiags = computed<SolDiagnostic[]>(() =>
  mode.value === 'browser-sim'
    ? (runEnvelope.value?.diagnostics ?? []).filter((d) => d.phase === 'Runtime')
    : [],
);

/**
 * Unified `RunResult`-shaped object. For browser-sim it's the
 * envelope's `run`. For controller-local we adapt the controller's
 * `RunRecord` shape — events + structured runtime errors land in
 * C.5; until then, controller-mode runs render output + return
 * value, and Failed maps to a synthetic generic-runtime-error.
 */
type UnifiedRunResult = NonNullable<RunEnvelope['run']>;
const runResult = computed<UnifiedRunResult | null>(() => {
  if (mode.value === 'browser-sim') return runEnvelope.value?.run ?? null;
  const c = controllerRun.value;
  if (c.kind !== 'done') return null;
  const r = c.record;
  const output = r.output;
  if (!output) return null;
  const isFailed = r.status === 'Failed';
  // The controller now records a real execution trace (same VM as
  // browser-sim), carried on the run output. Map it straight onto the
  // unified shape so the Trace tab renders controller runs identically.
  const trace = (output.trace ?? []) as UnifiedRunResult['trace'];
  // Point at the failing statement: the last error step's span.
  const errorSpan =
    [...trace].reverse().find((s) => s.kind === 'error')?.span ?? null;
  return {
    return_value: output.return_value,
    output: output.output,
    steps: output.steps,
    // C.5: controller will populate structured runtime errors via
    // the event stream. For C.2 we only know the run failed; the
    // `[controller] ...` line in `output.output` carries the reason.
    runtime_error: isFailed
      ? { kind: 'ExtCallBlocked', function_name: '(controller)', url: '(see output)' }
      : null,
    runtime_error_source_span: errorSpan,
    trace,
    trace_truncated: output.trace_truncated ?? false,
  };
});

const runErrorMsg = computed(() => {
  // For controller-local mode, render a tailored "Failed (see
  // output above)" message instead of the synthesized
  // ExtCallBlocked variant, which is misleading.
  if (mode.value === 'controller-local') {
    const c = controllerRun.value;
    if (c.kind === 'done' && c.record.status === 'Failed') {
      // Surface the controller's actual reason rather than a generic
      // placeholder: prefer structured diagnostics, then any captured
      // output lines, then a fallback pointer to the Output tab.
      const diags = (c.record.diagnostics ?? [])
        .map((d) => d.message)
        .filter((m): m is string => !!m);
      const outLines = (c.record.output?.output ?? []).filter((l) => l.trim().length > 0);
      if (diags.length) return diags.join(' · ');
      if (outLines.length) return outLines.join('\n');
      return 'Run failed on the controller. See the Output tab for the controller-side reason (often a capability the controller has no provider for).';
    }
    return null;
  }
  const err = runResult.value?.runtime_error;
  if (err) return formatRuntimeError(err);
  // Runtime-phase diagnostics without a structured runtime_error. The
  // most common is a call to a user-defined helper: the runtime executes
  // the workflow body and built-ins only, not helper-function bodies, so
  // add an actionable hint for that case.
  const diags = browserRuntimeDiags.value;
  if (diags.length === 0) return null;
  const msg = diags.map((d) => d.message).join('\n');
  if (/function '.*' not found/.test(msg)) {
    return `${msg}\n\nThe runtime runs the workflow body plus the built-ins (print, len, to_str, type_name) and imported capabilities. It does not execute user-defined helper functions, so calling one fails. Inline that logic into the workflow body, or expose it as a capability through an import.`;
  }
  return msg;
});

const completedOk = computed(() => {
  if (mode.value === 'browser-sim') {
    return (
      runEnvelope.value !== null
      && runEnvelope.value.ok
      && runResult.value !== null
      && runResult.value.runtime_error === null
      && browserRuntimeDiags.value.length === 0
    );
  }
  const c = controllerRun.value;
  return c.kind === 'done' && c.record.status === 'Succeeded';
});

// Classified, human-readable title for the error banner so the user
// knows WHAT KIND of failure this is: parse/compile, runtime, a blocked
// external Action, or an unsupported helper-function call.
const runErrorTitle = computed(() => {
  if (mode.value === 'controller-local') return 'Run failed on the controller';
  const err = runResult.value?.runtime_error;
  if (err) {
    switch (err.kind) {
      case 'ExtCallBlocked':
        return 'External Action blocked in Browser Simulation';
      case 'ExtCallFailed':
        return 'External Action failed';
      case 'StepLimit':
        return 'Step limit reached (possible infinite loop)';
      case 'DivByZero':
        return 'Runtime error: division by zero';
      case 'IndexOutOfBounds':
        return 'Runtime error: array index out of bounds';
      default:
        return `Runtime error: ${err.kind}`;
    }
  }
  if (browserRuntimeDiags.value.some((d) => /function '.*' not found/.test(d.message))) {
    return 'Runtime error: unsupported function call';
  }
  return 'Runtime error';
});

// Controller-specific display state.

const controllerPhaseLabel = computed(() => {
  const c = controllerRun.value;
  switch (c.kind) {
    case 'idle':
      return null;
    case 'compiling':
      return 'Compiling locally…';
    case 'compile_failed':
      return 'Compile failed';
    case 'submitting':
      return 'Submitting workflow to controller…';
    case 'running':
      return 'Running on controller…';
    case 'done':
      return c.record.status === 'Succeeded' ? 'Completed' : 'Failed';
    case 'controller_error':
      return 'Controller error';
  }
  return null;
});

const controllerMeta = computed(() => {
  const c = controllerRun.value;
  if (c.kind === 'running' || c.kind === 'done') {
    return {
      workflowId: c.workflowId,
      runId: c.runId,
      status: c.kind === 'done' ? c.record.status : 'Running',
      durationMs: c.kind === 'done' ? c.durationMs : null,
    };
  }
  if (c.kind === 'controller_error') {
    return {
      workflowId: c.workflowId ?? null,
      runId: c.runId ?? null,
      status: 'Errored',
      durationMs: null,
    };
  }
  return null;
});

/** Editor↔controller transport-layer error message (NOT runtime). */
const controllerErrorMessage = computed(() => {
  const c = controllerRun.value;
  if (c.kind !== 'controller_error') return null;
  return formatControllerError(c.phase, c.error);
});

function formatControllerError(phase: string, e: ControllerClientError): string {
  const phaseLabel = phase === 'submit'
    ? 'submitting workflow'
    : phase === 'create'
      ? 'creating run'
      : 'polling run status';
  const where = controller.runTarget === 'cloud' ? 'Cloud Controller' : 'Local Controller';
  switch (e.kind) {
    case 'network':
      return controller.runTarget === 'cloud'
        ? `${where} not reachable at ${controller.url}. Check the URL and that the controller is online, or switch to Browser Simulation.`
        : `Controller not reachable. Start the local controller (${controller.url}) or switch to Browser Simulation.`;
    case 'timeout':
      return `${where} timed out after ${e.timeoutMs}ms while ${phaseLabel} at ${controller.url}. The controller may be overloaded or stuck; retry, or switch to Browser Simulation.`;
    case 'http':
      // Phase C C.6 c95 — friendly saturation message when
      // the controller reports its queue is full (HTTP 503 +
      // code=queue_full from the controller's error envelope).
      if (e.status === 503 && e.code === 'queue_full') {
        return `Controller queue is full. Wait for in-flight runs to finish, then re-run. (${e.message})`;
      }
      return `Controller returned HTTP ${e.status}${e.code ? ` (${e.code})` : ''} while ${phaseLabel}. ${e.message}`;
    case 'decode':
      return `Controller response couldn't be parsed while ${phaseLabel}. ${e.message}`;
    case 'version':
      return `Host-spec major mismatch (controller=${e.controllerMajor}, editor=${e.editorMajor}). Reconnect from the controller settings modal after upgrading.`;
    // Phase C C.7 c99 — bearer-token rejection mid-run. Most
    // common cause: token rotated server-side, or the user just
    // started a controller with auth on but hasn't set the
    // token in the modal.
    case 'auth':
      return `Controller rejected your auth token while ${phaseLabel}. Open Controller Settings and set the correct token. (${e.code})`;
    // Phase C C.7 c99 — URL became invalid between settings save
    // and run dispatch (unusual; only happens if the user
    // mutates the URL mid-run).
    case 'invalid_url':
      return `Controller URL is invalid (${e.reason}). Fix it in Controller Settings.`;
    case 'aborted':
      return 'Controller run cancelled.';
  }
}

function formatRuntimeError(e: RuntimeError): string {
  switch (e.kind) {
    case 'DivByZero':
      return 'Division by zero.';
    case 'IndexOutOfBounds':
      return `Array index out of bounds: index ${e.index}, length ${e.length}.`;
    case 'StackUnderflow':
      return 'Stack underflow — this is a compiler bug; please report.';
    case 'StepLimit':
      return `Execution step limit reached (${e.limit.toLocaleString()} instructions). The program may have an infinite loop.`;
    case 'ExtCallBlocked':
      return `External function call to "${e.function_name}" at ${e.url} is blocked. External calls are not available in browser simulation — switch to controller-local mode (or deploy) to run them for real.`;
    case 'ExtCallFailed':
      return `External call "${e.function_name}" via connector "${e.connector}" failed: ${e.message}`;
    case 'HeapShapeMismatch':
      return `Heap shape mismatch: expected ${e.expected}, got ${e.got}. Likely a compiler bug; please report.`;
    case 'Cancelled':
      return 'Run cancelled.';
    case 'ResourceLimit':
      return `Resource limit exceeded: ${e.resource} > ${e.limit.toLocaleString()}. The controller halted execution to protect itself; raise the limit if the workload is legitimate.`;
  }
}

function replay() {
  if (trace.value) sim.play(trace.value, { workflow: graph.workflow });
}

const copyState = ref<'idle' | 'copied'>('idle');

async function copyOutput() {
  const out = runResult.value?.output;
  if (!out) return;
  const text = out.join('\n');
  try {
    await navigator.clipboard.writeText(text);
    copyState.value = 'copied';
    setTimeout(() => (copyState.value = 'idle'), 1200);
  } catch {
    /* clipboard refused */
  }
}

async function copySource() {
  try {
    await navigator.clipboard.writeText(graph.emitted.source);
    copyState.value = 'copied';
    setTimeout(() => (copyState.value = 'idle'), 1200);
  } catch {
    /* clipboard refused */
  }
}

// Auto-run each time the modal opens; cancel canvas playback on close.
watch(
  () => props.open,
  (now, prev) => {
    if (now && !prev) {
      trace.value = null;
      // Keep whatever run target the user chose. If a controller is
      // unreachable, the run surfaces a clear error rather than
      // silently falling back to the browser.
      execute();
    } else if (!now) {
      sim.cancel();
      // Abort any in-flight controller poll so the request doesn't
      // keep running after the modal closes.
      if (controllerAbort) {
        controllerAbort.abort();
        controllerAbort = null;
      }
      closeLiveStream();
    }
  },
);

// Re-run when the user switches run target while the modal is open so
// the Output reflects the newly selected target immediately.
watch(
  () => controller.runTarget,
  () => {
    if (props.open) execute();
  },
);

const sourceLines = computed(() => graph.emitted.source.split('\n'));

// ----- B.D c44: execution trace UX helpers -----

interface TraceRow {
  index: number;
  /** Step kind: statement, helper call/return, external call/result, error. */
  kind: 'stmt' | 'call' | 'return' | 'extcall' | 'extresult' | 'error';
  /** Workflow or helper function executing at this step. */
  fn: string;
  /** Call depth (0 = workflow body) — drives row indentation. */
  depth: number;
  /** Callee name for `call`; error message for `error`. */
  detail: string | null;
  span: SourceSpan | null;
  line: number | null;
  col: number | null;
  snippet: string;
  /** If the span maps to a graph node, the node's id (canvas
   *  focus target). Null means source-only navigation. */
  nodeId: string | null;
  /** Function name when nodeId is set (display only). */
  fnName: string | null;
}

/** 1-indexed (line, col) for a byte offset into `source`. */
function lineColAt(source: string, offset: number): { line: number; col: number } {
  let line = 1;
  let col = 1;
  for (let i = 0; i < offset && i < source.length; i++) {
    if (source.charCodeAt(i) === 10) { line++; col = 1; }
    else col++;
  }
  return { line, col };
}

/** Source slice for a span, clamped, with newline → ⏎ for inline display. */
function snippetFor(source: string, span: SourceSpan, max = 60): string {
  const clamped = source.slice(span.start, Math.min(span.end, source.length));
  const compact = clamped.replace(/\s+/g, ' ').trim();
  if (compact.length === 0) return '(empty)';
  return compact.length > max ? compact.slice(0, max - 1) + '…' : compact;
}

const traceRows = computed<TraceRow[]>(() => {
  const tr = runResult.value?.trace ?? [];
  if (tr.length === 0) return [];
  const source = graph.emitted.source;
  return tr.map((step, index) => {
    const span = step.span;
    // Prefer the VM-reported line; fall back to computing from the span.
    const computed = span ? lineColAt(source, span.start) : null;
    const line = step.line ?? computed?.line ?? null;
    const col = computed?.col ?? null;
    const match = span ? findNodeForSpan(graph.workflow, span) : null;
    // Snippet: source slice for spanned steps; for a return/error with no
    // span, fall back to the detail (callee name / error message).
    const snippet = span
      ? snippetFor(source, span)
      : step.detail ?? `(${step.kind})`;
    return {
      index,
      kind: step.kind,
      fn: step.function,
      depth: step.depth,
      detail: step.detail,
      span,
      line,
      col,
      snippet,
      nodeId: match?.node.id ?? null,
      fnName: match?.fn.name ?? null,
    };
  });
});

const runtimeErrorLocation = computed(() => {
  const r = runResult.value;
  if (!r || !r.runtime_error_source_span) return null;
  const source = graph.emitted.source;
  const { line, col } = lineColAt(source, r.runtime_error_source_span.start);
  const match = findNodeForSpan(graph.workflow, r.runtime_error_source_span);
  return {
    line,
    col,
    snippet: snippetFor(source, r.runtime_error_source_span),
    nodeId: match?.node.id ?? null,
    fnName: match?.fn.name ?? null,
  };
});

/**
 * Jump to a graph node on the canvas (uses the existing
 * `ui.requestFocus` mechanism that Canvas + DiagnosticsDrawer
 * also use) AND switch to its containing function. Closes the
 * Run modal so the canvas is visible.
 */
function jumpToNode(fnName: string | null, nodeId: string | null) {
  if (!nodeId) return;
  if (fnName) {
    const fn = graph.workflow.functions.find((f) => f.name === fnName);
    if (fn) graph.setActiveFunction(fn.id);
  }
  ui.requestFocus(nodeId);
  // Defer the close one frame so the focus request commits before
  // the modal unmounts.
  void nextTick(() => close());
}

/** Scroll the SOL preview tab to a specific line. Only meaningful
 *  when the user is on the 'sol' tab; we switch them there. */
function focusSourceLine(line: number) {
  activeTab.value = 'sol';
  void nextTick(() => {
    const el = document.querySelector(`[data-sol-line="${line}"]`);
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  });
}

function close() {
  emit('close');
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) close();
}

// Prod c51 — Escape-key close. Only active while open so closed
// modals don't intercept the editor's other shortcuts.
function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') close();
}

// ===== Draggable floating panel =====================================
// The Run panel is a movable floating window. Position is fixed (left/top)
// and clamped so the panel can never be dragged fully off-screen; the last
// position is remembered for the session. Drag starts only from the header
// (and never from a button/tab/input inside it), so all controls stay
// clickable and internal scroll areas are untouched.
const PANEL_W = 920;
const POS_KEY = 'solflow.runpanel.pos';
const EDGE = 8; // keep this gap between the panel and every viewport edge

const modalEl = ref<HTMLElement | null>(null);
const pos = ref<{ x: number; y: number } | null>(null);
const dragging = ref(false);
let dragOffset = { x: 0, y: 0 };

function panelWidth(): number {
  return Math.min(PANEL_W, window.innerWidth - 2 * EDGE);
}

/** Live panel size from the rendered element (falls back to the caps).
 *  Used to keep the WHOLE panel inside the viewport, not just its header. */
function panelSize(): { w: number; h: number } {
  const el = modalEl.value;
  if (el) return { w: el.offsetWidth, h: el.offsetHeight };
  return { w: panelWidth(), h: Math.min(window.innerHeight - 2 * EDGE, 560) };
}

/** Centered default that always fits the viewport on every edge. */
function defaultPos(): { x: number; y: number } {
  const { w, h } = panelSize();
  const x = Math.round((window.innerWidth - w) / 2);
  const y = Math.round(Math.min(window.innerHeight * 0.07, window.innerHeight - h - EDGE));
  return clampPos(x, y);
}

/** Clamp so the ENTIRE panel stays inside the viewport: left>=EDGE,
 *  top>=EDGE, right<=innerWidth-EDGE, bottom<=innerHeight-EDGE. The panel's
 *  own max-width/max-height keep it smaller than the viewport, so a valid
 *  position always exists. */
function clampPos(x: number, y: number): { x: number; y: number } {
  const { w, h } = panelSize();
  const maxX = Math.max(EDGE, window.innerWidth - w - EDGE);
  const maxY = Math.max(EDGE, window.innerHeight - h - EDGE);
  return {
    x: Math.min(Math.max(x, EDGE), maxX),
    y: Math.min(Math.max(y, EDGE), maxY),
  };
}

/** Re-clamp the current position against the live panel size (after the
 *  panel renders / its content height changes / the viewport resizes). */
function reclamp() {
  if (pos.value) pos.value = clampPos(pos.value.x, pos.value.y);
}

/** Set the initial position when the panel opens: a remembered position
 *  (always re-clamped inside the viewport) or the centered default. */
function initPos() {
  let next: { x: number; y: number } | null = null;
  try {
    const raw = sessionStorage.getItem(POS_KEY);
    if (raw) {
      const p = JSON.parse(raw);
      if (typeof p?.x === 'number' && typeof p?.y === 'number') next = { x: p.x, y: p.y };
    }
  } catch { /* ignore bad storage */ }
  pos.value = next ?? defaultPos();
  // Clamp once the element is measured so the whole panel is inside.
  void nextTick(() => reclamp());
}

/** Recenter the panel (double-click header, or after a viewport resize that
 *  would otherwise strand it). */
function recenter() {
  pos.value = defaultPos();
  persistPos();
}

function persistPos() {
  if (pos.value) {
    try { sessionStorage.setItem(POS_KEY, JSON.stringify(pos.value)); } catch { /* ignore */ }
  }
}

const modalStyle = computed(() =>
  pos.value
    ? { position: 'fixed' as const, left: `${pos.value.x}px`, top: `${pos.value.y}px`, margin: '0' }
    : {},
);

function onHeaderPointerDown(e: PointerEvent) {
  // Never start a drag from an interactive control inside the header.
  const el = e.target as HTMLElement;
  if (el.closest('button, a, input, select, textarea, .target-toggle')) return;
  if (e.button !== 0) return;
  dragging.value = true;
  const cur = pos.value ?? defaultPos();
  dragOffset = { x: e.clientX - cur.x, y: e.clientY - cur.y };
  window.addEventListener('pointermove', onHeaderPointerMove);
  window.addEventListener('pointerup', onHeaderPointerUp);
  // Prevent stray text selection across the page while dragging without
  // calling preventDefault() (which would suppress the header's click /
  // double-click-to-recenter events).
  document.body.style.userSelect = 'none';
}
function onHeaderPointerMove(e: PointerEvent) {
  if (!dragging.value) return;
  pos.value = clampPos(e.clientX - dragOffset.x, e.clientY - dragOffset.y);
}
function onHeaderPointerUp() {
  if (!dragging.value) return;
  dragging.value = false;
  window.removeEventListener('pointermove', onHeaderPointerMove);
  window.removeEventListener('pointerup', onHeaderPointerUp);
  document.body.style.userSelect = '';
  persistPos();
}

// Re-clamp on viewport resize so the whole panel stays inside the viewport.
function onResize() {
  reclamp();
}

watch(
  () => props.open,
  (isOpen) => { if (isOpen) initPos(); },
  { immediate: true },
);

// Keep the whole panel contained when its content height changes (e.g.
// switching to a tall Trace tab) by re-clamping whenever it resizes.
let resizeObs: ResizeObserver | null = null;
watch(modalEl, (el) => {
  resizeObs?.disconnect();
  if (el && typeof ResizeObserver !== 'undefined') {
    resizeObs = new ResizeObserver(() => reclamp());
    resizeObs.observe(el);
  }
});

onMounted(() => {
  document.addEventListener('keydown', onKey);
  window.addEventListener('resize', onResize);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
  window.removeEventListener('resize', onResize);
  window.removeEventListener('pointermove', onHeaderPointerMove);
  window.removeEventListener('pointerup', onHeaderPointerUp);
  resizeObs?.disconnect();
});
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="backdrop" @click="onBackdrop">
      <div ref="modalEl" class="modal" :class="{ dragging }" :style="modalStyle">
        <header
          class="modal-header drag-handle"
          @pointerdown="onHeaderPointerDown"
          @dblclick="recenter"
          title="Drag to move · double-click to recenter"
        >
          <div class="header-left">
            <span class="title">Run workflow</span>
            <span class="subtle">
              <template v-if="controller.runTarget === 'browser-sim'">Browser Simulation · WASM</template>
              <template v-else-if="controller.runTarget === 'local'">Local Controller · {{ controller.url }}</template>
              <template v-else>Cloud Controller · {{ controller.url }}</template>
            </span>
          </div>
          <div class="header-right">
            <!-- Run-target selector: Browser Simulation / Local / Cloud -->
            <div class="target-toggle" role="tablist" aria-label="Run target">
              <button
                v-for="t in runTargets"
                :key="t.id"
                class="target-btn"
                :class="{ active: controller.runTarget === t.id, unconfigured: !t.configured }"
                role="tab"
                :aria-selected="controller.runTarget === t.id"
                :disabled="!t.configured"
                :title="t.title"
                @click="selectRunTarget(t.id)"
              >
                <span
                  v-if="t.id !== 'browser-sim'"
                  class="target-dot"
                  :class="t.connecting ? 'pending' : t.connected ? 'ok' : 'off'"
                  :title="t.connecting ? 'Checking…' : t.connected ? 'Connected' : 'Disconnected'"
                />
                {{ t.label }}
              </button>
            </div>
            <button
              class="ghost"
              @click="replay"
              :disabled="isRunning || !trace || sim.isPlaying || mode === 'controller-local'"
              title="Replay simulation animation on canvas (browser-sim only)"
            >
              ▷ Replay
            </button>
            <button
              v-if="canCancel"
              class="ghost danger"
              @click="onCancelRun"
              :disabled="cancelInFlight"
              title="Send DELETE /runs/:id to the controller"
            >
              {{ cancelInFlight ? 'Cancelling…' : '✕ Cancel run' }}
            </button>
            <button class="ghost" @click="execute" :disabled="isRunning">
              {{ isRunning ? 'Running…' : 'Re-run' }}
            </button>
            <button class="ghost icon-only" @click="recenter" title="Recenter panel">
              <svg viewBox="0 0 14 14" width="13" height="13" fill="none">
                <rect x="3" y="3" width="8" height="8" rx="1.5" stroke="currentColor" stroke-width="1.3" />
                <circle cx="7" cy="7" r="1.4" fill="currentColor" />
              </svg>
            </button>
            <button class="ghost" @click="close" title="Close (Esc)">
              <svg viewBox="0 0 12 12" width="12" height="12" fill="none">
                <path
                  d="M3 3 9 9 M9 3 3 9"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                />
              </svg>
            </button>
          </div>
        </header>

        <nav class="tabs">
          <button
            v-for="t in tabs"
            :key="t"
            class="tab"
            :class="{ active: activeTab === t }"
            @click="activeTab = t"
          >
            {{ tabLabel(t) }}
            <span
              v-if="t === 'trace' && traceRows.length > 0"
              class="tab-badge"
            >{{ traceRows.length }}</span>
          </button>
          <div class="tab-spacer" />
          <div class="status" v-if="controllerPhaseLabel">
            <span
              class="status-dot"
              :class="
                controllerRun.kind === 'controller_error' ? 'err'
                : controllerRun.kind === 'done' && controllerRun.record.status === 'Failed' ? 'err'
                : controllerRun.kind === 'done' ? 'ok'
                : 'pending'
              "
            />
            {{ controllerPhaseLabel }}
            <span class="subtle" v-if="controllerMeta?.durationMs != null">
              · {{ controllerMeta.durationMs }}ms
            </span>
            <span class="subtle" v-if="runResult">
              · {{ runResult.steps.toLocaleString() }} step{{ runResult.steps === 1 ? '' : 's' }}
            </span>
          </div>
          <div class="status" v-else-if="hasResult">
            <span
              class="status-dot"
              :class="completedOk ? 'ok' : 'err'"
            />
            <template v-if="compileFailed">compile failed</template>
            <template v-else-if="runResult?.runtime_error">runtime error</template>
            <template v-else-if="completedOk">completed</template>
            <template v-else>—</template>
            <span class="subtle" v-if="runResult">
              · {{ runResult.steps.toLocaleString() }} step{{ runResult.steps === 1 ? '' : 's' }}
            </span>
          </div>
        </nav>

        <main class="body">
          <!-- Output tab -->
          <section v-if="activeTab === 'output'" class="pane">
            <!-- Recent controller runs (c64) — collapsed unless any exist -->
            <details
              v-if="mode === 'controller-local' && recentRuns.length > 0"
              class="recent-runs"
            >
              <summary>
                Recent runs on
                <code class="ctrl-url">{{ controller.url }}</code>
                <span class="subtle">({{ recentRuns.length }})</span>
              </summary>
              <ul class="recent-list">
                <li
                  v-for="r in recentRuns"
                  :key="r.runId"
                  class="recent-row"
                  :class="{
                    'is-current': controllerRun.kind === 'done' && controllerRun.runId === r.runId,
                  }"
                >
                  <span
                    class="recent-dot"
                    :class="
                      r.status === 'Succeeded' ? 'ok'
                      : r.status === 'Failed' ? 'err'
                      : r.status === 'Cancelled' ? 'err'
                      : 'pending'
                    "
                  />
                  <span class="recent-name">{{ r.workflowName }}</span>
                  <code class="recent-id">{{ r.runId }}</code>
                  <span class="recent-status">{{ r.status }}</span>
                  <span class="recent-dur subtle" v-if="r.durationMs != null">
                    {{ r.durationMs }}ms
                  </span>
                  <span class="recent-ago subtle">{{ relativeTimestamp(r.submittedAt) }}</span>
                  <button
                    class="recent-reopen ghost"
                    :disabled="isRunning"
                    @click="reopenRun(r.workflowId, r.runId)"
                    title="Re-fetch this run from the controller"
                  >Reopen</button>
                </li>
              </ul>
            </details>
            <div v-if="isRunning && !controllerPhaseLabel" class="empty">Running…</div>
            <!-- Controller mode: show in-flight phase + meta -->
            <div v-if="controllerPhaseLabel && controllerRun.kind !== 'done'" class="ctrl-status">
              <div class="ctrl-status-row">
                <span class="status-dot" :class="
                  controllerRun.kind === 'controller_error' ? 'err' : 'pending'
                " />
                <strong>{{ controllerPhaseLabel }}</strong>
              </div>
              <div v-if="controllerMeta" class="ctrl-meta">
                <div v-if="controllerMeta.workflowId" class="kv">
                  <span class="k">workflow</span><code class="v">{{ controllerMeta.workflowId }}</code>
                </div>
                <div v-if="controllerMeta.runId" class="kv">
                  <span class="k">run</span><code class="v">{{ controllerMeta.runId }}</code>
                </div>
              </div>
            </div>

            <!-- Controller-side error (network / version / http / etc) -->
            <div v-if="controllerErrorMessage" class="error">
              <strong>Controller error</strong>
              <div class="error-msg">{{ controllerErrorMessage }}</div>
            </div>

            <template v-if="hasResult">
              <!-- Compile errors short-circuit execution -->
              <div v-if="compileFailed" class="error">
                <strong>Compile failed — execution skipped</strong>
                <ul class="diag-list">
                  <li
                    v-for="(d, i) in compileDiagnostics"
                    :key="i"
                    :class="d.severity.toLowerCase()"
                  >
                    <span class="diag-code">{{ d.code }}</span>
                    <span class="diag-phase">{{ d.phase }}</span>
                    <span class="diag-msg">{{ d.message }}</span>
                  </li>
                </ul>
              </div>

              <!-- Runtime error from canonical VM -->
              <div v-else-if="runErrorMsg" class="error">
                <strong>{{ runErrorTitle }}</strong>
                <div class="error-msg">{{ runErrorMsg }}</div>
                <!-- B.D c44: source span + optional node link -->
                <div v-if="runtimeErrorLocation" class="error-where">
                  <span class="subtle">at line {{ runtimeErrorLocation.line }}:{{ runtimeErrorLocation.col }}</span>
                  ·
                  <button
                    class="link"
                    @click="focusSourceLine(runtimeErrorLocation.line)"
                  >show source</button>
                  <template v-if="runtimeErrorLocation.nodeId">
                    ·
                    <button
                      class="link"
                      @click="jumpToNode(runtimeErrorLocation.fnName, runtimeErrorLocation.nodeId)"
                    >show on canvas ({{ runtimeErrorLocation.fnName }})</button>
                  </template>
                </div>
              </div>

              <!-- Empty success -->
              <div
                v-else-if="runResult && runResult.output.length === 0 && completedOk"
                class="empty"
              >
                Program ran with no print output.
              </div>

              <!-- Print output -->
              <div v-if="runResult && runResult.output.length > 0" class="output-block">
                <div class="output-toolbar">
                  <span class="output-label">
                    stdout · {{ runResult.output.length }} {{ runResult.output.length === 1 ? 'line' : 'lines' }}
                  </span>
                  <button class="ghost" @click="copyOutput">
                    {{ copyState === 'copied' ? '✓ Copied' : 'Copy' }}
                  </button>
                </div>
                <div class="output-rows">
                  <div
                    v-for="(line, i) in runResult.output"
                    :key="i"
                    class="output-row"
                  >
                    <span class="row-num">{{ String(i + 1).padStart(2, ' ') }}</span>
                    <span class="row-text">{{ line }}</span>
                  </div>
                </div>
              </div>

              <!-- Return value (suppressed on runtime error) -->
              <div
                v-if="runResult && runResult.return_value !== null && completedOk"
                class="return-row"
              >
                <span class="subtle">return:</span>
                <code>{{ formatReturn(runResult.return_value) }}</code>
              </div>

              <!-- Controller-mode meta footer for completed runs -->
              <div
                v-if="mode === 'controller-local' && controllerMeta && controllerRun.kind === 'done'"
                class="ctrl-footer"
              >
                <div class="kv">
                  <span class="k">workflow</span><code class="v">{{ controllerMeta.workflowId }}</code>
                </div>
                <div class="kv">
                  <span class="k">run</span><code class="v">{{ controllerMeta.runId }}</code>
                </div>
                <div class="kv">
                  <span class="k">status</span><code class="v">{{ controllerMeta.status }}</code>
                </div>
                <div class="kv" v-if="controllerMeta.durationMs != null">
                  <span class="k">duration</span><code class="v">{{ controllerMeta.durationMs }}ms</code>
                </div>
              </div>
            </template>
          </section>

          <!-- Live event pane (C.5 c85) — controller-local only -->
          <section v-if="activeTab === 'live'" class="pane">
            <template v-if="mode !== 'controller-local'">
              <div class="empty">
                Live events stream from the controller. Switch to
                <strong>Controller-local</strong> mode to see them.
              </div>
            </template>
            <template v-else-if="liveEvents.length === 0">
              <div class="empty">
                <template v-if="isRunning">Waiting for events…</template>
                <template v-else>No events yet. Re-run to see the live stream.</template>
              </div>
            </template>
            <template v-else>
              <div class="output-toolbar">
                <span class="output-label">
                  {{ liveEvents.length }} event{{ liveEvents.length === 1 ? '' : 's' }}
                </span>
                <span class="subtle" v-if="!isRunning">
                  stream ended
                </span>
              </div>
              <ul class="live-list">
                <li
                  v-for="(ev, i) in liveEvents"
                  :key="i"
                  class="live-row"
                  :class="`kind-${ev.kind.toLowerCase()}`"
                >
                  <span class="live-seq">#{{ ev.seq }}</span>
                  <span class="live-kind">{{ ev.kind }}</span>
                  <span class="live-detail">
                    <template v-if="ev.kind === 'Print'">
                      <span class="live-print">{{ ev.text }}</span>
                      <button
                        v-if="ev.source_span"
                        class="link"
                        title="Show source"
                        @click="jumpFromEvent(ev.source_span!)"
                      >show source</button>
                    </template>
                    <template v-else-if="ev.kind === 'ExtCallStarted'">
                      connector <code>{{ ev.connector }}</code>
                      · fn <code>{{ ev.fn_name }}</code>
                    </template>
                    <template v-else-if="ev.kind === 'ExtCallCompleted'">
                      connector <code>{{ ev.connector }}</code>
                      · fn <code>{{ ev.fn_name }}</code>
                      <span :class="ev.ok ? 'ok-tag' : 'err-tag'">
                        {{ ev.ok ? '✓' : '✗' }}
                      </span>
                    </template>
                    <template v-else-if="ev.kind === 'Completed'">
                      return <code>{{ ev.output.return_value }}</code>
                      · {{ ev.output.steps }} steps
                    </template>
                    <template v-else-if="ev.kind === 'Failed'">
                      <code>{{ ev.error.kind }}</code>
                    </template>
                    <template v-else>
                      <span class="subtle">{{ ev.kind }}</span>
                    </template>
                  </span>
                </li>
              </ul>
            </template>
          </section>

          <!-- Trace pane (B.D c44; controller streaming → C.5) -->
          <section v-if="activeTab === 'trace'" class="pane">
            <template v-if="isRunning">
              <div class="empty">Running…</div>
            </template>
            <template v-else-if="!hasResult || compileFailed">
              <div class="empty">Run a program to see its result.</div>
            </template>
            <template v-else-if="traceRows.length === 0">
              <div class="empty">
                No execution trace was recorded for this run.
              </div>
            </template>
            <template v-else>
              <div class="output-toolbar">
                <span class="output-label">
                  {{ traceRows.length }} step{{ traceRows.length === 1 ? '' : 's' }}
                  ({{ runResult?.steps }} VM instruction{{ runResult?.steps === 1 ? '' : 's' }})
                </span>
                <span v-if="runResult?.trace_truncated" class="trunc-tag">
                  truncated at cap
                </span>
              </div>
              <ul class="trace-list">
                <li
                  v-for="row in traceRows"
                  :key="row.index"
                  class="trace-row"
                  :class="{
                    'has-node': !!row.nodeId,
                    'is-call': row.kind === 'call',
                    'is-return': row.kind === 'return',
                    'is-extcall': row.kind === 'extcall',
                    'is-extresult': row.kind === 'extresult',
                    'is-error': row.kind === 'error',
                  }"
                  :style="{ paddingLeft: 8 + row.depth * 16 + 'px' }"
                >
                  <span class="trace-step">#{{ row.index + 1 }}</span>
                  <span class="trace-kind" :class="'k-' + row.kind">{{ row.kind }}</span>
                  <span class="trace-fn" :title="`in ${row.fn}`">{{ row.fn }}</span>
                  <button
                    v-if="row.line !== null"
                    class="trace-loc"
                    :title="`Show source line ${row.line}`"
                    @click="focusSourceLine(row.line!)"
                  >line {{ row.line }}<template v-if="row.col">:{{ row.col }}</template></button>
                  <code class="trace-snippet">{{ row.snippet }}</code>
                  <button
                    v-if="row.nodeId"
                    class="trace-node"
                    :title="`Focus this node on the canvas (${row.fnName})`"
                    @click="jumpToNode(row.fnName, row.nodeId)"
                  >→ canvas</button>
                </li>
              </ul>
            </template>
          </section>

          <!-- SOL preview -->
          <section v-if="activeTab === 'sol'" class="pane">
            <div class="output-toolbar">
              <span class="output-label">{{ sourceLines.length }} lines</span>
              <button class="ghost" @click="copySource">
                {{ copyState === 'copied' ? '✓ Copied' : 'Copy SOL' }}
              </button>
            </div>
            <pre class="sol-pre"><span
              v-for="(line, i) in sourceLines"
              :key="i"
              class="sol-line"
              :data-sol-line="i + 1"
            ><span class="ln">{{ String(i + 1).padStart(2, ' ') }}</span>{{ line }}<br></span></pre>
          </section>
        </main>

        <footer class="modal-footer">
          <span class="subtle">
            <template v-if="mode === 'browser-sim'">
              Output comes from the canonical SOL VM compiled to WASM, running
              in this browser. External Actions are blocked in Browser
              Simulation. The canvas animation is a visual aid for per node
              highlighting; trust the text output for exact behavior.
            </template>
            <template v-else>
              Output comes from the same canonical SOL VM running inside the
              {{ controller.runTarget === 'cloud' ? 'cloud' : 'local' }} controller
              at {{ controller.url }}. External Actions run through the
              controller's connectors.
            </template>
          </span>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<script lang="ts">
function formatReturn(v: unknown): string {
  if (v === null || v === undefined) return 'void';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.12s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.backdrop {
  position: fixed;
  inset: 0;
  /* No dimming overlay: the trace panel + inspector dock on the right and the
     user wants to watch them live while the dialog is open. A transparent,
     click-through backdrop keeps the rest of the UI bright and interactive;
     the dialog stands out on its own shadow + border. */
  background: transparent;
  pointer-events: none;
  z-index: var(--sf-z-modal);
  display: flex;
  align-items: center;
  /* Pin the dialog to the left so it never reaches the right-docked panels. */
  justify-content: flex-start;
  /* Safe margins on every side so the dialog never clips on the left or
     bottom. The dialog is only ~640px wide and left-pinned, so it does
     not reach the right-docked panels on wide screens; narrow screens
     center it (media query below). */
  padding: clamp(16px, 3vh, 28px) clamp(16px, 4vw, 48px);
}
.modal {
  /* Re-enable interaction on the dialog itself (backdrop is click-through). */
  pointer-events: auto;
}
@media (max-width: 900px) {
  .backdrop {
    justify-content: center;
    padding: 24px;
  }
  .modal { width: min(720px, calc(100% - 32px)); }
}
.modal {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  /* Always fit the viewport: cap to the available width and height with
     a small margin so the dialog is never clipped. Wide enough that the
     header controls (run-target toggle + actions) fit; the header also
     wraps + truncates as a safety net so it can never clip horizontally. */
  width: min(920px, calc(100vw - 32px));
  max-width: calc(100vw - 32px);
  max-height: min(82vh, calc(100vh - 32px));
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  /* Wrap the action cluster below the title rather than clip it when the
     header is too narrow for everything on one line. */
  flex-wrap: wrap;
  row-gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
}
/* The header is the drag handle. Buttons/toggles inside keep their own
   pointer cursor and clickability (drag bails on interactive targets). */
.modal-header.drag-handle {
  cursor: move;
  user-select: none;
  touch-action: none;
}
.modal-header.drag-handle button,
.modal-header.drag-handle .target-toggle {
  cursor: pointer;
}
.modal.dragging {
  user-select: none;
}
.header-left {
  display: flex;
  align-items: baseline;
  gap: 10px;
  /* Allow the title row to shrink so the long subtitle truncates with an
     ellipsis instead of pushing the action cluster off the right edge. */
  flex: 1 1 auto;
  min-width: 0;
}
.title {
  flex: 0 0 auto;
}
/* The run-target subtitle (e.g. "Local Controller · http://127.0.0.1:3939")
   can be long; truncate it rather than letting it widen the header. */
.header-left .subtle {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.header-right {
  display: flex;
  gap: 4px;
  /* Never clip the actions: wrap them when they cannot fit on one line. */
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  min-width: 0;
}
.title {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--sf-text-0);
}
.subtle {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
}

.tabs {
  display: flex;
  align-items: center;
  padding: 0 12px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
  height: 32px;
}
.tab {
  background: transparent;
  border: none;
  padding: 0 12px;
  height: 100%;
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--sf-text-2);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  border-radius: 0;
}
.tab:hover {
  color: var(--sf-text-0);
}
.tab.active {
  color: var(--sf-text-0);
  border-bottom-color: var(--sf-accent);
}
.tab-spacer {
  flex: 1;
}
.status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
}
.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}
.status-dot.ok {
  background: var(--sf-success);
}
.status-dot.err {
  background: var(--sf-error);
}
.status-dot.pending {
  background: var(--sf-warning);
  animation: sf-pulse 1.2s ease-in-out infinite;
}
@keyframes sf-pulse {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.45; }
}

/* Run-target selector in the header */
.target-toggle {
  display: inline-flex;
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  margin-right: 4px;
  overflow: hidden;
}
.target-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: none;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
  padding: 4px 10px;
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s ease, color 0.12s ease;
}
.target-btn + .target-btn { border-left: 1px solid var(--sf-border); }
.target-btn:hover:not(.unconfigured):not(.active) {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.target-btn.active {
  background: var(--sf-accent, #5d8acf);
  color: white;
}
.target-btn.unconfigured,
.target-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.target-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.target-dot.ok { background: var(--sf-success); }
.target-dot.off { background: var(--sf-text-3); }
.target-dot.pending {
  background: var(--sf-warning);
  animation: sf-pulse 1.2s ease-in-out infinite;
}
.target-btn.active .target-dot.off { background: rgba(255, 255, 255, 0.6); }

/* Controller-mode status block (mid-run + meta footer) */
.ctrl-status {
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  padding: 10px 12px;
  margin-bottom: 12px;
}
.ctrl-status-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.75rem;
  color: var(--sf-text-0);
}
.ctrl-meta {
  margin-top: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ctrl-footer {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px dashed var(--sf-border);
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.kv {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 0.6875rem;
}
.kv .k {
  flex: 0 0 80px;
  font-size: 0.625rem;
  color: var(--sf-text-2);
  text-transform: uppercase;
  letter-spacing: 0.3px;
}
.kv .v {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
  background: var(--sf-bg-0);
  padding: 0 6px;
  border-radius: 2px;
}

/* Recent controller runs (c64) */
.recent-runs {
  margin-bottom: 12px;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
}
.recent-runs summary {
  list-style: none;
  padding: 8px 12px;
  cursor: pointer;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  display: flex;
  align-items: center;
  gap: 6px;
}
.recent-runs summary::-webkit-details-marker { display: none; }
.recent-runs summary::before {
  content: '▸';
  color: var(--sf-text-3);
  transition: transform 0.12s ease;
}
.recent-runs[open] summary::before { transform: rotate(90deg); }
.ctrl-url {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-0);
  padding: 0 6px;
  border-radius: 2px;
  color: var(--sf-text-0);
}
.recent-list {
  list-style: none;
  margin: 0;
  padding: 4px 12px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 220px;
  overflow-y: auto;
}
.recent-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.6875rem;
  padding: 4px 0;
  border-bottom: 1px solid rgba(255,255,255,0.04);
}
.recent-row:last-child { border-bottom: none; }
.recent-row.is-current { background: rgba(0,204,136,0.06); border-radius: 3px; padding: 4px 6px; }
.recent-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.recent-dot.ok { background: var(--sf-success); }
.recent-dot.err { background: var(--sf-error); }
.recent-dot.pending { background: var(--sf-warning); }
.recent-name { color: var(--sf-text-1); }
.recent-id {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  font-size: 0.625rem;
}
.recent-status {
  color: var(--sf-text-0);
  text-transform: uppercase;
  letter-spacing: 0.3px;
  font-size: 0.5625rem;
}
.recent-dur, .recent-ago { font-style: italic; }
.recent-reopen {
  margin-left: auto;
  font-size: 0.625rem;
  padding: 2px 8px;
}

.body {
  flex: 1;
  min-height: 0;
  /* The body itself does not scroll; each tab pane is its own scroll
     container so long content (e.g. a many-step Trace) scrolls within
     the dialog instead of growing it past the viewport. */
  overflow: hidden;
  background: var(--sf-bg-1);
}
.pane {
  padding: 16px;
  height: 100%;
  overflow-y: auto;
  box-sizing: border-box;
}
.empty {
  color: var(--sf-text-3);
  font-size: 0.75rem;
  font-style: italic;
}
.output-block {
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
}
.output-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  background: var(--sf-bg-1);
  border-bottom: 1px solid var(--sf-border);
}
.output-label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--sf-text-2);
}
.output-rows {
  display: flex;
  flex-direction: column;
  padding: 6px 0;
}
.output-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 2px 12px;
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  color: var(--sf-text-0);
}
.output-row:hover {
  background: rgba(255, 255, 255, 0.02);
}
.row-num {
  color: var(--sf-text-3);
  font-size: 0.625rem;
  flex-shrink: 0;
  user-select: none;
  padding-top: 1px;
}
.row-text {
  white-space: pre-wrap;
  word-break: break-word;
}
.error {
  background: rgba(255, 77, 79, 0.08);
  border: 1px solid rgba(255, 77, 79, 0.3);
  border-radius: var(--sf-radius-sm);
  padding: 10px 12px;
  color: var(--sf-error);
  font-size: 0.75rem;
  margin-bottom: 12px;
}
.error strong {
  display: block;
  margin-bottom: 4px;
}
.error-msg {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
  white-space: pre-wrap;
  word-break: break-word;
}
.diag-list {
  list-style: none;
  padding: 0;
  margin: 6px 0 0 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.diag-list li {
  display: grid;
  grid-template-columns: 60px 70px 1fr;
  gap: 8px;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  color: var(--sf-text-0);
  padding: 2px 0;
}
.diag-code { font-weight: 600; }
.diag-list li.error .diag-code { color: var(--sf-error, #d96666); }
.diag-list li.warning .diag-code { color: var(--sf-warning); }
.diag-phase {
  color: var(--sf-text-3);
  text-transform: lowercase;
  font-size: 0.625rem;
}
.diag-msg { white-space: pre-wrap; word-break: break-word; }

/* B.D c44 — execution trace UI */
.tab-badge {
  display: inline-block;
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(98, 154, 220, 0.18);
  color: var(--sf-text-0);
  font-size: 0.5625rem;
  font-family: var(--sf-font-mono);
}
.error-where {
  margin-top: 8px;
  font-size: 0.6875rem;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
}
.link {
  background: transparent;
  border: none;
  color: var(--sf-accent, #5d8acf);
  font-size: inherit;
  font-family: inherit;
  cursor: pointer;
  padding: 0;
  text-decoration: underline;
}
.link:hover { color: var(--sf-text-0); }
.trunc-tag {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  padding: 2px 8px;
  border-radius: 8px;
  background: rgba(232, 166, 87, 0.18);
  color: var(--sf-warning);
}
.trace-list { list-style: none; padding: 0; margin: 0; }
.trace-row {
  display: flex;
  gap: 10px;
  align-items: baseline;
  padding: 4px 12px;
  font-size: 0.6875rem;
  font-family: var(--sf-font-mono);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
}
.trace-row.has-node { background: rgba(98, 154, 220, 0.04); }
.trace-row.is-error {
  background: rgba(232, 110, 110, 0.10);
}
.trace-row.is-call { background: rgba(120, 190, 140, 0.05); }
.trace-row.is-return { background: rgba(160, 160, 200, 0.04); }
.trace-row.is-extcall { background: rgba(220, 170, 90, 0.07); }
.trace-row.is-extresult { background: rgba(220, 170, 90, 0.04); }
.trace-step {
  color: var(--sf-text-3);
  text-align: right;
  flex: 0 0 auto;
  min-width: 40px;
}
.trace-kind {
  flex: 0 0 auto;
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 1px 6px;
  border-radius: 8px;
  min-width: 62px;
  text-align: center;
  white-space: nowrap;
  background: rgba(255, 255, 255, 0.06);
  color: var(--sf-text-2);
}
.trace-kind.k-call { background: rgba(120, 190, 140, 0.20); color: #8fd6a6; }
.trace-kind.k-return { background: rgba(160, 160, 200, 0.18); color: #b3b3d8; }
.trace-kind.k-extcall { background: rgba(220, 170, 90, 0.24); color: #e8bf7a; }
.trace-kind.k-extresult { background: rgba(220, 170, 90, 0.16); color: #d8b683; }
.trace-kind.k-error { background: rgba(232, 110, 110, 0.22); color: #f0a0a0; }
.trace-fn {
  flex: 0 0 auto;
  color: var(--sf-text-3);
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.trace-loc {
  background: transparent;
  border: none;
  color: var(--sf-accent, #5d8acf);
  font-family: inherit;
  font-size: inherit;
  cursor: pointer;
  padding: 0;
  text-align: left;
}
.trace-loc:hover { text-decoration: underline; }
.trace-loc { flex: 0 0 auto; }
.trace-snippet {
  color: var(--sf-text-0);
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.trace-node {
  flex: 0 0 auto;
  background: rgba(98, 154, 220, 0.14);
  border: 1px solid rgba(98, 154, 220, 0.3);
  color: var(--sf-text-0);
  padding: 1px 8px;
  border-radius: 3px;
  font-size: 0.625rem;
  font-family: inherit;
  cursor: pointer;
}
.trace-node:hover {
  background: rgba(98, 154, 220, 0.28);
}
.trace-no-node {
  color: var(--sf-text-3);
  font-size: 0.625rem;
  font-style: italic;
}
.return-row {
  margin-top: 12px;
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-size: 0.75rem;
}
.return-row code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 1px 6px;
  border-radius: 3px;
  color: var(--sf-accent);
  border: 1px solid var(--sf-border);
}

.sol-pre {
  margin: 0;
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  color: var(--sf-text-0);
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  padding: 10px 12px;
  white-space: pre;
  overflow-x: auto;
}
.sol-line {
  display: block;
}
.ln {
  display: inline-block;
  width: 28px;
  color: var(--sf-text-3);
  text-align: right;
  padding-right: 12px;
  user-select: none;
}

.modal-footer {
  padding: 8px 16px;
  border-top: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
}

/* Phase C C.6 c93 — Cancel button affordance */
.ghost.danger {
  color: var(--sf-danger, #e75a5a);
  border: 1px solid rgba(231, 90, 90, 0.4);
}
.ghost.danger:hover {
  background: rgba(231, 90, 90, 0.08);
}

/* Live event stream (C.5 c85) */
.live-list {
  list-style: none;
  padding: 0;
  margin: 8px 0 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.live-row {
  display: grid;
  grid-template-columns: 44px 110px 1fr;
  gap: 8px;
  padding: 4px 6px;
  font-size: 0.6875rem;
  border-bottom: 1px solid rgba(255,255,255,0.03);
  align-items: baseline;
}
.live-seq {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  text-align: right;
}
.live-kind {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-1);
  letter-spacing: 0.3px;
}
.live-detail { color: var(--sf-text-0); }
.live-print {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
  margin-right: 6px;
}
.live-row.kind-print .live-kind { color: var(--sf-accent, #5d8acf); }
.live-row.kind-completed .live-kind { color: var(--sf-success); }
.live-row.kind-failed .live-kind { color: var(--sf-error); }
.live-row.kind-extcallstarted .live-kind,
.live-row.kind-extcallcompleted .live-kind { color: var(--sf-warning); }
.ok-tag { color: var(--sf-success); margin-left: 6px; }
.err-tag { color: var(--sf-error); margin-left: 6px; }
.live-detail code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
}
</style>
