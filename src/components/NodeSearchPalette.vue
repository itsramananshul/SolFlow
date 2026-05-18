<script setup lang="ts">
/**
 * Workflow-wide node search (Cmd/Ctrl+F). Lists every node in the
 * active function with its kind label and any node-specific text
 * (variable name, trigger event, note body, etc.) so a fuzzy query
 * can find "the WebhookOrderReceived node" in a 50-node workflow.
 *
 * Clicking a result pans the viewport to that node and selects it.
 * Read-only — never inserts; never mutates the graph.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import type { GraphNode, NodeData } from '@/graph/schema';
import { categoryColor, categoryForKind } from '@/graph/kinds';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'jump', nodeId: string): void;
}>();

const graph = useGraphStore();
const query = ref('');
const activeIdx = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);
const listRef = ref<HTMLDivElement | null>(null);

interface Hit {
  node: GraphNode;
  label: string;
  detail: string;
}

// Concatenate a "haystack" string per node so the user's query matches
// against label, kind, plus any node-specific text.
function describe(n: GraphNode): { label: string; detail: string } {
  const d = n.data;
  switch (d.kind) {
    case 'start':
      return { label: 'start()', detail: 'function entry' };
    case 'trigger':
      return {
        label: `${d.triggerKind} trigger`,
        detail: d.eventName + (d.webhookPath ? ` · ${d.webhookPath}` : '') + (d.cronExpr ? ` · ${d.cronExpr}` : ''),
      };
    case 'let':
      return { label: `let ${d.varName}`, detail: 'variable declaration' };
    case 'assign':
      return { label: `${d.varName} =`, detail: 'assignment' };
    case 'print':
      return { label: 'print', detail: 'output' };
    case 'return':
      return { label: 'return', detail: d.hasValue ? 'returns a value' : 'no value' };
    case 'branch':
      return { label: 'if / branch', detail: d.hasElse ? 'with else' : 'no else' };
    case 'while':
      return { label: 'while loop', detail: 'condition-driven loop' };
    case 'forEach':
      return { label: `for ${d.iteratorName}`, detail: 'array iteration' };
    case 'binaryOp':
      return { label: `op ${d.op}`, detail: 'binary operator' };
    case 'unaryOp':
      return { label: `op ${d.op}`, detail: 'unary operator' };
    case 'varGet':
      return { label: d.varName || 'varGet', detail: 'variable read' };
    case 'literal':
      return { label: `${d.litType}: ${d.value}`, detail: 'literal' };
    case 'arrayLiteral':
      return { label: `array[${d.length}]`, detail: 'array literal' };
    case 'structLiteral':
      return { label: `${d.structName} {}`, detail: 'struct literal' };
    case 'fieldAccess':
      return { label: `.${d.fieldName}`, detail: `from ${d.structName}` };
    case 'fieldSet':
      return { label: `.${d.fieldName} =`, detail: `into ${d.structName}` };
    case 'indexRead':
      return { label: 'arr[i]', detail: 'index read' };
    case 'indexSet':
      return { label: 'arr[i] =', detail: 'index write' };
    case 'enumVariant':
      return { label: `${d.enumName}::${d.variantName}`, detail: 'enum variant' };
    case 'call':
      return { label: 'call()', detail: 'function call' };
    case 'note':
      return { label: 'note', detail: d.text.slice(0, 80) };
    case 'frame':
      return { label: d.title || 'Section', detail: 'frame / group' };
  }
}

const allHits = computed<Hit[]>(() => {
  const fn = graph.activeFunction;
  if (!fn) return [];
  return fn.nodes.map((n) => {
    const { label, detail } = describe(n);
    return { node: n, label, detail };
  });
});

function score(q: string, h: Hit): number {
  if (!q) return 1;
  const ql = q.toLowerCase();
  const label = h.label.toLowerCase();
  const detail = h.detail.toLowerCase();
  const kind = h.node.data.kind.toLowerCase();
  if (label === ql) return 10000;
  if (label.startsWith(ql)) return 5000 + (100 - label.length);
  if (kind === ql) return 4500;
  if (label.includes(ql)) return 3000;
  if (detail.includes(ql)) return 1500;
  if (kind.includes(ql)) return 1000;
  // Subsequence fallback on label
  let qi = 0;
  for (let i = 0; i < label.length && qi < ql.length; i++) {
    if (label[i] === ql[qi]) qi++;
  }
  if (qi === ql.length) return 400;
  return 0;
}

const filtered = computed<Hit[]>(() => {
  const q = query.value.trim();
  return allHits.value
    .map((h) => ({ h, s: score(q, h) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .map((x) => x.h);
});

watch(
  () => props.open,
  (now) => {
    if (now) {
      query.value = '';
      activeIdx.value = 0;
      nextTick(() => inputRef.value?.focus());
    } else {
      inputRef.value?.blur();
    }
  },
);

watch(activeIdx, () => {
  nextTick(() => {
    const el = listRef.value?.querySelector(`.item[data-i="${activeIdx.value}"]`);
    if (el && 'scrollIntoView' in el) {
      (el as HTMLElement).scrollIntoView({ block: 'nearest' });
    }
  });
});

watch(filtered, () => {
  if (activeIdx.value >= filtered.value.length) activeIdx.value = 0;
});

function pick(i: number) {
  const hit = filtered.value[i];
  if (!hit) return;
  emit('jump', hit.node.id);
  emit('close');
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    if (filtered.value.length === 0) return;
    activeIdx.value = (activeIdx.value + 1) % filtered.value.length;
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    if (filtered.value.length === 0) return;
    activeIdx.value = (activeIdx.value - 1 + filtered.value.length) % filtered.value.length;
  } else if (e.key === 'Enter') {
    e.preventDefault();
    pick(activeIdx.value);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

function onWinKey(e: KeyboardEvent) {
  if (!props.open) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

onMounted(() => window.addEventListener('keydown', onWinKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onWinKey));
</script>

<template>
  <Teleport to="body">
    <Transition name="search-fade">
      <div v-if="open" class="search-backdrop" @click="onBackdrop">
        <div class="search-modal" @click.stop>
          <div class="search-header">
            <svg viewBox="0 0 16 16" width="12" height="12" class="search-icon" fill="none">
              <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.5" />
              <path d="M10.5 10.5 L13.5 13.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
            <input
              ref="inputRef"
              v-model="query"
              class="search-input"
              placeholder="Search nodes in this function…"
              spellcheck="false"
              @keydown="onKeyDown"
            />
            <span class="search-hint">↑↓ Enter Esc</span>
          </div>
          <div ref="listRef" class="search-list">
            <div v-if="filtered.length === 0" class="search-empty">
              No nodes match "{{ query }}"
            </div>
            <button
              v-for="(hit, i) in filtered"
              :key="hit.node.id"
              :data-i="i"
              class="item"
              :class="{ active: i === activeIdx }"
              @mousedown.prevent
              @click="pick(i)"
              @mouseenter="activeIdx = i"
            >
              <span
                class="dot"
                :style="{ background: categoryColor(categoryForKind(hit.node.data.kind)) }"
              />
              <div class="item-body">
                <div class="item-label">{{ hit.label }}</div>
                <div class="item-detail">{{ hit.detail || hit.node.data.kind }}</div>
              </div>
              <span class="kind-tag">{{ hit.node.data.kind }}</span>
            </button>
          </div>
          <div class="search-footer">
            {{ allHits.length }} nodes in this function
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.search-fade-enter-active,
.search-fade-leave-active {
  transition: opacity 0.12s ease;
}
.search-fade-enter-from,
.search-fade-leave-to {
  opacity: 0;
}
.search-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(2px);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 14vh;
  z-index: var(--sf-z-modal);
}
.search-modal {
  width: min(560px, 92vw);
  max-height: 60vh;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.search-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 11px 14px;
  background: var(--sf-bg-1);
  border-bottom: 1px solid var(--sf-border);
}
.search-icon {
  color: var(--sf-text-3);
  flex-shrink: 0;
}
.search-input {
  flex: 1;
  background: transparent;
  border: none;
  padding: 2px 0;
  color: var(--sf-text-0);
  font-size: 0.875rem;
  outline: none;
}
.search-hint {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
}
.search-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px;
}
.search-empty {
  padding: 18px 12px;
  color: var(--sf-text-3);
  font-style: italic;
  text-align: center;
}
.item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  background: transparent;
  border-radius: var(--sf-radius-sm);
  text-align: left;
  cursor: pointer;
  color: var(--sf-text-1);
  margin-bottom: 1px;
}
.item.active,
.item:focus {
  background: var(--sf-accent-dim);
  color: var(--sf-text-0);
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0.85;
}
.item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.item-label {
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  font-weight: 500;
  color: inherit;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.item-detail {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.kind-tag {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  padding: 1px 5px;
  background: var(--sf-bg-3);
  border-radius: 2px;
  flex-shrink: 0;
}
.search-footer {
  padding: 6px 14px;
  font-size: 0.625rem;
  color: var(--sf-text-3);
  border-top: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  font-family: var(--sf-font-mono);
  letter-spacing: 0.3px;
}
</style>
