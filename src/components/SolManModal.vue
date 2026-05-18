<script setup lang="ts">
/**
 * Sol Man modal — prompt → generated workflow preview → apply / cancel.
 *
 * Four lifecycle states (driven by sol-man.store.status):
 *   idle        — prompt input, history, example prompts
 *   generating  — loading state with cancel-disabled
 *   preview     — show meta + node summary + assumptions + Apply/Insert
 *   error       — error message + retry, with special-case for
 *                  configMissing (no API key on the deployment)
 *
 * Apply paths:
 *   - "Apply as new workflow"     — replaces the user's workflow
 *   - "Insert into this function" — pastes at viewport center via
 *                                    graph.insertBlock (auto-frames
 *                                    multi-node clusters)
 *
 * Honest about being detached from the canvas during preview: the
 * spec is described, not rendered as ghost nodes. Phase A keeps the
 * modal-only preview to ship the AI experience without a deeper
 * ghost-rendering refactor; can revisit if Phase B/C UX research
 * shows users want it.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useSolManStore } from '@/stores/sol-man.store';
import { useVueFlow } from '@vue-flow/core';

const sm = useSolManStore();
const { getViewport, screenToFlowCoordinate } = useVueFlow();

defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const promptRef = ref<HTMLTextAreaElement | null>(null);

const EXAMPLES = [
  'When an order over $1000 comes in, send it for approval; otherwise auto-approve.',
  'When a payment webhook is received, validate the payload, update SAP, and notify finance.',
  'Every 5 minutes, check system health and alert the on-call team if unhealthy.',
  'When a new employee is created, provision their accounts in Slack, GitHub, and Notion.',
];

function autosize() {
  const ta = promptRef.value;
  if (!ta) return;
  ta.style.height = 'auto';
  ta.style.height = Math.min(220, Math.max(80, ta.scrollHeight)) + 'px';
}
watch(() => sm.prompt, () => nextTick(autosize));

function pickExample(text: string) {
  sm.prompt = text;
  nextTick(autosize);
  promptRef.value?.focus();
}

function reuseHistory(text: string) {
  sm.prompt = text;
  nextTick(autosize);
  promptRef.value?.focus();
}

async function onGenerate() {
  await sm.generate();
}

function onApplyAsNew() {
  sm.applyAsNewWorkflow();
  emit('close');
}

function onInsertHere() {
  // Aim at the viewport center in flow coords so the inserted cluster
  // lands where the user can see it, regardless of pan/zoom.
  const center = screenToFlowCoordinate({
    x: window.innerWidth / 2,
    y: window.innerHeight / 2,
  });
  void getViewport(); // touch viewport to ensure it's ready
  sm.insertIntoCurrent(center);
  emit('close');
}

function onClose() {
  // Don't blow away an in-progress preview — only reset when the user
  // explicitly cancels, applies, or closes the modal entirely.
  sm.reset();
  emit('close');
}

function onCancelPreview() {
  // Go back to the prompt screen but keep the typed prompt around in
  // case the user wants to tweak and re-generate.
  sm.status = 'idle';
  sm.spec = null;
}

const nodeCount = computed(() => sm.spec?.nodes.length ?? 0);
const frameCount = computed(() => sm.spec?.frames?.length ?? 0);
const noteCount = computed(() => sm.spec?.notes?.length ?? 0);
const assumptions = computed(() => sm.spec?.assumptions ?? []);

function nodeSummaryLine(): string {
  const counts: Record<string, number> = {};
  for (const n of sm.spec?.nodes ?? []) {
    counts[n.kind] = (counts[n.kind] ?? 0) + 1;
  }
  const parts: string[] = [];
  const order = [
    'trigger',
    'let',
    'branch',
    'while',
    'forEach',
    'call',
    'print',
    'assign',
    'return',
  ];
  for (const k of order) {
    if (counts[k]) {
      parts.push(`${counts[k]} ${k}${counts[k] > 1 ? 's' : ''}`);
    }
  }
  return parts.join(' · ');
}

// Esc → close
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    onClose();
  } else if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
    if (sm.status === 'idle' || sm.status === 'error') {
      e.preventDefault();
      void onGenerate();
    }
  }
}
onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => document.removeEventListener('keydown', onKey));

// Autofocus the textarea when the modal opens
watch(
  () => sm.status,
  (s) => {
    if (s === 'idle' || s === 'error') {
      nextTick(() => promptRef.value?.focus());
    }
  },
);

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) onClose();
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="backdrop" @click="onBackdrop">
      <div class="modal" @click.stop>
        <header class="modal-header">
          <div class="title-block">
            <span class="title">Sol Man ✨</span>
            <span class="sub">describe a workflow; get an editable graph</span>
          </div>
          <button class="ghost" :aria-label="'Close Sol Man'" @click="onClose">
            <svg viewBox="0 0 12 12" width="11" height="11" fill="none" aria-hidden="true">
              <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        </header>

        <!-- IDLE / ERROR : prompt screen -->
        <div v-if="sm.status === 'idle' || sm.status === 'error'" class="body">
          <textarea
            ref="promptRef"
            v-model="sm.prompt"
            class="prompt-input"
            placeholder="Describe what should happen. E.g. 'When an order over $1000 comes in, send it for approval; otherwise auto-approve.'"
            spellcheck="true"
            rows="4"
            @input="autosize"
          />
          <div class="row gen-row">
            <button
              class="primary gen-btn"
              :disabled="!sm.prompt.trim()"
              @click="onGenerate"
            >
              Generate workflow
            </button>
            <span class="kbd-hint">⌘↵ to generate</span>
          </div>

          <div v-if="sm.status === 'error' && sm.errorMessage" class="error-banner">
            <div class="error-title">
              <svg viewBox="0 0 16 16" width="11" height="11" fill="none" aria-hidden="true">
                <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.3" />
                <path d="M8 5 V8.5 M8 10.5 V11.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
              </svg>
              <span>{{ sm.configMissing ? 'Sol Man not configured' : 'Generation failed' }}</span>
            </div>
            <div class="error-body">{{ sm.errorMessage }}</div>
            <div v-if="sm.configMissing" class="error-hint">
              Set <code>ANTHROPIC_API_KEY</code> in your Vercel project's environment variables (or in <code>.env.local</code> for <code>vercel dev</code>), then redeploy / restart.
            </div>
          </div>

          <div v-if="sm.history.length > 0" class="section">
            <div class="section-head">Recent</div>
            <div class="chip-stack">
              <button
                v-for="h in sm.history"
                :key="h"
                type="button"
                class="history-chip"
                :title="h"
                @click="reuseHistory(h)"
              >{{ h }}</button>
            </div>
          </div>

          <div class="section">
            <div class="section-head">Examples</div>
            <div class="chip-stack">
              <button
                v-for="ex in EXAMPLES"
                :key="ex"
                type="button"
                class="example-chip"
                @click="pickExample(ex)"
              >{{ ex }}</button>
            </div>
          </div>
        </div>

        <!-- GENERATING -->
        <div v-else-if="sm.status === 'generating'" class="body generating">
          <div class="loading-block">
            <div class="loading-dots" aria-hidden="true">
              <span /><span /><span />
            </div>
            <div class="loading-title">Sol Man is composing your workflow…</div>
            <div class="loading-sub">This usually takes 10–25 seconds.</div>
          </div>
        </div>

        <!-- PREVIEW -->
        <div v-else-if="sm.status === 'preview' && sm.spec" class="body">
          <div class="preview-meta">
            <div class="preview-name">{{ sm.spec.meta.name }}</div>
            <div class="preview-desc">{{ sm.spec.meta.description }}</div>
          </div>
          <div class="preview-stats">
            <span class="stat">{{ nodeCount }} nodes</span>
            <span v-if="frameCount > 0" class="stat-sep">·</span>
            <span v-if="frameCount > 0" class="stat">{{ frameCount }} {{ frameCount === 1 ? 'section' : 'sections' }}</span>
            <span v-if="noteCount > 0" class="stat-sep">·</span>
            <span v-if="noteCount > 0" class="stat">{{ noteCount }} {{ noteCount === 1 ? 'note' : 'notes' }}</span>
          </div>
          <div class="preview-shape">{{ nodeSummaryLine() }}</div>

          <div v-if="assumptions.length > 0" class="section">
            <div class="section-head amber">Assumptions Sol Man made</div>
            <ul class="assumption-list">
              <li v-for="(a, i) in assumptions" :key="i">{{ a }}</li>
            </ul>
          </div>

          <div class="apply-row">
            <button class="primary" @click="onApplyAsNew">Apply as new workflow</button>
            <button class="ghost" @click="onInsertHere">Insert into this function</button>
            <button class="ghost" @click="onCancelPreview">Back to prompt</button>
          </div>
          <div v-if="sm.lastModel" class="footer-note">
            Generated by <code>{{ sm.lastModel }}</code>. The workflow is fully editable — change any field, disconnect any wire, save as a reusable block. Cmd+Z always undoes.
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.14s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  z-index: var(--sf-z-modal-top);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 8vh 24px 24px;
  backdrop-filter: blur(3px);
}
.modal {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  width: min(680px, 100%);
  max-height: min(86vh, 720px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.title-block {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.title {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--sf-text-0);
}
.sub {
  font-size: 0.625rem;
  color: var(--sf-text-3);
}
.body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 18px 18px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.body.generating {
  align-items: center;
  justify-content: center;
  min-height: 220px;
}

.prompt-input {
  width: 100%;
  font-family: var(--sf-font-sans);
  font-size: 0.875rem;
  line-height: 1.55;
  padding: 10px 12px;
  resize: vertical;
  min-height: 80px;
  max-height: 220px;
  border: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
  border-radius: var(--sf-radius-md);
}
.prompt-input:focus {
  border-color: var(--sf-accent);
  background: var(--sf-bg-2);
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.gen-row {
  justify-content: space-between;
}
.gen-btn {
  font-size: 0.8125rem;
  font-weight: 600;
  padding: 7px 14px;
}
.kbd-hint {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  letter-spacing: 0.4px;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.section-head {
  font-size: 0.5625rem;
  letter-spacing: 0.6px;
  text-transform: uppercase;
  color: var(--sf-text-3);
  font-weight: 600;
}
.section-head.amber {
  color: var(--sf-cat-trigger);
}

.chip-stack {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}
.history-chip,
.example-chip {
  text-align: left;
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 999px;
  padding: 5px 12px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.history-chip:hover,
.example-chip:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.example-chip {
  border-color: rgba(50, 145, 255, 0.18);
}

.error-banner {
  background: rgba(255, 77, 79, 0.07);
  border: 1px solid rgba(255, 77, 79, 0.32);
  border-radius: var(--sf-radius-md);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.error-title {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--sf-error);
}
.error-body {
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  line-height: 1.5;
}
.error-hint {
  font-size: 0.625rem;
  color: var(--sf-text-2);
  line-height: 1.55;
  background: rgba(255, 77, 79, 0.05);
  padding: 6px 8px;
  border-radius: var(--sf-radius-sm);
}
.error-hint code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 5px;
  border-radius: 3px;
  color: var(--sf-text-0);
}

/* Generating animation */
.loading-block {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}
.loading-dots {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.loading-dots span {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--sf-accent);
  animation: sm-bounce 1s ease-in-out infinite;
}
.loading-dots span:nth-child(2) { animation-delay: 0.15s; }
.loading-dots span:nth-child(3) { animation-delay: 0.3s; }
@keyframes sm-bounce {
  0%, 60%, 100% { transform: translateY(0); opacity: 0.4; }
  30%           { transform: translateY(-5px); opacity: 1; }
}
.loading-title {
  font-size: 0.875rem;
  color: var(--sf-text-0);
}
.loading-sub {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
}

/* Preview */
.preview-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.preview-name {
  font-size: 1rem;
  font-weight: 600;
  color: var(--sf-text-0);
}
.preview-desc {
  font-size: 0.75rem;
  color: var(--sf-text-1);
  line-height: 1.55;
}
.preview-stats {
  display: flex;
  align-items: center;
  gap: 6px;
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-2);
}
.stat-sep {
  color: var(--sf-text-3);
}
.preview-shape {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-3);
  letter-spacing: 0.3px;
}
.assumption-list {
  margin: 0;
  padding-left: 18px;
  font-size: 0.75rem;
  color: var(--sf-text-1);
  line-height: 1.55;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.assumption-list li {
  list-style: disc;
}
.apply-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.apply-row .primary {
  font-weight: 600;
}
.footer-note {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  line-height: 1.55;
}
.footer-note code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 5px;
  border-radius: 3px;
  color: var(--sf-text-2);
}
</style>
