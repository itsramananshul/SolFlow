<script setup lang="ts">
/**
 * Blocks panel — reusable composition building blocks.
 *
 * Two sections:
 *   Patterns       — built-in factory subflows (retry, validation,
 *                    logging, etc.) defined in src/graph/blocks.ts.
 *   Your blocks    — user-saved selections, persisted via blocks.store
 *                    to localStorage.
 *
 * Drag any entry onto the canvas to insert. The Canvas handles drop
 * events with dataTransfer key "application/x-solflow-block".
 *
 * Rename / delete actions on user blocks only — built-ins are immutable.
 */
import { computed } from 'vue';
import { useBlocksStore } from '@/stores/blocks.store';
import { listBuiltinPatterns } from '@/graph/blocks';

const blocks = useBlocksStore();

interface RowEntry {
  origin: 'user' | 'builtin';
  id: string;        // user id for saved blocks; patternId for builtins
  name: string;
  description: string;
}

const patternRows = computed<RowEntry[]>(() =>
  listBuiltinPatterns().map((p) => ({
    origin: 'builtin',
    id: p.patternId,
    name: p.name,
    description: p.description,
  })),
);

const userRows = computed<RowEntry[]>(() =>
  blocks.userBlocks.map((b) => ({
    origin: 'user',
    id: b.id,
    name: b.name,
    description: b.description,
  })),
);

function onDragStart(event: DragEvent, row: RowEntry) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData(
    'application/x-solflow-block',
    JSON.stringify({ origin: row.origin, id: row.id }),
  );
  event.dataTransfer.effectAllowed = 'copy';
}

function renameBlock(id: string) {
  const b = blocks.findById(id);
  if (!b) return;
  // Native prompt — Phase A keeps this lightweight; a fancier rename
  // modal can come later if needed.
  const next = window.prompt('Rename block', b.name);
  if (next !== null && next.trim() !== '') {
    blocks.rename(id, next);
  }
}

function deleteBlock(id: string) {
  const b = blocks.findById(id);
  if (!b) return;
  if (window.confirm(`Delete reusable block "${b.name}"? This can't be undone.`)) {
    blocks.remove(id);
  }
}
</script>

<template>
  <div class="blocks-panel">
    <div class="head">
      <span class="title">Blocks</span>
      <span class="hint">drag to canvas</span>
    </div>

    <div class="body">
      <!-- Patterns section -->
      <div class="section">
        <div class="section-head">
          <span>Patterns</span>
          <span class="section-count">{{ patternRows.length }}</span>
        </div>
        <div
          v-for="row in patternRows"
          :key="`b:${row.id}`"
          class="block-item builtin"
          draggable="true"
          :title="row.description"
          @dragstart="onDragStart($event, row)"
        >
          <div class="row-main">
            <span class="row-icon">▩</span>
            <span class="row-name">{{ row.name }}</span>
            <span class="row-tag">builtin</span>
          </div>
          <div class="row-desc">{{ row.description }}</div>
        </div>
      </div>

      <!-- User blocks section -->
      <div class="section">
        <div class="section-head">
          <span>Your blocks</span>
          <span class="section-count">{{ userRows.length }}</span>
        </div>
        <div v-if="userRows.length === 0" class="section-empty">
          Select nodes on the canvas, then right-click → <em>"Save selection as block"</em> to add one here.
        </div>
        <div
          v-for="row in userRows"
          :key="`u:${row.id}`"
          class="block-item user"
          draggable="true"
          :title="row.description || row.name"
          @dragstart="onDragStart($event, row)"
        >
          <div class="row-main">
            <span class="row-icon">⌥</span>
            <span class="row-name">{{ row.name }}</span>
            <div class="row-actions nodrag">
              <button
                type="button"
                class="row-btn"
                title="Rename"
                :aria-label="`Rename block ${row.name}`"
                @click.stop="renameBlock(row.id)"
              >✎</button>
              <button
                type="button"
                class="row-btn danger"
                title="Delete"
                :aria-label="`Delete block ${row.name}`"
                @click.stop="deleteBlock(row.id)"
              >✕</button>
            </div>
          </div>
          <div v-if="row.description" class="row-desc">{{ row.description }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.blocks-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.head {
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
.body {
  flex: 1;
  overflow-y: auto;
  padding: 6px 6px 12px;
}
.section {
  margin-bottom: 14px;
}
.section-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 4px 6px;
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--sf-text-3);
  font-weight: 600;
}
.section-count {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
}
.section-empty {
  padding: 10px 8px;
  font-size: 0.6875rem;
  color: var(--sf-text-3);
  line-height: 1.5;
}
.section-empty em {
  color: var(--sf-text-2);
  font-style: normal;
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
}

.block-item {
  border: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  border-radius: var(--sf-radius-sm);
  padding: 7px 8px;
  margin-bottom: 4px;
  cursor: grab;
  transition: border-color 0.12s ease, background 0.12s ease;
}
.block-item:hover {
  background: var(--sf-bg-2);
  border-color: var(--sf-border-strong);
}
.block-item:active {
  cursor: grabbing;
  background: var(--sf-bg-3);
}
.block-item.builtin {
  border-left: 2px solid var(--sf-cat-flow);
}
.block-item.user {
  border-left: 2px solid var(--sf-cat-trigger);
}

.row-main {
  display: flex;
  align-items: center;
  gap: 7px;
}
.row-icon {
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  color: var(--sf-text-2);
  flex-shrink: 0;
}
.block-item.builtin .row-icon {
  color: var(--sf-cat-flow);
}
.block-item.user .row-icon {
  color: var(--sf-cat-trigger);
}
.row-name {
  flex: 1;
  font-size: 0.75rem;
  color: var(--sf-text-0);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-tag {
  font-family: var(--sf-font-mono);
  font-size: 0.5rem;
  letter-spacing: 0.5px;
  color: var(--sf-text-3);
  text-transform: uppercase;
  padding: 1px 5px;
  background: var(--sf-bg-3);
  border-radius: 2px;
}
.row-desc {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  margin-top: 4px;
  line-height: 1.4;
  /* descriptions can wrap inside the card. */
}

.row-actions {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.block-item:hover .row-actions {
  opacity: 1;
}
.row-btn {
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  cursor: pointer;
  padding: 2px 5px;
  border-radius: 3px;
  font-size: 0.625rem;
}
.row-btn:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-3);
}
.row-btn.danger:hover {
  color: var(--sf-error);
  background: rgba(255, 77, 79, 0.12);
}
</style>
