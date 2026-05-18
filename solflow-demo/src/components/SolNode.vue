<script setup lang="ts">
/**
 * Single Vue Flow custom node component that renders ALL 22 SolFlow node
 * kinds. The body switches on `data.data.kind`; handles are derived from
 * `data.data.ports`.
 *
 * Vue Flow passes the entire registered-node data via the component's
 * default `data` prop. We accept it typed.
 */
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';

import type {
  GraphNode,
  NodeData,
  Port,
} from '@/graph/schema';
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
const headerColor = computed(() =>
  categoryColor(categoryForKind(node.value.data.kind)),
);

const inputs = computed<Port[]>(() => node.value.ports.in);
const outputs = computed<Port[]>(() => node.value.ports.out);

const dataIns = computed(() => inputs.value.filter((p) => p.kind === 'data'));
const dataOuts = computed(() => outputs.value.filter((p) => p.kind === 'data'));
const controlIns = computed(() => inputs.value.filter((p) => p.kind === 'control'));
const controlOuts = computed(() => outputs.value.filter((p) => p.kind === 'control'));

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
      return `Let ${data.varName || '(unnamed)'}: ${typeLabel(data.varType)}`;
    case 'assign':
      return `Assign ${data.varName || '?'}`;
    case 'print':
      return 'Print';
    case 'return':
      return data.hasValue ? 'Return ⏎' : 'Return';
    case 'branch':
      return data.hasElse ? 'If / Else' : 'If';
    case 'while':
      return 'While';
    case 'forEach':
      return `For ${data.iteratorName || 'item'} in …`;
    case 'binaryOp':
      return `${data.op}`;
    case 'unaryOp':
      return `${data.op}x`;
    case 'varGet':
      return data.varName || '(var)';
    case 'literal':
      return `${data.litType}: ${formatLiteralPreview(data.litType, data.value)}`;
    case 'arrayLiteral':
      return `[${data.length}] ${typeLabel(data.itemType)}`;
    case 'structLiteral':
      return `${data.structName || '?'} { … }`;
    case 'fieldAccess':
      return `.${data.fieldName || 'field'}`;
    case 'fieldSet':
      return `.${data.fieldName || 'field'} ←`;
    case 'indexRead':
      return '[ ] →';
    case 'indexSet':
      return '[ ] ←';
    case 'enumVariant':
      return `${data.enumName || '?'}::${data.variantName || '?'}`;
    case 'call': {
      const fn = useGraphStore().workflow.functions.find((f) => f.id === data.functionId);
      return `Call ${fn?.name ?? '?'}()`;
    }
  }
}

function formatLiteralPreview(t: string, v: string): string {
  if (t === 'str') return `"${(v ?? '').slice(0, 18)}${(v ?? '').length > 18 ? '…' : ''}"`;
  if (t === 'char') return `'${(v ?? ' ')[0] ?? ' '}'`;
  return v || '0';
}
</script>

<template>
  <div :class="['sf-node', { selected }]">
    <div class="header" :style="{ background: headerColor }">
      <span class="title">{{ kindLabel }}</span>
      <button
        v-if="node.data.kind !== 'start'"
        class="close"
        title="Delete node"
        @click.stop="handleDelete"
      >
        ✕
      </button>
    </div>

    <div class="body">
      <!-- Inputs section -->
      <div v-if="dataIns.length > 0" class="ports col">
        <div
          v-for="p in dataIns"
          :key="`in:${p.id}`"
          class="port-row in"
          :class="{ required: p.required }"
        >
          <Handle
            :id="p.id"
            type="target"
            :position="Position.Left"
            :class="['handle', typeCssClass(p.type)]"
          />
          <span class="port-label">{{ p.name }}</span>
          <span class="port-type" :class="typeCssClass(p.type)">{{
            p.type ? typeLabel(p.type) : ''
          }}</span>
        </div>
      </div>

      <!-- Outputs section -->
      <div v-if="dataOuts.length > 0" class="ports col">
        <div
          v-for="p in dataOuts"
          :key="`out:${p.id}`"
          class="port-row out"
        >
          <span class="port-type" :class="typeCssClass(p.type)">{{
            p.type ? typeLabel(p.type) : ''
          }}</span>
          <span class="port-label">{{ p.name }}</span>
          <Handle
            :id="p.id"
            type="source"
            :position="Position.Right"
            :class="['handle', typeCssClass(p.type)]"
          />
        </div>
      </div>
    </div>

    <!-- Control flow handles (top for in, bottom for out) -->
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
  box-shadow: var(--sf-shadow-1);
  min-width: 180px;
  font-size: 11px;
  position: relative;
  user-select: none;
}
.sf-node.selected {
  border-color: var(--sf-accent);
  box-shadow: 0 0 0 2px var(--sf-accent-muted), var(--sf-shadow-2);
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border-radius: var(--sf-radius-md) var(--sf-radius-md) 0 0;
  color: white;
  font-weight: 600;
  font-size: 11px;
  letter-spacing: 0.2px;
}
.header .title {
  text-shadow: 0 1px 1px rgba(0, 0, 0, 0.3);
}
.header .close {
  background: transparent;
  border: none;
  color: rgba(255, 255, 255, 0.7);
  cursor: pointer;
  padding: 0 4px;
  font-size: 11px;
}
.header .close:hover {
  color: white;
}

.body {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ports {
  gap: 4px;
}
.port-row {
  display: flex;
  align-items: center;
  gap: 6px;
  position: relative;
  min-height: 20px;
}
.port-row.in {
  justify-content: flex-start;
  padding-left: 4px;
}
.port-row.out {
  justify-content: flex-end;
  padding-right: 4px;
}
.port-label {
  color: var(--sf-text-1);
}
.port-type {
  font-family: var(--sf-font-mono);
  font-size: 9px;
  opacity: 0.7;
  padding: 1px 4px;
  border-radius: 3px;
  background: var(--sf-bg-3);
}
.port-row.in .port-type {
  margin-left: auto;
}
.port-row.out .port-type {
  margin-right: auto;
}

.control-out-row {
  position: relative;
  height: 14px;
}
.control-out-label {
  position: absolute;
  bottom: -16px;
  transform: translateX(-50%);
  font-size: 9px;
  color: var(--sf-text-2);
  white-space: nowrap;
  pointer-events: none;
}

.handle.control {
  background: #cbd1de;
  border-color: var(--sf-bg-2);
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
