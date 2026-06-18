<script setup lang="ts">
/**
 * Controller Settings.
 *
 * Configures the three run targets and the two controller endpoints:
 *
 *   - Browser Simulation: the canonical SOL VM in this browser.
 *   - Local Controller:   a controller on this machine (127.0.0.1).
 *   - Cloud Controller:   a public HTTPS controller.
 *
 * Each controller has its own URL, a health check, and a live
 * connected / disconnected status. The bearer token is shared and
 * sent to whichever controller requires it. All settings persist to
 * this browser's localStorage and are never sent anywhere except to
 * the controller itself.
 */
import { computed, onBeforeUnmount, onMounted } from 'vue';
import {
  useControllerStore,
  type ConnectionState,
  type ControllerTarget,
} from '@/stores/controller.store';
import { HOST_SPEC_MAJOR } from '@/runtime-host/types';
import { classifyControllerUrl } from '@/runtime-host/client';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const ctrl = useControllerStore();

onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => document.removeEventListener('keydown', onKey));

function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

function onTokenInput(e: Event) {
  ctrl.setAuthToken((e.target as HTMLInputElement).value);
}

// ---- per-controller view model ----

interface ControllerView {
  id: ControllerTarget;
  title: string;
  placeholder: string;
  url: string;
  conn: ConnectionState;
  dotClass: 'idle' | 'connecting' | 'ok' | 'err';
  statusLabel: string;
  detail: ReturnType<typeof connectionDetail>;
  errorText: string | null;
  transportLabel: string;
  transportKind: string;
}

function statusDotClass(c: ConnectionState): 'idle' | 'connecting' | 'ok' | 'err' {
  switch (c.kind) {
    case 'idle': return 'idle';
    case 'connecting': return 'connecting';
    case 'connected': return 'ok';
    case 'error': return 'err';
  }
}

function statusLabel(c: ConnectionState): string {
  switch (c.kind) {
    case 'idle': return 'Not checked';
    case 'connecting': return 'Checking…';
    case 'connected': return 'Connected';
    case 'error':
      switch (c.reason.kind) {
        case 'invalid_url': return 'Invalid URL';
        case 'network': return 'Disconnected';
        case 'timeout': return 'Timed out';
        case 'http': return `Controller error (HTTP ${c.reason.status})`;
        case 'decode': return 'Bad response';
        case 'version': return 'Version mismatch';
        case 'auth': return 'Auth rejected';
        case 'unknown': return 'Failed';
      }
  }
}

function connectionDetail(c: ConnectionState) {
  if (c.kind !== 'connected') return null;
  const h = c.health;
  return {
    version: h.controller_version,
    hostSpec: h.host_spec_major,
    name: h.name ?? 'controller',
    authRequired: h.auth_required === true,
    connectedAtIso: new Date(c.connectedAt).toISOString(),
  };
}

/** A specific, human-readable error line per failure mode. */
function errorText(target: ControllerTarget, c: ConnectionState): string | null {
  if (c.kind !== 'error') return null;
  const where = target === 'cloud' ? 'Cloud Controller' : 'Local Controller';
  const r = c.reason;
  switch (r.kind) {
    case 'invalid_url':
      return `${r.message}. Enter a URL starting with http:// or https://.`;
    case 'network':
      return target === 'cloud'
        ? `Cloud controller not reachable at ${ctrl.cloudUrl}. Check the URL and that the controller is online, or switch to Browser Simulation.`
        : `Controller not reachable. Start the local controller or switch to Browser Simulation.`;
    case 'timeout':
      return `${where} did not respond in time (${r.message}). It may be overloaded; retry, or switch to Browser Simulation.`;
    case 'http':
      return `${where} returned HTTP ${r.status}${r.code ? ` (${r.code})` : ''}. ${r.message}`;
    case 'decode':
      return `${where} sent a response the editor could not read. ${r.message}`;
    case 'version':
      return `Version mismatch: the controller speaks host-spec major ${r.controllerMajor}, this editor speaks ${r.editorMajor}. Update either side so the majors match.`;
    case 'auth':
      return `${where} rejected the bearer token (${r.code}). Set the correct token below and check again.`;
    case 'unknown':
      return `${where} check failed: ${r.message}`;
  }
}

function viewFor(id: ControllerTarget): ControllerView {
  const url = id === 'cloud' ? ctrl.cloudUrl : ctrl.localUrl;
  const conn = id === 'cloud' ? ctrl.cloudConn : ctrl.localConn;
  const cls = classifyControllerUrl(url);
  return {
    id,
    title: id === 'cloud' ? 'Cloud Controller' : 'Local Controller',
    placeholder: id === 'cloud' ? 'https://controller.example.com' : 'http://127.0.0.1:3939',
    url,
    conn,
    dotClass: statusDotClass(conn),
    statusLabel: statusLabel(conn),
    detail: connectionDetail(conn),
    errorText: errorText(id, conn),
    transportLabel: transportLabel(cls.kind),
    transportKind: cls.kind,
  };
}

function transportLabel(kind: string): string {
  switch (kind) {
    case 'local': return 'local · HTTP';
    case 'loopback_https': return 'local · HTTPS';
    case 'https_remote': return 'remote · HTTPS';
    case 'unsafe_remote': return 'remote · HTTP (unsafe)';
    default: return '—';
  }
}

const localView = computed(() => viewFor('local'));
const cloudView = computed(() => viewFor('cloud'));

function onUrlInput(id: ControllerTarget, e: Event) {
  const v = (e.target as HTMLInputElement).value;
  if (id === 'cloud') ctrl.setCloudUrl(v);
  else ctrl.setLocalUrl(v);
}

function onCheck(id: ControllerTarget) {
  void ctrl.checkHealth(id);
}

// ---- run target ----

interface RunTargetCard {
  id: 'browser-sim' | 'local' | 'cloud';
  label: string;
  description: string;
  status: string;
  available: boolean;
}

const runTargetCards = computed<RunTargetCard[]>(() => [
  {
    id: 'browser-sim',
    label: 'Browser Simulation',
    description: 'Runs in this browser via the canonical SOL VM. External Actions are blocked.',
    status: 'Always available',
    available: true,
  },
  {
    id: 'local',
    label: 'Local Controller',
    description: 'Runs on a controller on your own machine.',
    status: ctrl.localConnected ? 'Connected' : 'Disconnected',
    available: ctrl.localUrl.trim().length > 0,
  },
  {
    id: 'cloud',
    label: 'Cloud Controller',
    description: 'Runs on a hosted HTTPS controller with real capability providers.',
    status: ctrl.cloudUrl.trim().length > 0 ? (ctrl.cloudConnected ? 'Connected' : 'Disconnected') : 'No URL set',
    available: ctrl.cloudUrl.trim().length > 0,
  },
]);

function selectRunTarget(id: 'browser-sim' | 'local' | 'cloud') {
  ctrl.setRunTarget(id);
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <span class="title">Controller Settings</span>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <!-- Run target -->
          <section class="settings-section">
            <h3>Run target</h3>
            <div class="target-cards">
              <button
                v-for="t in runTargetCards"
                :key="t.id"
                class="target-card"
                :class="{ active: ctrl.runTarget === t.id, disabled: !t.available }"
                :disabled="!t.available"
                @click="selectRunTarget(t.id)"
              >
                <span class="target-radio" :class="{ on: ctrl.runTarget === t.id }" />
                <span class="target-body">
                  <span class="target-label">{{ t.label }}</span>
                  <span class="target-desc">{{ t.description }}</span>
                </span>
                <span class="target-status" :class="{ ok: t.status === 'Connected' }">{{ t.status }}</span>
              </button>
            </div>
          </section>

          <!-- Local + Cloud controller cards -->
          <section
            v-for="v in [localView, cloudView]"
            :key="v.id"
            class="settings-section"
          >
            <h3>{{ v.title }}</h3>
            <label class="field">
              <div class="url-row">
                <input
                  type="url"
                  class="field-input"
                  :placeholder="v.placeholder"
                  :value="v.url"
                  @input="(e) => onUrlInput(v.id, e)"
                  :disabled="v.conn.kind === 'connecting'"
                />
                <span class="transport-badge" :class="`badge-${v.transportKind}`">{{ v.transportLabel }}</span>
              </div>
            </label>

            <div class="status-row">
              <span class="status-dot" :class="v.dotClass" />
              <span class="status-label">{{ v.statusLabel }}</span>
              <div class="status-actions">
                <button class="primary" :disabled="v.conn.kind === 'connecting'" @click="onCheck(v.id)">
                  {{ v.conn.kind === 'connecting' ? 'Checking…' : 'Check connection' }}
                </button>
              </div>
            </div>

            <div v-if="v.errorText" class="inline-error">{{ v.errorText }}</div>

            <div v-if="v.detail" class="connection-detail">
              <div class="kv"><span class="k">controller</span><code class="v">{{ v.detail.name }}</code></div>
              <div class="kv"><span class="k">version</span><code class="v">{{ v.detail.version }}</code></div>
              <div class="kv">
                <span class="k">host-spec major</span><code class="v">{{ v.detail.hostSpec }}</code>
                <span class="subtle">(editor: {{ HOST_SPEC_MAJOR }})</span>
              </div>
              <div class="kv">
                <span class="k">auth required</span><code class="v">{{ v.detail.authRequired ? 'yes' : 'no' }}</code>
                <span v-if="v.detail.authRequired && !ctrl.authToken" class="subtle warn-text">set a token below</span>
              </div>
            </div>
          </section>

          <!-- Authentication (shared) -->
          <section class="settings-section">
            <h3>Authentication</h3>
            <label class="field">
              <span class="field-label">Bearer token (optional)</span>
              <input
                type="password"
                class="field-input"
                placeholder="leave blank for unauthenticated controllers"
                :value="ctrl.authToken"
                @input="onTokenInput"
                autocomplete="off"
                spellcheck="false"
              />
              <span class="field-help">
                Sent as <code>Authorization: Bearer &lt;token&gt;</code> to whichever
                controller requires it. Stored in this browser only.
              </span>
            </label>
          </section>

          <!-- Connectors on the active controller -->
          <section v-if="ctrl.isConnected" class="settings-section">
            <h3>Connectors ({{ ctrl.activeTarget === 'cloud' ? 'cloud' : 'local' }})</h3>
            <div v-if="ctrl.connectors.length === 0" class="subtle">
              No connectors reported by this controller.
            </div>
            <ul v-else class="connectors-list">
              <li v-for="c in ctrl.connectors" :key="c.name" class="connector-row">
                <span class="conn-name">{{ c.name }}</span>
                <span class="conn-desc">{{ c.description }}</span>
                <span class="conn-version subtle">v{{ c.version }}</span>
                <span class="conn-policy subtle">
                  timeout {{ c.default_policy.timeout_ms }}ms ·
                  retries {{ c.default_policy.retry_attempts }} ·
                  max-resp {{ (c.default_policy.max_response_bytes / 1024).toFixed(0) }}KiB
                </span>
              </li>
            </ul>
          </section>
        </div>

        <footer class="modal-footer">
          <span class="subtle">Settings are stored in this browser only.</span>
          <button class="ghost" @click="emit('close')">Done</button>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active { transition: opacity 0.12s ease; }
.fade-enter-from,
.fade-leave-to { opacity: 0; }

.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--sf-z-modal, 1000);
}
.modal {
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 6px;
  width: min(640px, 92vw);
  max-height: 84vh;
  display: flex;
  flex-direction: column;
  color: var(--sf-text-0);
}
.modal-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 18px;
  border-bottom: 1px solid var(--sf-border);
}
.title { font-size: 0.875rem; font-weight: 600; }
.close {
  margin-left: auto;
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  font-size: 0.875rem;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 3px;
}
.close:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }

.body {
  padding: 14px 18px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 18px;
}
.settings-section h3 {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
  margin: 0 0 8px 0;
}

/* Run-target cards */
.target-cards { display: flex; flex-direction: column; gap: 6px; }
.target-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  background: var(--sf-bg-1);
  cursor: pointer;
  text-align: left;
  color: var(--sf-text-0);
}
.target-card:hover:not(.disabled) { background: var(--sf-bg-2); }
.target-card.active {
  border-color: var(--sf-accent, #5d8acf);
  background: rgba(93, 138, 207, 0.08);
}
.target-card.disabled { opacity: 0.5; cursor: not-allowed; }
.target-radio {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid var(--sf-text-3);
  flex: 0 0 auto;
}
.target-radio.on {
  border-color: var(--sf-accent, #5d8acf);
  background: radial-gradient(circle at center, var(--sf-accent, #5d8acf) 0 3px, transparent 4px);
}
.target-body { display: flex; flex-direction: column; gap: 2px; flex: 1; }
.target-label { font-size: 0.75rem; font-weight: 600; }
.target-desc { font-size: 0.625rem; color: var(--sf-text-3); }
.target-status {
  font-size: 0.5625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  background: var(--sf-bg-2);
  padding: 2px 6px;
  border-radius: 3px;
}
.target-status.ok { color: var(--sf-success); }

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
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  padding: 6px 10px;
  border-radius: 4px;
  width: 100%;
}
.field-input:focus { outline: 1px solid var(--sf-accent, #5d8acf); }
.field-input:disabled { opacity: 0.6; cursor: not-allowed; }
.field-help { font-size: 0.625rem; color: var(--sf-text-3); font-style: italic; }
.field-help code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
  font-style: normal;
}

.url-row { display: flex; align-items: center; gap: 8px; }
.url-row .field-input { flex: 1; }
.transport-badge {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  padding: 4px 8px;
  border-radius: 3px;
  white-space: nowrap;
  border: 1px solid var(--sf-border);
  background: var(--sf-bg-2);
  color: var(--sf-text-2);
  flex: 0 0 auto;
}
.transport-badge.badge-local {
  background: rgba(93, 138, 207, 0.12);
  color: var(--sf-accent, #5d8acf);
  border-color: rgba(93, 138, 207, 0.3);
}
.transport-badge.badge-loopback_https,
.transport-badge.badge-https_remote {
  background: rgba(0, 204, 136, 0.12);
  color: var(--sf-success);
  border-color: rgba(0, 204, 136, 0.3);
}
.transport-badge.badge-unsafe_remote {
  background: rgba(231, 90, 90, 0.12);
  color: var(--sf-danger, #e75a5a);
  border-color: rgba(231, 90, 90, 0.3);
}

.status-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 0.6875rem;
  padding: 8px 0 2px;
}
.status-label { flex: 0 0 auto; min-width: 110px; }
.status-actions { margin-left: auto; display: flex; gap: 6px; }
.status-dot { width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto; }
.status-dot.idle { background: var(--sf-text-3); }
.status-dot.connecting { background: var(--sf-warning); animation: pulse 1.2s ease-in-out infinite; }
.status-dot.ok { background: var(--sf-success); }
.status-dot.err { background: var(--sf-danger, #e75a5a); }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }

.inline-error {
  margin-top: 8px;
  font-size: 0.6875rem;
  color: var(--sf-danger, #e75a5a);
  background: rgba(231, 90, 90, 0.06);
  border: 1px solid rgba(231, 90, 90, 0.2);
  border-radius: 4px;
  padding: 8px 10px;
  line-height: 1.5;
}

.connection-detail {
  margin-top: 8px;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.kv { display: flex; align-items: baseline; gap: 10px; font-size: 0.6875rem; }
.kv .k {
  flex: 0 0 130px;
  font-size: 0.625rem;
  color: var(--sf-text-2);
  letter-spacing: 0.3px;
  text-transform: uppercase;
}
.kv .v { font-family: var(--sf-font-mono); color: var(--sf-text-0); }
.warn-text { color: var(--sf-danger, #e75a5a); }

.modal-footer {
  border-top: 1px solid var(--sf-border);
  padding: 10px 18px;
  display: flex;
  align-items: center;
  justify-content: space-between;
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
  padding: 5px 14px;
  border-radius: 4px;
  font-size: 0.6875rem;
  cursor: pointer;
}
.ghost:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
.subtle { color: var(--sf-text-3); font-size: 0.625rem; }
.subtle code { font-family: var(--sf-font-mono); background: var(--sf-bg-2); padding: 0 4px; border-radius: 2px; }

.connectors-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
.connector-row {
  display: grid;
  grid-template-columns: auto 1fr auto;
  grid-template-rows: auto auto;
  column-gap: 10px;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  padding: 6px 10px;
  font-size: 0.6875rem;
}
.conn-name { font-family: var(--sf-font-mono); font-weight: 600; grid-row: 1; grid-column: 1; }
.conn-desc { color: var(--sf-text-1); grid-row: 1; grid-column: 2; }
.conn-version { grid-row: 1; grid-column: 3; font-family: var(--sf-font-mono); }
.conn-policy { grid-row: 2; grid-column: 1 / -1; font-style: italic; margin-top: 2px; }
</style>
