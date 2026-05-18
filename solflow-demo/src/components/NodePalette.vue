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
      <div class="cat-header" :style="{ color: categoryColor(cat) }">
        {{ CATEGORY_LABELS[cat] }}
      </div>
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
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
  font-size: 12px;
}
.cat-header {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
  padding: 2px 4px;
  margin-bottom: 4px;
}
.palette-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--sf-radius-sm);
  cursor: grab;
  transition: background 0.1s ease;
  user-select: none;
}
.palette-item:hover {
  background: var(--sf-bg-3);
}
.palette-item:active {
  cursor: grabbing;
  background: var(--sf-bg-4);
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.label {
  color: var(--sf-text-1);
}
</style>
