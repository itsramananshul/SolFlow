<script setup lang="ts">
/**
 * Quick-add command palette. Floats near the cursor and lets the user
 * type to insert a node. Used for:
 *   - Space hotkey (insert at last cursor)
 *   - Double-click on pane (insert where clicked)
 *   - Cmd/Ctrl+K (insert at viewport center)
 *   - Drag-edge-to-empty (insert + auto-connect to the dangling edge's
 *     source port; the caller passes sourceContext)
 *
 * Filters PALETTE entries via a small fuzzy scorer (label > kind >
 * description > subsequence). Up/Down navigates; Enter inserts; Esc
 * cancels.
 */
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from 'vue';
import {
  PALETTE,
  CATEGORY_LABELS,
  categoryColor,
  isAdvancedCategory,
  type PaletteEntry,
} from '@/graph/kinds';
import type { NodeData, NodeKind, SolType } from '@/graph/schema';
import { listBuiltinPatterns } from '@/graph/blocks';
import { useBlocksStore } from '@/stores/blocks.store';

export interface SourceContext {
  nodeId: string;
  portId: string;
  edgeKind: 'control' | 'data';
  type?: SolType;
}

const props = defineProps<{
  open: boolean;
  /** Screen-space anchor (top-left of the popover). */
  x: number;
  y: number;
  /** Optional: drag-from-port context, will auto-connect on select. */
  sourceContext?: SourceContext;
}>();
const emit = defineEmits<{
  (
    e: 'select',
    kind: NodeKind,
    ctx?: SourceContext,
    initialData?: Partial<NodeData>,
  ): void;
  (e: 'select-block', payload: { origin: 'user' | 'builtin'; id: string }): void;
  (e: 'close'): void;
}>();

const blocks = useBlocksStore();

const query = ref('');
const activeIdx = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);
const listRef = ref<HTMLDivElement | null>(null);

// Unified entry shape — palette node kinds AND reusable blocks (built-in
// patterns + user-saved blocks) share one list so the user can search
// "retry" or "branch" without thinking about which kind of thing they
// want. Block entries are suppressed when a sourceContext is present
// (auto-connect into a multi-node cluster isn't well-defined).
type QuickEntry =
  | {
      kind: 'node';
      label: string;
      description: string;
      searchKey: string; // node-kind id, used for stable Vue keys
      categoryLabel: string;
      categoryColor: string;
      node: PaletteEntry;
    }
  | {
      kind: 'block';
      label: string;
      description: string;
      searchKey: string;
      categoryLabel: string;
      categoryColor: string;
      block: { origin: 'user' | 'builtin'; id: string; name: string };
    };

const nodeEntries = computed<QuickEntry[]>(() => {
  return PALETTE.filter((p) => p.draggable).map<QuickEntry>((p) => ({
    kind: 'node',
    label: p.label,
    description: p.description,
    searchKey: `node:${p.kind}`,
    categoryLabel: CATEGORY_LABELS[p.category],
    categoryColor: categoryColor(p.category),
    node: p,
  }));
});

const blockEntries = computed<QuickEntry[]>(() => {
  // Blocks land as multi-node clusters — they can't be auto-connected
  // to a single source port. Hide them in port-drag flows; show in all
  // others (Space, ⌘K, double-click on empty).
  if (props.sourceContext) return [];
  const builtin: QuickEntry[] = listBuiltinPatterns().map((p) => ({
    kind: 'block',
    label: p.name,
    description: p.description,
    searchKey: `block:builtin:${p.patternId}`,
    categoryLabel: 'Pattern',
    categoryColor: 'var(--sf-cat-flow)',
    block: { origin: 'builtin', id: p.patternId, name: p.name },
  }));
  const user: QuickEntry[] = blocks.userBlocks.map((b) => ({
    kind: 'block',
    label: b.name,
    description: b.description || 'Saved reusable block',
    searchKey: `block:user:${b.id}`,
    categoryLabel: 'Your block',
    categoryColor: 'var(--sf-cat-trigger)',
    block: { origin: 'user', id: b.id, name: b.name },
  }));
  return [...user, ...builtin];
});

// Empty-query view: keep the common path tidy. Advanced node categories
// (operator/literal/access) are filtered out unless the user types
// something — they're searchable but not on display by default.
const visibleEntries = computed<QuickEntry[]>(() => {
  const q = query.value.trim();
  if (q !== '') {
    return [...blockEntries.value, ...nodeEntries.value];
  }
  return [
    ...blockEntries.value,
    ...nodeEntries.value.filter(
      (e) => e.kind !== 'node' || !isAdvancedCategory(e.node.category),
    ),
  ];
});

// Score each entry against the query. 0 means "doesn't match".
function score(q: string, entry: QuickEntry): number {
  if (!q) {
    // No query: blocks float to the top of the list as they're more
    // valuable "starting points" than individual nodes.
    return entry.kind === 'block' ? 2 : 1;
  }
  const ql = q.toLowerCase();
  const label = entry.label.toLowerCase();
  const id = entry.searchKey.toLowerCase();
  const desc = entry.description.toLowerCase();
  if (label === ql) return 10000;
  if (id.endsWith(`:${ql}`)) return 9000;
  if (label.startsWith(ql)) return 5000 + (100 - label.length);
  if (id.includes(ql)) return 4500;
  if (label.includes(ql)) return 3000 + (100 - label.length);
  if (desc.includes(ql)) return 1000;
  let qi = 0;
  for (let i = 0; i < label.length && qi < ql.length; i++) {
    if (label[i] === ql[qi]) qi++;
  }
  if (qi === ql.length) return 500;
  return 0;
}

const filtered = computed<QuickEntry[]>(() => {
  const q = query.value.trim();
  const scored = visibleEntries.value
    .map((e) => ({ e, s: score(q, e) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s);
  return scored.map((x) => x.e);
});

// Reset state every time the palette opens.
watch(
  () => props.open,
  (now) => {
    if (now) {
      query.value = '';
      activeIdx.value = 0;
      nextTick(() => inputRef.value?.focus());
    }
  },
  { immediate: true },
);

// Keep the active item visible when arrowing through a long list.
watch(activeIdx, () => {
  nextTick(() => {
    const el = listRef.value?.querySelector(`.item[data-i="${activeIdx.value}"]`);
    if (el && 'scrollIntoView' in el) {
      (el as HTMLElement).scrollIntoView({ block: 'nearest' });
    }
  });
});

// Keep activeIdx in bounds when filtering changes.
watch(filtered, () => {
  if (activeIdx.value >= filtered.value.length) activeIdx.value = 0;
});

function pickIndex(i: number) {
  const entry = filtered.value[i];
  if (!entry) return;
  if (entry.kind === 'block') {
    emit('select-block', { origin: entry.block.origin, id: entry.block.id });
  } else {
    emit('select', entry.node.kind, props.sourceContext, entry.node.initialData);
  }
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
    activeIdx.value =
      (activeIdx.value - 1 + filtered.value.length) % filtered.value.length;
  } else if (e.key === 'Enter') {
    e.preventDefault();
    pickIndex(activeIdx.value);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

// Click outside closes the palette.
function onDocClick(e: MouseEvent) {
  if (!props.open) return;
  const t = e.target as HTMLElement;
  if (!t.closest('.qa-popover')) emit('close');
}
// Window-level Escape catcher. The input's @keydown only fires while the
// search input has focus — and there are short focus-gap windows (e.g.
// drag-edge-to-empty opens the palette before nextTick autofocuses it)
// where Escape would otherwise do nothing and the user perceives the
// palette as "stuck". Bind on window so Escape always dismisses.
function onWinKey(e: KeyboardEvent) {
  if (!props.open) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}
// If `open` is flipped to false externally (e.g. a node was selected, a
// new workflow was loaded), the input may still have focus — blur it so
// future Space/keystrokes go back to the canvas, not the palette input.
watch(
  () => props.open,
  (now) => {
    if (!now) inputRef.value?.blur();
  },
);
onMounted(() => {
  // Capture phase, not bubble: Vue Flow's node drag-start handler calls
  // event.stopPropagation() on mousedown in the bubble phase, which
  // otherwise prevents this listener from ever firing when the user
  // clicks a node. Capture phase runs first, so this always fires
  // regardless of who later stops propagation.
  document.addEventListener('mousedown', onDocClick, true);
  window.addEventListener('keydown', onWinKey);
});
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocClick, true);
  window.removeEventListener('keydown', onWinKey);
});

// Adjust position to stay inside the viewport.
const POPOVER_W = 320;
const POPOVER_H = 360;
const adjusted = computed(() => {
  if (typeof window === 'undefined') return { x: 0, y: 0 };
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  return {
    x: Math.min(props.x, vw - POPOVER_W - 12),
    y: Math.min(props.y, vh - POPOVER_H - 12),
  };
});

const headline = computed(() => {
  if (!props.sourceContext) return 'Add node';
  return props.sourceContext.edgeKind === 'control'
    ? 'Add node + connect (control)'
    : 'Add node + connect (data)';
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="qa-popover"
      :style="{ left: adjusted.x + 'px', top: adjusted.y + 'px' }"
      @click.stop
    >
      <div class="qa-header">
        <svg viewBox="0 0 16 16" width="11" height="11" class="search-icon" fill="none">
          <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.5" />
          <path
            d="M10.5 10.5 L13.5 13.5"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
        <input
          ref="inputRef"
          v-model="query"
          class="qa-input"
          placeholder="Add node…"
          spellcheck="false"
          @keydown="onKeyDown"
        />
        <span class="qa-hint">↑↓ Enter</span>
      </div>
      <div v-if="sourceContext" class="qa-meta">{{ headline }}</div>
      <div ref="listRef" class="qa-list">
        <div v-if="filtered.length === 0" class="qa-empty">
          No matches for "{{ query }}"
        </div>
        <button
          v-for="(entry, i) in filtered"
          :key="entry.searchKey"
          :data-i="i"
          class="item"
          :class="[{ active: i === activeIdx }, `kind-${entry.kind}`]"
          @mousedown.prevent
          @click="pickIndex(i)"
          @mouseenter="activeIdx = i"
        >
          <span class="dot" :style="{ background: entry.categoryColor }" />
          <div class="item-body">
            <div class="item-label">
              <span v-if="entry.kind === 'block'" class="block-glyph">▩</span>
              {{ entry.label }}
            </div>
            <div class="item-desc">{{ entry.description }}</div>
          </div>
          <span class="cat-tag">{{ entry.categoryLabel }}</span>
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qa-popover {
  position: fixed;
  z-index: var(--sf-z-popover);
  /* Fluid width so the popover stays usable on a 1366px laptop without
     leaking past the viewport edge on tiny screens. */
  width: clamp(260px, 28vw, 360px);
  max-height: min(360px, 60vh);
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-3);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  font-size: 0.75rem;
}
.qa-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
}
.search-icon {
  color: var(--sf-text-3);
  flex-shrink: 0;
}
.qa-input {
  flex: 1;
  background: transparent;
  border: none;
  padding: 2px 0;
  color: var(--sf-text-0);
  font-size: 0.8125rem;
  outline: none;
}
.qa-hint {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
}
.qa-meta {
  padding: 6px 12px;
  font-size: 0.625rem;
  color: var(--sf-accent);
  background: rgba(50, 145, 255, 0.06);
  border-bottom: 1px solid var(--sf-border);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}
.qa-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px;
}
.qa-empty {
  padding: 14px 10px;
  color: var(--sf-text-3);
  font-style: italic;
  text-align: center;
}
.item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 6px 8px;
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
  opacity: 0.8;
}
.item-body {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.item-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: inherit;
}
.item-desc {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cat-tag {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  padding: 1px 5px;
  background: var(--sf-bg-3);
  border-radius: 2px;
  flex-shrink: 0;
}
.item.kind-block .cat-tag {
  color: var(--sf-cat-trigger);
  background: rgba(232, 166, 87, 0.12);
}
.item.kind-block .item-label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.block-glyph {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-cat-flow);
  flex-shrink: 0;
}
.item.kind-block .block-glyph {
  color: var(--sf-cat-trigger);
}
</style>
