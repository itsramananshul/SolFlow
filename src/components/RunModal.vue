<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useSimulationStore } from '@/stores/simulation.store';
import { recordTrace, type Trace } from '@/runtime/simulate';

const graph = useGraphStore();
const sim = useSimulationStore();

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const trace = ref<Trace | null>(null);
const isRunning = ref(false);
const result = computed(() => trace.value?.result ?? null);

const tabs = ['output', 'sol'] as const;
type Tab = (typeof tabs)[number];
const activeTab = ref<Tab>('output');

function execute() {
  isRunning.value = true;
  // Defer so the UI updates before the synchronous interpreter runs.
  setTimeout(() => {
    try {
      trace.value = recordTrace(graph.workflow);
      // Kick off canvas playback alongside the modal display.
      if (trace.value) sim.play(trace.value);
    } finally {
      isRunning.value = false;
    }
  }, 50);
}

function replay() {
  if (trace.value) sim.play(trace.value);
}

const copyState = ref<'idle' | 'copied'>('idle');

async function copyOutput() {
  if (!result.value) return;
  const text = result.value.output.join('\n');
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
            <span class="subtle">in-browser interpreter · Phase A</span>
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
            {{ t === 'output' ? 'Output' : 'Generated SOL' }}
          </button>
          <div class="tab-spacer" />
          <div class="status" v-if="result">
            <span
              class="status-dot"
              :class="result.ok ? 'ok' : 'err'"
            />
            {{ result.ok ? 'completed' : 'failed' }}
            <span class="subtle">
              · {{ result.steps }} steps · {{ result.durationMs }}ms
            </span>
          </div>
        </nav>

        <main class="body">
          <!-- Output tab -->
          <section v-if="activeTab === 'output'" class="pane">
            <div v-if="isRunning" class="empty">Running…</div>
            <template v-else-if="result">
              <div v-if="result.error" class="error">
                <strong>Runtime error</strong>
                <div class="error-msg">{{ result.error }}</div>
              </div>
              <div v-if="result.output.length === 0 && !result.error" class="empty">
                Program ran with no print output.
              </div>
              <div v-if="result.output.length > 0" class="output-block">
                <div class="output-toolbar">
                  <span class="output-label">stdout · {{ result.output.length }} {{ result.output.length === 1 ? 'line' : 'lines' }}</span>
                  <button class="ghost" @click="copyOutput">
                    {{ copyState === 'copied' ? '✓ Copied' : 'Copy' }}
                  </button>
                </div>
                <div class="output-rows">
                  <div v-for="(line, i) in result.output" :key="i" class="output-row">
                    <span class="row-num">{{ String(i + 1).padStart(2, ' ') }}</span>
                    <span class="row-text">{{ line }}</span>
                  </div>
                </div>
              </div>
              <div v-if="result.returnValue !== undefined" class="return-row">
                <span class="subtle">return:</span>
                <code>{{ formatReturn(result.returnValue) }}</code>
              </div>
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
            ><span class="ln">{{ String(i + 1).padStart(2, ' ') }}</span>{{ line }}<br></span></pre>
          </section>
        </main>

        <footer class="modal-footer">
          <span class="subtle">
            Phase A interpreter: walks the wired graph and evaluates inline
            expressions client-side. The real SOL VM runs server-side in
            Phase B.
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
  width: min(720px, 100%);
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
