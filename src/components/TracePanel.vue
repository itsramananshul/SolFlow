<script setup lang="ts">
/**
 * Floating Execution Trace debug window.
 *
 * A movable, viewport-clamped IDE panel (not a fixed slab) that renders the
 * run's execution trace independently of the Run panel, so the user can
 * keep it open while working the canvas. Supports a Floating mode and a
 * Dock-right mode. Clicking a row focuses the SOL source line; clicking the
 * canvas link focuses the node.
 */
import { computed, watch } from 'vue';
import { useTracePanelStore } from '@/stores/tracePanel.store';
import { useUIStore } from '@/stores/ui.store';
import { useGraphStore } from '@/stores/graph.store';
import { useDraggablePanel } from '@/composables/useDraggablePanel';

const trace = useTracePanelStore();
const ui = useUIStore();
const graph = useGraphStore();

const { panelRef, panelStyle, onHeaderPointerDown, recenter, bringToFront, open } =
  useDraggablePanel('trace', { width: 460, placement: 'right' });

// In dock-right mode the panel pins to the right edge full-height and is not
// draggable; in float mode it uses the draggable position.
const style = computed(() =>
  trace.dock === 'right'
    ? {
        position: 'fixed' as const,
        top: '52px',
        right: '8px',
        bottom: '8px',
        width: '340px',
        margin: '0',
        zIndex: '110',
      }
    : panelStyle.value,
);

// Re-place the panel when it opens in float mode.
watch(
  () => trace.open,
  (o) => { if (o && trace.dock === 'float') open(); },
);

function onRowLine(line: number | null) {
  if (line != null) ui.focusSourceLine(line);
}
function onRowCanvas(fnName: string | null, nodeId: string | null) {
  if (!nodeId) return;
  if (fnName) {
    const fn = graph.workflow.functions.find((f) => f.name === fnName);
    if (fn) graph.setActiveFunction(fn.id);
  }
  ui.requestFocus(nodeId);
}
function toggleDock() {
  trace.setDock(trace.dock === 'right' ? 'float' : 'right');
  if (trace.dock === 'float') open();
}
</script>

<template>
  <div
    v-if="trace.open"
    ref="panelRef"
    class="trace-window"
    :class="{ docked: trace.dock === 'right' }"
    :style="style"
    @pointerdown="bringToFront()"
  >
    <header
      class="tw-header"
      :class="{ 'drag-handle': trace.dock === 'float' }"
      @pointerdown="trace.dock === 'float' && onHeaderPointerDown($event)"
      @dblclick="trace.dock === 'float' && recenter()"
      :title="trace.dock === 'float' ? 'Drag to move · double-click to recenter' : ''"
    >
      <span class="tw-title">Execution Trace</span>
      <span class="tw-count" v-if="trace.rows.length">{{ trace.rows.length }} steps</span>
      <span v-if="trace.truncated" class="tw-trunc">truncated</span>
      <div class="tw-spacer" />
      <button class="tw-btn" @click="toggleDock" :title="trace.dock === 'right' ? 'Float' : 'Dock right'">
        {{ trace.dock === 'right' ? '↗ Float' : '⇥ Dock' }}
      </button>
      <button v-if="trace.dock === 'float'" class="tw-btn" @click="recenter()" title="Recenter">⊙</button>
      <button class="tw-btn" @click="trace.close()" title="Close trace">✕</button>
    </header>

    <div class="tw-body">
      <div v-if="trace.rows.length === 0" class="tw-empty">No execution trace.</div>
      <ul v-else class="tw-list">
        <li
          v-for="row in trace.rows"
          :key="row.index"
          class="tw-row"
          :class="{
            'is-call': row.kind === 'call',
            'is-return': row.kind === 'return',
            'is-extcall': row.kind === 'extcall',
            'is-extresult': row.kind === 'extresult',
            'is-error': row.kind === 'error',
          }"
          :style="{ paddingLeft: 8 + row.depth * 14 + 'px' }"
        >
          <span class="tw-step">#{{ row.index + 1 }}</span>
          <span class="tw-kind" :class="'k-' + row.kind">{{ row.kind }}</span>
          <span class="tw-fn" :title="`in ${row.fn}`">{{ row.fn }}</span>
          <button
            v-if="row.line !== null"
            class="tw-line no-drag"
            @click="onRowLine(row.line)"
            :title="`Show source line ${row.line}`"
          >line {{ row.line }}</button>
          <code class="tw-snippet">{{ row.snippet }}</code>
          <button
            v-if="row.nodeId"
            class="tw-canvas no-drag"
            @click="onRowCanvas(row.fnName, row.nodeId)"
            :title="`Focus on canvas (${row.fnName})`"
          >→</button>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.trace-window {
  display: flex;
  flex-direction: column;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  width: 460px;
  max-width: calc(100vw - 16px);
  max-height: min(70vh, calc(100vh - 16px));
  overflow: hidden;
}
.trace-window.docked {
  max-height: none;
  border-radius: var(--sf-radius-md);
}
.tw-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
  flex: 0 0 auto;
}
.tw-header.drag-handle { cursor: move; user-select: none; touch-action: none; }
.tw-header.drag-handle button { cursor: pointer; }
.tw-title { font-size: 0.75rem; font-weight: 600; color: var(--sf-text-0); }
.tw-count { font-size: 0.625rem; color: var(--sf-text-3); }
.tw-trunc {
  font-size: 0.5625rem;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(232, 166, 87, 0.18);
  color: var(--sf-warning);
}
.tw-spacer { flex: 1; }
.tw-btn {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-2);
  font-size: 0.625rem;
  padding: 2px 7px;
  border-radius: 4px;
  cursor: pointer;
}
.tw-btn:hover { color: var(--sf-text-0); border-color: var(--sf-border-strong); }
.tw-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--sf-bg-1);
}
.tw-empty { color: var(--sf-text-3); font-size: 0.75rem; font-style: italic; padding: 14px; }
.tw-list { list-style: none; margin: 0; padding: 0; }
.tw-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 3px 10px;
  font-size: 0.6875rem;
  font-family: var(--sf-font-mono);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
}
.tw-row.is-error { background: rgba(232, 110, 110, 0.10); }
.tw-row.is-extcall { background: rgba(220, 170, 90, 0.07); }
.tw-row.is-extresult { background: rgba(220, 170, 90, 0.04); }
.tw-row.is-call { background: rgba(120, 190, 140, 0.05); }
.tw-row.is-return { background: rgba(160, 160, 200, 0.04); }
.tw-step { color: var(--sf-text-3); flex: 0 0 auto; min-width: 34px; text-align: right; }
.tw-kind {
  flex: 0 0 auto;
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 1px 5px;
  border-radius: 8px;
  min-width: 58px;
  text-align: center;
  white-space: nowrap;
  background: rgba(255, 255, 255, 0.06);
  color: var(--sf-text-2);
}
.tw-kind.k-call { background: rgba(120, 190, 140, 0.20); color: #8fd6a6; }
.tw-kind.k-return { background: rgba(160, 160, 200, 0.18); color: #b3b3d8; }
.tw-kind.k-extcall { background: rgba(220, 170, 90, 0.24); color: #e8bf7a; }
.tw-kind.k-extresult { background: rgba(220, 170, 90, 0.16); color: #d8b683; }
.tw-kind.k-error { background: rgba(232, 110, 110, 0.22); color: #f0a0a0; }
.tw-fn {
  flex: 0 0 auto;
  color: var(--sf-text-3);
  max-width: 96px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tw-line {
  flex: 0 0 auto;
  background: transparent;
  border: none;
  color: var(--sf-accent);
  font: inherit;
  cursor: pointer;
  padding: 0;
}
.tw-line:hover { text-decoration: underline; }
.tw-snippet {
  color: var(--sf-text-0);
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tw-canvas {
  flex: 0 0 auto;
  background: rgba(98, 154, 220, 0.14);
  border: 1px solid rgba(98, 154, 220, 0.3);
  color: var(--sf-text-0);
  border-radius: 3px;
  font-size: 0.625rem;
  padding: 0 6px;
  cursor: pointer;
}
</style>
