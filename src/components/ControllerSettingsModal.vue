<script setup lang="ts">
/**
 * Controller Settings — Phase C.1 STUB.
 *
 * This modal is intentionally display-only. It establishes the
 * UI surface for the controller connection that C.2 will make
 * real. Until then:
 *
 *   - The URL field accepts text but goes nowhere
 *   - Connection status is always "not connected"
 *   - The connectors list shows the future shape
 *
 * Documented prominently in-modal so users who open it during
 * the C.1 phase understand they're seeing scaffolding, not a
 * broken feature.
 *
 * Wired into the Toolbar's overflow menu (or equivalent
 * surface). Triggered by an explicit user action — no hidden
 * auto-popups.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { HOST_SPEC_MAJOR } from '@/runtime-host/types';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

/** User-entered URL. Persisted in localStorage so the value
 *  survives reloads even though it doesn't yet DO anything. */
const url = ref<string>('');

const STORAGE_KEY = 'solflow.controller.url';

onMounted(() => {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved) url.value = saved;
  document.addEventListener('keydown', onKey);
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
});

function onKey(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close');
}

function onUrlInput(e: Event) {
  url.value = (e.target as HTMLInputElement).value;
  localStorage.setItem(STORAGE_KEY, url.value);
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

/** Always 'not-connected' in C.1. In C.2 this becomes
 *  reactive based on the actual /healthz response. */
const status = computed<'not-connected' | 'connecting' | 'connected'>(
  () => 'not-connected',
);
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="overlay" @click="onBackdrop">
      <div class="modal">
        <header class="modal-header">
          <div class="header-left">
            <span class="title">Controller Settings</span>
            <span class="phase-tag">Phase C.1 scaffold</span>
          </div>
          <button class="close" @click="emit('close')" aria-label="Close">✕</button>
        </header>

        <div class="body">
          <!-- Honest scaffold disclaimer at the very top -->
          <div class="scaffold-notice">
            <strong>This is a UI scaffold for Phase C.</strong>
            The connection isn't live yet. C.2 (next milestone)
            ships the real controller binary + makes this modal
            actually connect. See
            <a
              href="https://github.com/itsramananshul/SolFlow/blob/main/docs/dev/PHASE_C_ROADMAP.md"
              target="_blank"
              rel="noopener noreferrer"
              class="link"
            >Phase C Roadmap →</a>
            for the timeline.
          </div>

          <section class="settings-section">
            <label class="field">
              <span class="field-label">Controller URL</span>
              <input
                type="url"
                class="field-input"
                placeholder="http://localhost:3939 (C.2+)"
                :value="url"
                @input="onUrlInput"
              />
              <span class="field-help">
                The URL of a running SolFlow controller. Stored
                locally; never sent anywhere in C.1.
              </span>
            </label>
          </section>

          <section class="settings-section">
            <h3>Connection status</h3>
            <div class="status-row" :class="status">
              <span class="status-dot" :class="status" />
              <template v-if="status === 'not-connected'">
                Not connected — controller-mode execution isn't
                wired up until C.2.
              </template>
              <template v-else-if="status === 'connecting'">
                Connecting…
              </template>
              <template v-else>Connected</template>
            </div>
            <div class="proto-detail">
              <span class="subtle">Host-spec major version this editor supports:</span>
              <code>{{ HOST_SPEC_MAJOR }}</code>
            </div>
          </section>

          <section class="settings-section">
            <h3>Connectors (controller-managed)</h3>
            <div class="connectors-note">
              When connected, the controller exposes which
              connectors are configured (HTTP, etc.). Connector
              credentials NEVER leave the controller process.
            </div>
            <ul class="connectors-list">
              <li class="connector-row placeholder">
                <span class="conn-name">http</span>
                <span class="conn-desc">— HTTP/REST reference (lands in C.4)</span>
                <span class="conn-tag">not yet configured</span>
              </li>
            </ul>
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
                <span class="mode-tag">Today (Phase B)</span>
              </div>
              <div class="mode-row disabled">
                <span class="mode-dot" />
                <div class="mode-text">
                  <strong>controller-local</strong>
                  <span class="subtle">— canonical VM hosted by a controller on this machine</span>
                </div>
                <span class="mode-tag">C.2</span>
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
            Editor stays on browser-sim mode for all execution
            until controller integration lands.
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
  width: min(560px, 92vw);
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
  background: rgba(232, 166, 87, 0.16);
  color: var(--sf-warning);
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
  gap: 16px;
}

.scaffold-notice {
  background: rgba(232, 166, 87, 0.08);
  border: 1px solid rgba(232, 166, 87, 0.22);
  border-radius: 4px;
  padding: 10px 12px;
  font-size: 0.6875rem;
  line-height: 1.5;
  color: var(--sf-text-1);
}
.scaffold-notice strong { color: var(--sf-text-0); }

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
.field-help {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  font-style: italic;
}

.status-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.6875rem;
  padding: 4px 0;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.status-dot.not-connected { background: var(--sf-text-3); }
.status-dot.connecting    { background: var(--sf-warning); }
.status-dot.connected     { background: var(--sf-success); }

.proto-detail {
  margin-top: 6px;
  font-size: 0.6875rem;
}
.proto-detail code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-2);
  padding: 1px 6px;
  border-radius: 3px;
  color: var(--sf-text-0);
}

.connectors-note {
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  margin-bottom: 6px;
}
.connectors-list { list-style: none; padding: 0; margin: 0; }
.connector-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 4px 0;
  font-size: 0.6875rem;
}
.connector-row.placeholder { opacity: 0.6; }
.conn-name {
  font-family: var(--sf-font-mono);
  font-weight: 600;
  color: var(--sf-text-0);
}
.conn-desc { color: var(--sf-text-3); flex: 1; }
.conn-tag {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-warning);
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
.link {
  color: var(--sf-accent, #5d8acf);
  text-decoration: none;
}
.link:hover { color: var(--sf-text-0); text-decoration: underline; }
</style>
