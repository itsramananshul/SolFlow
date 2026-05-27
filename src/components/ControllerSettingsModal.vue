<script setup lang="ts">
/**
 * Controller Settings — Phase C C.2 c62.
 *
 * Live connection management for the SolFlow local controller.
 * Backed by `useControllerStore`; this component is the user-facing
 * surface for connect / disconnect / retry / mismatch handling.
 *
 * State→UX mapping (matches `ConnectionState`):
 *
 *   idle          → "Connect" button enabled; status dot grey
 *   connecting    → button disabled with spinner; dot orange
 *   connected     → "Disconnect" + "Re-check" buttons; dot green;
 *                   controller version + host-spec major shown
 *   error.invalid_url → "Connect" disabled until URL fixed; red
 *                       inline message under the URL field
 *   error.network → "Retry" + "controller unreachable" hint
 *   error.timeout → "Retry" + "took too long" hint
 *   error.http    → status + structured error code
 *   error.version → prominent banner: controller speaks vN, editor
 *                   speaks vM — refuses to use it
 */
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useControllerStore } from '@/stores/controller.store';
import { HOST_SPEC_MAJOR } from '@/runtime-host/types';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const ctrl = useControllerStore();

onMounted(() => {
  document.addEventListener('keydown', onKey);
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
});

function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}

function onUrlInput(e: Event) {
  ctrl.setUrl((e.target as HTMLInputElement).value);
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

const stateKind = computed(() => ctrl.connection.kind);

const statusLabel = computed(() => {
  const c = ctrl.connection;
  switch (c.kind) {
    case 'idle':
      return 'Not connected';
    case 'connecting':
      return 'Connecting…';
    case 'connected':
      return 'Connected';
    case 'error': {
      switch (c.reason.kind) {
        case 'invalid_url':
          return 'Invalid URL';
        case 'network':
          return 'Unreachable';
        case 'timeout':
          return 'Timed out';
        case 'http':
          return `Controller error (HTTP ${c.reason.status})`;
        case 'decode':
          return 'Bad response shape';
        case 'version':
          return 'Version mismatch';
        case 'unknown':
          return 'Failed';
      }
    }
  }
  return '';
});

const statusDotClass = computed(() => {
  switch (ctrl.connection.kind) {
    case 'idle':
      return 'idle';
    case 'connecting':
      return 'connecting';
    case 'connected':
      return 'ok';
    case 'error':
      return 'err';
  }
  return 'idle';
});

/** Pretty controller version banner — appears only when connected. */
const connectionDetail = computed(() => {
  if (ctrl.connection.kind !== 'connected') return null;
  return {
    version: ctrl.connection.health.controller_version,
    hostSpec: ctrl.connection.health.host_spec_major,
    connectedAtIso: new Date(ctrl.connection.connectedAt).toISOString(),
  };
});

/** Inline error message under the URL field when error state. */
const errorDetail = computed(() => {
  const c = ctrl.connection;
  if (c.kind !== 'error') return null;
  return c.reason;
});

/** Disabled while URL is empty OR mid-connection. */
const connectDisabled = computed(
  () => ctrl.connection.kind === 'connecting' || ctrl.url.trim().length === 0,
);

function onConnect() {
  void ctrl.connect();
}

function onRetry() {
  void ctrl.retry();
}

function onDisconnect() {
  ctrl.disconnect();
}

function onRecheck() {
  void ctrl.connect();
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Controller Settings</span>
            <span class="phase-tag">Phase C.2</span>
          </div>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <section class="settings-section">
            <label class="field">
              <span class="field-label">Controller URL</span>
              <input
                type="url"
                class="field-input"
                placeholder="http://127.0.0.1:3939"
                :value="ctrl.url"
                @input="onUrlInput"
                :disabled="stateKind === 'connecting'"
              />
              <span class="field-help">
                The URL of your locally running
                <code>solflow-controller</code>. Stored in this
                browser; never sent anywhere except to the
                controller itself.
              </span>
            </label>
            <div v-if="errorDetail?.kind === 'invalid_url'" class="inline-error">
              {{ errorDetail.message }}
            </div>
          </section>

          <section class="settings-section">
            <h3>Connection</h3>
            <div class="status-row" :class="statusDotClass">
              <span class="status-dot" :class="statusDotClass" />
              <span class="status-label">{{ statusLabel }}</span>
              <div class="status-actions">
                <button
                  v-if="stateKind === 'idle' || (stateKind === 'error' && errorDetail?.kind === 'invalid_url')"
                  class="primary"
                  :disabled="connectDisabled"
                  @click="onConnect"
                >Connect</button>
                <button
                  v-else-if="stateKind === 'connecting'"
                  class="primary"
                  disabled
                >Connecting…</button>
                <template v-else-if="stateKind === 'connected'">
                  <button class="ghost" @click="onRecheck">Re-check</button>
                  <button class="ghost" @click="onDisconnect">Disconnect</button>
                </template>
                <template v-else-if="stateKind === 'error'">
                  <button class="primary" @click="onRetry">Retry</button>
                </template>
              </div>
            </div>

            <!-- Version-mismatch banner — controller refused -->
            <div
              v-if="errorDetail?.kind === 'version'"
              class="version-banner"
            >
              <strong>Host-spec version mismatch.</strong>
              The controller speaks host-spec major
              <code>{{ errorDetail.controllerMajor }}</code>,
              this editor speaks
              <code>{{ errorDetail.editorMajor }}</code>.
              Connection refused. Update either side so the major
              versions match before retrying.
            </div>

            <!-- Network / timeout / http error inline detail -->
            <div
              v-if="errorDetail && errorDetail.kind !== 'invalid_url' && errorDetail.kind !== 'version'"
              class="inline-error"
            >
              {{ errorDetail.message }}
              <template v-if="errorDetail.kind === 'network'">
                <br />
                <span class="subtle">
                  Is the controller running? Try
                  <code>cargo run -p solflow_controller</code>.
                </span>
              </template>
            </div>

            <!-- Connection detail -->
            <div v-if="connectionDetail" class="connection-detail">
              <div class="kv">
                <span class="k">controller version</span>
                <code class="v">{{ connectionDetail.version }}</code>
              </div>
              <div class="kv">
                <span class="k">host-spec major</span>
                <code class="v">{{ connectionDetail.hostSpec }}</code>
                <span class="subtle">(editor: {{ HOST_SPEC_MAJOR }})</span>
              </div>
              <div class="kv">
                <span class="k">connected at</span>
                <code class="v">{{ connectionDetail.connectedAtIso }}</code>
              </div>
            </div>
          </section>

          <section v-if="ctrl.isConnected" class="settings-section">
            <h3>Connectors</h3>
            <div v-if="ctrl.connectors.length === 0" class="subtle">
              No connectors reported. (Older controller, or
              <code>GET /connectors</code> blocked.)
            </div>
            <ul v-else class="connectors-list">
              <li
                v-for="c in ctrl.connectors"
                :key="c.name"
                class="connector-row"
              >
                <span class="conn-name">{{ c.name }}</span>
                <span class="conn-desc">{{ c.description }}</span>
                <span class="conn-policy subtle">
                  timeout {{ c.default_policy.timeout_ms }}ms ·
                  retries {{ c.default_policy.retry_attempts }} ·
                  max-resp {{ (c.default_policy.max_response_bytes / 1024).toFixed(0) }}KiB
                </span>
                <span class="conn-version subtle">v{{ c.version }}</span>
              </li>
            </ul>
            <div class="conn-help subtle">
              Use these from SOL via
              <code>connector://&lt;name&gt;?...</code> URLs in
              <code>ext function</code> endpoints. ExtCall via
              an unknown connector produces a structured
              <code>ExtCallFailed</code> runtime error.
            </div>
          </section>

          <section class="settings-section">
            <h3>Execution mode</h3>
            <div class="mode-list">
              <div class="mode-row active">
                <span class="mode-dot ok" />
                <div class="mode-text">
                  <strong>browser-sim</strong>
                  <span class="subtle">— canonical SOL VM in your browser; ExtCall blocked</span>
                </div>
                <span class="mode-tag">Always available</span>
              </div>
              <div
                class="mode-row"
                :class="{ active: ctrl.isConnected, disabled: !ctrl.isConnected }"
              >
                <span class="mode-dot" :class="{ ok: ctrl.isConnected }" />
                <div class="mode-text">
                  <strong>controller-local</strong>
                  <span class="subtle">— canonical VM hosted by a controller on this machine</span>
                </div>
                <span class="mode-tag">
                  {{ ctrl.isConnected ? 'Available' : 'Connect to enable' }}
                </span>
              </div>
              <div class="mode-row disabled">
                <span class="mode-dot" />
                <div class="mode-text">
                  <strong>controller-remote</strong>
                  <span class="subtle">— remote controller over HTTPS</span>
                </div>
                <span class="mode-tag">C.7</span>
              </div>
            </div>
          </section>
        </div>

        <footer class="modal-footer">
          <span class="subtle">
            <template v-if="ctrl.isConnected">
              Run modal now shows the mode selector — pick
              controller-local to execute via the controller.
            </template>
            <template v-else>
              Connect to a controller to unlock controller-local
              execution mode.
            </template>
          </span>
          <button class="ghost" @click="emit('close')">Done</button>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.12s ease;
}
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
  width: min(620px, 92vw);
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
.header-left {
  display: flex;
  align-items: baseline;
  gap: 10px;
}
.title { font-size: 0.875rem; font-weight: 600; }
.phase-tag {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  background: rgba(0, 204, 136, 0.16);
  color: var(--sf-success);
  padding: 2px 8px;
  border-radius: 3px;
}
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
  margin: 0 0 6px 0;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
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
}
.field-input:focus {
  outline: 1px solid var(--sf-accent, #5d8acf);
}
.field-input:disabled { opacity: 0.6; cursor: not-allowed; }
.field-help {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  font-style: italic;
}
.field-help code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
  font-style: normal;
}

.inline-error {
  margin-top: 6px;
  font-size: 0.6875rem;
  color: var(--sf-danger, #e75a5a);
  background: rgba(231, 90, 90, 0.06);
  border: 1px solid rgba(231, 90, 90, 0.2);
  border-radius: 4px;
  padding: 6px 10px;
}
.inline-error code {
  font-family: var(--sf-font-mono);
  background: rgba(0, 0, 0, 0.12);
  padding: 0 4px;
  border-radius: 2px;
}

.status-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 0.6875rem;
  padding: 6px 0;
}
.status-label { flex: 0 0 auto; min-width: 110px; }
.status-actions { margin-left: auto; display: flex; gap: 6px; }
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.status-dot.idle        { background: var(--sf-text-3); }
.status-dot.connecting  { background: var(--sf-warning); animation: pulse 1.2s ease-in-out infinite; }
.status-dot.ok          { background: var(--sf-success); }
.status-dot.err         { background: var(--sf-danger, #e75a5a); }
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.45; }
}

.version-banner {
  margin-top: 6px;
  font-size: 0.6875rem;
  background: rgba(231, 90, 90, 0.08);
  border: 1px solid rgba(231, 90, 90, 0.32);
  border-radius: 4px;
  padding: 8px 12px;
  line-height: 1.55;
  color: var(--sf-text-1);
}
.version-banner strong { color: var(--sf-danger, #e75a5a); }
.version-banner code {
  font-family: var(--sf-font-mono);
  background: rgba(0, 0, 0, 0.14);
  padding: 0 4px;
  border-radius: 2px;
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
.kv {
  display: flex;
  align-items: baseline;
  gap: 10px;
  font-size: 0.6875rem;
}
.kv .k {
  flex: 0 0 130px;
  font-size: 0.625rem;
  color: var(--sf-text-2);
  letter-spacing: 0.3px;
  text-transform: uppercase;
}
.kv .v {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
}

.mode-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.mode-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border: 1px solid var(--sf-border);
  border-radius: 4px;
  background: var(--sf-bg-1);
}
.mode-row.active {
  border-color: rgba(0, 204, 136, 0.32);
  background: rgba(0, 204, 136, 0.04);
}
.mode-row.disabled { opacity: 0.55; }
.mode-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--sf-text-3);
  flex: 0 0 auto;
}
.mode-dot.ok { background: var(--sf-success); }
.mode-text {
  flex: 1;
  font-size: 0.6875rem;
}
.mode-text strong { color: var(--sf-text-0); }
.mode-tag {
  font-size: 0.5625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  background: var(--sf-bg-2);
  padding: 2px 6px;
  border-radius: 3px;
}
.mode-row.active .mode-tag {
  color: var(--sf-success);
}

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
.subtle code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 0 4px;
  border-radius: 2px;
}

/* Connectors list (C.4 c78) */
.connectors-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
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
.conn-name {
  font-family: var(--sf-font-mono);
  font-weight: 600;
  color: var(--sf-text-0);
  grid-row: 1;
  grid-column: 1;
}
.conn-desc {
  color: var(--sf-text-1);
  grid-row: 1;
  grid-column: 2;
}
.conn-version {
  grid-row: 1;
  grid-column: 3;
  font-family: var(--sf-font-mono);
}
.conn-policy {
  grid-row: 2;
  grid-column: 1 / -1;
  font-style: italic;
  margin-top: 2px;
}
.conn-help {
  margin-top: 8px;
  line-height: 1.4;
}
</style>
