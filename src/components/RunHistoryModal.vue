<script setup lang="ts">
/**
 * Run History modal — Phase C C.5 c86.
 *
 * Lists past runs on the connected controller filtered by
 * workflow + status + limit; clicking a row opens an inline
 * event-replay panel that streams every persisted RunEvent for
 * that run via the SSE endpoint (terminal-event closes the
 * stream).
 *
 * Defaults the workflow-id field to the most-recently-submitted
 * workflow on this controller (read from the run-history store)
 * so the common "show me what I just did" path is one click.
 *
 * Read-only — this modal doesn't submit, cancel, or reopen runs
 * for re-execution. The Run modal's Recent Runs section already
 * handles re-execution; this view exists to inspect history
 * across past runs (success criterion: past runs queryable by
 * status + time + workflow).
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useControllerStore } from '@/stores/controller.store';
import { useControllerRunHistoryStore } from '@/stores/controller-run-history.store';
import {
  ControllerClientErr,
  type ControllerClientError,
} from '@/runtime-host/client';
import {
  openRunEventStream,
  type RunEventStreamHandle,
} from '@/runtime-host/event-stream';
import type {
  RunEvent,
  RunRecord,
  RunStatus,
} from '@/runtime-host/types';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const controller = useControllerStore();
const runHistory = useControllerRunHistoryStore();

onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
  closeStream();
});
function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}
function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

// ----- filter state -----

const STATUSES = ['All', 'Queued', 'Running', 'Succeeded', 'Failed', 'Cancelled'] as const;
type StatusFilter = (typeof STATUSES)[number];

const workflowId = ref<string>('');
const statusFilter = ref<StatusFilter>('All');
const limit = ref<number>(50);

const defaultWorkflowId = computed(() => {
  if (!controller.url) return '';
  return runHistory.listFor(controller.url)[0]?.workflowId ?? '';
});

watch(
  () => [props.open, defaultWorkflowId.value],
  ([nowOpen, def]) => {
    if (nowOpen && !workflowId.value && typeof def === 'string') {
      workflowId.value = def;
    }
  },
  { immediate: true },
);

// ----- runs list -----

const runs = ref<RunRecord[]>([]);
const listLoading = ref(false);
const listError = ref<ControllerClientError | null>(null);

async function refreshList() {
  if (!controller.isConnected || !workflowId.value.trim()) {
    runs.value = [];
    return;
  }
  listError.value = null;
  listLoading.value = true;
  try {
    const client = controller.getClient();
    const status = statusFilter.value === 'All' ? undefined : (statusFilter.value as RunStatus);
    runs.value = await client.listRuns(workflowId.value.trim(), {
      status,
      limit: limit.value,
    });
  } catch (e) {
    runs.value = [];
    if (e instanceof ControllerClientErr) listError.value = e.payload;
    else listError.value = { kind: 'network', message: String(e) };
  } finally {
    listLoading.value = false;
  }
}

watch(
  () => [props.open, workflowId.value, statusFilter.value, limit.value, controller.isConnected],
  () => {
    if (props.open) void refreshList();
  },
  { immediate: true },
);

// ----- inline event-replay panel -----

const selectedRunId = ref<string | null>(null);
const replayEvents = ref<RunEvent[]>([]);
const replayLoading = ref(false);
let replayHandle: RunEventStreamHandle | null = null;

function closeStream() {
  if (replayHandle && !replayHandle.isDone) replayHandle.close();
  replayHandle = null;
}

function viewEvents(record: RunRecord) {
  closeStream();
  selectedRunId.value = record.id;
  replayEvents.value = [];
  replayLoading.value = true;
  replayHandle = openRunEventStream({
    baseUrl: controller.url,
    runId: record.id,
    onEvent: (ev) => replayEvents.value.push(ev),
    onDone: () => {
      replayLoading.value = false;
    },
    onError: () => {
      replayLoading.value = false;
    },
  });
}

function closeReplay() {
  closeStream();
  selectedRunId.value = null;
  replayEvents.value = [];
  replayLoading.value = false;
}

watch(
  () => props.open,
  (nowOpen) => {
    if (!nowOpen) {
      closeReplay();
    }
  },
);

// ----- display helpers -----

function formatTime(ms: number | undefined | null): string {
  if (ms == null) return '—';
  const d = new Date(ms);
  const diff = Date.now() - ms;
  if (diff < 60_000) return `${Math.round(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.round(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.round(diff / 3_600_000)}h ago`;
  return d.toISOString().replace('T', ' ').slice(0, 19);
}

function runDuration(r: RunRecord): string {
  if (r.started_at == null || r.completed_at == null) return '—';
  return `${r.completed_at - r.started_at}ms`;
}

function statusDot(status: RunStatus): string {
  switch (status) {
    case 'Succeeded': return 'ok';
    case 'Failed':
    case 'Cancelled':
    case 'TimedOut':
    case 'Rejected':
      return 'err';
    case 'Running':
    case 'Starting':
    case 'Cancelling':
      return 'pending';
    case 'Queued':
      return 'idle';
  }
}

function triggerLabel(t: RunRecord['trigger']): string {
  switch (t.kind) {
    case 'Manual': return 'Manual';
    case 'Timer': return `Timer · ${t.cron}`;
    case 'Event': return `Event · ${t.source}`;
  }
}

const listErrorMsg = computed(() =>
  listError.value ? listError.value.message : null,
);
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Run history</span>
          </div>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <div v-if="!controller.isConnected" class="warn">
            <strong>Controller not connected.</strong>
            Open Controller Settings, connect to a running
            controller, then return here.
          </div>

          <template v-else>
            <section class="settings-section">
              <div class="filter-row">
                <label class="field">
                  <span class="field-label">Workflow ID</span>
                  <input
                    class="field-input mono"
                    v-model="workflowId"
                    placeholder="wf_…"
                  />
                </label>
                <label class="field">
                  <span class="field-label">Status</span>
                  <select class="field-input" v-model="statusFilter">
                    <option v-for="s in STATUSES" :key="s" :value="s">{{ s }}</option>
                  </select>
                </label>
                <label class="field">
                  <span class="field-label">Limit</span>
                  <input
                    class="field-input narrow"
                    type="number"
                    min="1"
                    max="1000"
                    v-model.number="limit"
                  />
                </label>
                <button
                  class="ghost refresh"
                  :disabled="listLoading"
                  @click="refreshList"
                >{{ listLoading ? '…' : 'Refresh' }}</button>
              </div>
              <div v-if="listErrorMsg" class="inline-error">{{ listErrorMsg }}</div>
            </section>

            <section class="settings-section">
              <div v-if="runs.length === 0 && !listLoading && !listErrorMsg" class="empty">
                No runs match this filter.
              </div>
              <ul v-if="runs.length > 0" class="run-list">
                <li
                  v-for="r in runs"
                  :key="r.id"
                  class="run-row"
                  :class="{ 'is-selected': selectedRunId === r.id }"
                >
                  <span class="status-dot" :class="statusDot(r.status)" />
                  <code class="run-id">{{ r.id }}</code>
                  <span class="run-status">{{ r.status }}</span>
                  <span class="run-trigger subtle">{{ triggerLabel(r.trigger) }}</span>
                  <span class="run-dur subtle">{{ runDuration(r) }}</span>
                  <span class="run-when subtle">{{ formatTime(r.created_at) }}</span>
                  <button class="ghost small" @click="viewEvents(r)">View events</button>
                </li>
              </ul>
            </section>

            <section v-if="selectedRunId" class="settings-section replay">
              <div class="section-head">
                <h3>Events · <code>{{ selectedRunId }}</code></h3>
                <button class="ghost small" @click="closeReplay">Close</button>
              </div>
              <div v-if="replayLoading && replayEvents.length === 0" class="subtle">
                Streaming events…
              </div>
              <ul v-if="replayEvents.length > 0" class="replay-list">
                <li
                  v-for="(ev, i) in replayEvents"
                  :key="i"
                  class="replay-row"
                  :class="`kind-${ev.kind.toLowerCase()}`"
                >
                  <span class="r-seq">#{{ ev.seq }}</span>
                  <span class="r-kind">{{ ev.kind }}</span>
                  <span class="r-detail">
                    <template v-if="ev.kind === 'Print'">
                      <code class="r-print">{{ ev.text }}</code>
                    </template>
                    <template v-else-if="ev.kind === 'ExtCallStarted'">
                      {{ ev.connector }} · {{ ev.fn_name }}
                    </template>
                    <template v-else-if="ev.kind === 'ExtCallCompleted'">
                      {{ ev.connector }} · {{ ev.fn_name }} ·
                      <span :class="ev.ok ? 'ok-tag' : 'err-tag'">
                        {{ ev.ok ? '✓' : '✗' }}
                      </span>
                    </template>
                    <template v-else-if="ev.kind === 'Completed'">
                      return <code>{{ ev.output.return_value }}</code>
                    </template>
                    <template v-else-if="ev.kind === 'Failed'">
                      <code>{{ ev.error.kind }}</code>
                    </template>
                  </span>
                </li>
              </ul>
            </section>
          </template>
        </div>

        <footer class="modal-footer">
          <span class="subtle">
            Past runs persist in the controller's SQLite + their
            event log replays on demand via SSE.
          </span>
          <button class="ghost" @click="emit('close')">Done</button>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active, .fade-leave-active { transition: opacity 0.12s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

.overlay {
  position: fixed; inset: 0;
  background: rgba(0,0,0,0.55);
  display: flex; align-items: center; justify-content: center;
  z-index: var(--sf-z-modal, 1000);
}
.modal {
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 6px;
  width: min(820px, 94vw);
  max-height: 88vh;
  display: flex; flex-direction: column;
  color: var(--sf-text-0);
}
.modal-header {
  display: flex; align-items: center; gap: 12px;
  padding: 12px 18px;
  border-bottom: 1px solid var(--sf-border);
}
.header-left { display: flex; align-items: baseline; gap: 10px; }
.title { font-size: 0.875rem; font-weight: 600; }
.phase-tag {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  background: rgba(0,204,136,0.16);
  color: var(--sf-success);
  padding: 2px 8px; border-radius: 3px;
}
.close {
  margin-left: auto;
  background: transparent; border: none;
  color: var(--sf-text-3);
  font-size: 0.875rem; cursor: pointer;
  padding: 4px 8px; border-radius: 3px;
}
.close:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
.body {
  padding: 14px 18px;
  overflow-y: auto;
  display: flex; flex-direction: column; gap: 12px;
}
.warn {
  background: rgba(232,166,87,0.08);
  border: 1px solid rgba(232,166,87,0.22);
  border-radius: 4px;
  padding: 10px 12px;
  font-size: 0.6875rem;
  line-height: 1.5;
  color: var(--sf-text-1);
}
.warn strong { color: var(--sf-warning); }
.settings-section h3 {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
  margin: 0 0 6px 0;
}
.section-head {
  display: flex; align-items: center; justify-content: space-between;
}
.field { display: flex; flex-direction: column; gap: 4px; }
.field-label {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
}
.field-input {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  color: var(--sf-text-0);
  font-size: 0.75rem;
  padding: 4px 8px;
  border-radius: 4px;
}
.field-input.mono { font-family: var(--sf-font-mono); }
.field-input.narrow { width: 70px; }
.field-input:focus { outline: 1px solid var(--sf-accent, #5d8acf); }
.filter-row {
  display: flex; align-items: flex-end; gap: 12px; flex-wrap: wrap;
}
.refresh { align-self: flex-end; }

.inline-error {
  font-size: 0.6875rem;
  color: var(--sf-danger, #e75a5a);
  background: rgba(231,90,90,0.06);
  border: 1px solid rgba(231,90,90,0.2);
  border-radius: 4px;
  padding: 6px 10px;
  margin-top: 8px;
}

.empty {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
  font-style: italic;
  padding: 8px 0;
}
.subtle { color: var(--sf-text-3); font-size: 0.625rem; }

.run-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
.run-row {
  display: grid;
  grid-template-columns: 12px minmax(200px, auto) 80px minmax(140px, 1fr) 70px 90px auto;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  font-size: 0.6875rem;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
}
.run-row.is-selected {
  border-color: rgba(0,204,136,0.32);
  background: rgba(0,204,136,0.04);
}
.run-id {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.625rem;
}
.run-status {
  color: var(--sf-text-0);
  text-transform: uppercase;
  letter-spacing: 0.3px;
  font-size: 0.5625rem;
}
.run-trigger { font-family: var(--sf-font-mono); }
.status-dot { width: 8px; height: 8px; border-radius: 50%; }
.status-dot.idle { background: var(--sf-text-3); }
.status-dot.pending { background: var(--sf-warning); }
.status-dot.ok { background: var(--sf-success); }
.status-dot.err { background: var(--sf-error); }

.replay {
  margin-top: 4px;
  border-top: 1px solid var(--sf-border);
  padding-top: 12px;
}
.section-head code { font-family: var(--sf-font-mono); color: var(--sf-text-0); font-size: 0.6875rem; }
.replay-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 2px; }
.replay-row {
  display: grid;
  grid-template-columns: 40px 120px 1fr;
  gap: 8px;
  padding: 3px 6px;
  font-size: 0.6875rem;
  align-items: baseline;
}
.r-seq { font-family: var(--sf-font-mono); color: var(--sf-text-3); text-align: right; }
.r-kind { font-family: var(--sf-font-mono); color: var(--sf-text-1); }
.r-print { font-family: var(--sf-font-mono); background: var(--sf-bg-2); padding: 0 4px; border-radius: 2px; }
.kind-print .r-kind { color: var(--sf-accent, #5d8acf); }
.kind-completed .r-kind { color: var(--sf-success); }
.kind-failed .r-kind { color: var(--sf-error); }
.kind-extcallstarted .r-kind,
.kind-extcallcompleted .r-kind { color: var(--sf-warning); }
.ok-tag { color: var(--sf-success); }
.err-tag { color: var(--sf-error); }

.modal-footer {
  border-top: 1px solid var(--sf-border);
  padding: 10px 18px;
  display: flex; align-items: center; justify-content: space-between;
  gap: 12px;
}
.ghost {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 5px 14px;
  border-radius: 4px;
  font-size: 0.6875rem;
  cursor: pointer;
}
.ghost.small {
  padding: 3px 10px;
  font-size: 0.625rem;
}
.ghost:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
</style>
