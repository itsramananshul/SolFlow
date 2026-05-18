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
import { computed } from 'vue';
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
    <div class="header">
      <span class="cat-dot" :style="{ background: categoryDot }" />
      <span class="title" :title="kindLabel">{{ kindLabel }}</span>
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
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.sf-node:hover {
  border-color: var(--sf-border-strong);
}
.sf-node.selected {
  border-color: var(--sf-accent);
  box-shadow: 0 0 0 1px var(--sf-accent-dim);
}

.sf-node.cat-trigger {
  /* Amber left-strip so triggers read as entry points at a glance. */
  border-left: 3px solid var(--sf-cat-trigger);
  background: linear-gradient(
    90deg,
    rgba(232, 166, 87, 0.06) 0%,
    var(--sf-bg-2) 24%
  );
}
.sf-node.cat-trigger .header {
  /* Hairline under the trigger badge to keep the title legible against the tint. */
  border-bottom-color: rgba(232, 166, 87, 0.22);
}

.header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  border-bottom: 1px solid var(--sf-border);
}
.cat-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
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
  height: 14px;
}
.control-out-label {
  position: absolute;
  bottom: -18px;
  transform: translateX(-50%);
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  white-space: nowrap;
  pointer-events: none;
  background: var(--sf-bg-1);
  padding: 1px 6px;
  border-radius: 3px;
  border: 1px solid var(--sf-border);
  letter-spacing: 0.4px;
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
</style>
