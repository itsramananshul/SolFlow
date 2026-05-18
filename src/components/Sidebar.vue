<script setup lang="ts">
import { computed } from 'vue';
import { useUIStore } from '@/stores/ui.store';
import { useGraphStore } from '@/stores/graph.store';
import NodePalette from './NodePalette.vue';
import TypesPanel from './TypesPanel.vue';
import ImportsPanel from './ImportsPanel.vue';

const ui = useUIStore();
const graph = useGraphStore();

const typesCount = computed(
  () => graph.workflow.structs.length + graph.workflow.enums.length,
);
const importsCount = computed(() => graph.workflow.imports.length);
</script>

<template>
  <aside class="sidebar">
    <nav class="tabs">
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'palette' }"
        @click="ui.setSidebarTab('palette')"
      >
        Nodes
      </button>
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'types' }"
        @click="ui.setSidebarTab('types')"
      >
        Types<span v-if="typesCount > 0" class="count">{{ typesCount }}</span>
      </button>
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'imports' }"
        @click="ui.setSidebarTab('imports')"
      >
        Imports<span v-if="importsCount > 0" class="count">{{ importsCount }}</span>
      </button>
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'policies' }"
        @click="ui.setSidebarTab('policies')"
      >
        Policies
      </button>
    </nav>
    <div class="tab-body">
      <NodePalette v-if="ui.sidebarTab === 'palette'" />
      <TypesPanel v-else-if="ui.sidebarTab === 'types'" />
      <ImportsPanel v-else-if="ui.sidebarTab === 'imports'" />
      <div v-else class="policies-placeholder">
        <div class="policies-card">
          <div class="policies-title">Policies</div>
          <p class="policies-body">
            Per-workflow guardrails — rate limits, retries, idempotency keys,
            role-based access, audit logging.
          </p>
          <div class="policies-tag">Coming soon</div>
        </div>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  background: var(--sf-bg-1);
  border-right: 1px solid var(--sf-border);
  min-height: 0;
}
.tabs {
  display: flex;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
  padding: 0 4px;
  gap: 2px;
}
.tab {
  flex: 1;
  background: transparent;
  border: none;
  border-radius: 0;
  padding: 9px 0;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
  font-weight: 500;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  letter-spacing: 0.1px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
}
.tab:hover {
  color: var(--sf-text-0);
}
.tab.active {
  color: var(--sf-text-0);
  border-bottom-color: var(--sf-accent);
}
.count {
  display: inline-block;
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-2);
  background: var(--sf-bg-3);
  padding: 1px 5px;
  border-radius: 8px;
  letter-spacing: 0;
}
.tab.active .count {
  color: var(--sf-accent);
  background: var(--sf-accent-dim);
}
.tab-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.tab-body > * {
  flex: 1;
  min-height: 0;
}
.policies-placeholder {
  padding: 16px;
  overflow-y: auto;
}
.policies-card {
  border: 1px dashed var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  padding: 14px;
  background: var(--sf-bg-0);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.policies-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--sf-text-0);
  letter-spacing: 0.2px;
}
.policies-body {
  margin: 0;
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  line-height: 1.5;
}
.policies-tag {
  align-self: flex-start;
  font-size: 0.5625rem;
  letter-spacing: 0.6px;
  text-transform: uppercase;
  padding: 2px 7px;
  color: var(--sf-cat-trigger);
  background: rgba(232, 166, 87, 0.12);
  border-radius: 2px;
  font-weight: 600;
}
</style>
