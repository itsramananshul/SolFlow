<script setup lang="ts">
import { ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { typeLabel } from '@/graph/schema';
import type { Param, SolType, SolPrimitive } from '@/graph/schema';

const graph = useGraphStore();
const ui = useUIStore();

const signatureOpenFor = ref<string | null>(null);

function setActive(id: string) {
  graph.setActiveFunction(id);
  ui.selectNode(null);
}

function rename(id: string, e: Event) {
  const target = e.target as HTMLInputElement;
  graph.renameFunction(id, target.value);
}

function deleteFn(id: string) {
  if (graph.workflow.functions.length <= 1) return;
  if (!confirm('Delete this function and all its nodes?')) return;
  graph.deleteFunction(id);
}

function newFn() {
  graph.addFunction('fn');
}

function toggleSig(id: string) {
  signatureOpenFor.value = signatureOpenFor.value === id ? null : id;
}

const PRIMS: SolPrimitive[] = ['int', 'float', 'bool', 'str', 'char'];

function typeAsString(t: SolType): string {
  if (t.kind === 'named') return t.name;
  return t.kind;
}

function typeFromString(s: string): SolType {
  if (PRIMS.includes(s as SolPrimitive)) return { kind: s as SolPrimitive };
  if (s === 'void') return { kind: 'void' };
  return { kind: 'named', name: s };
}

function namedTypeOptions(): string[] {
  return [
    'void',
    ...PRIMS,
    ...graph.workflow.structs.map((s) => s.name),
    ...graph.workflow.enums.map((e) => e.name),
  ];
}

function addParam(fnId: string) {
  const fn = graph.workflow.functions.find((f) => f.id === fnId);
  if (!fn) return;
  const newParam: Param = { name: `p${fn.params.length}`, type: { kind: 'int' } };
  graph.updateFunctionSignature(fnId, [...fn.params, newParam], fn.returnType);
}
function removeParam(fnId: string, idx: number) {
  const fn = graph.workflow.functions.find((f) => f.id === fnId);
  if (!fn) return;
  graph.updateFunctionSignature(
    fnId,
    fn.params.filter((_, i) => i !== idx),
    fn.returnType,
  );
}
function updateParamName(fnId: string, idx: number, name: string) {
  const fn = graph.workflow.functions.find((f) => f.id === fnId);
  if (!fn) return;
  const params = fn.params.map((p, i) => (i === idx ? { ...p, name } : p));
  graph.updateFunctionSignature(fnId, params, fn.returnType);
}
function updateParamType(fnId: string, idx: number, typeStr: string) {
  const fn = graph.workflow.functions.find((f) => f.id === fnId);
  if (!fn) return;
  const params = fn.params.map((p, i) =>
    i === idx ? { ...p, type: typeFromString(typeStr) } : p,
  );
  graph.updateFunctionSignature(fnId, params, fn.returnType);
}
function updateReturnType(fnId: string, typeStr: string) {
  const fn = graph.workflow.functions.find((f) => f.id === fnId);
  if (!fn) return;
  graph.updateFunctionSignature(fnId, fn.params, typeFromString(typeStr));
}
</script>

<template>
  <div class="fn-tabs-wrap">
    <div class="fn-tabs">
      <div
        v-for="fn in graph.workflow.functions"
        :key="fn.id"
        class="fn-tab"
        :class="{ active: fn.id === graph.activeFunctionId }"
        @click="setActive(fn.id)"
      >
        <input
          class="fn-name"
          :value="fn.name"
          @input="(e) => rename(fn.id, e)"
          @click.stop
        />
        <span class="muted fn-sig">
          ({{ fn.params.length }}){{
            fn.returnType.kind !== 'void' ? ' → ' + typeLabel(fn.returnType) : ''
          }}
        </span>
        <button class="ghost icon" @click.stop="toggleSig(fn.id)" title="Edit signature">
          ⚙
        </button>
        <button
          class="ghost icon"
          v-if="graph.workflow.functions.length > 1"
          @click.stop="deleteFn(fn.id)"
          title="Delete function"
        >
          ×
        </button>
      </div>
      <button class="ghost add" @click="newFn">+ Function</button>
    </div>

    <div v-if="signatureOpenFor" class="signature-editor">
      <template
        v-for="fn in graph.workflow.functions.filter((f) => f.id === signatureOpenFor)"
        :key="fn.id"
      >
        <div class="sig-row">
          <span class="sig-label">params</span>
          <div class="sig-params">
            <div v-for="(p, idx) in fn.params" :key="idx" class="sig-param">
              <input
                class="sig-param-name"
                :value="p.name"
                @input="(e) => updateParamName(fn.id, idx, (e.target as HTMLInputElement).value)"
              />
              <select
                class="sig-param-type"
                :value="typeAsString(p.type)"
                @change="(e) => updateParamType(fn.id, idx, (e.target as HTMLSelectElement).value)"
              >
                <option v-for="t in namedTypeOptions().filter((x) => x !== 'void')" :key="t" :value="t">{{ t }}</option>
              </select>
              <button class="ghost" @click="removeParam(fn.id, idx)">×</button>
            </div>
            <button class="ghost add" @click="addParam(fn.id)">+ param</button>
          </div>
        </div>
        <div class="sig-row">
          <span class="sig-label">returns</span>
          <select
            class="sig-param-type"
            :value="typeAsString(fn.returnType)"
            @change="(e) => updateReturnType(fn.id, (e.target as HTMLSelectElement).value)"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.fn-tabs-wrap {
  background: var(--sf-bg-1);
  border-bottom: 1px solid var(--sf-border);
}
.fn-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 8px;
  overflow-x: auto;
}
.fn-tab {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border-radius: var(--sf-radius-sm) var(--sf-radius-sm) 0 0;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-bottom: none;
  cursor: pointer;
  min-width: 0;
  font-size: 12px;
}
.fn-tab.active {
  background: var(--sf-bg-3);
  border-color: var(--sf-accent);
}
.fn-name {
  background: transparent;
  border: none;
  padding: 0 2px;
  color: var(--sf-text-0);
  font-family: var(--sf-font-mono);
  font-size: 12px;
  width: 110px;
  min-width: 60px;
}
.fn-name:focus {
  background: var(--sf-bg-1);
  border-radius: var(--sf-radius-sm);
}
.fn-sig {
  font-size: 10px;
  font-family: var(--sf-font-mono);
}
.icon {
  padding: 0 4px;
  min-width: 20px;
  font-size: 11px;
  background: transparent;
  border: none;
}
.add {
  font-size: 11px;
  padding: 4px 8px;
  background: transparent;
  border: 1px dashed var(--sf-border);
}
.signature-editor {
  padding: 8px 12px;
  background: var(--sf-bg-0);
  border-top: 1px solid var(--sf-border);
  font-size: 11px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.sig-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.sig-label {
  width: 60px;
  color: var(--sf-text-2);
  font-weight: 600;
  text-transform: uppercase;
  font-size: 10px;
  letter-spacing: 1px;
}
.sig-params {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.sig-param {
  display: flex;
  gap: 2px;
  align-items: center;
  background: var(--sf-bg-2);
  padding: 2px 4px;
  border-radius: var(--sf-radius-sm);
  border: 1px solid var(--sf-border);
}
.sig-param-name {
  width: 70px;
  font-family: var(--sf-font-mono);
  font-size: 11px;
}
.sig-param-type {
  width: 90px;
  font-family: var(--sf-font-mono);
  font-size: 11px;
}
</style>
