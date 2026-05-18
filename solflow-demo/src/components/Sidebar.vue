<script setup lang="ts">
import { useUIStore } from '@/stores/ui.store';
import NodePalette from './NodePalette.vue';
import TypesPanel from './TypesPanel.vue';
import ImportsPanel from './ImportsPanel.vue';

const ui = useUIStore();
</script>

<template>
  <aside class="sidebar">
    <nav class="tabs">
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'palette' }"
        @click="ui.setSidebarTab('palette')"
      >
        Palette
      </button>
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'types' }"
        @click="ui.setSidebarTab('types')"
      >
        Types
      </button>
      <button
        class="tab"
        :class="{ active: ui.sidebarTab === 'imports' }"
        @click="ui.setSidebarTab('imports')"
      >
        Imports
      </button>
    </nav>
    <div class="tab-body">
      <NodePalette v-if="ui.sidebarTab === 'palette'" />
      <TypesPanel v-else-if="ui.sidebarTab === 'types'" />
      <ImportsPanel v-else />
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
}
.tab {
  flex: 1;
  background: transparent;
  border: none;
  border-radius: 0;
  padding: 8px 0;
  color: var(--sf-text-2);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  border-bottom: 2px solid transparent;
}
.tab:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-1);
}
.tab.active {
  color: var(--sf-text-0);
  border-bottom-color: var(--sf-accent);
  background: var(--sf-bg-1);
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
</style>
