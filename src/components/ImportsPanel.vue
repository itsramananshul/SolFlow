<script setup lang="ts">
import { useGraphStore } from '@/stores/graph.store';

const graph = useGraphStore();

function updatePath(id: string, value: string) {
  const parts = value.split('.').map((p) => p.trim()).filter((p) => p.length > 0);
  graph.updateImport(id, { path: parts });
}
</script>

<template>
  <div class="imports">
    <div class="section">
      <div class="section-header">
        <span>Imports</span>
        <button class="ghost" @click="graph.addImport()">+ Import</button>
      </div>
      <p class="note">
        Imports declare the modules your workflow calls. They appear at the
        top of the emitted SOL and are resolved by the controller at run time.
      </p>
      <div v-if="graph.workflow.imports.length === 0" class="empty">
        No imports.
      </div>
      <div v-for="imp in graph.workflow.imports" :key="imp.id" class="card">
        <div class="row">
          <input
            class="path"
            :value="imp.path.join('.')"
            placeholder="Router.App.Endpoint"
            @input="(e) => updatePath(imp.id, (e.target as HTMLInputElement).value)"
          />
        </div>
        <div class="row">
          <span class="lbl">as</span>
          <input
            class="alias"
            :value="imp.alias"
            placeholder="Alias"
            @input="(e) => graph.updateImport(imp.id, { alias: (e.target as HTMLInputElement).value })"
          />
          <button class="ghost danger" @click="graph.deleteImport(imp.id)">×</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.imports {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
  font-size: 0.75rem;
}
.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
  color: var(--sf-text-1);
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
}
.note {
  color: var(--sf-text-3);
  font-size: 0.6875rem;
  margin: 0 0 8px 0;
  line-height: 1.4;
}
.card {
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  padding: 6px;
  margin-bottom: 6px;
}
.row {
  gap: 4px;
  margin-bottom: 4px;
}
.path {
  flex: 1;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.alias {
  flex: 1;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.lbl {
  color: var(--sf-text-2);
  font-style: italic;
}
.empty {
  color: var(--sf-text-3);
}
.danger:hover {
  color: var(--sf-error);
}
</style>
