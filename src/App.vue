<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
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
import Splitter from '@/components/Splitter.vue';
import SolManModal from '@/components/SolManModal.vue';
import WelcomeScreen from '@/components/WelcomeScreen.vue';
import { useSimulationStore } from '@/stores/simulation.store';
import { useBlocksStore } from '@/stores/blocks.store';
import { useSolManConfigStore } from '@/stores/sol-man-config.store';

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();
const blocks = useBlocksStore();
const solManConfig = useSolManConfigStore();
const runOpen = ref(false);
const helpOpen = ref(false);
const solManOpen = ref(false);
const welcomeOpen = ref(false);

/**
 * First-run welcome / gallery visibility.
 *
 * Auto-shown if localStorage doesn't yet contain
 * `solflow.welcome.dismissed`. Once dismissed, only re-opens via the
 * toolbar (Brand button → Show welcome). Stored as a string so the
 * presence of the key is the source of truth — its value is ignored.
 */
function maybeShowWelcomeOnMount() {
  if (typeof localStorage === 'undefined') return;
  const dismissed = localStorage.getItem('solflow.welcome.dismissed');
  if (!dismissed) {
    welcomeOpen.value = true;
  }
}

// =============================================================
//  Resizable layout — left sidebar / right panel / inspector split
// =============================================================
// Persistence keys; tied to the workspace shell so we never collide
// with feature-level localStorage entries.
const LS_LEFT = 'solflow.layout.leftWidth';
const LS_RIGHT = 'solflow.layout.rightWidth';
const LS_INSPECTOR_RATIO = 'solflow.layout.inspectorRatio';

const LEFT_MIN = 220;
const LEFT_MAX = 420;
const LEFT_DEFAULT = 260;

const RIGHT_MIN = 320;
const RIGHT_MAX = 700;
const RIGHT_DEFAULT = 420;

const INS_MIN = 0.2;
const INS_MAX = 0.85;
const INS_DEFAULT = 0.55;

function readNum(key: string, fallback: number): number {
  if (typeof localStorage === 'undefined') return fallback;
  const raw = localStorage.getItem(key);
  if (raw === null) return fallback;
  const n = Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return n;
}

const leftWidth = ref(readNum(LS_LEFT, LEFT_DEFAULT));
const rightWidth = ref(readNum(LS_RIGHT, RIGHT_DEFAULT));
const inspectorRatio = ref(readNum(LS_INSPECTOR_RATIO, INS_DEFAULT));

// Cap right panel against viewport so a previously-persisted huge size
// doesn't crush the canvas on a small screen. Re-evaluated whenever the
// browser resizes; reads from window each time so SSR-safe.
const viewportW = ref(typeof window !== 'undefined' ? window.innerWidth : 1440);
function onResize() {
  if (typeof window === 'undefined') return;
  viewportW.value = window.innerWidth;
}

const effectiveRightMax = computed(() =>
  Math.min(RIGHT_MAX, Math.floor(viewportW.value * 0.6)),
);
const effectiveLeftMax = computed(() =>
  Math.min(LEFT_MAX, Math.floor(viewportW.value * 0.32)),
);
const clampedLeft = computed(() =>
  Math.max(LEFT_MIN, Math.min(effectiveLeftMax.value, leftWidth.value)),
);
const clampedRight = computed(() =>
  Math.max(RIGHT_MIN, Math.min(effectiveRightMax.value, rightWidth.value)),
);

// Right-pane height in CSS px — passed to the inner Splitter so its
// fractional drag math knows the container size. Refreshed whenever
// the right pane DOM ref updates.
const rightPaneRef = ref<HTMLDivElement | null>(null);
const rightPaneHeight = ref(0);
function measureRightPane() {
  if (!rightPaneRef.value) return;
  rightPaneHeight.value = rightPaneRef.value.getBoundingClientRect().height;
}

watch(leftWidth, (n) => localStorage.setItem(LS_LEFT, String(Math.round(n))));
watch(rightWidth, (n) => localStorage.setItem(LS_RIGHT, String(Math.round(n))));
watch(inspectorRatio, (n) =>
  localStorage.setItem(LS_INSPECTOR_RATIO, n.toFixed(3)),
);

onMounted(() => {
  graph.bootstrap();
  blocks.bootstrap();
  solManConfig.bootstrap();
  window.addEventListener('keydown', onKey);
  window.addEventListener('resize', onResize);
  // Initial measure after the layout has settled.
  requestAnimationFrame(measureRightPane);
  // Welcome screen check runs AFTER bootstrap so the localStorage read
  // and dismissed-flag both have the same lifecycle.
  maybeShowWelcomeOnMount();
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey);
  window.removeEventListener('resize', onResize);
});

// Re-measure the right pane when its width changes (splitter drag).
watch(clampedRight, () => {
  requestAnimationFrame(measureRightPane);
});

/**
 * Selection-validity guard.
 *
 * After any graph change — undo, redo, multi-select delete, function
 * switch, Load workflow — the previously-selected or hovered node may
 * no longer exist. Without this guard, the Inspector renders against
 * a stale id and shows ghost data, or quick-actions act on phantom
 * nodes.
 *
 * Watches a string fingerprint of the active function's node ids so
 * the validation runs cheaply on every structural change without a
 * deep-watch sweep.
 */
const activeNodeFingerprint = computed(() => {
  const fn = graph.activeFunction;
  if (!fn) return '';
  return fn.nodes.map((n) => n.id).join('|');
});
watch(activeNodeFingerprint, (fingerprint) => {
  const ids = new Set(fingerprint ? fingerprint.split('|') : []);
  if (ui.selectedNodeId && !ids.has(ui.selectedNodeId)) {
    ui.selectNode(null);
  }
  if (ui.hoveredNodeId && !ids.has(ui.hoveredNodeId)) {
    ui.setHovered(null);
  }
});

function onKey(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey;
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
  if (mod && e.key.toLowerCase() === 's') {
    e.preventDefault();
    downloadGraph();
    return;
  }
  if (mod && e.key === 'Enter') {
    e.preventDefault();
    runOpen.value = true;
    return;
  }
  if (mod && e.key.toLowerCase() === 'e') {
    e.preventDefault();
    downloadSol();
    return;
  }
  // Cmd/Ctrl+J → open Sol Man (AI workflow generation)
  if (mod && e.key.toLowerCase() === 'j') {
    e.preventDefault();
    solManOpen.value = true;
    return;
  }
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
  if (e.key === 'Escape') {
    if (sim.isPlaying) {
      sim.cancel();
      return;
    }
    if (welcomeOpen.value) {
      welcomeOpen.value = false;
      return;
    }
    if (solManOpen.value) {
      solManOpen.value = false;
      return;
    }
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
    <Toolbar
      :run-open="runOpen"
      @open-run="runOpen = true"
      @open-help="helpOpen = true"
      @open-sol-man="solManOpen = true"
      @open-welcome="welcomeOpen = true"
    />
    <FunctionTabs />
    <div
      class="workspace"
      :style="{
        gridTemplateColumns: `${clampedLeft}px auto 1fr auto ${clampedRight}px`,
      }"
    >
      <Sidebar />
      <Splitter
        orientation="vertical"
        :size="leftWidth"
        :min="LEFT_MIN"
        :max="effectiveLeftMax"
        :default-size="LEFT_DEFAULT"
        @update:size="(v) => (leftWidth = v)"
      />
      <div class="canvas-region">
        <Canvas />
        <DiagnosticsDrawer v-if="ui.drawerOpen" />
      </div>
      <Splitter
        orientation="vertical"
        :size="rightWidth"
        :min="RIGHT_MIN"
        :max="effectiveRightMax"
        :default-size="RIGHT_DEFAULT"
        @update:size="(v) => (rightWidth = v)"
      />
      <div class="right-pane" ref="rightPaneRef">
        <div
          class="inspector-slot"
          :style="{ flexBasis: `${inspectorRatio * 100}%` }"
        >
          <Inspector />
        </div>
        <Splitter
          orientation="horizontal"
          :size="inspectorRatio"
          :min="INS_MIN"
          :max="INS_MAX"
          :default-size="INS_DEFAULT"
          :fraction="true"
          :container-px="rightPaneHeight"
          @update:size="(v) => (inspectorRatio = v)"
        />
        <div
          class="source-slot"
          :style="{ flexBasis: `${(1 - inspectorRatio) * 100}%` }"
        >
          <SourcePreview />
        </div>
      </div>
    </div>
    <StatusBar />
    <RunModal :open="runOpen" @close="runOpen = false" />
    <HelpModal :open="helpOpen" @close="helpOpen = false" />
    <SolManModal :open="solManOpen" @close="solManOpen = false" />
    <WelcomeScreen
      :open="welcomeOpen"
      @close="welcomeOpen = false"
      @open-sol-man="solManOpen = true"
      @open-file="welcomeOpen = false"
    />
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
  /* Columns: sidebar | splitter | canvas | splitter | right-pane.
     Sized inline so the px tracks come straight from reactive refs. */
  flex: 1;
  min-height: 0;
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
  min-width: 0;
  overflow: hidden;
}
.inspector-slot,
.source-slot {
  flex-grow: 0;
  flex-shrink: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>

<style>
/* Global helper applied to <body> during a splitter drag so text
   highlighting doesn't follow the cursor across the page. */
body.sf-splitter-drag {
  user-select: none;
  -webkit-user-select: none;
}
</style>
