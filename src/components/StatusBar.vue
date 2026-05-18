<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from 'vue';
import { useGraphStore } from '@/stores/graph.store';

const graph = useGraphStore();

const fn = computed(() => graph.activeFunction);
const nodeCount = computed(() => fn.value?.nodes.length ?? 0);
const edgeCount = computed(() => fn.value?.edges.length ?? 0);
const fnCount = computed(() => graph.workflow.functions.length);

const lastSavedAt = computed(() => graph.workflow.meta.updatedAt);
const now = ref(Date.now());
let tick: number | undefined;
onMounted(() => {
  tick = window.setInterval(() => (now.value = Date.now()), 1000);
});
onBeforeUnmount(() => {
  if (tick !== undefined) window.clearInterval(tick);
});

const savedAgo = computed(() => {
  if (!lastSavedAt.value) return '';
  const sec = Math.max(0, Math.floor((now.value - Date.parse(lastSavedAt.value)) / 1000));
  if (sec < 5) return 'just now';
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  return `${hr}h ago`;
});

const errorCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'error').length,
);
const warningCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'warning').length,
);
</script>

<template>
  <footer class="status-bar">
    <div class="left">
      <span class="cell">
        <span class="dot acc" />
        <span class="label">function</span>
        <code>{{ fn?.name ?? '—' }}</code>
      </span>
      <span class="cell">
        <span class="label">nodes</span>
        <code>{{ nodeCount }}</code>
      </span>
      <span class="cell">
        <span class="label">edges</span>
        <code>{{ edgeCount }}</code>
      </span>
      <span class="cell">
        <span class="label">fns</span>
        <code>{{ fnCount }}</code>
      </span>
    </div>
    <div class="right">
      <span v-if="errorCount > 0" class="cell err">
        <span class="dot err-dot" />
        {{ errorCount }} error{{ errorCount === 1 ? '' : 's' }}
      </span>
      <span v-if="warningCount > 0" class="cell warn">
        <span class="dot warn-dot" />
        {{ warningCount }} warning{{ warningCount === 1 ? '' : 's' }}
      </span>
      <span v-if="errorCount === 0 && warningCount === 0" class="cell ok">
        <span class="dot ok-dot" />
        clean
      </span>
      <span class="cell">
        <span class="label">autosaved</span>
        <span class="time">{{ savedAgo }}</span>
      </span>
    </div>
  </footer>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  height: 24px;
  background: var(--sf-bg-0);
  border-top: 1px solid var(--sf-border);
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  flex-shrink: 0;
}
.left,
.right {
  display: flex;
  align-items: center;
  gap: 14px;
}
.cell {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  white-space: nowrap;
}
.label {
  color: var(--sf-text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
  font-size: 0.5625rem;
}
code {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
  font-size: 0.625rem;
}
.dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot.acc {
  background: var(--sf-accent);
}
.dot.err-dot {
  background: var(--sf-error);
}
.dot.warn-dot {
  background: var(--sf-warning);
}
.dot.ok-dot {
  background: var(--sf-success);
}
.cell.err {
  color: var(--sf-error);
}
.cell.warn {
  color: var(--sf-warning);
}
.cell.ok {
  color: var(--sf-success);
}
.time {
  color: var(--sf-text-2);
}
</style>
