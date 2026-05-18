<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  paletteByCategory,
  CATEGORY_LABELS,
  categoryColor,
  isAdvancedCategory,
  PALETTE,
} from '@/graph/kinds';
import type { Category, PaletteEntry } from '@/graph/kinds';

const grouped = computed(() => paletteByCategory());

// Primary categories rendered top-level. Operator / Literal / Access live
// behind the Advanced toggle because their names lean on SOL's AST and
// they're rarely the first thing a new user needs.
const primaryOrder: Category[] = [
  'entry',
  'trigger',
  'flow',
  'variable',
  'call',
  'io',
  'annotation',
];
const advancedOrder: Category[] = ['operator', 'literal', 'access'];

const advancedOpen = ref(false);
function toggleAdvanced() {
  advancedOpen.value = !advancedOpen.value;
}

const query = ref('');

// When user types in the search input, show a flat filtered list.
const filtered = computed<PaletteEntry[]>(() => {
  const q = query.value.trim().toLowerCase();
  if (q === '') return [];
  return PALETTE.filter(
    (p) =>
      p.draggable &&
      (p.label.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.kind.toLowerCase().includes(q)),
  );
});

function onDragStart(event: DragEvent, entry: PaletteEntry) {
  event.dataTransfer?.setData('application/x-solflow-kind', entry.kind);
  if (entry.initialData) {
    event.dataTransfer?.setData(
      'application/x-solflow-init',
      JSON.stringify(entry.initialData),
    );
  }
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
}

function clearSearch() {
  query.value = '';
}
</script>

<template>
  <div class="palette">
    <div class="search">
      <svg viewBox="0 0 16 16" width="11" height="11" class="search-icon" fill="none">
        <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.5" />
        <path d="M10.5 10.5 L13.5 13.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
      <input
        v-model="query"
        type="search"
        placeholder="Search nodes…"
        spellcheck="false"
        class="search-input"
      />
      <button v-if="query" class="search-clear" @click="clearSearch" title="Clear">
        <svg viewBox="0 0 12 12" width="9" height="9" fill="none">
          <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>

    <!-- Filtered flat list when searching -->
    <div v-if="query.trim()" class="results">
      <div v-if="filtered.length === 0" class="empty">
        No nodes match "{{ query }}"
      </div>
      <div
        v-for="entry in filtered"
        :key="entry.kind"
        class="palette-item"
        draggable="true"
        @dragstart="onDragStart($event, entry)"
        :title="entry.description"
      >
        <span class="dot" :style="{ background: categoryColor(entry.category) }" />
        <span class="label">{{ entry.label }}</span>
        <span class="cat-tag">{{ CATEGORY_LABELS[entry.category] }}</span>
      </div>
    </div>

    <!-- Grouped categories when not searching -->
    <template v-else>
      <div v-for="cat in primaryOrder" :key="cat" class="cat">
        <div class="cat-header">{{ CATEGORY_LABELS[cat] }}</div>
        <div
          v-for="entry in grouped[cat]"
          :key="entry.kind"
          class="palette-item"
          draggable="true"
          @dragstart="onDragStart($event, entry)"
          :title="entry.description"
        >
          <span class="dot" :style="{ background: categoryColor(cat) }" />
          <span class="label">{{ entry.label }}</span>
        </div>
      </div>

      <!--
        Advanced disclosure: operator / literal / access nodes are useful
        but their labels echo SOL's AST (binaryOp, structLiteral, etc.).
        Hidden by default so new users see a short approachable list.
      -->
      <div class="advanced-section">
        <button
          type="button"
          class="advanced-toggle"
          :class="{ open: advancedOpen }"
          @click="toggleAdvanced"
        >
          <span class="caret">{{ advancedOpen ? '▾' : '▸' }}</span>
          <span class="advanced-label">Advanced</span>
          <span class="advanced-sub">expression nodes</span>
        </button>
        <div v-if="advancedOpen" class="advanced-body">
          <div v-for="cat in advancedOrder" :key="cat" class="cat">
            <div class="cat-header">{{ CATEGORY_LABELS[cat] }}</div>
            <div
              v-for="entry in grouped[cat]"
              :key="entry.kind"
              class="palette-item"
              draggable="true"
              @dragstart="onDragStart($event, entry)"
              :title="entry.description"
            >
              <span class="dot" :style="{ background: categoryColor(cat) }" />
              <span class="label">{{ entry.label }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.palette {
  padding: 8px 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
  font-size: 0.75rem;
}
.search {
  position: relative;
  display: flex;
  align-items: center;
}
.search-icon {
  position: absolute;
  left: 8px;
  color: var(--sf-text-3);
  pointer-events: none;
}
.search-input {
  width: 100%;
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  padding: 6px 26px 6px 24px;
  font-size: 0.75rem;
  color: var(--sf-text-0);
}
.search-input::-webkit-search-cancel-button {
  display: none;
}
.search-clear {
  position: absolute;
  right: 6px;
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  cursor: pointer;
  padding: 2px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.search-clear:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-3);
  border-radius: 2px;
}
.cat {
  display: flex;
  flex-direction: column;
}
.cat-header {
  font-size: 0.5625rem;
  font-weight: 600;
  letter-spacing: 0.8px;
  text-transform: uppercase;
  padding: 2px 6px;
  margin-bottom: 4px;
  color: var(--sf-text-3);
}
.palette-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 6px;
  border-radius: var(--sf-radius-sm);
  cursor: grab;
  transition: background 0.1s ease, color 0.1s ease;
  user-select: none;
  color: var(--sf-text-1);
}
.palette-item:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.palette-item:active {
  cursor: grabbing;
  background: var(--sf-bg-3);
}
.dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0.7;
}
.label {
  font-size: 0.75rem;
}
.cat-tag {
  margin-left: auto;
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  padding: 1px 5px;
  background: var(--sf-bg-2);
  border-radius: 2px;
}
.results {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.empty {
  padding: 12px 6px;
  color: var(--sf-text-3);
  font-style: italic;
}
.advanced-section {
  margin-top: 4px;
  border-top: 1px dashed var(--sf-border);
  padding-top: 8px;
}
.advanced-toggle {
  display: flex;
  align-items: baseline;
  gap: 6px;
  width: 100%;
  background: transparent;
  border: none;
  padding: 4px 4px;
  cursor: pointer;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
  letter-spacing: 0.2px;
  border-radius: var(--sf-radius-sm);
}
.advanced-toggle:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-2);
}
.advanced-toggle .caret {
  font-family: var(--sf-font-mono);
}
.advanced-toggle .advanced-label {
  font-weight: 600;
}
.advanced-toggle .advanced-sub {
  color: var(--sf-text-3);
  font-size: 0.625rem;
  margin-left: auto;
}
.advanced-body {
  margin-top: 4px;
}
</style>
