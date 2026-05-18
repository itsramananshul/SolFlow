<script setup lang="ts">
/**
 * First-run welcome / gallery.
 *
 * Shown automatically the first time SolFlow loads on this browser
 * (no `solflow.welcome.dismissed` in localStorage). Re-openable from
 * the toolbar. Dismissed forever once the user picks an action.
 *
 * Three primary doors, each a card the user can click:
 *
 *   ↳ Start blank       — closes the welcome, leaves the auto-placed
 *                         Start node so the canvas is ready to author.
 *   ↳ Generate with AI  — opens Sol Man (existing modal).
 *   ↳ Browse samples    — picks any sample from samples/index.
 *
 * Plus an "Open file" path for users with a workflow they've already
 * saved, and a "Skip" link for those who want the empty canvas.
 *
 * This is a presentation surface — the first impression a CTO sees.
 * Keep copy crisp; let the cards do the talking.
 */
import { computed } from 'vue';
import { SAMPLES, type Sample } from '@/samples';
import { useGraphStore } from '@/stores/graph.store';

const graph = useGraphStore();

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'open-sol-man'): void;
  (e: 'open-file'): void;
}>();

function dismissForever() {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('solflow.welcome.dismissed', '1');
  }
}

function onSkip() {
  dismissForever();
  emit('close');
}

function onStartBlank() {
  // The bootstrap path already lays down an empty function with a
  // Start node; we don't need to call newWorkflow() if the user
  // already has one in flight. Just close the welcome.
  dismissForever();
  emit('close');
}

function onSolMan() {
  dismissForever();
  emit('open-sol-man');
  emit('close');
}

function onOpenFile() {
  dismissForever();
  emit('open-file');
  emit('close');
}

function onPickSample(s: Sample) {
  graph.loadWorkflow(s.build());
  dismissForever();
  emit('close');
}

// Sample cards take the existing curated SAMPLES list verbatim. The
// "enterprise" sample is the showpiece — let's surface it first so the
// CTO sees the multi-region orchestration before the smaller demos.
const orderedSamples = computed<Sample[]>(() => {
  const enterprise = SAMPLES.find((s) => s.id === 'enterprise');
  const rest = SAMPLES.filter((s) => s.id !== 'enterprise');
  return enterprise ? [enterprise, ...rest] : SAMPLES;
});
</script>

<template>
  <Transition name="welcome-fade">
    <div v-if="open" class="welcome-backdrop">
      <div class="welcome-shell">
        <header class="welcome-header">
          <div class="brand-row">
            <svg class="brand-mark" width="22" height="22" viewBox="0 0 24 24" fill="none">
              <path
                d="M6 6h8a4 4 0 010 8h-4a4 4 0 000 8h0"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
              />
              <circle cx="6" cy="6" r="2" fill="currentColor" />
              <circle cx="18" cy="22" r="2" fill="currentColor" />
            </svg>
            <div class="brand-text">
              <h1>SolFlow</h1>
              <p>Visual orchestration IDE for SOL.</p>
            </div>
          </div>
          <button class="skip-btn" type="button" @click="onSkip">
            Skip for now →
          </button>
        </header>

        <div class="welcome-tagline">
          What do you want to build today?
        </div>

        <!-- Primary action cards. AI gets the amber accent so the
             user's eye lands on it first; the blank option stays
             quiet at the end of the row. -->
        <div class="card-row">
          <button class="card primary-card ai-card" type="button" @click="onSolMan">
            <div class="card-icon">✨</div>
            <div class="card-title">Generate with AI</div>
            <div class="card-desc">
              Describe a workflow in plain English. Sol Man generates an editable graph
              you can refine on the canvas.
            </div>
            <div class="card-cta">Open Sol Man →</div>
          </button>

          <button class="card primary-card samples-card" type="button" @click="onPickSample(orderedSamples[0])">
            <div class="card-icon">⌘</div>
            <div class="card-title">Open a sample</div>
            <div class="card-desc">
              Five curated workflows ranging from a small two-function pipeline to a
              40-node enterprise orchestration with triggers, frames, and loops.
            </div>
            <div class="card-cta">Load "{{ orderedSamples[0]?.name ?? 'sample' }}" →</div>
          </button>

          <button class="card primary-card blank-card" type="button" @click="onStartBlank">
            <div class="card-icon">▢</div>
            <div class="card-title">Start blank</div>
            <div class="card-desc">
              Empty canvas with a Start node. Drag from the left palette to compose,
              or press Space to open Quick Add.
            </div>
            <div class="card-cta">Open empty canvas →</div>
          </button>
        </div>

        <!-- Secondary samples list — the "browse" view. -->
        <section class="samples-section">
          <header class="section-head">
            <span class="head-title">All samples</span>
            <span class="head-sub">Click any one to load. Cmd+Z always undoes.</span>
          </header>
          <div class="sample-grid">
            <button
              v-for="s in orderedSamples"
              :key="s.id"
              type="button"
              class="sample-card"
              @click="onPickSample(s)"
            >
              <div class="sample-name">{{ s.name }}</div>
              <div class="sample-desc">{{ s.description }}</div>
              <div class="sample-cta">Load →</div>
            </button>
          </div>
        </section>

        <footer class="welcome-footer">
          <button class="footer-link" type="button" @click="onOpenFile">
            Open a .solgraph.json file
          </button>
          <span class="footer-sep">·</span>
          <a
            class="footer-link"
            href="https://github.com/itsramananshul/SolFlow"
            target="_blank"
            rel="noreferrer noopener"
          >GitHub</a>
          <span class="footer-sep">·</span>
          <span class="footer-version">Phase A · v0.1</span>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.welcome-fade-enter-active,
.welcome-fade-leave-active {
  transition: opacity 0.2s ease;
}
.welcome-fade-enter-from,
.welcome-fade-leave-to {
  opacity: 0;
}
.welcome-backdrop {
  position: fixed;
  inset: 0;
  background: radial-gradient(
      120% 80% at 50% 0%,
      rgba(50, 145, 255, 0.05) 0%,
      transparent 70%
    ),
    var(--sf-bg-0);
  z-index: var(--sf-z-modal-top);
  overflow-y: auto;
  padding: clamp(24px, 6vh, 64px) clamp(16px, 4vw, 48px);
  display: flex;
  justify-content: center;
}
.welcome-shell {
  width: min(1100px, 100%);
  display: flex;
  flex-direction: column;
  gap: 36px;
}

/* Header */
.welcome-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.brand-row {
  display: flex;
  align-items: center;
  gap: 14px;
}
.brand-mark {
  color: var(--sf-text-0);
  flex-shrink: 0;
}
.brand-text h1 {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--sf-text-0);
}
.brand-text p {
  margin: 0;
  font-size: 0.75rem;
  color: var(--sf-text-2);
}
.skip-btn {
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  font-size: 0.75rem;
  cursor: pointer;
  padding: 6px 12px;
  border-radius: 999px;
  transition: color 0.12s ease, background 0.12s ease;
}
.skip-btn:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-2);
}

/* Big tagline */
.welcome-tagline {
  font-size: clamp(1.5rem, 3.2vw, 2.25rem);
  font-weight: 600;
  color: var(--sf-text-0);
  letter-spacing: -0.02em;
  line-height: 1.15;
}

/* Primary action cards */
.card-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 14px;
}
.card {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-lg);
  padding: 22px 22px 18px;
  text-align: left;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 8px;
  transition:
    background 0.16s ease,
    border-color 0.16s ease,
    transform 0.16s ease,
    box-shadow 0.16s ease;
  color: var(--sf-text-1);
  min-height: 188px;
}
.card:hover {
  background: var(--sf-bg-2);
  border-color: var(--sf-border-strong);
  transform: translateY(-2px);
  box-shadow: var(--sf-shadow-2);
}
.card:active {
  transform: translateY(0);
}
.card-icon {
  font-size: 1.5rem;
  line-height: 1;
  margin-bottom: 4px;
}
.card-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--sf-text-0);
  letter-spacing: -0.01em;
}
.card-desc {
  font-size: 0.75rem;
  color: var(--sf-text-2);
  line-height: 1.55;
  flex: 1;
}
.card-cta {
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--sf-text-1);
  margin-top: 4px;
  letter-spacing: 0.1px;
}
.card:hover .card-cta {
  color: var(--sf-accent);
}

/* AI card gets the amber-tinted treatment matching Sol Man's identity. */
.ai-card {
  border-color: rgba(232, 166, 87, 0.28);
  background: rgba(232, 166, 87, 0.04);
}
.ai-card:hover {
  border-color: rgba(232, 166, 87, 0.55);
  background: rgba(232, 166, 87, 0.08);
}
.ai-card .card-icon {
  filter: drop-shadow(0 0 8px rgba(232, 166, 87, 0.4));
}
.ai-card:hover .card-cta {
  color: var(--sf-cat-trigger);
}

/* Samples section */
.samples-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.section-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--sf-border);
}
.head-title {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.8px;
  text-transform: uppercase;
  color: var(--sf-text-2);
}
.head-sub {
  font-size: 0.625rem;
  color: var(--sf-text-3);
}
.sample-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 10px;
}
.sample-card {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-md);
  padding: 14px;
  text-align: left;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 5px;
  transition: background 0.12s ease, border-color 0.12s ease, transform 0.12s ease;
  color: var(--sf-text-1);
}
.sample-card:hover {
  background: var(--sf-bg-2);
  border-color: var(--sf-border-strong);
  transform: translateY(-1px);
}
.sample-name {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--sf-text-0);
}
.sample-desc {
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  line-height: 1.5;
  flex: 1;
}
.sample-cta {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  margin-top: 2px;
}
.sample-card:hover .sample-cta {
  color: var(--sf-accent);
}

/* Footer */
.welcome-footer {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--sf-border);
  font-size: 0.625rem;
  color: var(--sf-text-3);
}
.footer-link {
  background: transparent;
  border: none;
  color: var(--sf-text-2);
  cursor: pointer;
  font-size: 0.625rem;
  text-decoration: none;
  padding: 0;
}
.footer-link:hover {
  color: var(--sf-text-0);
  text-decoration: underline;
}
.footer-sep {
  color: var(--sf-text-3);
}
.footer-version {
  font-family: var(--sf-font-mono);
  letter-spacing: 0.3px;
}
</style>
