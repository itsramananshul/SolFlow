<script setup lang="ts">
import { onMounted } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import Toolbar from '@/components/Toolbar.vue';
import FunctionTabs from '@/components/FunctionTabs.vue';
import Sidebar from '@/components/Sidebar.vue';
import Canvas from '@/components/Canvas.vue';
import Inspector from '@/components/Inspector.vue';
import SourcePreview from '@/components/SourcePreview.vue';
import DiagnosticsDrawer from '@/components/DiagnosticsDrawer.vue';

const graph = useGraphStore();
const ui = useUIStore();

onMounted(() => {
  graph.bootstrap();
});
</script>

<template>
  <div class="app">
    <Toolbar />
    <FunctionTabs />
    <div class="workspace">
      <Sidebar />
      <div class="canvas-region">
        <Canvas />
        <DiagnosticsDrawer v-if="ui.drawerOpen" />
      </div>
      <div class="right-pane">
        <Inspector />
        <SourcePreview />
      </div>
    </div>
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}
.workspace {
  display: grid;
  grid-template-columns: 240px 1fr 380px;
  flex: 1;
  min-height: 0;
}
.canvas-region {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.right-pane {
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  min-height: 0;
}
</style>
