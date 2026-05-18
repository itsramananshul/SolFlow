<script setup lang="ts">
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';

const graph = useGraphStore();
const ui = useUIStore();

function jumpTo(d: { nodeId?: string; functionId?: string }) {
  if (d.functionId) graph.setActiveFunction(d.functionId);
  if (d.nodeId) ui.selectNode(d.nodeId);
}
</script>

<template>
  <div class="drawer">
    <div class="header">
      <span class="title">Diagnostics</span>
      <button class="ghost" @click="ui.toggleDrawer">Hide</button>
    </div>
    <div class="list">
      <div v-if="graph.diagnostics.length === 0" class="empty">
        No diagnostics — graph is clean.
      </div>
      <div
        v-for="(d, i) in graph.diagnostics"
        :key="i"
        class="row"
        :class="d.severity"
        @click="jumpTo(d)"
      >
        <span class="badge" :class="d.severity">{{ d.severity }}</span>
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
  height: 200px;
  background: var(--sf-bg-0);
  border-top: 1px solid var(--sf-border);
  display: flex;
  flex-direction: column;
  z-index: 5;
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  background: var(--sf-bg-1);
  border-bottom: 1px solid var(--sf-border);
}
.title {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
}
.list {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}
.empty {
  color: var(--sf-text-3);
  padding: 16px;
  text-align: center;
  font-size: 0.6875rem;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: var(--sf-radius-sm);
  font-size: 0.6875rem;
  cursor: pointer;
}
.row:hover {
  background: var(--sf-bg-2);
}
.row.error {
  border-left: 3px solid var(--sf-error);
}
.row.warning {
  border-left: 3px solid var(--sf-warning);
}
.badge {
  font-size: 0.5625rem;
  text-transform: uppercase;
  padding: 1px 5px;
  border-radius: 3px;
  font-weight: 600;
}
.badge.error {
  background: var(--sf-error);
  color: white;
}
.badge.warning {
  background: var(--sf-warning);
  color: var(--sf-bg-0);
}
.code {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-3);
  font-size: 0.625rem;
}
.msg {
  color: var(--sf-text-1);
  flex: 1;
}
</style>
