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
  type PaletteEntry,
} from '@/graph/kinds';
import type { NodeKind, SolType } from '@/graph/schema';

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
  (e: 'select', kind: NodeKind, ctx?: SourceContext): void;
  (e: 'close'): void;
}>();

const query = ref('');
const activeIdx = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);
const listRef = ref<HTMLDivElement | null>(null);

// All draggable palette entries (Start is not user-insertable).
const baseEntries = computed(() => PALETTE.filter((p) => p.draggable));

// Score each entry against the query. 0 means "doesn't match".
function score(q: string, entry: PaletteEntry): number {
  if (!q) return 1; // everything passes with neutral score when empty
  const ql = q.toLowerCase();
  const label = entry.label.toLowerCase();
  const kind = entry.kind.toLowerCase();
  const desc = entry.description.toLowerCase();
  if (label === ql) return 10000;
  if (kind === ql) return 9000;
  if (label.startsWith(ql)) return 5000 + (100 - label.length);
  if (kind.startsWith(ql)) return 4500 + (100 - kind.length);
  if (label.includes(ql)) return 3000 + (100 - label.length);
  if (kind.includes(ql)) return 2500;
  if (desc.includes(ql)) return 1000;
  // subsequence fallback (qchars must appear in order in label)
  let qi = 0;
  for (let i = 0; i < label.length && qi < ql.length; i++) {
    if (label[i] === ql[qi]) qi++;
  }
  if (qi === ql.length) return 500;
  return 0;
}

const filtered = computed<PaletteEntry[]>(() => {
  const q = query.value.trim();
  const scored = baseEntries.value
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
  emit('select', entry.kind, props.sourceContext);
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
onMounted(() => {
  document.addEventListener('mousedown', onDocClick);
});
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocClick);
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
          :key="entry.kind"
          :data-i="i"
          class="item"
          :class="{ active: i === activeIdx }"
          @mousedown.prevent
          @click="pickIndex(i)"
          @mouseenter="activeIdx = i"
        >
          <span class="dot" :style="{ background: categoryColor(entry.category) }" />
          <div class="item-body">
            <div class="item-label">{{ entry.label }}</div>
            <div class="item-desc">{{ entry.description }}</div>
          </div>
          <span class="cat-tag">{{ CATEGORY_LABELS[entry.category] }}</span>
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qa-popover {
  position: fixed;
  z-index: 200;
  width: 320px;
  max-height: 360px;
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
</style>
