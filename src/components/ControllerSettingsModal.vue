<script setup lang="ts">
/**
 * Controller Settings — Phase C C.2 c62 + C.7 c101.
 *
 * Live connection management for a SolFlow controller. C.7
 * extended this to surface the full remote-controller posture:
 * transport (HTTP/HTTPS), unsafe-HTTP warnings, bearer-token
 * field, auth status, and per-failure-mode UX.
 *
 * Backed by `useControllerStore`; this component is the user-
 * facing surface for connect / disconnect / retry / mismatch /
 * unauthenticated / unsafe-URL handling.
 *
 * State→UX mapping (matches `ConnectionState`):
 *
 *   idle              → "Connect" button enabled; status dot grey
 *   connecting        → button disabled with spinner; dot orange
 *   connected         → "Disconnect" + "Re-check" buttons; dot green;
 *                       controller version, name, host-spec major,
 *                       auth_required shown
 *   error.invalid_url → "Connect" disabled until URL fixed; red
 *                       inline message under the URL field
 *   error.network     → "Retry" + "controller unreachable" hint
 *   error.timeout     → "Retry" + "took too long" hint
 *   error.http        → status + structured error code
 *   error.version     → prominent banner: controller speaks vN,
 *                       editor speaks vM — refuses to use it
 *   error.auth        → "Set / fix your token" guidance + code
 */
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useControllerStore } from '@/stores/controller.store';
import { HOST_SPEC_MAJOR } from '@/runtime-host/types';
import { classifyControllerUrl } from '@/runtime-host/client';

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

function onTokenInput(e: Event) {
  ctrl.setAuthToken((e.target as HTMLInputElement).value);
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

const stateKind = computed(() => ctrl.connection.kind);

// Phase C C.7 c101 — classification of the typed URL. Drives the
// transport badge + the unsafe-HTTP warning. Recomputes live as
// the user types.
const urlClassification = computed(() => classifyControllerUrl(ctrl.url));

const transportBadgeLabel = computed(() => {
  switch (urlClassification.value.kind) {
    case 'local':           return 'local · HTTP';
    case 'loopback_https':  return 'local · HTTPS';
    case 'https_remote':    return 'remote · HTTPS';
    case 'unsafe_remote':   return 'remote · HTTP ⚠';
    case 'invalid':         return '—';
  }
});

const transportBadgeKind = computed(() => urlClassification.value.kind);

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
        case 'auth':
          return 'Auth rejected';
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
  const h = ctrl.connection.health;
  return {
    version: h.controller_version,
    hostSpec: h.host_spec_major,
    name: h.name ?? '(pre-C.7 controller)',
    authRequired: h.auth_required === true,
    connectedAtIso: new Date(ctrl.connection.connectedAt).toISOString(),
  };
});

/** Inline error message under the URL field when error state. */
const errorDetail = computed(() => {
  const c = ctrl.connection;
  if (c.kind !== 'error') return null;
  return c.reason;
});

/** Auth-specific human-friendly guidance per failure code. */
const authErrorGuidance = computed(() => {
  const c = ctrl.connection;
  if (c.kind !== 'error' || c.reason.kind !== 'auth') return null;
  switch (c.reason.code) {
    case 'auth_missing':
      return 'The controller requires a bearer token. Paste one into the Authentication field above and re-try.';
    case 'auth_mismatch':
      return 'The token you sent doesn\'t match the controller\'s. Re-check the token from your operator and re-try.';
    case 'auth_malformed':
      return 'Your token header is malformed. Make sure the value is just the token — the client adds the "Bearer " prefix automatically.';
    case 'unauthorized':
    default:
      return 'The controller refused your credentials. Re-check your token, then re-try.';
  }
});

/** Disabled while URL is empty OR mid-connection. */
const connectDisabled = computed(
  () => ctrl.connection.kind === 'connecting'
      || ctrl.url.trim().length === 0
      || urlClassification.value.kind === 'invalid',
);

/** Phase C C.7 — true when the URL warrants an in-modal warning
 *  banner. Today: HTTP to a non-loopback host. */
const showUnsafeWarning = computed(
  () => urlClassification.value.kind === 'unsafe_remote',
);

/** Phase C C.7 — remote-mode availability. Available when
 *  connected AND the URL classifies as a non-loopback host.
 *  Otherwise the editor's RunModal stays on browser-sim or
 *  controller-local. */
const remoteModeAvailable = computed(
  () => ctrl.isConnected
    && (urlClassification.value.kind === 'https_remote'
       || urlClassification.value.kind === 'unsafe_remote'),
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
            <span class="phase-tag">Phase C.7</span>
          </div>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <section class="settings-section">
            <label class="field">
              <span class="field-label">Controller URL</span>
              <div class="url-row">
                <input
                  type="url"
                  class="field-input"
                  placeholder="http://127.0.0.1:3939"
                  :value="ctrl.url"
                  @input="onUrlInput"
                  :disabled="stateKind === 'connecting'"
                />
                <span
                  class="transport-badge"
                  :class="`badge-${transportBadgeKind}`"
                  :title="urlClassification.warnings.join('\n') || 'transport posture'"
                >{{ transportBadgeLabel }}</span>
              </div>
              <span class="field-help">
                The URL of a SolFlow controller. Local + remote
                supported. Stored in this browser; never sent
                anywhere except to the controller itself.
              </span>
            </label>

            <!-- Unsafe-HTTP-remote warning -->
            <div v-if="showUnsafeWarning" class="warn-banner">
              <strong>Unsafe transport.</strong>
              {{ urlClassification.warnings[0] }}
            </div>

            <div v-if="errorDetail?.kind === 'invalid_url'" class="inline-error">
              {{ errorDetail.message }}
            </div>
          </section>

          <!-- Phase C C.7 c101 — Authentication section. Always
               rendered; the field is optional. Connecting to a
               controller that requires auth surfaces a clear
               401-friendly message below. -->
          <section class="settings-section">
            <h3>Authentication</h3>
            <label class="field">
              <span class="field-label">Bearer token (optional)</span>
              <input
                type="password"
                class="field-input"
                placeholder="leave blank for local-dev / unauthenticated controllers"
                :value="ctrl.authToken"
                @input="onTokenInput"
                autocomplete="off"
                spellcheck="false"
              />
              <span class="field-help">
                Sent as <code>Authorization: Bearer &lt;token&gt;</code>
                on every protected request. Required when the
                controller is started with
                <code>SOLFLOW_CONTROLLER_AUTH_TOKEN</code>. Stored
                in this browser only.
              </span>
            </label>
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

            <!-- Phase C C.7 c101 — auth-error specific banner -->
            <div
              v-if="errorDetail?.kind === 'auth'"
              class="auth-banner"
            >
              <strong>Controller rejected auth.</strong>
              <span class="auth-code"><code>{{ errorDetail.code }}</code></span>
              <p class="auth-guidance">{{ authErrorGuidance }}</p>
            </div>

            <!-- Network / timeout / http error inline detail -->
            <div
              v-if="errorDetail
                    && errorDetail.kind !== 'invalid_url'
                    && errorDetail.kind !== 'version'
                    && errorDetail.kind !== 'auth'"
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
                <span class="k">controller</span>
                <code class="v">{{ connectionDetail.name }}</code>
              </div>
              <div class="kv">
                <span class="k">version</span>
                <code class="v">{{ connectionDetail.version }}</code>
              </div>
              <div class="kv">
                <span class="k">host-spec major</span>
                <code class="v">{{ connectionDetail.hostSpec }}</code>
                <span class="subtle">(editor: {{ HOST_SPEC_MAJOR }})</span>
              </div>
              <div class="kv">
                <span class="k">auth required</span>
                <code class="v">{{ connectionDetail.authRequired ? 'yes' : 'no' }}</code>
                <span
                  v-if="connectionDetail.authRequired && !ctrl.authToken"
                  class="subtle warn-text"
                >
                  controller wants a token — set one above
                </span>
              </div>
              <div class="kv">
                <span class="k">transport</span>
                <code class="v">{{ transportBadgeLabel }}</code>
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
                :class="{
                  active: ctrl.isConnected && !remoteModeAvailable,
                  disabled: !ctrl.isConnected,
                }"
              >
                <span class="mode-dot" :class="{ ok: ctrl.isConnected && !remoteModeAvailable }" />
                <div class="mode-text">
                  <strong>controller-local</strong>
                  <span class="subtle">— canonical VM hosted by a controller on this machine</span>
                </div>
                <span class="mode-tag">
                  {{ !ctrl.isConnected
                       ? 'Connect to enable'
                       : (remoteModeAvailable ? 'Remote' : 'Available') }}
                </span>
              </div>
              <div
                class="mode-row"
                :class="{
                  active: remoteModeAvailable,
                  disabled: !remoteModeAvailable,
                }"
              >
                <span class="mode-dot" :class="{ ok: remoteModeAvailable }" />
                <div class="mode-text">
                  <strong>controller-remote</strong>
                  <span class="subtle">— remote controller over HTTPS (Phase C.7)</span>
                </div>
                <span class="mode-tag">
                  {{ remoteModeAvailable ? 'Available' : (ctrl.isConnected ? 'Local URL' : 'Connect to enable') }}
                </span>
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
  width: 100%;
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

/* Phase C C.7 c101 — URL + transport badge inline */
.url-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
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
.transport-badge.badge-invalid {
  color: var(--sf-text-3);
}

.warn-banner {
  margin-top: 8px;
  font-size: 0.6875rem;
  background: rgba(231, 90, 90, 0.08);
  border: 1px solid rgba(231, 90, 90, 0.32);
  border-radius: 4px;
  padding: 8px 12px;
  line-height: 1.55;
  color: var(--sf-text-1);
}
.warn-banner strong { color: var(--sf-danger, #e75a5a); }

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

.version-banner,
.auth-banner {
  margin-top: 6px;
  font-size: 0.6875rem;
  background: rgba(231, 90, 90, 0.08);
  border: 1px solid rgba(231, 90, 90, 0.32);
  border-radius: 4px;
  padding: 8px 12px;
  line-height: 1.55;
  color: var(--sf-text-1);
}
.version-banner strong,
.auth-banner strong {
  color: var(--sf-danger, #e75a5a);
}
.version-banner code,
.auth-banner code {
  font-family: var(--sf-font-mono);
  background: rgba(0, 0, 0, 0.14);
  padding: 0 4px;
  border-radius: 2px;
}
.auth-banner .auth-code { margin-left: 6px; }
.auth-banner .auth-guidance {
  margin: 4px 0 0 0;
  color: var(--sf-text-1);
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
.warn-text { color: var(--sf-danger, #e75a5a); }

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
