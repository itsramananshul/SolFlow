<script setup lang="ts">
import { computed } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';

const graph = useGraphStore();
const ui = useUIStore();

const errorCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'error').length,
);
const warningCount = computed(
  () => graph.diagnostics.filter((d) => d.severity === 'warning').length,
);

function jumpTo(d: { nodeId?: string; functionId?: string }) {
  if (d.functionId) graph.setActiveFunction(d.functionId);
  if (d.nodeId) ui.selectNode(d.nodeId);
}
</script>

<template>
  <div class="drawer">
    <div class="header">
      <div class="header-left">
        <span class="title">Diagnostics</span>
        <span v-if="errorCount > 0" class="count err">
          <span class="dot err" />
          {{ errorCount }} {{ errorCount === 1 ? 'error' : 'errors' }}
        </span>
        <span v-if="warningCount > 0" class="count warn">
          <span class="dot warn" />
          {{ warningCount }} {{ warningCount === 1 ? 'warning' : 'warnings' }}
        </span>
        <span v-if="graph.diagnostics.length === 0" class="count ok">
          <span class="dot ok" />
          all clear
        </span>
      </div>
      <button class="ghost" @click="ui.toggleDrawer">Hide</button>
    </div>
    <div class="list">
      <div v-if="graph.diagnostics.length === 0" class="empty">
        Graph is clean — no issues detected.
      </div>
      <div
        v-for="(d, i) in graph.diagnostics"
        :key="i"
        class="row"
        :class="d.severity"
        @click="jumpTo(d)"
      >
        <span class="dot" :class="d.severity" />
        <span class="code">{{ d.code }}</span>
        <span class="msg">{{ d.message }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.drawer {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 240px;
  background: var(--sf-bg-0);
  border-top: 1px solid var(--sf-border);
  display: flex;
  flex-direction: column;
  z-index: var(--sf-z-drawer);
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  background: var(--sf-bg-1);
  border-bottom: 1px solid var(--sf-border);
}
.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}
.title {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.count {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 0.6875rem;
  color: var(--sf-text-2);
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}
.dot.error,
.dot.err {
  background: var(--sf-error);
}
.dot.warning,
.dot.warn {
  background: var(--sf-warning);
}
.dot.ok {
  background: var(--sf-success);
}
.list {
  flex: 1;
  overflow-y: auto;
  padding: 6px 8px;
}
.empty {
  color: var(--sf-text-3);
  padding: 16px;
  text-align: center;
  font-size: 0.6875rem;
  font-style: italic;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: var(--sf-radius-sm);
  font-size: 0.75rem;
  cursor: pointer;
  border-left: 2px solid transparent;
  transition: background 0.1s ease;
}
.row:hover {
  background: var(--sf-bg-2);
}
.row.error {
  border-left-color: var(--sf-error);
}
.row.warning {
  border-left-color: var(--sf-warning);
}
.code {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.625rem;
  background: var(--sf-bg-2);
  padding: 1px 5px;
  border-radius: 2px;
  flex-shrink: 0;
}
.msg {
  color: var(--sf-text-1);
  flex: 1;
}
</style>
