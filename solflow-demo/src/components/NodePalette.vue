<script setup lang="ts">
import { computed } from 'vue';
import { paletteByCategory, CATEGORY_LABELS, categoryColor } from '@/graph/kinds';
import type { Category } from '@/graph/kinds';
import type { NodeKind } from '@/graph/schema';

const grouped = computed(() => paletteByCategory());

const order: Category[] = ['flow', 'variable', 'operator', 'literal', 'access', 'call', 'io'];

function onDragStart(event: DragEvent, kind: NodeKind) {
  event.dataTransfer?.setData('application/x-solflow-kind', kind);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
}
</script>

<template>
  <div class="palette">
    <div v-for="cat in order" :key="cat" class="cat">
      <div class="cat-header">{{ CATEGORY_LABELS[cat] }}</div>
      <div
        v-for="entry in grouped[cat]"
        :key="entry.kind"
        class="palette-item"
        draggable="true"
        @dragstart="onDragStart($event, entry.kind)"
        :title="entry.description"
      >
        <span class="dot" :style="{ background: categoryColor(cat) }" />
        <span class="label">{{ entry.label }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.palette {
  padding: 12px 10px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
  font-size: 12px;
}
.cat-header {
  font-size: 9px;
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
  font-size: 12px;
}
</style>
