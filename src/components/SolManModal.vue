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
import { useSolManConfigStore } from '@/stores/sol-man-config.store';
import { useVueFlow } from '@vue-flow/core';

const sm = useSolManStore();
const cfg = useSolManConfigStore();
const { getViewport, screenToFlowCoordinate } = useVueFlow();

// Screen modes inside the modal: 'generate' is the prompt + preview
// flow we already had; 'settings' is the new BYO-key configuration
// panel.
type Screen = 'generate' | 'settings';
const screen = ref<Screen>('generate');

// On open, route to settings automatically when nothing is configured
// yet — both client (browser config) and server (env vars) absent.
// The server tells us via configMissing on first generate attempt;
// here on initial open we only know the CLIENT state.
function ensureScreenOnOpen() {
  if (!cfg.isConfigured && screen.value === 'generate') {
    // Stay on generate; first generate attempt will route to settings
    // if the server has no fallback env either. This avoids forcing
    // the panel on every open when env vars are providing a shared
    // key.
  }
}

// When the server returns configMissing, jump to settings so the user
// can fix it without hunting for the gear icon.
watch(
  () => sm.configMissing,
  (missing) => {
    if (missing) screen.value = 'settings';
  },
);

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

// Staged loading copy. The actual HTTP call is opaque from the
// client (no progress events), so this is a purely visual reassurance
// while the user waits 10-25s. The labels rotate every 4s and stop
// when the request completes.
const LOADING_STAGES = [
  'Composing nodes…',
  'Wiring up the flow…',
  'Reviewing assumptions…',
];
const loadingStageIndex = ref(0);
let loadingStageTimer: number | undefined;
function startLoadingStageRotation() {
  loadingStageIndex.value = 0;
  if (loadingStageTimer !== undefined) window.clearInterval(loadingStageTimer);
  loadingStageTimer = window.setInterval(() => {
    // Walk forward but stop at the last stage rather than wrapping
    // — looping back to "Composing…" would lie to the user about
    // restarting.
    if (loadingStageIndex.value < LOADING_STAGES.length - 1) {
      loadingStageIndex.value++;
    }
  }, 4500);
}
function stopLoadingStageRotation() {
  if (loadingStageTimer !== undefined) {
    window.clearInterval(loadingStageTimer);
    loadingStageTimer = undefined;
  }
}

watch(
  () => sm.isGenerating,
  (gen) => {
    if (gen) {
      startLoadingStageRotation();
    } else {
      stopLoadingStageRotation();
    }
  },
);

onBeforeUnmount(() => {
  stopLoadingStageRotation();
});

async function onGenerate() {
  await sm.generate();
}

// Reliability hardening: Retry button preserves the prompt + skips
// the silent auto-retry that runs inside `generate()` (a manual
// press becomes one call, not two).
async function onRetry() {
  await sm.retry();
}

// Copy a copy-pastable diagnostic blob to the clipboard. Used by
// the Copy error details button on the error banner; pastes cleanly
// into a bug report. Does NOT include the prompt or any API key.
const copyFlash = ref<'idle' | 'copied'>('idle');
async function onCopyErrorDetails() {
  try {
    await navigator.clipboard.writeText(sm.buildErrorDetailsBlob());
    copyFlash.value = 'copied';
    setTimeout(() => (copyFlash.value = 'idle'), 1500);
  } catch {
    // Clipboard API may be unavailable (sandboxed iframe, etc.) —
    // fall back to a toast so the user knows something happened.
    // Intentionally minimal; the modal already has plenty of UI.
    copyFlash.value = 'idle';
  }
}

// Friendly title per error kind. Defaults to "Generation failed".
const errorBannerTitle = computed(() => {
  if (sm.configMissing) return 'Sol Man has no provider configured';
  switch (sm.errorKind) {
    case 'gateway_timeout':
      return 'Sol Man timed out';
    case 'invalid_json':
      return 'Sol Man returned invalid JSON';
    case 'validation_failed':
      return 'Generated workflow failed validation';
    case 'empty_response':
      return 'Sol Man returned an empty response';
    case 'provider_error':
      return 'Provider rejected the request';
    case 'network':
      return 'Network error reaching Sol Man';
    case 'bad_request':
      return 'Request was rejected';
    default:
      return 'Generation failed';
  }
});

function onApplyAsNew(force = false) {
  const res = sm.applyAsNewWorkflow({ force });
  if (res.ok) emit('close');
}

function onInsertHere(force = false) {
  // Aim at the viewport center in flow coords so the inserted cluster
  // lands where the user can see it, regardless of pan/zoom.
  const center = screenToFlowCoordinate({
    x: window.innerWidth / 2,
    y: window.innerHeight / 2,
  });
  void getViewport(); // touch viewport to ensure it's ready
  const res = sm.insertIntoCurrent(center, { force });
  if (res.ok) emit('close');
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

// =============================================================
//  Settings screen state
// =============================================================
// Local form state; copied from the config store on open, written
// back on save so a half-edited field doesn't leak into the next
// generate attempt.

const PROVIDER_OPTIONS = [
  { id: 'anthropic', name: 'Anthropic Claude',          envKey: 'ANTHROPIC_API_KEY',  defaultModel: 'claude-sonnet-4-6',                        needsBase: false, note: '' },
  { id: 'openai',    name: 'OpenAI',                    envKey: 'OPENAI_API_KEY',     defaultModel: 'gpt-4o',                                    needsBase: false, note: '' },
  { id: 'gemini',    name: 'Google Gemini',             envKey: 'GEMINI_API_KEY',     defaultModel: 'gemini-2.0-flash',                          needsBase: false, note: '' },
  { id: 'grok',      name: 'xAI Grok',                  envKey: 'GROK_API_KEY',       defaultModel: 'grok-3',                                    needsBase: false, note: '' },
  { id: 'openrouter', name: 'OpenRouter',               envKey: 'OPENROUTER_API_KEY', defaultModel: 'meta-llama/llama-3.3-70b-instruct:free',    needsBase: false, note: 'Aggregator with many free models. Default is Llama 3.3 70B (free).' },
  { id: 'openai-compatible', name: 'OpenAI-compatible (custom)', envKey: 'SOL_MAN_API_KEY', defaultModel: '', needsBase: true, note: 'For local Ollama, Together, vLLM, or any OpenAI-protocol endpoint.' },
];

const formProvider = ref('');
const formApiKey = ref('');
const formModel = ref('');
const formBaseUrl = ref('');
const showKey = ref(false);
const saveFlash = ref<'idle' | 'saved'>('idle');

function syncFormFromStore() {
  formProvider.value = cfg.providerId || 'anthropic';
  formApiKey.value = cfg.apiKey;
  formModel.value = cfg.model;
  formBaseUrl.value = cfg.baseUrl;
}

watch(screen, (s) => {
  if (s === 'settings') {
    syncFormFromStore();
    showKey.value = false;
  }
});

const formProviderInfo = computed(() =>
  PROVIDER_OPTIONS.find((p) => p.id === formProvider.value) ?? PROVIDER_OPTIONS[0],
);

function onSaveConfig() {
  cfg.providerId = formProvider.value;
  cfg.apiKey = formApiKey.value.trim();
  cfg.model = formModel.value.trim();
  cfg.baseUrl = formBaseUrl.value.trim();
  cfg.save();
  saveFlash.value = 'saved';
  setTimeout(() => (saveFlash.value = 'idle'), 1200);
  // Reset error/config state so the next generate attempts cleanly.
  sm.errorMessage = null;
  sm.configMissing = false;
  // Hop back to generate so the user can immediately try their setup.
  screen.value = 'generate';
}

function onClearConfig() {
  if (!window.confirm('Forget your saved provider + API key on this browser?')) return;
  cfg.clear();
  syncFormFromStore();
}

const formIsComplete = computed<boolean>(() => {
  if (!formProvider.value) return false;
  if (!formApiKey.value.trim()) return false;
  if (formProviderInfo.value.needsBase) {
    if (!formBaseUrl.value.trim()) return false;
    if (!formModel.value.trim()) return false; // custom requires explicit model
  }
  return true;
});

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
    if (screen.value === 'generate' && (sm.status === 'idle' || sm.status === 'error')) {
      e.preventDefault();
      void onGenerate();
    }
  }
}
onMounted(() => {
  document.addEventListener('keydown', onKey);
  ensureScreenOnOpen();
});
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
            <span class="sub">
              <template v-if="screen === 'settings'">configure your AI provider</template>
              <template v-else>describe a workflow; get an editable graph</template>
            </span>
          </div>
          <div class="header-actions">
            <!-- Provider chip when configured — click to manage. Reads
                 "Claude · sk-…abcd" or "GPT · sk-…abcd" so the user
                 can see at a glance what's plugged in. -->
            <button
              v-if="screen === 'generate' && cfg.isConfigured"
              class="provider-chip"
              :title="`Using ${PROVIDER_OPTIONS.find((p) => p.id === cfg.providerId)?.name ?? cfg.providerId}. Click to change.`"
              :aria-label="'Manage Sol Man provider settings'"
              @click="screen = 'settings'"
            >
              <span class="chip-dot" />
              <span class="chip-name">
                {{ PROVIDER_OPTIONS.find((p) => p.id === cfg.providerId)?.name ?? cfg.providerId }}
              </span>
              <span class="chip-key">{{ cfg.maskedKey() }}</span>
            </button>
            <button
              v-if="screen === 'generate'"
              class="icon-btn"
              :aria-label="'Open Sol Man settings'"
              title="Provider settings"
              @click="screen = 'settings'"
            >
              <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
                <circle cx="8" cy="8" r="2.2" stroke="currentColor" stroke-width="1.3" />
                <path d="M8 1.5 V3.5 M8 12.5 V14.5 M1.5 8 H3.5 M12.5 8 H14.5 M3.5 3.5 L4.9 4.9 M11.1 11.1 L12.5 12.5 M3.5 12.5 L4.9 11.1 M11.1 4.9 L12.5 3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
              </svg>
            </button>
            <button
              v-if="screen === 'settings'"
              class="icon-btn"
              :aria-label="'Back to prompt'"
              title="Back"
              @click="screen = 'generate'"
            >
              <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
                <path d="M10 3 L5 8 L10 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
            <button class="icon-btn" :aria-label="'Close Sol Man'" @click="onClose">
              <svg viewBox="0 0 12 12" width="11" height="11" fill="none" aria-hidden="true">
                <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
              </svg>
            </button>
          </div>
        </header>

        <!-- SETTINGS — BYO-key configuration -->
        <div v-if="screen === 'settings'" class="body settings-body">
          <div class="settings-blurb">
            Sol Man uses your own API key. It's stored only in this browser's local
            storage; SolFlow proxies it to your chosen provider on each request and
            <strong>never logs or persists it server-side</strong>. Change providers
            or rotate keys any time.
          </div>

          <label class="field">
            <span class="field-label">Provider</span>
            <select v-model="formProvider">
              <option v-for="p in PROVIDER_OPTIONS" :key="p.id" :value="p.id">
                {{ p.name }}
              </option>
            </select>
          </label>

          <label class="field">
            <span class="field-label">
              API key
              <button
                type="button"
                class="link-btn"
                @click="showKey = !showKey"
              >{{ showKey ? 'Hide' : 'Show' }}</button>
            </span>
            <input
              v-model="formApiKey"
              :type="showKey ? 'text' : 'password'"
              autocomplete="off"
              spellcheck="false"
              :placeholder="`Paste your ${formProviderInfo.name} API key`"
            />
            <span class="field-hint">
              Stored in this browser only. Cleared with "Forget key" or by
              clearing site data.
            </span>
          </label>

          <label class="field">
            <span class="field-label">
              Model
              <span class="dim">(optional)</span>
            </span>
            <input
              v-model="formModel"
              type="text"
              spellcheck="false"
              :placeholder="
                formProviderInfo.defaultModel
                  ? `Default: ${formProviderInfo.defaultModel}`
                  : 'Required — e.g. anthropic/claude-3.5-sonnet for OpenRouter'
              "
            />
          </label>

          <label v-if="formProviderInfo.needsBase" class="field">
            <span class="field-label">Base URL</span>
            <input
              v-model="formBaseUrl"
              type="text"
              spellcheck="false"
              placeholder="https://openrouter.ai/api/v1 — or your Ollama / Together / custom endpoint"
            />
            <span class="field-hint">
              Must speak the OpenAI Chat Completions protocol
              (POST <code>/chat/completions</code>).
            </span>
          </label>

          <div v-if="cfg.savedAt" class="saved-stamp">
            Last saved {{ new Date(cfg.savedAt).toLocaleString() }}
          </div>

          <div class="settings-actions">
            <button
              class="primary"
              :disabled="!formIsComplete"
              @click="onSaveConfig"
            >
              <template v-if="saveFlash === 'saved'">✓ Saved</template>
              <template v-else>Save &amp; use</template>
            </button>
            <button
              v-if="cfg.isConfigured"
              class="ghost"
              @click="onClearConfig"
            >Forget key</button>
            <button class="ghost" @click="screen = 'generate'">Cancel</button>
          </div>
        </div>

        <!-- IDLE / ERROR : prompt screen -->
        <div
          v-if="screen === 'generate' && (sm.status === 'idle' || sm.status === 'error')"
          class="body"
        >
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
              <span>{{ errorBannerTitle }}</span>
              <span v-if="sm.errorKind" class="error-code">{{ sm.errorKind }}</span>
              <span v-if="sm.errorAttempts > 1" class="error-retries">
                · {{ sm.errorAttempts }} attempts
              </span>
            </div>
            <div class="error-body">{{ sm.errorMessage }}</div>

            <!-- Action row: Retry (when the kind is retryable AND
                 not a config issue) + Copy details (always, so
                 users can paste into a bug report). -->
            <div v-if="!sm.configMissing" class="error-actions">
              <button
                v-if="sm.errorRetryable"
                class="primary small"
                @click="onRetry"
              >
                Retry
              </button>
              <button class="ghost small" @click="onCopyErrorDetails">
                {{ copyFlash === 'copied' ? '✓ Copied' : 'Copy error details' }}
              </button>
            </div>

            <!-- Structured config screen when no provider is set.
                 Lists every supported provider with its env var so the
                 deployer can pick whichever LLM they prefer. -->
            <div
              v-if="sm.configMissing && sm.availableProviders.length > 0"
              class="provider-list"
            >
              <div class="provider-list-head">
                Set ONE of the following in your Vercel project environment
                (or <code>.env.local</code> for <code>vercel dev</code>):
              </div>
              <div
                v-for="p in sm.availableProviders"
                :key="p.id"
                class="provider-row"
              >
                <div class="provider-name">{{ p.name }}</div>
                <div class="provider-envs">
                  <code>{{ p.envKey }}</code>
                  <code v-if="p.envBase">+ {{ p.envBase }}</code>
                </div>
                <div class="provider-model" :title="`Default model. Override with SOL_MAN_MODEL.`">
                  <span v-if="p.defaultModel">default · <code>{{ p.defaultModel }}</code></span>
                  <span v-else class="dim">requires <code>SOL_MAN_MODEL</code></span>
                </div>
              </div>
              <div class="provider-hint">
                Auto-detection picks the first key set. To force a
                specific provider, also set <code>SOL_MAN_PROVIDER</code>
                (e.g. <code>openai</code>, <code>gemini</code>, <code>grok</code>).
              </div>
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
        <div
          v-else-if="screen === 'generate' && sm.status === 'generating'"
          class="body generating"
        >
          <div class="loading-block">
            <div class="loading-dots" aria-hidden="true">
              <span /><span /><span />
            </div>
            <div class="loading-title">
              Sol Man is working on it
              <span class="loading-ellipsis" aria-hidden="true">…</span>
            </div>
            <div class="loading-stage" aria-live="polite">
              {{ LOADING_STAGES[loadingStageIndex] }}
            </div>
            <div class="loading-progress" role="progressbar" aria-label="Generation in progress">
              <div class="loading-progress-bar" />
            </div>
            <div class="loading-sub">This usually takes 10–25 seconds.</div>
          </div>
        </div>

        <!-- PREVIEW -->
        <div
          v-else-if="screen === 'generate' && sm.status === 'preview' && sm.spec"
          class="body"
        >
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
            <span class="stat-sep">·</span>
            <span
              class="stat assumption-stat"
              :class="{ 'has-many': assumptions.length >= 3 }"
              :title="assumptions.length === 0
                ? 'Sol Man had enough detail to skip every assumption'
                : `Sol Man made ${assumptions.length} ${assumptions.length === 1 ? 'assumption' : 'assumptions'} — read them below before applying`"
            >{{ assumptions.length }} {{ assumptions.length === 1 ? 'assumption' : 'assumptions' }}</span>
            <template v-if="sm.lastAttempts > 1">
              <span class="stat-sep">·</span>
              <span
                class="stat recovered-stat"
                :title="`Sol Man auto-recovered after ${sm.lastAttempts - 1} retry attempt${sm.lastAttempts === 2 ? '' : 's'} — the first response failed parsing or semantic lint.`"
              >recovered ({{ sm.lastAttempts }} attempts)</span>
            </template>
            <template v-if="sm.lastRepairApplied">
              <span class="stat-sep">·</span>
              <span
                class="stat repaired-stat"
                title="Sol Man's server-side repair pass corrected at least one expression. See the assumptions list for what changed."
              >auto-repaired</span>
            </template>
          </div>
          <div class="preview-shape">{{ nodeSummaryLine() }}</div>

          <div v-if="assumptions.length > 0" class="section">
            <div class="section-head amber">
              Assumptions Sol Man made
              <span
                v-if="assumptions.length >= 3"
                class="assumption-count-badge"
                :title="`${assumptions.length} assumptions — review carefully before applying`"
              >{{ assumptions.length }}</span>
            </div>
            <div class="assumption-cards">
              <div
                v-for="(a, i) in assumptions"
                :key="i"
                class="assumption-card"
              >
                <div class="assumption-rail" aria-hidden="true" />
                <div class="assumption-body">{{ a }}</div>
              </div>
            </div>
          </div>
          <div v-else class="section assumption-empty">
            <span class="empty-glyph" aria-hidden="true">✓</span>
            <span class="empty-text">
              Sol Man had enough detail — no assumptions needed.
            </span>
          </div>

          <!-- Diagnostics gate. We translated the generated spec into a
               prospective workflow and ran the live validator against
               it. If there are errors we DO NOT apply silently; the
               user must either go back, or explicitly opt in to apply
               a draft they know is broken. -->
          <div
            v-if="sm.hasErrors || sm.hasWarnings"
            class="section diag-section"
            :class="{ 'diag-errors': sm.hasErrors, 'diag-warnings-only': !sm.hasErrors && sm.hasWarnings }"
          >
            <div class="section-head" :class="sm.hasErrors ? 'red' : 'amber'">
              <template v-if="sm.hasErrors">
                {{ sm.previewErrors.length }} error{{ sm.previewErrors.length === 1 ? '' : 's' }} in generated workflow
              </template>
              <template v-else>
                {{ sm.hasWarnings ? 'Generation notes' : '' }}
              </template>
            </div>
            <ul class="diag-list">
              <li
                v-for="(d, i) in sm.previewErrors"
                :key="`err-${i}`"
                class="diag diag-err"
              >
                <span class="diag-bullet">error</span>
                <span class="diag-msg">{{ d.message }}</span>
              </li>
              <li
                v-for="(d, i) in sm.previewWarningsDiagnostics"
                :key="`warn-${i}`"
                class="diag diag-warn"
              >
                <span class="diag-bullet">warn</span>
                <span class="diag-msg">{{ d.message }}</span>
              </li>
              <li
                v-for="(w, i) in sm.previewWarnings"
                :key="`repair-${i}`"
                class="diag diag-warn"
              >
                <span class="diag-bullet">repair</span>
                <span class="diag-msg">{{ w }}</span>
              </li>
            </ul>
            <div v-if="sm.hasBlockingErrors" class="diag-explainer">
              <strong>This draft cannot be applied.</strong> The errors
              flagged above (missing required inputs or unsafe inline
              expressions) would silently produce broken or dangerous
              SOL. Go back to the prompt with more detail about the
              missing pieces.
            </div>
            <div v-else-if="sm.hasErrors" class="diag-explainer">
              The generated graph won't run as-is. Sol Man recommends
              going back to the prompt and rewording — adding more
              detail about the actions usually fixes this — or applying
              the draft anyway and finishing it by hand on the canvas.
            </div>
          </div>

          <div class="apply-row">
            <template v-if="sm.hasBlockingErrors">
              <button class="ghost" @click="onCancelPreview">Back to prompt</button>
            </template>
            <template v-else-if="sm.hasErrors">
              <button class="ghost" @click="onCancelPreview">Back to prompt</button>
              <button class="primary danger" @click="onApplyAsNew(true)">
                Apply draft with errors
              </button>
              <button class="ghost danger-ghost" @click="onInsertHere(true)">
                Insert draft anyway
              </button>
            </template>
            <template v-else>
              <button class="primary" @click="onApplyAsNew(false)">Apply as new workflow</button>
              <button class="ghost" @click="onInsertHere(false)">Insert into this function</button>
              <button class="ghost" @click="onCancelPreview">Back to prompt</button>
            </template>
          </div>
          <div v-if="sm.lastModel" class="footer-note">
            Generated by
            <code v-if="sm.lastProvider">{{ sm.lastProvider.name }}</code>
            <code v-else>{{ sm.lastModel }}</code>
            <span v-if="sm.lastProvider"> · </span>
            <code v-if="sm.lastProvider">{{ sm.lastModel }}</code>.
            The workflow is fully editable — change any field, disconnect any wire, save as a reusable block. Cmd+Z always undoes.
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
.section-head.red {
  color: var(--sf-error);
}

/* Diagnostics panel inside the preview */
.diag-section {
  background: rgba(255, 77, 79, 0.05);
  border: 1px solid rgba(255, 77, 79, 0.25);
  border-radius: var(--sf-radius-md);
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.diag-section.diag-warnings-only {
  background: rgba(255, 184, 0, 0.05);
  border-color: rgba(255, 184, 0, 0.28);
}
.diag-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.diag {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  font-size: 0.6875rem;
  line-height: 1.5;
  color: var(--sf-text-1);
}
.diag-bullet {
  flex-shrink: 0;
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  padding: 1px 6px;
  border-radius: 999px;
  color: var(--sf-text-0);
}
.diag-err .diag-bullet {
  background: rgba(255, 77, 79, 0.18);
  color: var(--sf-error);
}
.diag-warn .diag-bullet {
  background: rgba(255, 184, 0, 0.18);
  color: var(--sf-cat-trigger);
}
.diag-msg {
  flex: 1;
}
.diag-explainer {
  font-size: 0.625rem;
  color: var(--sf-text-2);
  line-height: 1.55;
  padding-top: 4px;
  border-top: 1px dashed rgba(255, 77, 79, 0.2);
}
.primary.danger {
  background: var(--sf-error);
  color: white;
}
.primary.danger:hover {
  filter: brightness(1.08);
}
.ghost.danger-ghost {
  border-color: rgba(255, 77, 79, 0.4);
  color: var(--sf-error);
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
.error-code {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  font-weight: 500;
  color: var(--sf-error);
  background: rgba(255, 77, 79, 0.16);
  padding: 1px 6px;
  border-radius: 3px;
  margin-left: 4px;
}
.error-retries {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  font-weight: 400;
}
.error-actions {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}
.error-actions .small {
  font-size: 0.625rem;
  padding: 3px 10px;
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

/* Structured provider-config list shown when no API key is set. */
.provider-list {
  margin-top: 4px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-md);
  padding: 10px 12px;
}
.provider-list-head {
  font-size: 0.625rem;
  color: var(--sf-text-2);
  line-height: 1.5;
  margin-bottom: 2px;
}
.provider-list-head code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-1);
}
.provider-row {
  display: grid;
  grid-template-columns: 130px 1fr auto;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  border-top: 1px solid var(--sf-border);
}
.provider-row:first-of-type {
  border-top: none;
}
.provider-name {
  font-size: 0.6875rem;
  color: var(--sf-text-0);
  font-weight: 500;
}
.provider-envs {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.provider-envs code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 0.625rem;
  color: var(--sf-accent);
}
.provider-model {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  text-align: right;
}
.provider-model code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-2);
  font-size: 0.5625rem;
}
.provider-model .dim {
  font-style: italic;
}
.provider-hint {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  line-height: 1.5;
  margin-top: 4px;
  padding-top: 6px;
  border-top: 1px dashed var(--sf-border);
}
.provider-hint code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-2);
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
  display: inline-flex;
  align-items: baseline;
  gap: 2px;
}
.loading-ellipsis {
  display: inline-block;
  animation: sm-ellipsis 1.4s steps(4, end) infinite;
  width: 0.9em;
  text-align: left;
  overflow: hidden;
  color: var(--sf-text-3);
}
@keyframes sm-ellipsis {
  0%   { clip-path: inset(0 100% 0 0); }
  100% { clip-path: inset(0 0 0 0); }
}
.loading-stage {
  font-size: 0.75rem;
  color: var(--sf-accent);
  font-family: var(--sf-font-mono);
  letter-spacing: 0.2px;
  text-align: center;
  min-height: 1.1em;
  transition: color 0.18s ease;
}
.loading-progress {
  width: 220px;
  height: 3px;
  border-radius: 999px;
  background: var(--sf-bg-3);
  overflow: hidden;
}
.loading-progress-bar {
  height: 100%;
  width: 30%;
  background: linear-gradient(
    90deg,
    var(--sf-accent),
    color-mix(in srgb, var(--sf-accent) 60%, transparent)
  );
  border-radius: 999px;
  animation: sm-progress-sweep 1.8s ease-in-out infinite;
}
@keyframes sm-progress-sweep {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(330%); }
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
.assumption-cards {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.assumption-card {
  display: flex;
  background: rgba(255, 184, 0, 0.05);
  border: 1px solid rgba(255, 184, 0, 0.20);
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
}
.assumption-rail {
  width: 3px;
  background: var(--sf-cat-trigger);
  flex-shrink: 0;
}
.assumption-body {
  padding: 7px 10px;
  font-size: 0.75rem;
  color: var(--sf-text-1);
  line-height: 1.55;
  flex: 1;
  word-wrap: break-word;
}

/* Assumption-count chip in the stats line. Amber when many were
   made — visual signal "you should review these before applying"
   without taking up a separate row. */
.assumption-stat {
  color: var(--sf-text-2);
}
.assumption-stat.has-many {
  color: var(--sf-cat-trigger);
  font-weight: 500;
}

/* Phase A semantic-correctness pass — chip that lights up when
   the server's auto-retry or semantic-repair layer kicked in.
   Quiet but visible; the user can hover to see what happened. */
.recovered-stat {
  color: var(--sf-cat-action, var(--sf-accent, #5d8acf));
}
.repaired-stat {
  color: var(--sf-success);
  font-weight: 500;
}

/* Small pill that sits next to the "Assumptions Sol Man made"
   section header when the count is meaningful (≥ 3). Reinforces
   the count without forcing the user to scan the list. */
.assumption-count-badge {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 7px;
  background: rgba(255, 184, 0, 0.18);
  color: var(--sf-cat-trigger);
  border-radius: 999px;
  font-size: 0.5625rem;
  font-weight: 600;
  letter-spacing: 0;
  text-transform: none;
}

/* Affirming empty state — replaces the previously-hidden section
   when assumptions.length === 0. Quieter visual treatment than
   the cards (no rail, no panel) so it reads as a check, not as a
   warning. */
.assumption-empty {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 6px 10px;
  align-self: flex-start;
  border-radius: var(--sf-radius-sm);
  background: rgba(0, 204, 136, 0.05);
  border: 1px solid rgba(0, 204, 136, 0.18);
}
.assumption-empty .empty-glyph {
  font-size: 0.6875rem;
  color: var(--sf-success);
  font-weight: 700;
}
.assumption-empty .empty-text {
  font-size: 0.6875rem;
  color: var(--sf-text-1);
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

/* =================================================================
 *  Header actions — provider chip + icon buttons
 * ================================================================= */
.header-actions {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.provider-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 999px;
  padding: 3px 10px 3px 8px;
  font-size: 0.625rem;
  color: var(--sf-text-1);
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
}
.provider-chip:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.chip-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--sf-success);
}
.chip-name {
  font-weight: 500;
}
.chip-key {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
}
.icon-btn {
  width: 26px;
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 50%;
  color: var(--sf-text-2);
  cursor: pointer;
  padding: 0;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.icon-btn:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border);
}

/* =================================================================
 *  Settings screen
 * ================================================================= */
.settings-body {
  gap: 12px;
}
.settings-blurb {
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  line-height: 1.55;
  padding: 8px 10px;
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
}
.settings-blurb strong {
  color: var(--sf-text-0);
  font-weight: 600;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  font-weight: 500;
}
.field-label .dim {
  font-size: 0.5625rem;
  font-weight: 400;
  color: var(--sf-text-3);
}
.field-hint {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  line-height: 1.5;
}
.field-hint code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-2);
}
.link-btn {
  background: transparent;
  border: none;
  color: var(--sf-accent);
  font-size: 0.5625rem;
  cursor: pointer;
  padding: 0;
  text-decoration: underline;
}
.link-btn:hover {
  color: var(--sf-accent-hover);
}
.saved-stamp {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  font-family: var(--sf-font-mono);
}
.settings-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  padding-top: 4px;
  border-top: 1px dashed var(--sf-border);
}
.settings-actions .primary {
  font-weight: 600;
}
</style>
