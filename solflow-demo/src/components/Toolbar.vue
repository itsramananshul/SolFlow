<script setup lang="ts">
import { ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { SAMPLES } from '@/samples';
import type { SolWorkflow } from '@/graph/schema';

const graph = useGraphStore();
const ui = useUIStore();

const fileInput = ref<HTMLInputElement | null>(null);
const sampleMenuOpen = ref(false);

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
    <div class="brand">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
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
    </div>

    <div class="actions">
      <button class="ghost" @click="newWorkflow">New</button>
      <button class="ghost" @click="openFilePicker">Open</button>
      <button class="ghost" @click="downloadGraph">Save</button>

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

      <button class="ghost" @click="ui.toggleDrawer">
        <span v-if="graph.diagnostics.length > 0" class="badge">
          {{ graph.diagnostics.length }}
        </span>
        Diagnostics
      </button>

      <div class="separator" />

      <button class="primary" @click="downloadSol">Export .sol</button>
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
  padding: 0 16px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
  height: 44px;
  flex-shrink: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--sf-text-0);
}
.brand svg {
  color: var(--sf-text-0);
}
.name {
  font-weight: 600;
  font-size: 13px;
  letter-spacing: -0.01em;
}
.version {
  font-family: var(--sf-font-mono);
  font-size: 10px;
  color: var(--sf-text-3);
  padding: 2px 6px;
  border: 1px solid var(--sf-border);
  border-radius: 3px;
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
  z-index: 20;
  min-width: 320px;
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
  font-size: 12px;
  color: var(--sf-text-0);
}
.menu-desc {
  font-size: 11px;
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
  font-size: 9px;
  font-weight: 600;
  padding: 1px 5px;
  border-radius: 8px;
  margin-right: 4px;
  font-family: var(--sf-font-mono);
}
</style>
