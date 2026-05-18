<script setup lang="ts">
/**
 * Workflow outline — a structural tree of the active function.
 *
 * Different from ⌘F search:
 *   Search = "find a node I know by name"
 *   Outline = "see the structure of the whole thing"
 *
 * Organization:
 *   - For each Frame in the active function, list the contained nodes
 *     (geometric center-of-node-inside-frame test, same heuristic as
 *     Canvas's frame drag).
 *   - Nodes that don't sit inside any frame are listed under a
 *     "Loose nodes" section at the bottom.
 *   - Each frame section is collapsible; collapsed state persists per
 *     frame id in component-local storage so it survives function tab
 *     switches.
 *
 * Clicking any row jumps the canvas viewport to that node + selects it
 * via the same ui.requestFocus pipeline used by the diagnostics drawer
 * and the search palette. Read-only — never mutates the graph.
 */
import { computed, ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { categoryColor, categoryForKind } from '@/graph/kinds';
import type { GraphNode } from '@/graph/schema';

const graph = useGraphStore();
const ui = useUIStore();

const collapsedFrames = ref<Set<string>>(new Set());
function toggleFrame(id: string) {
  const next = new Set(collapsedFrames.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  collapsedFrames.value = next;
}
function isCollapsed(id: string): boolean {
  return collapsedFrames.value.has(id);
}

interface OutlineRow {
  node: GraphNode;
  label: string;
  detail: string;
}

interface OutlineGroup {
  /** Frame node id, or '__loose' for the free-floating bucket. */
  id: string;
  title: string;
  rows: OutlineRow[];
  /** Whether this group is a real frame (has a backing GraphNode). */
  isFrame: boolean;
  frameNode?: GraphNode;
}

function describe(n: GraphNode): { label: string; detail: string } {
  const d = n.data;
  switch (d.kind) {
    case 'start':       return { label: 'start()',          detail: 'function entry' };
    case 'trigger':     return { label: `${d.triggerKind} trigger`, detail: d.eventName };
    case 'let':         return { label: `let ${d.varName}`, detail: 'variable' };
    case 'assign':      return { label: `${d.varName} =`,   detail: 'assignment' };
    case 'print':       return { label: 'print',            detail: 'output' };
    case 'return':      return { label: 'return',           detail: d.hasValue ? 'returns value' : 'no value' };
    case 'branch':      return { label: 'branch',           detail: d.hasElse ? 'if / else' : 'if' };
    case 'while':       return { label: 'while',            detail: 'loop' };
    case 'forEach':     return { label: `for ${d.iteratorName}`, detail: 'iteration' };
    case 'binaryOp':    return { label: `op ${d.op}`,       detail: '' };
    case 'unaryOp':     return { label: `op ${d.op}`,       detail: '' };
    case 'varGet':      return { label: d.varName || 'varGet', detail: '' };
    case 'literal':     return { label: `${d.litType}: ${d.value}`, detail: '' };
    case 'arrayLiteral':  return { label: `array[${d.length}]`,    detail: '' };
    case 'structLiteral': return { label: `${d.structName} {}`,    detail: '' };
    case 'fieldAccess': return { label: `.${d.fieldName}`,         detail: d.structName };
    case 'fieldSet':    return { label: `.${d.fieldName} =`,       detail: d.structName };
    case 'indexRead':   return { label: 'arr[i]',           detail: '' };
    case 'indexSet':    return { label: 'arr[i] =',         detail: '' };
    case 'enumVariant': return { label: `${d.enumName}::${d.variantName}`, detail: '' };
    case 'call':        return { label: 'call()',           detail: 'function call' };
    case 'note':        return { label: 'note',             detail: d.text.slice(0, 40) };
    case 'frame':       return { label: d.title || 'Section', detail: 'group' };
  }
}

const groups = computed<OutlineGroup[]>(() => {
  const fn = graph.activeFunction;
  if (!fn) return [];

  const frames = fn.nodes.filter((n) => n.data.kind === 'frame');
  const nonFrames = fn.nodes.filter((n) => n.data.kind !== 'frame');

  // Map each non-frame node to its containing frame (if any). Same
  // center-of-node-inside-frame test as Canvas's frame-drag logic.
  function frameOf(n: GraphNode): GraphNode | null {
    const cx = n.position.x + 110;
    const cy = n.position.y + 28;
    for (const f of frames) {
      if (f.data.kind !== 'frame') continue;
      if (
        cx >= f.position.x &&
        cx <= f.position.x + f.data.width &&
        cy >= f.position.y &&
        cy <= f.position.y + f.data.height
      ) {
        return f;
      }
    }
    return null;
  }

  const rowsByFrame = new Map<string, OutlineRow[]>();
  const loose: OutlineRow[] = [];
  for (const n of nonFrames) {
    const { label, detail } = describe(n);
    const row: OutlineRow = { node: n, label, detail };
    const f = frameOf(n);
    if (f) {
      const arr = rowsByFrame.get(f.id) ?? [];
      arr.push(row);
      rowsByFrame.set(f.id, arr);
    } else {
      loose.push(row);
    }
  }

  // Sort rows inside each frame by y then x — reflects reading order
  // on the canvas (top-to-bottom, left-to-right).
  const byPosition = (a: OutlineRow, b: OutlineRow) =>
    a.node.position.y - b.node.position.y || a.node.position.x - b.node.position.x;

  const result: OutlineGroup[] = [];
  // Frames first, in their canvas order (also y-then-x).
  const sortedFrames = [...frames].sort((a, b) =>
    a.position.y - b.position.y || a.position.x - b.position.x,
  );
  for (const f of sortedFrames) {
    if (f.data.kind !== 'frame') continue;
    const rows = (rowsByFrame.get(f.id) ?? []).sort(byPosition);
    result.push({
      id: f.id,
      title: f.data.title || 'Section',
      rows,
      isFrame: true,
      frameNode: f,
    });
  }
  // Loose-node bucket at the end (only if non-empty).
  if (loose.length > 0) {
    result.push({
      id: '__loose',
      title: 'Loose nodes',
      rows: loose.sort(byPosition),
      isFrame: false,
    });
  }
  return result;
});

function nodeCount(): number {
  const fn = graph.activeFunction;
  return fn ? fn.nodes.length : 0;
}

function jumpToNode(n: GraphNode) {
  ui.requestFocus(n.id);
}

function jumpToFrame(f: GraphNode | undefined) {
  if (!f) return;
  ui.requestFocus(f.id);
}
</script>

<template>
  <div class="outline-panel">
    <div class="outline-header">
      <span class="title">Outline</span>
      <span class="hint">{{ nodeCount() }} nodes · click to jump</span>
    </div>

    <div v-if="groups.length === 0" class="empty">
      No nodes in this function yet — drag from Nodes to start.
    </div>

    <div v-else class="outline-body">
      <div v-for="g in groups" :key="g.id" class="group">
        <button
          class="group-header"
          :class="{ frame: g.isFrame, collapsed: isCollapsed(g.id) }"
          @click="toggleFrame(g.id)"
        >
          <span class="caret">{{ isCollapsed(g.id) ? '▸' : '▾' }}</span>
          <span class="group-title">{{ g.title }}</span>
          <span class="group-count">{{ g.rows.length }}</span>
          <button
            v-if="g.isFrame"
            class="frame-jump"
            title="Jump to this section on the canvas"
            @click.stop="jumpToFrame(g.frameNode)"
          >→</button>
        </button>

        <div v-if="!isCollapsed(g.id)" class="rows">
          <button
            v-for="row in g.rows"
            :key="row.node.id"
            class="row"
            :class="{ selected: ui.selectedNodeId === row.node.id }"
            @click="jumpToNode(row.node)"
          >
            <span
              class="dot"
              :style="{ background: categoryColor(categoryForKind(row.node.data.kind)) }"
            />
            <span class="row-label">{{ row.label }}</span>
            <span v-if="row.detail" class="row-detail">{{ row.detail }}</span>
          </button>
          <div v-if="g.rows.length === 0" class="rows-empty">
            <em>empty section</em>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.outline-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.outline-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
}
.title {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.hint {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  font-family: var(--sf-font-mono);
}
.empty {
  padding: 18px 14px;
  color: var(--sf-text-3);
  font-size: 0.6875rem;
  font-style: italic;
  text-align: center;
}
.outline-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 4px 8px;
}
.group {
  margin-bottom: 4px;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  background: transparent;
  border: none;
  padding: 5px 7px;
  border-radius: var(--sf-radius-sm);
  color: var(--sf-text-1);
  cursor: pointer;
  font-size: 0.6875rem;
  letter-spacing: 0.2px;
  text-align: left;
}
.group-header:hover {
  background: var(--sf-bg-2);
}
.group-header.frame .group-title {
  color: var(--sf-text-0);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  font-size: 0.625rem;
}
.group-header .caret {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  font-size: 0.625rem;
  flex-shrink: 0;
}
.group-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.group-count {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  padding: 1px 5px;
  background: var(--sf-bg-3);
  border-radius: 8px;
}
.frame-jump {
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  cursor: pointer;
  padding: 1px 5px;
  font-size: 0.75rem;
  border-radius: 3px;
}
.frame-jump:hover {
  background: var(--sf-bg-3);
  color: var(--sf-accent);
}
.rows {
  padding-left: 14px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  margin-top: 1px;
}
.row {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  background: transparent;
  border: none;
  padding: 4px 7px;
  border-radius: var(--sf-radius-sm);
  text-align: left;
  cursor: pointer;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
}
.row:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.row.selected {
  background: var(--sf-accent-dim);
  color: var(--sf-text-0);
}
.dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0.85;
}
.row-label {
  font-family: var(--sf-font-mono);
  flex-shrink: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 60%;
}
.row-detail {
  flex: 1;
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
}
.rows-empty {
  padding: 4px 10px;
  font-size: 0.625rem;
  color: var(--sf-text-3);
  font-style: italic;
}
</style>
