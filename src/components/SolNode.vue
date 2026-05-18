<script setup lang="ts">
/**
 * Vue Flow custom node renderer. One component handles all 22 SolFlow node
 * kinds; the body switches on `data.data.kind`. Handles are derived from
 * `data.data.ports`. Unwired data inputs get an inline `<input>` directly
 * on the card — typing into it sets `node.expressions[portId]` and the
 * emitter uses it as the SOL expression for that port. Wired ports show
 * a "wired" pill instead. Click-stop prevents Vue Flow from dragging the
 * node when the user clicks into an input.
 */
import { computed, ref } from 'vue';
import { Handle, Position } from '@vue-flow/core';

import type { GraphNode, NodeData, Port } from '@/graph/schema';
import { typeCssClass, typeLabel } from '@/graph/schema';
import { categoryColor, categoryForKind } from '@/graph/kinds';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { useSimulationStore } from '@/stores/simulation.store';

interface Props {
  id: string;
  data: GraphNode;
  selected?: boolean;
}

const props = defineProps<Props>();
const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();

const node = computed(() => props.data);
const category = computed(() => categoryForKind(node.value.data.kind));
const kindLabel = computed(() => labelForKind(node.value.data));
const categoryDot = computed(() => categoryColor(category.value));
const simStatus = computed(() => sim.getNodeStatus(node.value.id));

// Role glyph: a single mono character that hints at what the node DOES at
// a glance. Branch = directional, loops = iterative, trigger = entry,
// terminal = end. Kept tiny + monospace so it reads as type, not decoration.
const roleGlyph = computed<string | null>(() => {
  const k = node.value.data.kind;
  if (k === 'branch') return '⌥';
  if (k === 'while' || k === 'forEach') return '↻';
  if (k === 'trigger') return '⚡';
  if (k === 'return') return '⏎';
  if (k === 'start') return '▸';
  return null;
});

// Tiny header badge used for triggers — communicates "this is an
// entrypoint" without leaning on a color-only differentiator.
const headerBadge = computed<string | null>(() => {
  const d = node.value.data;
  if (d.kind === 'trigger') return d.triggerKind.toUpperCase();
  return null;
});

// Plain-English explanations shown on a hover tooltip. The point is to
// teach concepts without forcing the user to open the Inspector or read
// docs. Kept under ~120 chars per entry so the tooltip stays scan-able.
function explainKind(d: NodeData): string {
  switch (d.kind) {
    case 'start':
      return 'Entry point of this function. Execution starts here and follows the wires below.';
    case 'trigger': {
      const k = d.triggerKind;
      if (k === 'webhook') return 'Starts the workflow when someone POSTs to your webhook URL.';
      if (k === 'timer')   return 'Starts the workflow on a schedule (every N minutes / hours / days).';
      if (k === 'event')   return `Starts the workflow when the event "${d.eventName}" happens in your system.`;
      if (k === 'http')    return 'Starts the workflow when a specific HTTP request is received.';
      return 'Starts the workflow when someone clicks "Trigger Event ▷" — useful for testing.';
    }
    case 'let':
      return 'Stores a value in a named variable. Downstream nodes can read this name.';
    case 'assign':
      return 'Changes the value of an existing variable.';
    case 'print':
      return 'Outputs a value (string, number, etc.) to the run log.';
    case 'return':
      return d.hasValue ? 'Ends the function and gives back a value.' : 'Ends the function with no return value.';
    case 'branch':
      return 'Sends execution one of two ways based on a condition: then (true) or else (false).';
    case 'while':
      return 'Repeats the body steps while the condition stays true.';
    case 'forEach':
      return 'Walks through each item in an array, running the body once per item.';
    case 'binaryOp':
      return `Combines two values with an operator (${d.op}). Math, comparison, or logic.`;
    case 'unaryOp':
      return `Applies a one-sided operator to a value (${d.op}).`;
    case 'varGet':
      return 'Reads the current value of a variable.';
    case 'literal':
      return `A fixed value of type ${d.litType}.`;
    case 'arrayLiteral':
      return 'A fixed-length array — each slot gets its own value.';
    case 'structLiteral':
      return 'Constructs a struct by filling in each of its fields.';
    case 'fieldAccess':
      return 'Reads a single field out of a struct value.';
    case 'fieldSet':
      return 'Writes a new value into a struct field.';
    case 'indexRead':
      return 'Reads the array element at a given index.';
    case 'indexSet':
      return 'Writes a value into the array element at a given index.';
    case 'enumVariant':
      return 'A specific value of an enum (e.g. Status::Active).';
    case 'call':
      return 'Calls another function defined in this workflow.';
  }
}

const explanation = computed(() => explainKind(node.value.data));
const tooltipVisible = ref(false);
let tooltipTimer: number | undefined;
function showTooltip() {
  if (tooltipTimer !== undefined) window.clearTimeout(tooltipTimer);
  // Delay so quick mouse passes don't flash the tooltip.
  tooltipTimer = window.setTimeout(() => {
    tooltipVisible.value = true;
  }, 600);
}
function hideTooltip() {
  if (tooltipTimer !== undefined) {
    window.clearTimeout(tooltipTimer);
    tooltipTimer = undefined;
  }
  tooltipVisible.value = false;
}

const dataIns = computed<Port[]>(() =>
  node.value.ports.in.filter((p) => p.kind === 'data'),
);
const dataOuts = computed<Port[]>(() =>
  node.value.ports.out.filter((p) => p.kind === 'data'),
);
const controlIns = computed<Port[]>(() =>
  node.value.ports.in.filter((p) => p.kind === 'control'),
);
const controlOuts = computed<Port[]>(() =>
  node.value.ports.out.filter((p) => p.kind === 'control'),
);
// Multi-out flow nodes (branch/while/forEach) carry directional labels
// (then/else/after/body) that the user genuinely needs to read. Every
// other node has a single `next` out — the label is noise. Suppress it
// so the canvas reads cleanly when many statements stack vertically.
const showControlOutLabels = computed(
  () => controlOuts.value.length > 1,
);

function inlineExprFor(portId: string): string {
  return node.value.expressions?.[portId] ?? '';
}

function isPortWired(portId: string): boolean {
  const fn = graph.activeFunction;
  if (!fn) return false;
  return fn.edges.some(
    (e) =>
      e.kind === 'data' &&
      e.target.node === node.value.id &&
      e.target.port === portId,
  );
}

function onExprInput(portId: string, e: Event) {
  const text = (e.target as HTMLInputElement).value;
  graph.updateNodeExpression(node.value.id, portId, text);
}

function placeholderFor(portId: string, kind: string): string {
  if (portId === 'cond') return 'counter < 4';
  if (portId === 'value' && kind === 'print') return '"hello"';
  if (portId === 'value' && kind === 'return') return '0';
  if (portId === 'value' && kind === 'let') return '5 + 3';
  if (portId === 'value' && kind === 'assign') return 'counter + 1';
  if (portId === 'array') return 'arr';
  if (portId === 'index') return 'i';
  if (portId === 'target') return 'node';
  if (portId === 'lhs' || portId === 'rhs') return '0';
  if (portId === 'operand') return 'x';
  if (portId.startsWith('arg:')) return portId.slice(4);
  if (portId.startsWith('field:')) return portId.slice(6);
  if (portId.startsWith('item:')) return '0';
  return 'expression';
}

function handleDelete() {
  if (node.value.data.kind === 'start') return;
  graph.removeNode(node.value.id);
  if (ui.selectedNodeId === node.value.id) ui.selectNode(null);
}

function handleDuplicate() {
  if (node.value.data.kind === 'start') return;
  const dup = graph.duplicateNode(node.value.id);
  if (dup) ui.selectNode(dup.id);
}

function labelForKind(data: NodeData): string {
  switch (data.kind) {
    case 'start':
      return 'start()';
    case 'trigger': {
      const k = data.triggerKind;
      if (k === 'webhook') return `webhook · ${data.webhookPath ?? ''}`;
      if (k === 'timer') return `timer · ${data.cronExpr ?? ''}`;
      if (k === 'http') return `${data.httpMethod ?? 'POST'} ${data.httpPath ?? ''}`;
      if (k === 'manual') return `manual · ${data.eventName}`;
      return `event · ${data.eventName}`;
    }
    case 'let':
      return `let ${data.varName || '_'}: ${typeLabel(data.varType)}`;
    case 'assign':
      return `${data.varName || '_'} =`;
    case 'print':
      return 'print';
    case 'return':
      return 'return';
    case 'branch':
      return data.hasElse ? 'if / else' : 'if';
    case 'while':
      return 'while';
    case 'forEach':
      return `for ${data.iteratorName || 'item'} in`;
    case 'binaryOp':
      return data.op;
    case 'unaryOp':
      return `${data.op}x`;
    case 'varGet':
      return data.varName || 'var';
    case 'literal':
      return formatLiteralPreview(data.litType, data.value);
    case 'arrayLiteral':
      return `[${data.length}] ${typeLabel(data.itemType)}`;
    case 'structLiteral':
      return `${data.structName || 'struct'} {}`;
    case 'fieldAccess':
      return `.${data.fieldName || 'field'}`;
    case 'fieldSet':
      return `.${data.fieldName || 'field'} =`;
    case 'indexRead':
      return '[i]';
    case 'indexSet':
      return '[i] =';
    case 'enumVariant':
      return `${data.enumName || '?'}::${data.variantName || '?'}`;
    case 'call': {
      const fn = graph.workflow.functions.find((f) => f.id === data.functionId);
      return `${fn?.name ?? 'call'}()`;
    }
  }
}

function formatLiteralPreview(t: string, v: string): string {
  if (t === 'str') {
    const s = v ?? '';
    return `"${s.length > 20 ? s.slice(0, 20) + '…' : s}"`;
  }
  if (t === 'char') return `'${(v ?? ' ')[0] ?? ' '}'`;
  return v || '0';
}
</script>

<template>
  <div
    :class="[
      'sf-node',
      `cat-${category}`,
      { selected, 'is-running': simStatus === 'running', 'is-visited': simStatus === 'visited', 'is-failed': simStatus === 'failed' },
    ]"
    @mouseenter="ui.setHovered(node.id)"
    @mouseleave="ui.setHovered(null)"
  >
    <div
      class="header"
      @mouseenter="showTooltip"
      @mouseleave="hideTooltip"
    >
      <span class="cat-dot" :style="{ background: categoryDot }" />
      <span v-if="roleGlyph" class="role-glyph">{{ roleGlyph }}</span>
      <span class="title" :title="kindLabel">{{ kindLabel }}</span>
      <span v-if="headerBadge" class="header-badge">{{ headerBadge }}</span>
      <Transition name="tip">
        <div v-if="tooltipVisible" class="node-tooltip">
          <div class="tip-title">{{ kindLabel }}</div>
          <div class="tip-body">{{ explanation }}</div>
        </div>
      </Transition>
      <div v-if="node.data.kind !== 'start'" class="quick-actions nodrag">
        <button
          class="qa-btn"
          title="Duplicate"
          @click.stop="handleDuplicate"
          @mousedown.stop
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none">
            <rect x="2.5" y="2.5" width="5" height="5" rx="1" stroke="currentColor" stroke-width="1.2" />
            <rect x="4.5" y="4.5" width="5" height="5" rx="1" stroke="currentColor" stroke-width="1.2" />
          </svg>
        </button>
        <button
          class="qa-btn"
          title="Delete"
          @click.stop="handleDelete"
          @mousedown.stop
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none">
            <path
              d="M3 3 9 9 M9 3 3 9"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>
    </div>

    <div v-if="dataIns.length > 0 || dataOuts.length > 0" class="body">
      <!-- Data inputs (left side) -->
      <div v-if="dataIns.length > 0" class="ports in">
        <div v-for="p in dataIns" :key="`in:${p.id}`" class="port-row">
          <Handle
            :id="p.id"
            type="target"
            :position="Position.Left"
            :class="['handle', typeCssClass(p.type)]"
          />
          <div class="port-cell">
            <div class="port-meta">
              <span class="port-label">{{ p.name }}</span>
              <span
                v-if="isPortWired(p.id)"
                class="pill wire"
                title="Wired from another node"
              >wired</span>
              <span v-else class="port-type">{{ p.type ? typeLabel(p.type) : '' }}</span>
            </div>
            <input
              v-if="!isPortWired(p.id)"
              class="port-input nodrag nopan"
              :value="inlineExprFor(p.id)"
              :placeholder="placeholderFor(p.id, node.data.kind)"
              spellcheck="false"
              @click.stop
              @mousedown.stop
              @input="onExprInput(p.id, $event)"
            />
          </div>
        </div>
      </div>

      <!-- Data outputs (right side) -->
      <div v-if="dataOuts.length > 0" class="ports out">
        <div v-for="p in dataOuts" :key="`out:${p.id}`" class="port-row">
          <div class="port-meta right">
            <span class="port-type">{{ p.type ? typeLabel(p.type) : '' }}</span>
            <span class="port-label">{{ p.name }}</span>
          </div>
          <Handle
            :id="p.id"
            type="source"
            :position="Position.Right"
            :class="['handle', typeCssClass(p.type)]"
          />
        </div>
      </div>
    </div>

    <!-- Control flow handles -->
    <Handle
      v-for="p in controlIns"
      :key="`cin:${p.id}`"
      :id="p.id"
      type="target"
      :position="Position.Top"
      class="handle control"
    />
    <div v-if="controlOuts.length > 0" class="control-out-row">
      <Handle
        v-for="(p, i) in controlOuts"
        :key="`cout:${p.id}`"
        :id="p.id"
        type="source"
        :position="Position.Bottom"
        :style="{ left: `${((i + 0.5) / controlOuts.length) * 100}%` }"
        class="handle control"
      />
    </div>
    <!--
      Inline footer band for multi-out flow nodes. Labels live INSIDE the
      card so they never collide with downstream nodes when graphs pack
      tight. Suppressed for single-out nodes (most statements) — `next`
      would just add noise.
    -->
    <div v-if="showControlOutLabels" class="control-out-labels">
      <div
        v-for="(p, i) in controlOuts"
        :key="`coutlbl:${p.id}`"
        class="control-out-label"
        :style="{ left: `${((i + 0.5) / controlOuts.length) * 100}%` }"
      >
        {{ p.name }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.sf-node {
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-md);
  min-width: 220px;
  max-width: 400px;
  font-size: 0.6875rem;
  position: relative;
  user-select: none;
  cursor: grab;
  transition:
    border-color 0.12s ease,
    box-shadow 0.12s ease,
    transform 0.12s ease,
    background 0.12s ease;
}
.sf-node:active {
  cursor: grabbing;
}
.sf-node:hover {
  border-color: var(--sf-border-strong);
  box-shadow: var(--sf-shadow-2);
}
.sf-node.selected {
  border-color: var(--sf-accent);
  box-shadow:
    0 0 0 1px var(--sf-accent-ring),
    var(--sf-shadow-2);
}

.header {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--sf-border);
}
.cat-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0.85;
}
.role-glyph {
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  line-height: 1;
  color: var(--sf-text-2);
  flex-shrink: 0;
}
.title {
  flex: 1;
  color: var(--sf-text-0);
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: -0.1px;
}
.header-badge {
  font-family: var(--sf-font-mono);
  font-size: 0.5rem;
  letter-spacing: 0.6px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 3px;
  background: rgba(232, 166, 87, 0.14);
  color: var(--sf-cat-trigger);
  flex-shrink: 0;
}
.quick-actions {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.sf-node:hover .quick-actions,
.sf-node.selected .quick-actions {
  opacity: 1;
}
.qa-btn {
  background: transparent;
  border: none;
  padding: 3px;
  border-radius: 2px;
  color: var(--sf-text-3);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.qa-btn:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-4);
}
.qa-btn:last-child:hover {
  color: var(--sf-error);
  background: rgba(255, 77, 79, 0.12);
}

.body {
  padding: 6px 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ports {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.port-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  position: relative;
  min-height: 18px;
}
.ports.in .port-row {
  padding-left: 10px;
  padding-right: 8px;
}
.ports.out .port-row {
  padding-left: 8px;
  padding-right: 10px;
  justify-content: flex-end;
}
.port-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}
.port-meta {
  display: flex;
  align-items: center;
  gap: 6px;
}
.port-meta.right {
  justify-content: flex-end;
}
.port-label {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-1);
}
.port-type {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
}
.pill.wire {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-accent);
  background: var(--sf-accent-dim);
  padding: 1px 5px;
  border-radius: 2px;
  letter-spacing: 0.4px;
  text-transform: uppercase;
}
.port-input {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  color: var(--sf-text-0);
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 3px;
  padding: 3px 6px;
  outline: none;
  width: 100%;
  transition: border-color 0.12s ease;
}
.port-input:hover {
  border-color: var(--sf-border-strong);
}
.port-input:focus {
  border-color: var(--sf-accent);
  background: var(--sf-bg-2);
  box-shadow: 0 0 0 1px var(--sf-accent-dim);
}
.port-input::placeholder {
  color: var(--sf-text-3);
  font-style: italic;
}

.control-out-row {
  position: relative;
  height: 10px;
}
.control-out-labels {
  position: relative;
  height: 16px;
  border-top: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  border-bottom-left-radius: var(--sf-radius-md);
  border-bottom-right-radius: var(--sf-radius-md);
}
.control-out-label {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
  font-size: 0.5625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  white-space: nowrap;
  pointer-events: none;
  letter-spacing: 0.5px;
  text-transform: lowercase;
}

.handle.control {
  background: var(--sf-text-2);
  border-color: var(--sf-bg-2);
  border-radius: 2px;
}
.handle.data-int { background: var(--sf-type-int); }
.handle.data-float { background: var(--sf-type-float); }
.handle.data-bool { background: var(--sf-type-bool); }
.handle.data-str { background: var(--sf-type-str); }
.handle.data-char { background: var(--sf-type-char); }
.handle.data-array { background: var(--sf-type-array); }
.handle.data-struct { background: var(--sf-type-struct); }
.handle.data-enum { background: var(--sf-type-enum); }
.handle.data-any { background: var(--sf-type-any); }

/* Hover-help tooltip floating above the node header. Teaches what a
   kind does in one sentence so users don't need to open the Inspector
   to learn. Pointer-events: none so it doesn't intercept node drags. */
.node-tooltip {
  position: absolute;
  bottom: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-2);
  padding: 8px 10px;
  width: max-content;
  max-width: 280px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  line-height: 1.4;
  z-index: 10;
  pointer-events: none;
}
.node-tooltip .tip-title {
  font-family: var(--sf-font-mono);
  font-weight: 600;
  color: var(--sf-text-0);
  font-size: 0.625rem;
  letter-spacing: 0.3px;
  margin-bottom: 3px;
}
.node-tooltip .tip-body {
  color: var(--sf-text-2);
  white-space: normal;
}
.tip-enter-active,
.tip-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.tip-enter-from,
.tip-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(4px);
}
</style>
