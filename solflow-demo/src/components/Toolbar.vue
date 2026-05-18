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
      <span class="logo">▸</span>
      <span class="name">SolFlow</span>
      <span class="muted version">phase A</span>
    </div>

    <div class="actions">
      <button class="ghost" @click="newWorkflow" title="Start fresh">New</button>
      <button class="ghost" @click="openFilePicker" title="Load workflow JSON">
        Load
      </button>
      <button class="ghost" @click="downloadGraph" title="Save workflow JSON">
        Save
      </button>
      <div class="sample-dropdown">
        <button class="ghost" @click="toggleSampleMenu">Load sample ▾</button>
        <div v-if="sampleMenuOpen" class="dropdown-menu">
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
      <button class="primary" @click="downloadSol" title="Download canonical .sol">
        Export .sol
      </button>
      <button
        class="ghost"
        @click="ui.toggleDrawer"
        :title="ui.drawerOpen ? 'Hide diagnostics' : 'Show diagnostics'"
      >
        <span v-if="graph.diagnostics.length > 0" class="badge">
          {{ graph.diagnostics.length }}
        </span>
        Diagnostics
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
  padding: 8px 12px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
  height: 48px;
  flex-shrink: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.logo {
  color: var(--sf-accent);
  font-size: 18px;
  font-weight: bold;
}
.name {
  font-weight: 600;
  letter-spacing: 0.5px;
}
.version {
  font-size: 10px;
  opacity: 0.6;
}
.actions {
  display: flex;
  align-items: center;
  gap: 6px;
  position: relative;
}
.sample-dropdown {
  position: relative;
}
.dropdown-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  box-shadow: var(--sf-shadow-2);
  z-index: 10;
  min-width: 320px;
  overflow: hidden;
}
.menu-item {
  display: block;
  width: 100%;
  text-align: left;
  background: transparent;
  border: none;
  padding: 10px 12px;
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
  font-weight: 600;
  font-size: 12px;
  color: var(--sf-text-0);
}
.menu-desc {
  font-size: 11px;
  color: var(--sf-text-2);
  margin-top: 2px;
}
.file-hidden {
  display: none;
}
.badge {
  display: inline-block;
  background: var(--sf-error);
  color: white;
  font-size: 10px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 8px;
  margin-right: 4px;
}
</style>
