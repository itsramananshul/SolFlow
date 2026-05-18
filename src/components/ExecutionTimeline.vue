<script setup lang="ts">
/**
 * Sliding execution timeline. Anchored to the right edge of the
 * canvas, visible while a trace is loaded. Lists meaningful events
 * (enters / branch decisions / errors / returns) chronologically.
 * Skips the noisy ones (every exit and every edge traversal) so the
 * timeline reads as a story rather than a debug log.
 *
 * Each row shows the step index, an icon for the event kind, the
 * node label, and the runtime summary (for value events). Clicking
 * a row pauses playback, seeks the simulation state to just after
 * that event, and pans the canvas to the relevant node.
 */
import { computed, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useSimulationStore } from '@/stores/simulation.store';
import { useUIStore } from '@/stores/ui.store';
import type { StepEvent } from '@/runtime/simulate';
import type { GraphNode } from '@/graph/schema';

const graph = useGraphStore();
const sim = useSimulationStore();
const ui = useUIStore();

const collapsed = ref(false);
const listRef = ref<HTMLDivElement | null>(null);

interface TimelineRow {
  index: number; // index in the original trace.events array
  kind: 'enter' | 'value' | 'error' | 'return';
  summary: string;
  nodeId: string;
  nodeLabel: string;
}

function nodeShortLabel(n: GraphNode): string {
  const d = n.data;
  switch (d.kind) {
    case 'start':       return 'start()';
    case 'trigger':     return `${d.triggerKind} trigger`;
    case 'let':         return `let ${d.varName}`;
    case 'assign':      return `${d.varName} =`;
    case 'print':       return 'print';
    case 'return':      return 'return';
    case 'branch':      return 'branch';
    case 'while':       return 'while';
    case 'forEach':     return `for ${d.iteratorName}`;
    case 'binaryOp':    return `op ${d.op}`;
    case 'unaryOp':     return `op ${d.op}`;
    case 'varGet':      return d.varName || 'varGet';
    case 'literal':     return `${d.value}`;
    case 'arrayLiteral':  return `array[${d.length}]`;
    case 'structLiteral': return d.structName || 'struct';
    case 'fieldAccess': return `.${d.fieldName}`;
    case 'fieldSet':    return `.${d.fieldName} =`;
    case 'indexRead':   return 'arr[i]';
    case 'indexSet':    return 'arr[i] =';
    case 'enumVariant': return `${d.enumName}::${d.variantName}`;
    case 'call':        return 'call()';
    case 'note':        return 'note';
    case 'frame':       return d.title || 'Section';
  }
}

const rows = computed<TimelineRow[]>(() => {
  const trace = sim.loadedTrace;
  if (!trace) return [];
  const fn = graph.activeFunction;
  if (!fn) return [];
  const nodeMap = new Map<string, GraphNode>();
  for (const n of fn.nodes) nodeMap.set(n.id, n);

  const out: TimelineRow[] = [];
  trace.events.forEach((ev, i) => {
    let row: TimelineRow | null = null;
    switch (ev.type) {
      case 'enter': {
        const n = nodeMap.get(ev.id);
        if (!n) return;
        // Suppress enters for purely transient nodes — start fires
        // an enter but offers no insight; same for trigger entries.
        // Other enters become the row header for the value event
        // that follows.
        if (n.data.kind === 'start') {
          row = { index: i, kind: 'enter', summary: 'execution started', nodeId: ev.id, nodeLabel: 'start' };
        }
        break;
      }
      case 'value': {
        const n = nodeMap.get(ev.id);
        if (!n) return;
        const label = nodeShortLabel(n);
        row = {
          index: i,
          kind: n.data.kind === 'return' ? 'return' : 'value',
          summary: ev.summary,
          nodeId: ev.id,
          nodeLabel: label,
        };
        break;
      }
      case 'error': {
        const n = nodeMap.get(ev.id);
        if (!n) return;
        row = {
          index: i,
          kind: 'error',
          summary: ev.message,
          nodeId: ev.id,
          nodeLabel: nodeShortLabel(n),
        };
        break;
      }
      default:
        return;
    }
    if (row) out.push(row);
  });
  return out;
});

/**
 * Index of the timeline row that's currently "active" — the most
 * recently-applied event at or before sim.stepIndex. Auto-scrolled
 * into view so the user can follow the live cursor.
 */
const activeRowIdx = computed<number>(() => {
  const list = rows.value;
  let last = -1;
  for (let i = 0; i < list.length; i++) {
    if (list[i].index < sim.stepIndex) last = i;
    else break;
  }
  return last;
});

watch(activeRowIdx, () => {
  if (collapsed.value) return;
  // Scroll the active row into view as playback advances.
  requestAnimationFrame(() => {
    const list = listRef.value;
    if (!list) return;
    const el = list.querySelector(`.timeline-row[data-i="${activeRowIdx.value}"]`);
    if (el && 'scrollIntoView' in el) {
      (el as HTMLElement).scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  });
});

function onRowClick(row: TimelineRow) {
  sim.pause();
  ui.requestFocus(row.nodeId);
}

function iconFor(kind: TimelineRow['kind']): string {
  switch (kind) {
    case 'enter':  return '▸';
    case 'value':  return '·';
    case 'return': return '⏎';
    case 'error':  return '!';
  }
}
</script>

<template>
  <Transition name="timeline-slide">
    <aside v-if="sim.hasTrace" class="timeline" :class="{ collapsed }">
      <div class="timeline-header" @click="collapsed = !collapsed">
        <span class="timeline-title">Trace</span>
        <span class="timeline-count">{{ rows.length }} steps</span>
        <button
          class="collapse-btn"
          type="button"
          :aria-label="collapsed ? 'Expand execution timeline' : 'Collapse execution timeline'"
          @click.stop="collapsed = !collapsed"
        >
          {{ collapsed ? '◂' : '▸' }}
        </button>
      </div>
      <div v-if="!collapsed" ref="listRef" class="timeline-body">
        <div v-if="rows.length === 0" class="timeline-empty">
          No events captured for this run.
        </div>
        <button
          v-for="(row, i) in rows"
          :key="`${row.index}:${i}`"
          :data-i="i"
          type="button"
          class="timeline-row"
          :class="[
            `kind-${row.kind}`,
            { active: i === activeRowIdx },
          ]"
          @click="onRowClick(row)"
        >
          <span class="row-step">{{ i + 1 }}</span>
          <span class="row-icon">{{ iconFor(row.kind) }}</span>
          <span class="row-node">{{ row.nodeLabel }}</span>
          <span class="row-summary">{{ row.summary }}</span>
        </button>
      </div>
    </aside>
  </Transition>
</template>

<style scoped>
.timeline {
  position: absolute;
  right: 12px;
  top: 12px;
  bottom: 72px; /* leaves room for ExecutionControls strip */
  width: clamp(220px, 22vw, 320px);
  background: rgba(10, 10, 10, 0.94);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-3);
  backdrop-filter: blur(6px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  z-index: var(--sf-z-ambient);
  font-size: 0.6875rem;
}
.timeline.collapsed {
  width: max-content;
  bottom: auto;
}
.timeline-slide-enter-active,
.timeline-slide-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.timeline-slide-enter-from,
.timeline-slide-leave-to {
  opacity: 0;
  transform: translateX(6px);
}

.timeline-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  cursor: pointer;
  user-select: none;
}
.timeline.collapsed .timeline-header {
  border-bottom: none;
}
.timeline-title {
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  font-size: 0.625rem;
  color: var(--sf-text-1);
}
.timeline-count {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  flex: 1;
}
.collapse-btn {
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  cursor: pointer;
  padding: 0 4px;
}
.collapse-btn:hover {
  color: var(--sf-text-0);
}

.timeline-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 6px;
}
.timeline-empty {
  padding: 16px 12px;
  color: var(--sf-text-3);
  font-style: italic;
  text-align: center;
}
.timeline-row {
  display: grid;
  grid-template-columns: 18px 12px 1fr;
  grid-template-rows: auto auto;
  column-gap: 6px;
  row-gap: 0;
  width: 100%;
  background: transparent;
  border: none;
  border-left: 2px solid transparent;
  padding: 5px 7px;
  border-radius: var(--sf-radius-sm);
  cursor: pointer;
  text-align: left;
  color: var(--sf-text-2);
  margin-bottom: 1px;
}
.timeline-row:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.timeline-row.active {
  background: var(--sf-accent-dim);
  color: var(--sf-text-0);
  border-left-color: var(--sf-accent);
}
.row-step {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  grid-row: 1 / span 2;
  align-self: center;
}
.row-icon {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  text-align: center;
  align-self: start;
  margin-top: 1px;
  color: var(--sf-text-2);
}
.row-node {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-summary {
  grid-column: 3;
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.timeline-row.kind-return .row-icon,
.timeline-row.kind-return .row-summary {
  color: var(--sf-accent);
}
.timeline-row.kind-error {
  border-left-color: var(--sf-error);
}
.timeline-row.kind-error .row-icon,
.timeline-row.kind-error .row-summary {
  color: var(--sf-error);
}
.timeline-row.kind-enter .row-icon {
  color: var(--sf-success);
}
</style>
