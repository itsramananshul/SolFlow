<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { SAMPLES } from '@/samples';
import type { SolWorkflow } from '@/graph/schema';

defineProps<{ runOpen: boolean }>();
const emit = defineEmits<{
  (e: 'open-run'): void;
  (e: 'open-help'): void;
  (e: 'open-sol-man'): void;
  (e: 'open-welcome'): void;
  (e: 'toggle-presentation'): void;
}>();

const graph = useGraphStore();
const ui = useUIStore();

const fileInput = ref<HTMLInputElement | null>(null);
const sampleMenuOpen = ref(false);
const modKey = ref<'⌘' | 'Ctrl'>('Ctrl');

onMounted(() => {
  if (typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)) {
    modKey.value = '⌘';
  }
  document.addEventListener('click', closeSampleMenuOnOutsideClick);
});
onBeforeUnmount(() => {
  document.removeEventListener('click', closeSampleMenuOnOutsideClick);
});

function closeSampleMenuOnOutsideClick(e: MouseEvent) {
  if (!sampleMenuOpen.value) return;
  const t = e.target as HTMLElement;
  if (!t.closest('.sample-dropdown')) sampleMenuOpen.value = false;
}

function openRun() {
  emit('open-run');
}
function openHelp() {
  emit('open-help');
}
function openSolMan() {
  emit('open-sol-man');
}
function openWelcome() {
  emit('open-welcome');
}
function togglePresentation() {
  emit('toggle-presentation');
}

function newWorkflow() {
  if (
    !confirm(
      'Start a new workflow? Your current draft will be cleared. (Tip: download it first if you want to keep it.)',
    )
  )
    return;
  graph.newWorkflow();
}

function downloadGraph() {
  const blob = new Blob([JSON.stringify(graph.workflow, null, 2)], {
    type: 'application/json',
  });
  triggerDownload(blob, `${graph.workflow.meta.name || 'workflow'}.solgraph.json`);
}

function downloadSol() {
  const blob = new Blob([graph.emitted.source], { type: 'text/plain' });
  triggerDownload(blob, `${graph.workflow.meta.name || 'workflow'}.sol`);
}

function triggerDownload(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function openFilePicker() {
  fileInput.value?.click();
}

async function onFileChosen(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    const parsed = JSON.parse(text) as SolWorkflow;
    if (parsed.schemaVersion !== 1 || !Array.isArray(parsed.functions)) {
      throw new Error('Not a valid SolFlow workflow file.');
    }
    graph.loadWorkflow(parsed);
  } catch (e) {
    alert(`Could not load workflow: ${(e as Error).message}`);
  } finally {
    input.value = '';
  }
}

function loadSample(id: string) {
  const sample = SAMPLES.find((s) => s.id === id);
  if (!sample) return;
  graph.loadWorkflow(sample.build());
  sampleMenuOpen.value = false;
}

function toggleSampleMenu() {
  sampleMenuOpen.value = !sampleMenuOpen.value;
}
</script>

<template>
  <header class="toolbar">
    <button
      class="brand"
      type="button"
      title="Open the welcome / gallery screen"
      aria-label="Open welcome screen"
      @click="openWelcome"
    >
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M6 6h8a4 4 0 010 8h-4a4 4 0 000 8h0"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        />
        <circle cx="6" cy="6" r="2" fill="currentColor" />
        <circle cx="18" cy="22" r="2" fill="currentColor" />
      </svg>
      <span class="name">SolFlow</span>
      <span class="version">v0.1</span>
    </button>

    <!--
      Workflow header — editable name + optional one-line description.
      Sits between the brand mark and the action cluster so the workflow
      identity is the visual anchor of the top bar, not the SolFlow logo.
    -->
    <div class="workflow-header">
      <input
        class="wf-name"
        :value="graph.workflow.meta.name"
        placeholder="Untitled workflow"
        spellcheck="false"
        @input="(e) => graph.updateWorkflowMeta({ name: (e.target as HTMLInputElement).value })"
      />
      <input
        class="wf-desc"
        :value="graph.workflow.meta.description ?? ''"
        placeholder="Add a description…"
        spellcheck="false"
        @input="(e) => graph.updateWorkflowMeta({ description: (e.target as HTMLInputElement).value })"
      />
    </div>

    <div class="actions">
      <button class="ghost" @click="newWorkflow">New</button>
      <button class="ghost" @click="openFilePicker">Open</button>
      <button class="ghost" @click="downloadGraph" :title="`Save workflow JSON (${modKey}+S)`">
        Save
      </button>

      <div class="separator" />

      <button
        class="ghost icon-btn"
        :disabled="!graph.canUndo()"
        @click="graph.undo()"
        :title="`Undo (${modKey}+Z)`"
        aria-label="Undo"
      >
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none">
          <path d="M3 8 L8 8 A4 4 0 1 1 4 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M5 6 L3 8 L5 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>
      <button
        class="ghost icon-btn"
        :disabled="!graph.canRedo()"
        @click="graph.redo()"
        :title="`Redo (${modKey}+Shift+Z)`"
        aria-label="Redo"
      >
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none">
          <path d="M13 8 L8 8 A4 4 0 1 0 12 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M11 6 L13 8 L11 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <div class="separator" />

      <div class="sample-dropdown">
        <button class="ghost" @click="toggleSampleMenu">
          Samples
          <svg viewBox="0 0 8 5" width="8" height="5" style="margin-left: 4px;">
            <path d="M1 1l3 3 3-3" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" />
          </svg>
        </button>
        <div v-if="sampleMenuOpen" class="dropdown-menu" @click.stop>
          <button
            v-for="s in SAMPLES"
            :key="s.id"
            class="menu-item"
            @click="loadSample(s.id)"
          >
            <div class="menu-title">{{ s.name }}</div>
            <div class="menu-desc">{{ s.description }}</div>
          </button>
        </div>
      </div>

      <button
        class="ghost diag-btn"
        :class="{ active: ui.drawerOpen, 'has-issues': graph.diagnostics.length > 0 }"
        @click="ui.toggleDrawer"
      >
        <span v-if="graph.diagnostics.length > 0" class="badge">
          {{ graph.diagnostics.length }}
        </span>
        Diagnostics
      </button>

      <div class="separator" />

      <button class="ghost" @click="downloadSol" :title="`Export .sol (${modKey}+E)`">
        Export .sol
      </button>
      <button
        class="ghost sol-man-btn"
        @click="openSolMan"
        :title="`Sol Man — AI workflow generation (${modKey}+J)`"
      >
        <span class="sm-sparkle" aria-hidden="true">✨</span>
        Sol Man
      </button>
      <button
        class="primary run-btn"
        @click="openRun"
        :title="`Run workflow (${modKey}+Enter)`"
      >
        <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor" style="margin-right: 5px;">
          <path d="M3 2 L10 6 L3 10 Z" />
        </svg>
        Run
      </button>

      <button
        class="ghost icon-btn"
        @click="togglePresentation"
        title="Presentation mode (P) — hide chrome for demos"
        aria-label="Toggle presentation mode"
      >
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
          <rect x="2" y="3" width="12" height="9" rx="1" stroke="currentColor" stroke-width="1.4" />
          <path d="M5 14 H11" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
      <button class="ghost icon-btn help-btn" @click="openHelp" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none">
          <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.4" />
          <path d="M6 6.5a2 2 0 1 1 3 1.6c-.7.4-1 .8-1 1.4M8 12v.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
    </div>

    <input
      ref="fileInput"
      type="file"
      accept=".json,application/json"
      class="file-hidden"
      @change="onFileChosen"
    />
  </header>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 clamp(8px, 1.5vw, 16px);
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
  height: clamp(36px, 3.2vw, 44px);
  flex-shrink: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--sf-text-0);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--sf-radius-sm);
  padding: 4px 8px;
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease;
}
.brand:hover {
  background: var(--sf-bg-2);
  border-color: var(--sf-border);
}
.brand svg {
  color: var(--sf-text-0);
}
.name {
  font-weight: 600;
  font-size: 0.8125rem;
  letter-spacing: -0.01em;
}
.version {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-3);
  padding: 2px 6px;
  border: 1px solid var(--sf-border);
  border-radius: 3px;
}
.workflow-header {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  margin: 0 clamp(8px, 2vw, 24px);
  min-width: 0;
}
.wf-name,
.wf-desc {
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--sf-radius-sm);
  outline: none;
  text-align: center;
  width: 100%;
  max-width: 480px;
  padding: 2px 8px;
  font-family: var(--sf-font-sans);
}
.wf-name {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--sf-text-0);
  letter-spacing: -0.005em;
}
.wf-desc {
  font-size: 0.625rem;
  color: var(--sf-text-2);
  margin-top: 1px;
}
.wf-name:hover,
.wf-desc:hover {
  border-color: var(--sf-border);
}
.wf-name:focus,
.wf-desc:focus {
  border-color: var(--sf-accent);
  background: var(--sf-bg-1);
}
.wf-name::placeholder,
.wf-desc::placeholder {
  color: var(--sf-text-3);
  font-weight: 400;
}
.actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
.separator {
  width: 1px;
  height: 16px;
  background: var(--sf-border);
  margin: 0 6px;
}
.sample-dropdown {
  position: relative;
}
.dropdown-menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-3);
  z-index: var(--sf-z-popover);
  /* Fluid width so the samples dropdown doesn't push past the toolbar
     edge on narrow viewports. */
  min-width: clamp(240px, 22vw, 320px);
  max-width: min(360px, 80vw);
  overflow: hidden;
}
.menu-item {
  display: block;
  width: 100%;
  text-align: left;
  background: transparent;
  border: none;
  padding: 10px 14px;
  border-radius: 0;
  border-bottom: 1px solid var(--sf-border);
  cursor: pointer;
}
.menu-item:last-child {
  border-bottom: none;
}
.menu-item:hover {
  background: var(--sf-bg-3);
}
.menu-title {
  font-weight: 500;
  font-size: 0.75rem;
  color: var(--sf-text-0);
}
.menu-desc {
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  margin-top: 2px;
  line-height: 1.4;
}
.file-hidden {
  display: none;
}
.badge {
  display: inline-block;
  background: var(--sf-error);
  color: white;
  font-size: 0.5625rem;
  font-weight: 600;
  padding: 1px 5px;
  border-radius: 8px;
  margin-right: 4px;
  font-family: var(--sf-font-mono);
}
.run-btn {
  display: inline-flex;
  align-items: center;
  font-weight: 600;
}
.sol-man-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--sf-text-0);
  border-color: rgba(232, 166, 87, 0.32);
  background: rgba(232, 166, 87, 0.06);
}
.sol-man-btn:hover {
  background: rgba(232, 166, 87, 0.14);
  border-color: rgba(232, 166, 87, 0.5);
}
.sm-sparkle {
  font-size: 0.6875rem;
}
.diag-btn.active {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
}
.diag-btn.has-issues {
  border-color: rgba(255, 77, 79, 0.3);
}
.icon-btn {
  padding: 5px 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
}
.icon-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
  background: transparent;
}
.icon-btn:disabled:hover {
  background: transparent;
  border-color: var(--sf-border);
}
</style>
