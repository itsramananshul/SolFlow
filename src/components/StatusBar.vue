<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { useSimulationStore } from '@/stores/simulation.store';
import type { GraphNode } from '@/graph/schema';

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();

const fn = computed(() => graph.activeFunction);
const nodeCount = computed(() => fn.value?.nodes.length ?? 0);
const edgeCount = computed(() => fn.value?.edges.length ?? 0);
const fnCount = computed(() => graph.workflow.functions.length);

const lastSavedAt = computed(() => graph.workflow.meta.updatedAt);
const now = ref(Date.now());
let tick: number | undefined;
onMounted(() => {
  tick = window.setInterval(() => (now.value = Date.now()), 1000);
});
onBeforeUnmount(() => {
  if (tick !== undefined) window.clearInterval(tick);
});

const savedAgo = computed(() => {
  if (!lastSavedAt.value) return '';
  const sec = Math.max(0, Math.floor((now.value - Date.parse(lastSavedAt.value)) / 1000));
  if (sec < 5) return 'just now';
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  return `${hr}h ago`;
});

const errorCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'error').length,
);
const warningCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'warning').length,
);

/**
 * Selection breadcrumb: gives a "you are here" trail when a node is
 * selected on a large workflow. Format:
 *   <function> › [<frame title>] › <node label>
 * The frame segment is only shown when the selected node visually sits
 * inside a Frame (center-of-node-inside-frame test, same heuristic as
 * frame drag). On medium-density graphs the frame title is the single
 * most useful piece of context — "I'm editing a node inside the
 * RISK CHECK section."
 */
const selectedNode = computed<GraphNode | null>(() => {
  const f = fn.value;
  if (!f || !ui.selectedNodeId) return null;
  return f.nodes.find((n) => n.id === ui.selectedNodeId) ?? null;
});

const containingFrameTitle = computed<string | null>(() => {
  const sel = selectedNode.value;
  const f = fn.value;
  if (!sel || !f) return null;
  if (sel.data.kind === 'frame') return null; // a frame doesn't sit inside itself
  // Center-of-node test mirrors Canvas's frame drag logic.
  const cx = sel.position.x + 110;
  const cy = sel.position.y + 28;
  for (const n of f.nodes) {
    if (n.data.kind !== 'frame') continue;
    if (
      cx >= n.position.x &&
      cx <= n.position.x + n.data.width &&
      cy >= n.position.y &&
      cy <= n.position.y + n.data.height
    ) {
      return n.data.title || 'Section';
    }
  }
  return null;
});

function nodeBreadcrumbLabel(n: GraphNode): string {
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
    case 'binaryOp':    return d.op;
    case 'unaryOp':     return `${d.op}x`;
    case 'varGet':      return d.varName || 'varGet';
    case 'literal':     return `${d.litType}: ${d.value}`;
    case 'arrayLiteral':  return `array[${d.length}]`;
    case 'structLiteral': return d.structName || 'struct';
    case 'fieldAccess': return `.${d.fieldName}`;
    case 'fieldSet':    return `.${d.fieldName} =`;
    case 'indexRead':   return 'arr[i]';
    case 'indexSet':    return 'arr[i] =';
    case 'enumVariant': return `${d.enumName}::${d.variantName}`;
    case 'call':        return 'call()';
    case 'action':      return `call("${d.capability}")`;
    case 'note':        return 'note';
    case 'frame':       return d.title || 'Section';
  }
}

const selectionCrumb = computed<string>(() => {
  const sel = selectedNode.value;
  if (!sel) return '';
  return nodeBreadcrumbLabel(sel);
});
</script>

<template>
  <footer class="status-bar">
    <div class="left">
      <!--
        When a node is selected, the breadcrumb takes priority and reads
        as: function › [frame] › node-label. When nothing is selected,
        we fall back to the per-function counters that were here before.
      -->
      <template v-if="selectedNode">
        <span class="cell crumb">
          <span class="dot acc" />
          <code>{{ fn?.name ?? '—' }}</code>
          <span v-if="containingFrameTitle" class="crumb-arrow">›</span>
          <code v-if="containingFrameTitle" class="frame-crumb">{{ containingFrameTitle }}</code>
          <span class="crumb-arrow">›</span>
          <code class="leaf-crumb">{{ selectionCrumb }}</code>
        </span>
      </template>
      <template v-else>
        <span class="cell">
          <span class="dot acc" />
          <span class="label">function</span>
          <code>{{ fn?.name ?? '—' }}</code>
        </span>
        <span class="cell">
          <span class="label">nodes</span>
          <code>{{ nodeCount }}</code>
        </span>
        <span class="cell">
          <span class="label">edges</span>
          <code>{{ edgeCount }}</code>
        </span>
        <span class="cell">
          <span class="label">fns</span>
          <code>{{ fnCount }}</code>
        </span>
      </template>
    </div>
    <div class="right">
      <span v-if="sim.isPlaying" class="cell sim">
        <span class="dot acc" />
        simulating
        <span class="time">step {{ sim.stepIndex }} / {{ sim.totalSteps }}</span>
      </span>
      <span v-else-if="errorCount > 0" class="cell err">
        <span class="dot err-dot" />
        {{ errorCount }} error{{ errorCount === 1 ? '' : 's' }}
      </span>
      <span v-else-if="warningCount > 0" class="cell warn">
        <span class="dot warn-dot" />
        {{ warningCount }} warning{{ warningCount === 1 ? '' : 's' }}
      </span>
      <span v-else class="cell ok">
        <span class="dot ok-dot" />
        clean
      </span>
      <span class="cell">
        <span class="label">autosaved</span>
        <span class="time">{{ savedAgo }}</span>
      </span>
    </div>
  </footer>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 clamp(6px, 1vw, 12px);
  height: clamp(20px, 2vw, 24px);
  background: var(--sf-bg-0);
  border-top: 1px solid var(--sf-border);
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  flex-shrink: 0;
  overflow: hidden;
}
.left,
.right {
  display: flex;
  align-items: center;
  gap: clamp(6px, 1vw, 14px);
  min-width: 0;
}
.cell {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  white-space: nowrap;
}
.label {
  color: var(--sf-text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
  font-size: 0.5625rem;
}
code {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
  font-size: 0.625rem;
}
.dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot.acc {
  background: var(--sf-accent);
}
.dot.err-dot {
  background: var(--sf-error);
}
.dot.warn-dot {
  background: var(--sf-warning);
}
.dot.ok-dot {
  background: var(--sf-success);
}
.cell.err {
  color: var(--sf-error);
}
.cell.warn {
  color: var(--sf-warning);
}
.cell.ok {
  color: var(--sf-success);
}
.time {
  color: var(--sf-text-2);
}
.crumb {
  min-width: 0;
  overflow: hidden;
}
.crumb code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.crumb-arrow {
  color: var(--sf-text-3);
  margin: 0 2px;
  font-size: 0.625rem;
}
.frame-crumb {
  color: var(--sf-cat-trigger);
}
.leaf-crumb {
  color: var(--sf-accent);
}
</style>
