<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useSimulationStore } from '@/stores/simulation.store';
import { useUIStore } from '@/stores/ui.store';
import { useControllerStore } from '@/stores/controller.store';
import { useControllerRunHistoryStore } from '@/stores/controller-run-history.store';
import { recordTrace, type Trace } from '@/runtime/simulate';
import { runSource } from '@/compiler/api';
import { compileForController } from '@/runtime-host/encode';
import {
  ControllerClientErr,
  type ControllerClientError,
} from '@/runtime-host/client';
import type {
  RunEnvelope,
  RuntimeError,
  SolDiagnostic,
  SourceSpan,
} from '@/compiler/types';
import type { RunRecord } from '@/runtime-host/types';
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

type ExecutionMode = 'browser-sim' | 'controller-local';

const STORAGE_KEY_MODE = 'solflow.run.mode';
const mode = ref<ExecutionMode>(loadStoredMode());

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

/** Abort signal for the active controller run (poll loop). */
let controllerAbort: AbortController | null = null;

function loadStoredMode(): ExecutionMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY_MODE);
    return v === 'controller-local' ? 'controller-local' : 'browser-sim';
  } catch {
    return 'browser-sim';
  }
}

function setMode(next: ExecutionMode) {
  if (next === mode.value) return;
  mode.value = next;
  try {
    localStorage.setItem(STORAGE_KEY_MODE, next);
  } catch {
    /* ignore */
  }
}

/** Disabled when the controller isn't connected. Tooltip explains why. */
const controllerModeDisabled = computed(() => !controller.isConnected);

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

const tabs = ['output', 'trace', 'sol'] as const;
type Tab = (typeof tabs)[number];
const activeTab = ref<Tab>('output');

function tabLabel(t: Tab): string {
  if (t === 'output') return 'Output';
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
  const abortSignal = controllerAbort.signal;

  // 1. Compile for wire (WASM, same canonical pipeline as browser-sim).
  controllerRun.value = { kind: 'compiling' };
  const env = await compileForController(graph.emitted.source);
  if (!env.ok || !env.value) {
    controllerRun.value = {
      kind: 'compile_failed',
      diagnostics: env.diagnostics,
    };
    return;
  }

  // 2. Submit workflow + create run + poll until terminal.
  controllerRun.value = { kind: 'submitting' };
  const client = controller.getClient();
  const submitStart = Date.now();
  let workflowId: string | undefined;
  let runId: string | undefined;
  try {
    const submitRes = await client.submitWorkflow(
      {
        name: workflowDisplayName(),
        bytecode: env.value.bytecode,
        instruction_spans: env.value.instruction_spans,
        source: graph.emitted.source,
      },
      { signal: abortSignal, timeoutMs: 10_000 },
    );
    workflowId = submitRes.workflow_id;

    const created = await client.createRun(
      {
        workflow_id: workflowId,
        trigger: { kind: 'Manual' },
      },
      { signal: abortSignal, timeoutMs: 5_000 },
    );
    runId = created.run_id;
    controllerRun.value = {
      kind: 'running',
      workflowId,
      runId,
      record: {
        id: runId,
        workflow_id: workflowId,
        status: created.status,
        trigger: { kind: 'Manual' },
        inputs: {},
        diagnostics: [],
        created_at: submitStart,
      },
      startedAt: submitStart,
    };

    // c64: record into history as soon as we have a runId, so the
    // user sees it even if poll fails partway through.
    runHistory.record({
      controllerUrl: controller.url,
      workflowId,
      runId,
      workflowName: workflowDisplayName(),
      status: created.status,
      durationMs: null,
      submittedAt: submitStart,
    });

    // 3. Poll until terminal.
    const final = await client.pollRun(runId, {
      intervalMs: 150,
      overallTimeoutMs: 60_000,
      signal: abortSignal,
    });
    const durationMs = Date.now() - submitStart;
    controllerRun.value = {
      kind: 'done',
      workflowId,
      runId,
      record: final,
      durationMs,
    };
    // c64: update history with final status + duration.
    runHistory.update(controller.url, runId, {
      status: final.status,
      durationMs,
    });
  } catch (e) {
    if (e instanceof ControllerClientErr) {
      const phase: 'submit' | 'create' | 'poll' = workflowId
        ? (runId ? 'poll' : 'create')
        : 'submit';
      controllerRun.value = {
        kind: 'controller_error',
        phase,
        error: e.payload,
        workflowId,
        runId,
      };
    } else {
      controllerRun.value = {
        kind: 'controller_error',
        phase: 'submit',
        error: { kind: 'network', message: e instanceof Error ? e.message : String(e) },
      };
    }
  } finally {
    controllerAbort = null;
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
  if (mode.value === 'browser-sim') return runEnvelope.value?.diagnostics ?? [];
  const c = controllerRun.value;
  return c.kind === 'compile_failed' ? c.diagnostics : [];
});

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
    runtime_error_source_span: null,
    // Trace streaming lands in C.5. Empty here is honest, not a
    // missing feature — the trace tab surfaces this explicitly.
    trace: [],
    trace_truncated: false,
  };
});

const runErrorMsg = computed(() => {
  // For controller-local mode, render a tailored "Failed (see
  // output above)" message instead of the synthesized
  // ExtCallBlocked variant, which is misleading.
  if (mode.value === 'controller-local') {
    const c = controllerRun.value;
    if (c.kind === 'done' && c.record.status === 'Failed') {
      return 'Run failed on the controller. See the output below for the controller-side reason. Structured runtime-error details (div-by-zero / step-limit / etc.) stream from the controller in Phase C C.5.';
    }
    return null;
  }
  const err = runResult.value?.runtime_error;
  if (!err) return null;
  return formatRuntimeError(err);
});

const completedOk = computed(() => {
  if (mode.value === 'browser-sim') {
    return (
      runEnvelope.value !== null
      && runEnvelope.value.ok
      && runResult.value !== null
      && runResult.value.runtime_error === null
    );
  }
  const c = controllerRun.value;
  return c.kind === 'done' && c.record.status === 'Succeeded';
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
  switch (e.kind) {
    case 'network':
      return `Controller unreachable while ${phaseLabel}. Is solflow-controller still running at ${controller.url}? ${e.message}`;
    case 'timeout':
      return `Controller timed out while ${phaseLabel} (${e.timeoutMs}ms). ${e.message}`;
    case 'http':
      return `Controller returned HTTP ${e.status}${e.code ? ` (${e.code})` : ''} while ${phaseLabel}. ${e.message}`;
    case 'decode':
      return `Controller response couldn't be parsed while ${phaseLabel}. ${e.message}`;
    case 'version':
      return `Host-spec major mismatch (controller=${e.controllerMajor}, editor=${e.editorMajor}). Reconnect from the controller settings modal after upgrading.`;
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
      // If the user previously chose controller-local but the
      // controller is no longer connected, silently fall back to
      // browser-sim rather than executing in a broken mode.
      if (mode.value === 'controller-local' && !controller.isConnected) {
        setMode('browser-sim');
      }
      execute();
    } else if (!now) {
      sim.cancel();
      // Abort any in-flight controller poll so the request doesn't
      // keep running after the modal closes.
      if (controllerAbort) {
        controllerAbort.abort();
        controllerAbort = null;
      }
    }
  },
);

// If the controller disconnects while the modal is open and the
// user is on controller-local mode, drop back to browser-sim.
watch(
  () => controller.isConnected,
  (connected) => {
    if (!connected && mode.value === 'controller-local' && props.open) {
      setMode('browser-sim');
    }
  },
);

const sourceLines = computed(() => graph.emitted.source.split('\n'));

// ----- B.D c44: execution trace UX helpers -----

interface TraceRow {
  index: number;
  span: SourceSpan;
  line: number;
  col: number;
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
  return tr.map((span, index) => {
    const { line, col } = lineColAt(source, span.start);
    const match = findNodeForSpan(graph.workflow, span);
    return {
      index,
      span,
      line,
      col,
      snippet: snippetFor(source, span),
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
onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => document.removeEventListener('keydown', onKey));
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="backdrop" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Run workflow</span>
            <span class="subtle">
              <template v-if="mode === 'browser-sim'">canonical SOL VM · WASM</template>
              <template v-else>canonical SOL VM · controller {{ controller.url }}</template>
            </span>
          </div>
          <div class="header-right">
            <!-- Phase C C.2 c63: execution-mode selector -->
            <div class="mode-toggle" role="tablist" aria-label="Execution mode">
              <button
                class="mode-btn"
                :class="{ active: mode === 'browser-sim' }"
                role="tab"
                :aria-selected="mode === 'browser-sim'"
                @click="setMode('browser-sim')"
                title="Run in this browser via WASM. External calls blocked."
              >Browser sim</button>
              <button
                class="mode-btn"
                :class="{ active: mode === 'controller-local', disabled: controllerModeDisabled }"
                role="tab"
                :aria-selected="mode === 'controller-local'"
                :disabled="controllerModeDisabled"
                @click="setMode('controller-local')"
                :title="controllerModeDisabled
                  ? 'Connect a controller from Controller Settings to enable.'
                  : 'Run via the connected SolFlow controller.'"
              >Controller-local</button>
            </div>
            <button
              class="ghost"
              @click="replay"
              :disabled="isRunning || !trace || sim.isPlaying || mode === 'controller-local'"
              title="Replay simulation animation on canvas (browser-sim only)"
            >
              ▷ Replay
            </button>
            <button class="ghost" @click="execute" :disabled="isRunning">
              {{ isRunning ? 'Running…' : 'Re-run' }}
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
                <strong>Runtime error · {{ runResult?.runtime_error?.kind }}</strong>
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

          <!-- Trace pane (B.D c44; controller streaming → C.5) -->
          <section v-if="activeTab === 'trace'" class="pane">
            <template v-if="isRunning">
              <div class="empty">Running…</div>
            </template>
            <template v-else-if="mode === 'controller-local'">
              <div class="empty">
                Execution trace doesn't stream from the controller
                yet — that lands in Phase C C.5 (event log + live
                stream). For trace, switch to Browser sim.
              </div>
            </template>
            <template v-else-if="!hasResult || compileFailed">
              <div class="empty">No execution trace — run a clean program to see one.</div>
            </template>
            <template v-else-if="traceRows.length === 0">
              <div class="empty">Execution produced no source-mapped steps.</div>
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
                  :class="{ 'has-node': !!row.nodeId }"
                >
                  <span class="trace-step">#{{ row.index + 1 }}</span>
                  <button
                    class="trace-loc"
                    :title="`Show source line ${row.line}`"
                    @click="focusSourceLine(row.line)"
                  >line {{ row.line }}:{{ row.col }}</button>
                  <code class="trace-snippet">{{ row.snippet }}</code>
                  <button
                    v-if="row.nodeId"
                    class="trace-node"
                    :title="`Focus this node on the canvas (${row.fnName})`"
                    @click="jumpToNode(row.fnName, row.nodeId)"
                  >→ canvas</button>
                  <span v-else class="trace-no-node">(no graph mapping)</span>
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
              Output above comes from the canonical SOL VM compiled to WASM.
              Canvas playback animation uses an approximate JS interpreter for
              per-node highlighting only — trust the text output for semantics.
              External calls are blocked in browser simulation.
            </template>
            <template v-else>
              Output above comes from the same canonical SOL VM running inside
              the connected controller. Run history persists across controller
              restarts. External calls land in C.4 (HTTP connector); they
              still produce an honest "blocked" diagnostic for now.
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
  background: rgba(0, 0, 0, 0.78);
  z-index: var(--sf-z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 32px;
}
.modal {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  width: min(720px, calc(100% - 32px));
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
}
.header-left {
  display: flex;
  align-items: baseline;
  gap: 10px;
}
.header-right {
  display: flex;
  gap: 4px;
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

/* Phase C C.2 c63: execution-mode toggle in the header */
.mode-toggle {
  display: inline-flex;
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  margin-right: 4px;
  overflow: hidden;
}
.mode-btn {
  background: transparent;
  border: none;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
  padding: 4px 10px;
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease;
}
.mode-btn + .mode-btn { border-left: 1px solid var(--sf-border); }
.mode-btn:hover:not(.disabled):not(.active) {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.mode-btn.active {
  background: var(--sf-accent, #5d8acf);
  color: white;
}
.mode-btn.disabled,
.mode-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

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
  overflow: auto;
  background: var(--sf-bg-1);
}
.pane {
  padding: 16px;
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
  display: grid;
  grid-template-columns: 48px 88px 1fr auto;
  gap: 10px;
  align-items: baseline;
  padding: 4px 12px;
  font-size: 0.6875rem;
  font-family: var(--sf-font-mono);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
}
.trace-row.has-node { background: rgba(98, 154, 220, 0.04); }
.trace-step {
  color: var(--sf-text-3);
  text-align: right;
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
.trace-snippet {
  color: var(--sf-text-0);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.trace-node {
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
</style>
