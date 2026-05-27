<script setup lang="ts">
/**
 * Schedules modal — Phase C C.3 c71.
 *
 * Lets the user manage Timer + Event-trigger schedules on the
 * connected controller. Workflow-scoped — every schedule belongs
 * to a workflow id (the editor doesn't yet track a stable
 * workflow identity across re-submissions, so the modal defaults
 * to the most-recently-submitted workflow id from the run-history
 * store and lets the user paste any other id).
 *
 * Surfaces:
 *
 *   - Workflow-id selector (defaults to last-submitted)
 *   - Schedule list with enable/disable + delete
 *   - Create form (Timer cron / Event path)
 *   - "Trigger event manually" pane that fires
 *     POST /events/:path with a JSON body (useful for testing
 *     webhook integrations without an external sender)
 *
 * Reads connection state from `useControllerStore`; if not
 * connected, shows a friendly "connect first" block and disables
 * everything else.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useControllerStore } from '@/stores/controller.store';
import { useControllerRunHistoryStore } from '@/stores/controller-run-history.store';
import {
  ControllerClientErr,
  type ControllerClientError,
} from '@/runtime-host/client';
import type {
  RunRecord,
  ScheduleRecord,
} from '@/runtime-host/types';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const controller = useControllerStore();
const runHistory = useControllerRunHistoryStore();

onMounted(() => {
  document.addEventListener('keydown', onKey);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
});
function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}
function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

// ----- workflow-id selection -----

/** Most recently submitted workflow_id on the connected controller. */
const recentWorkflowId = computed(() => {
  if (!controller.url) return '';
  const history = runHistory.listFor(controller.url);
  return history[0]?.workflowId ?? '';
});

const workflowId = ref<string>('');
watch(
  () => [props.open, recentWorkflowId.value],
  ([nowOpen, recent]) => {
    if (nowOpen && !workflowId.value && typeof recent === 'string') {
      workflowId.value = recent;
    }
  },
  { immediate: true },
);

// ----- schedule list -----

const schedules = ref<ScheduleRecord[]>([]);
const listError = ref<ControllerClientError | null>(null);
const listLoading = ref(false);

async function refreshList() {
  if (!controller.isConnected || !workflowId.value.trim()) {
    schedules.value = [];
    return;
  }
  listError.value = null;
  listLoading.value = true;
  try {
    const client = controller.getClient();
    schedules.value = await client.listSchedules(workflowId.value.trim());
  } catch (e) {
    schedules.value = [];
    if (e instanceof ControllerClientErr) listError.value = e.payload;
    else listError.value = { kind: 'network', message: String(e) };
  } finally {
    listLoading.value = false;
  }
}

watch(
  () => [props.open, workflowId.value, controller.isConnected],
  () => {
    if (props.open) void refreshList();
  },
  { immediate: true },
);

// ----- create form -----

type CreateKind = 'Timer' | 'Event';
const createKind = ref<CreateKind>('Timer');
const createCron = ref<string>('*/5 * * * *');
const createPath = ref<string>('deploy');
const createEnabled = ref<boolean>(true);
const createError = ref<ControllerClientError | null>(null);
const createBusy = ref(false);

async function onCreate() {
  if (!controller.isConnected || !workflowId.value.trim()) return;
  createError.value = null;
  createBusy.value = true;
  try {
    const client = controller.getClient();
    const trigger = createKind.value === 'Timer'
      ? { kind: 'Timer' as const, schedule_id: '', cron: createCron.value.trim() }
      : { kind: 'Event' as const, source: createPath.value.trim() };
    await client.createSchedule(workflowId.value.trim(), {
      trigger,
      enabled: createEnabled.value,
    });
    await refreshList();
  } catch (e) {
    if (e instanceof ControllerClientErr) createError.value = e.payload;
    else createError.value = { kind: 'network', message: String(e) };
  } finally {
    createBusy.value = false;
  }
}

// ----- per-row actions -----

const busyId = ref<string | null>(null);
async function toggleEnabled(s: ScheduleRecord) {
  busyId.value = s.id;
  try {
    const client = controller.getClient();
    await client.setScheduleEnabled(s.id, !s.enabled);
    await refreshList();
  } catch (e) {
    // Re-display via listError so a temporary failure is visible
    // without yanking the row out of the table.
    if (e instanceof ControllerClientErr) listError.value = e.payload;
  } finally {
    busyId.value = null;
  }
}

async function deleteSchedule(s: ScheduleRecord) {
  if (!confirm(`Delete schedule ${s.id}?`)) return;
  busyId.value = s.id;
  try {
    const client = controller.getClient();
    await client.cancelSchedule(s.id);
    await refreshList();
  } catch (e) {
    if (e instanceof ControllerClientErr) listError.value = e.payload;
  } finally {
    busyId.value = null;
  }
}

// ----- manual event trigger -----

const triggerPath = ref<string>('deploy');
const triggerBody = ref<string>('{}');
const triggerError = ref<ControllerClientError | string | null>(null);
const triggerResult = ref<RunRecord | null>(null);
const triggerBusy = ref(false);

async function onTrigger() {
  if (!controller.isConnected) return;
  triggerError.value = null;
  triggerResult.value = null;
  let body: unknown;
  try {
    body = triggerBody.value.trim() ? JSON.parse(triggerBody.value) : {};
  } catch (e) {
    triggerError.value = `Body must be valid JSON (${e instanceof Error ? e.message : String(e)})`;
    return;
  }
  triggerBusy.value = true;
  try {
    const client = controller.getClient();
    triggerResult.value = await client.triggerEvent(triggerPath.value.trim(), body);
    // Refresh list so any newly-created run timestamps reflect.
  } catch (e) {
    if (e instanceof ControllerClientErr) triggerError.value = e.payload;
    else triggerError.value = String(e);
  } finally {
    triggerBusy.value = false;
  }
}

// ----- display helpers -----

function triggerLabel(t: ScheduleRecord['trigger']): string {
  switch (t.kind) {
    case 'Timer':
      return `Timer · ${t.cron}`;
    case 'Event':
      return `Event · ${t.source}`;
    case 'Manual':
      return 'Manual';
  }
}

function formatTime(ms: number | undefined | null): string {
  if (ms == null) return '—';
  const now = Date.now();
  const diff = ms - now;
  if (Math.abs(diff) < 60_000) {
    return diff >= 0 ? `in ${Math.round(diff / 1000)}s` : `${Math.round(-diff / 1000)}s ago`;
  }
  if (Math.abs(diff) < 3_600_000) {
    return diff >= 0 ? `in ${Math.round(diff / 60_000)}m` : `${Math.round(-diff / 60_000)}m ago`;
  }
  return new Date(ms).toISOString();
}

function formatError(e: ControllerClientError | string | null): string | null {
  if (e === null) return null;
  if (typeof e === 'string') return e;
  return e.message;
}

const listErrorMsg = computed(() => formatError(listError.value));
const createErrorMsg = computed(() => formatError(createError.value));
const triggerErrorMsg = computed(() => formatError(triggerError.value));
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Schedules</span>
            <span class="phase-tag">Phase C.3</span>
          </div>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <!-- Not-connected block -->
          <div v-if="!controller.isConnected" class="warn">
            <strong>Controller not connected.</strong>
            Open Controller Settings, connect to a running controller,
            then return here to manage schedules.
          </div>

          <template v-else>
            <section class="settings-section">
              <label class="field">
                <span class="field-label">Workflow ID</span>
                <input
                  class="field-input"
                  v-model="workflowId"
                  placeholder="wf_… (paste from Recent runs in the Run modal)"
                />
                <span class="field-help">
                  Schedules belong to a workflow. Defaults to the most
                  recently submitted workflow on this controller. Submit
                  a workflow in controller-local mode first if this is
                  empty.
                </span>
              </label>
            </section>

            <section class="settings-section">
              <div class="section-head">
                <h3>Existing schedules</h3>
                <button class="ghost" :disabled="listLoading" @click="refreshList">
                  {{ listLoading ? 'Refreshing…' : 'Refresh' }}
                </button>
              </div>
              <div v-if="listErrorMsg" class="inline-error">{{ listErrorMsg }}</div>
              <div v-if="schedules.length === 0 && !listLoading && !listErrorMsg" class="empty">
                No schedules for this workflow.
              </div>
              <ul v-if="schedules.length > 0" class="sched-list">
                <li v-for="s in schedules" :key="s.id" class="sched-row">
                  <span class="sched-dot" :class="s.enabled ? 'ok' : 'idle'" />
                  <span class="sched-trigger">{{ triggerLabel(s.trigger) }}</span>
                  <code class="sched-id">{{ s.id }}</code>
                  <span class="sched-next subtle">
                    next: {{ formatTime(s.next_fire_at) }}
                  </span>
                  <button
                    class="ghost"
                    :disabled="busyId === s.id"
                    @click="toggleEnabled(s)"
                  >
                    {{ s.enabled ? 'Disable' : 'Enable' }}
                  </button>
                  <button
                    class="ghost danger"
                    :disabled="busyId === s.id"
                    @click="deleteSchedule(s)"
                  >Delete</button>
                </li>
              </ul>
            </section>

            <section class="settings-section">
              <h3>Create a schedule</h3>
              <div class="create-form">
                <div class="row">
                  <label class="radio">
                    <input type="radio" v-model="createKind" value="Timer" /> Timer (cron)
                  </label>
                  <label class="radio">
                    <input type="radio" v-model="createKind" value="Event" /> Event (webhook)
                  </label>
                </div>
                <div v-if="createKind === 'Timer'" class="row">
                  <label class="field-label">Cron expression</label>
                  <input class="field-input mono" v-model="createCron" placeholder="*/5 * * * *" />
                  <span class="field-help">
                    Standard 5-field cron (min hour dom mon dow). Examples:
                    <code>*/5 * * * *</code> every 5 minutes,
                    <code>0 9 * * 1-5</code> 9am weekdays.
                  </span>
                </div>
                <div v-else class="row">
                  <label class="field-label">Event path</label>
                  <input class="field-input mono" v-model="createPath" placeholder="ci/build" />
                  <span class="field-help">
                    Webhook listens on
                    <code>POST {{ controller.url }}/events/{{ createPath || '…' }}</code>.
                    Body is forwarded as the run's <code>inputs</code>.
                  </span>
                </div>
                <label class="row checkbox">
                  <input type="checkbox" v-model="createEnabled" />
                  Enabled
                </label>
                <div v-if="createErrorMsg" class="inline-error">{{ createErrorMsg }}</div>
                <div class="row">
                  <button
                    class="primary"
                    :disabled="createBusy || !workflowId.trim()"
                    @click="onCreate"
                  >{{ createBusy ? 'Creating…' : 'Create' }}</button>
                </div>
              </div>
            </section>

            <section class="settings-section">
              <h3>Test webhook trigger</h3>
              <div class="create-form">
                <div class="row">
                  <label class="field-label">Path</label>
                  <input class="field-input mono" v-model="triggerPath" />
                </div>
                <div class="row">
                  <label class="field-label">JSON body</label>
                  <textarea
                    class="field-input mono code"
                    v-model="triggerBody"
                    rows="4"
                    placeholder='{ "ref": "main" }'
                  />
                </div>
                <div v-if="triggerErrorMsg" class="inline-error">{{ triggerErrorMsg }}</div>
                <div v-if="triggerResult" class="inline-ok">
                  Created run
                  <code>{{ triggerResult.id }}</code>
                  (status: {{ triggerResult.status }}). Poll
                  <code>GET /runs/{{ triggerResult.id }}</code>
                  to see the outcome.
                </div>
                <div class="row">
                  <button
                    class="primary"
                    :disabled="triggerBusy"
                    @click="onTrigger"
                  >{{ triggerBusy ? 'Firing…' : 'Fire event' }}</button>
                </div>
              </div>
            </section>
          </template>
        </div>

        <footer class="modal-footer">
          <span class="subtle">
            Schedules persist in the controller's SQLite DB and
            survive restarts. Cron syntax: 5 fields
            (min hour dom mon dow).
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
  width: min(700px, 92vw);
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
  display: flex; flex-direction: column; gap: 16px;
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
  padding: 6px 10px;
  border-radius: 4px;
}
.field-input.mono { font-family: var(--sf-font-mono); }
.field-input.code { white-space: pre; }
.field-input:focus { outline: 1px solid var(--sf-accent, #5d8acf); }
.field-help {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  font-style: italic;
  line-height: 1.4;
}
.field-help code, .inline-ok code, .row code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
  font-style: normal;
  color: var(--sf-text-0);
}
.row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.row.checkbox { font-size: 0.6875rem; color: var(--sf-text-1); }
.create-form { display: flex; flex-direction: column; gap: 8px; }
.radio { font-size: 0.6875rem; color: var(--sf-text-1); display: flex; align-items: center; gap: 4px; }

.inline-error {
  font-size: 0.6875rem;
  color: var(--sf-danger, #e75a5a);
  background: rgba(231,90,90,0.06);
  border: 1px solid rgba(231,90,90,0.2);
  border-radius: 4px;
  padding: 6px 10px;
}
.inline-ok {
  font-size: 0.6875rem;
  color: var(--sf-success);
  background: rgba(0,204,136,0.06);
  border: 1px solid rgba(0,204,136,0.2);
  border-radius: 4px;
  padding: 6px 10px;
}

.sched-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
.sched-row {
  display: flex; align-items: center; gap: 8px;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  padding: 6px 10px;
  font-size: 0.6875rem;
}
.sched-dot { width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto; }
.sched-dot.ok { background: var(--sf-success); }
.sched-dot.idle { background: var(--sf-text-3); }
.sched-trigger { flex: 0 0 auto; color: var(--sf-text-0); font-family: var(--sf-font-mono); }
.sched-id { font-family: var(--sf-font-mono); color: var(--sf-text-3); font-size: 0.625rem; }
.sched-next { margin-left: auto; }
.subtle { color: var(--sf-text-3); font-style: italic; }

.empty {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
  font-style: italic;
  padding: 8px 0;
}

.modal-footer {
  border-top: 1px solid var(--sf-border);
  padding: 10px 18px;
  display: flex; align-items: center; justify-content: space-between;
  gap: 12px;
}
.primary {
  background: var(--sf-accent, #5d8acf);
  border: 1px solid var(--sf-accent, #5d8acf);
  color: white;
  padding: 5px 14px;
  border-radius: 4px;
  font-size: 0.6875rem;
  font-weight: 500;
  cursor: pointer;
}
.primary:disabled { opacity: 0.55; cursor: not-allowed; }
.primary:not(:disabled):hover { filter: brightness(1.1); }
.ghost {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 4px 10px;
  border-radius: 4px;
  font-size: 0.625rem;
  cursor: pointer;
}
.ghost:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
.ghost.danger { color: var(--sf-danger, #e75a5a); }
.ghost.danger:hover { background: rgba(231,90,90,0.08); }
</style>
