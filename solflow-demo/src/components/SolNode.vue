<script setup lang="ts">
/**
 * Vue Flow custom node renderer. One component handles all 22 SolFlow node
 * kinds; the body switches on `data.data.kind`. Handles are derived from
 * `data.data.ports`. Inline expressions (from data.data.expressions) are
 * rendered next to their port labels — what you see is what gets emitted.
 */
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';

import type { GraphNode, NodeData, Port } from '@/graph/schema';
import { typeCssClass, typeLabel } from '@/graph/schema';
import { categoryColor, categoryForKind } from '@/graph/kinds';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';

interface Props {
  id: string;
  data: GraphNode;
  selected?: boolean;
}

const props = defineProps<Props>();
const graph = useGraphStore();
const ui = useUIStore();

const node = computed(() => props.data);
const kindLabel = computed(() => labelForKind(node.value.data));
const categoryDot = computed(() =>
  categoryColor(categoryForKind(node.value.data.kind)),
);

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

function inlineExprFor(portId: string): string | undefined {
  const v = node.value.expressions?.[portId];
  return v && v.trim() !== '' ? v : undefined;
}

function handleDelete() {
  if (node.value.data.kind === 'start') return;
  graph.removeNode(node.value.id);
  if (ui.selectedNodeId === node.value.id) ui.selectNode(null);
}

function labelForKind(data: NodeData): string {
  switch (data.kind) {
    case 'start':
      return 'Start';
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
      return `${data.structName || 'struct'} { }`;
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
      const fn = useGraphStore().workflow.functions.find((f) => f.id === data.functionId);
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
  <div :class="['sf-node', { selected }]">
    <div class="header">
      <span class="cat-dot" :style="{ background: categoryDot }" />
      <span class="title" :title="kindLabel">{{ kindLabel }}</span>
      <button
        v-if="node.data.kind !== 'start'"
        class="close"
        title="Delete node"
        @click.stop="handleDelete"
      >
        <svg viewBox="0 0 12 12" width="10" height="10" fill="none">
          <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
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
          <div class="port-meta">
            <span class="port-label">{{ p.name }}</span>
            <span v-if="inlineExprFor(p.id)" class="inline-expr">{{ inlineExprFor(p.id) }}</span>
            <span v-else class="port-type">{{ p.type ? typeLabel(p.type) : '' }}</span>
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
  min-width: 160px;
  max-width: 280px;
  font-size: 11px;
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
  font-size: 11px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.close {
  background: transparent;
  border: none;
  padding: 2px;
  border-radius: 2px;
  color: var(--sf-text-3);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.close:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-4);
}

.body {
  padding: 8px 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ports {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.port-row {
  display: flex;
  align-items: center;
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
.port-meta {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.port-meta.right {
  align-items: flex-end;
}
.port-label {
  font-family: var(--sf-font-mono);
  font-size: 10px;
  color: var(--sf-text-1);
}
.port-type {
  font-family: var(--sf-font-mono);
  font-size: 9px;
  color: var(--sf-text-3);
}
.inline-expr {
  font-family: var(--sf-font-mono);
  font-size: 10px;
  color: var(--sf-accent);
  background: var(--sf-accent-dim);
  padding: 1px 5px;
  border-radius: 2px;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.control-out-row {
  position: relative;
  height: 12px;
}
.control-out-label {
  position: absolute;
  bottom: -16px;
  transform: translateX(-50%);
  font-size: 9px;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  white-space: nowrap;
  pointer-events: none;
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
