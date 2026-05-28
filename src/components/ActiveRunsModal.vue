<script setup lang="ts">
/**
 * Active Runs modal — Phase C C.6 c93.
 *
 * Live view of every in-flight run on the connected controller
 * plus the controller-wide concurrency snapshot. Lets the user:
 *
 *   - See what's currently running + when each was dispatched
 *   - Cancel any active run (DELETE /runs/:id)
 *   - Spot saturation: queued depth + max caps + "running 8/8"
 *
 * Polls `/runs/active` + `/controller/concurrency` every 2 s
 * while the modal is open. Polling is the simplest fit for
 * orchestration introspection — the data is low-frequency
 * (typically &lt;1 update/sec) and SSE for per-controller-wide
 * state would mean a second endpoint family. Editor's existing
 * SSE infra covers per-run streaming.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useControllerStore } from '@/stores/controller.store';
import { useToastStore } from '@/stores/toast.store';
import {
  ControllerClientErr,
  type ControllerClientError,
} from '@/runtime-host/client';
import type {
  ActiveRunSummary,
  ConcurrencyMetrics,
} from '@/runtime-host/types';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const controller = useControllerStore();
const toast = useToastStore();

onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
  stopPolling();
});
function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}
function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

const active = ref<ActiveRunSummary[]>([]);
const metrics = ref<ConcurrencyMetrics | null>(null);
const pollError = ref<ControllerClientError | null>(null);
const refreshing = ref(false);

let pollTimer: ReturnType<typeof setInterval> | null = null;

function startPolling() {
  stopPolling();
  if (!controller.isConnected) return;
  void refresh();
  pollTimer = setInterval(() => {
    void refresh();
  }, 2_000);
}

function stopPolling() {
  if (pollTimer != null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function refresh() {
  if (!controller.isConnected) return;
  refreshing.value = true;
  pollError.value = null;
  try {
    const c = controller.getClient();
    const [a, m] = await Promise.all([
      c.listActiveRuns({ timeoutMs: 4_000 }),
      c.getConcurrencyMetrics({ timeoutMs: 4_000 }),
    ]);
    active.value = a;
    metrics.value = m;
  } catch (e) {
    if (e instanceof ControllerClientErr) {
      pollError.value = e.payload;
    } else {
      pollError.value = {
        kind: 'network',
        message: e instanceof Error ? e.message : String(e),
      };
    }
  } finally {
    refreshing.value = false;
  }
}

watch(
  () => [props.open, controller.isConnected],
  ([nowOpen]) => {
    if (nowOpen) startPolling();
    else stopPolling();
  },
  { immediate: true },
);

const cancellingId = ref<string | null>(null);

async function cancel(run_id: string) {
  cancellingId.value = run_id;
  try {
    await controller.getClient().cancelRun(run_id, { timeoutMs: 5_000 });
    toast.info(`Cancelling ${run_id}…`);
    // Refresh so the row disappears once the run lands terminal.
    await refresh();
  } catch (e) {
    const msg = e instanceof ControllerClientErr ? e.payload.message : String(e);
    toast.add('error', 'Cancel failed', { body: msg });
  } finally {
    cancellingId.value = null;
  }
}

function relTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 1000) return 'just now';
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return new Date(ms).toLocaleTimeString();
}

const saturationLevel = computed(() => {
  if (!metrics.value) return 'idle';
  const m = metrics.value;
  const active = m.active_runs;
  const queued = m.queued_runs;
  const cap = m.max_concurrent_runs;
  if (active >= cap || queued > 0) return 'busy';
  if (active >= Math.max(1, cap - 1)) return 'near';
  return 'idle';
});

const pollErrorMsg = computed(() => pollError.value?.message ?? null);
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Active runs</span>
            <span class="phase-tag">Phase C.6</span>
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
            <!-- Concurrency banner -->
            <section v-if="metrics" class="concurrency" :class="saturationLevel">
              <div class="metric">
                <span class="metric-label">Running</span>
                <span class="metric-value">
                  {{ metrics.active_runs }}/{{ metrics.max_concurrent_runs }}
                </span>
              </div>
              <div class="metric">
                <span class="metric-label">Queued</span>
                <span class="metric-value">
                  {{ metrics.queued_runs }}/{{ metrics.max_queued_runs }}
                </span>
              </div>
              <div class="metric">
                <span class="metric-label">Saturation</span>
                <span class="metric-value mono">
                  {{ metrics.saturation_policy }}
                </span>
              </div>
              <button
                class="ghost"
                :disabled="refreshing"
                @click="refresh"
                title="Re-poll now"
              >{{ refreshing ? '…' : 'Refresh' }}</button>
            </section>

            <div v-if="pollErrorMsg" class="inline-error">{{ pollErrorMsg }}</div>

            <section class="settings-section">
              <h3>In flight</h3>
              <div v-if="active.length === 0" class="empty">
                No active runs.
              </div>
              <ul v-else class="active-list">
                <li
                  v-for="r in active"
                  :key="r.run_id"
                  class="active-row"
                >
                  <span class="status-dot pending" />
                  <code class="run-id">{{ r.run_id }}</code>
                  <span class="wf subtle">{{ r.workflow_id }}</span>
                  <span class="since subtle">running {{ relTime(r.dispatched_at) }}</span>
                  <button
                    class="ghost danger"
                    :disabled="cancellingId === r.run_id"
                    @click="cancel(r.run_id)"
                  >
                    {{ cancellingId === r.run_id ? 'Cancelling…' : '✕ Cancel' }}
                  </button>
                </li>
              </ul>
            </section>
          </template>
        </div>

        <footer class="modal-footer">
          <span class="subtle">
            Live snapshot — polls /runs/active + /controller/concurrency
            every 2&thinsp;s while open. Cancel routes through
            <code>DELETE /runs/:id</code>; the VM observes the
            cancel between instructions.
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
  width: min(720px, 92vw);
  max-height: 84vh;
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

.concurrency {
  display: flex; align-items: center; gap: 18px;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  padding: 8px 12px;
  font-size: 0.6875rem;
}
.concurrency.busy { border-color: rgba(232,166,87,0.4); }
.concurrency.busy .metric-value { color: var(--sf-warning); }
.concurrency.near { border-color: rgba(0,204,136,0.32); }
.metric {
  display: flex; flex-direction: column; gap: 1px;
}
.metric-label {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}
.metric-value {
  font-family: var(--sf-font-mono);
  font-size: 0.8125rem;
  color: var(--sf-text-0);
}
.metric-value.mono { font-size: 0.6875rem; }
.concurrency .ghost {
  margin-left: auto;
}

.settings-section h3 {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
  margin: 0 0 6px 0;
}
.empty {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
  font-style: italic;
  padding: 8px 0;
}
.subtle { color: var(--sf-text-3); font-size: 0.625rem; }
.inline-error {
  font-size: 0.6875rem;
  color: var(--sf-danger, #e75a5a);
  background: rgba(231,90,90,0.06);
  border: 1px solid rgba(231,90,90,0.2);
  border-radius: 4px;
  padding: 6px 10px;
}

.active-list {
  list-style: none;
  padding: 0; margin: 0;
  display: flex; flex-direction: column; gap: 4px;
}
.active-row {
  display: grid;
  grid-template-columns: 10px minmax(180px, auto) 1fr auto auto;
  gap: 8px;
  align-items: center;
  padding: 6px 8px;
  font-size: 0.6875rem;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
}
.status-dot {
  width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto;
}
.status-dot.pending {
  background: var(--sf-warning);
  animation: sf-pulse 1.2s ease-in-out infinite;
}
@keyframes sf-pulse {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.45; }
}
.run-id {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.625rem;
}
.wf { font-family: var(--sf-font-mono); }
.since { font-style: italic; }

.modal-footer {
  border-top: 1px solid var(--sf-border);
  padding: 10px 18px;
  display: flex; align-items: center; justify-content: space-between;
  gap: 12px;
}
.modal-footer code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
}
.ghost {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 4px 12px;
  border-radius: 4px;
  font-size: 0.6875rem;
  cursor: pointer;
}
.ghost:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
.ghost.danger { color: var(--sf-danger, #e75a5a); border-color: rgba(231,90,90,0.4); }
.ghost.danger:hover { background: rgba(231,90,90,0.08); }
.ghost:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
