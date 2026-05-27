<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useSimulationStore } from '@/stores/simulation.store';
import { useUIStore } from '@/stores/ui.store';
import { recordTrace, type Trace } from '@/runtime/simulate';
import { runSource } from '@/compiler/api';
import type { RunEnvelope, RuntimeError, SourceSpan } from '@/compiler/types';
import { findNodeForSpan } from '@/graph/nodeLookup';

const graph = useGraphStore();
const sim = useSimulationStore();
const ui = useUIStore();

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

// --- B.10: canonical SOL VM execution (primary) ------------
//
// `runEnvelope` holds the result of `compile(emitted) + run`. The
// `output`, `return_value`, and `runtime_error` shown to the user
// come from here — these are CANONICAL SOL semantics, not the
// JS approximation.
const runEnvelope = ref<RunEnvelope | null>(null);

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
  // Defer to next tick so the UI shows "Running…" before WASM kicks in.
  await new Promise((r) => setTimeout(r, 0));
  try {
    // Canonical run (the authoritative output).
    runEnvelope.value = await runSource(graph.emitted.source);
    // Legacy JS trace for canvas animation only.
    trace.value = recordTrace(graph.workflow);
    if (trace.value) sim.play(trace.value, { workflow: graph.workflow });
  } finally {
    isRunning.value = false;
  }
}

// ---- Derived display state ----
const compileFailed = computed(
  () => runEnvelope.value !== null && !runEnvelope.value.ok,
);
const compileDiagnostics = computed(
  () => runEnvelope.value?.diagnostics ?? [],
);
const runResult = computed(() => runEnvelope.value?.run ?? null);
const runErrorMsg = computed(() => {
  const err = runResult.value?.runtime_error;
  if (!err) return null;
  return formatRuntimeError(err);
});
const completedOk = computed(
  () =>
    runEnvelope.value !== null
    && runEnvelope.value.ok
    && runResult.value !== null
    && runResult.value.runtime_error === null,
);

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
      return `External function call to "${e.function_name}" at ${e.url} is blocked. External calls are not available in browser simulation — deploy to run them for real.`;
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
      execute();
    } else if (!now) {
      sim.cancel();
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
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="backdrop" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Run workflow</span>
            <span class="subtle">canonical SOL VM · WASM</span>
          </div>
          <div class="header-right">
            <button
              class="ghost"
              @click="replay"
              :disabled="isRunning || !trace || sim.isPlaying"
              title="Replay simulation animation on canvas"
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
          <div class="status" v-if="runEnvelope">
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
            <div v-if="isRunning" class="empty">Running…</div>
            <template v-else-if="runEnvelope">
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
            </template>
          </section>

          <!-- Trace pane (B.D c44) -->
          <section v-if="activeTab === 'trace'" class="pane">
            <template v-if="isRunning">
              <div class="empty">Running…</div>
            </template>
            <template v-else-if="!runEnvelope || compileFailed">
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
            Output above comes from the canonical SOL VM compiled to WASM.
            Canvas playback animation uses an approximate JS interpreter for
            per-node highlighting only — trust the text output for semantics.
            External calls are blocked in browser simulation.
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
