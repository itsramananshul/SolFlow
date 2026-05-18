<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import Toolbar from '@/components/Toolbar.vue';
import FunctionTabs from '@/components/FunctionTabs.vue';
import Sidebar from '@/components/Sidebar.vue';
import Canvas from '@/components/Canvas.vue';
import Inspector from '@/components/Inspector.vue';
import SourcePreview from '@/components/SourcePreview.vue';
import DiagnosticsDrawer from '@/components/DiagnosticsDrawer.vue';
import RunModal from '@/components/RunModal.vue';
import StatusBar from '@/components/StatusBar.vue';
import HelpModal from '@/components/HelpModal.vue';

const graph = useGraphStore();
const ui = useUIStore();
const runOpen = ref(false);
const helpOpen = ref(false);

onMounted(() => {
  graph.bootstrap();
  window.addEventListener('keydown', onKey);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey);
});

function onKey(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey;
  // Cmd/Ctrl+Z → undo / Cmd+Shift+Z (or Ctrl+Y) → redo
  if (mod && e.key.toLowerCase() === 'z' && !e.shiftKey) {
    e.preventDefault();
    graph.undo();
    return;
  }
  if (mod && ((e.key.toLowerCase() === 'z' && e.shiftKey) || e.key.toLowerCase() === 'y')) {
    e.preventDefault();
    graph.redo();
    return;
  }
  // Cmd/Ctrl+S → download workflow JSON
  if (mod && e.key.toLowerCase() === 's') {
    e.preventDefault();
    downloadGraph();
    return;
  }
  // Cmd/Ctrl+Enter → run
  if (mod && e.key === 'Enter') {
    e.preventDefault();
    runOpen.value = true;
    return;
  }
  // Cmd/Ctrl+E → export .sol
  if (mod && e.key.toLowerCase() === 'e') {
    e.preventDefault();
    downloadSol();
    return;
  }
  // ? → help (with no modifier; skip if user is in an input)
  if (e.key === '?' && !mod) {
    const t = e.target as HTMLElement;
    if (
      t.tagName !== 'INPUT' &&
      t.tagName !== 'TEXTAREA' &&
      !t.isContentEditable
    ) {
      e.preventDefault();
      helpOpen.value = !helpOpen.value;
      return;
    }
  }
  // Esc → close any open modal / drawer
  if (e.key === 'Escape') {
    if (helpOpen.value) {
      helpOpen.value = false;
      return;
    }
    if (runOpen.value) {
      runOpen.value = false;
      return;
    }
    if (ui.drawerOpen) {
      ui.toggleDrawer();
      return;
    }
    ui.selectNode(null);
  }
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
</script>

<template>
  <div class="app">
    <Toolbar :run-open="runOpen" @open-run="runOpen = true" />
    <FunctionTabs />
    <div class="workspace">
      <Sidebar />
      <div class="canvas-region">
        <Canvas />
        <DiagnosticsDrawer v-if="ui.drawerOpen" />
      </div>
      <div class="right-pane">
        <Inspector />
        <SourcePreview />
      </div>
    </div>
    <StatusBar />
    <RunModal :open="runOpen" @close="runOpen = false" />
    <HelpModal :open="helpOpen" @close="helpOpen = false" />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}
.workspace {
  display: grid;
  grid-template-columns: minmax(220px, 16vw) 1fr minmax(360px, 28vw);
  flex: 1;
  min-height: 0;
}
@media (min-width: 2200px) {
  .workspace {
    grid-template-columns: 320px 1fr 520px;
  }
}
.canvas-region {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.right-pane {
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  min-height: 0;
}
</style>
