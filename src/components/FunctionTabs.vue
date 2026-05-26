<script setup lang="ts">
import { computed, ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { useToastStore } from '@/stores/toast.store';
import { typeLabel } from '@/graph/schema';
import type { Param, SolType, SolPrimitive } from '@/graph/schema';
import ContextMenu, { type ContextMenuItem } from './ContextMenu.vue';

const graph = useGraphStore();
const ui = useUIStore();
const toasts = useToastStore();

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
  // Linear-style destructive action: snapshot, delete, offer Undo
  // toast. The undo restores the whole workflow rather than the
  // single function so any cross-function call ids stay consistent.
  const snapshot = JSON.parse(JSON.stringify(graph.workflow));
  const fnName = graph.workflow.functions.find((f) => f.id === id)?.name ?? 'function';
  graph.deleteFunction(id);
  toasts.add('warning', `Deleted "${fnName}"`, {
    body: 'The function and its nodes are gone.',
    action: {
      label: 'Undo',
      onClick: () => graph.loadWorkflow(snapshot),
    },
  });
}

function duplicateFn(id: string) {
  const newId = graph.duplicateFunction(id);
  const fn = newId ? graph.workflow.functions.find((f) => f.id === newId) : null;
  toasts.success(fn ? `Duplicated to "${fn.name}"` : 'Duplicated function');
}

// =============================================================
//  Per-tab context menu (right-click on a tab)
// =============================================================
//
// Reuses the canvas ContextMenu component. Rename triggers the inline
// input's focus + select — the input is already there, we just shift
// focus to it. Duplicate and Delete are direct actions.

const ctxMenu = ref<{ open: boolean; x: number; y: number; fnId: string | null }>({
  open: false,
  x: 0,
  y: 0,
  fnId: null,
});

function openTabContextMenu(e: MouseEvent, fnId: string) {
  e.preventDefault();
  e.stopPropagation();
  ctxMenu.value = { open: true, x: e.clientX, y: e.clientY, fnId };
}
function closeTabContextMenu() {
  ctxMenu.value = { open: false, x: 0, y: 0, fnId: null };
}

function focusRename(fnId: string) {
  // The function-tab name input has a stable id-driven selector via
  // its place in the v-for; the easiest cross-browser path is to find
  // it by data-fn-id attribute. nextTick not needed because the input
  // is already mounted.
  const el = document.querySelector(`.fn-tab[data-fn-id="${fnId}"] .fn-name`) as HTMLInputElement | null;
  if (el) {
    el.focus();
    el.select();
  }
}

const ctxItems = computed<ContextMenuItem[]>(() => {
  const id = ctxMenu.value.fnId;
  if (!id) return [];
  const onlyOne = graph.workflow.functions.length <= 1;
  return [
    {
      label: 'Rename',
      shortcut: 'F2',
      action: () => focusRename(id),
    },
    {
      label: 'Duplicate',
      action: () => duplicateFn(id),
    },
    {
      label: 'Delete',
      danger: true,
      disabled: onlyOne,
      action: () => deleteFn(id),
    },
  ];
});

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

function signatureTooltip(fn: { name: string; params: Param[]; returnType: SolType }): string {
  const params = fn.params.map((p) => `${p.name}: ${typeLabel(p.type)}`).join(', ');
  const ret = fn.returnType.kind === 'void' ? '' : ` -> ${typeLabel(fn.returnType)}`;
  return `function ${fn.name}(${params})${ret}`;
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
        :data-fn-id="fn.id"
        @click="setActive(fn.id)"
        @contextmenu="(e) => openTabContextMenu(e, fn.id)"
      >
        <span class="fn-prefix">fn</span>
        <input
          class="fn-name"
          :value="fn.name"
          @input="(e) => rename(fn.id, e)"
          @click.stop
        />
        <span class="fn-sig" :title="signatureTooltip(fn)">
          ({{ fn.params.map((p) => p.name + ': ' + typeLabel(p.type)).join(', ') || '' }}){{
            fn.returnType.kind !== 'void' ? ' → ' + typeLabel(fn.returnType) : ''
          }}
        </span>
        <button class="ghost icon" @click.stop="toggleSig(fn.id)" title="Edit signature">
          <svg viewBox="0 0 12 12" width="11" height="11" fill="none">
            <path d="M2 10l3-1 5-5-2-2-5 5-1 3z" stroke="currentColor" stroke-width="1.2" />
          </svg>
        </button>
        <button
          v-if="graph.workflow.functions.length > 1"
          class="ghost icon"
          @click.stop="deleteFn(fn.id)"
          title="Delete function"
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none">
            <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
          </svg>
        </button>
      </div>
      <button class="ghost add-fn" @click="newFn">+ Function</button>
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
              <button class="ghost icon" @click="removeParam(fn.id, idx)" title="Remove param">
                <svg viewBox="0 0 12 12" width="10" height="10" fill="none">
                  <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                </svg>
              </button>
            </div>
            <button class="ghost add-fn" @click="addParam(fn.id)">+ param</button>
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
    <ContextMenu
      :open="ctxMenu.open"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :items="ctxItems"
      @close="closeTabContextMenu"
    />
  </div>
</template>

<style scoped>
.fn-tabs-wrap {
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.fn-tabs {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 12px;
  overflow-x: auto;
  min-height: 36px;
}
.fn-tab {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 8px;
  border-radius: var(--sf-radius-sm);
  background: transparent;
  border: 1px solid transparent;
  cursor: pointer;
  min-width: 0;
  font-size: 0.75rem;
  transition: background 0.12s ease, border-color 0.12s ease;
}
.fn-tab:hover {
  background: var(--sf-bg-2);
}
.fn-tab.active {
  background: var(--sf-bg-3);
  border-color: var(--sf-border-strong);
}
.fn-prefix {
  color: var(--sf-text-3);
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.fn-name {
  background: transparent;
  border: none;
  padding: 0 2px;
  color: var(--sf-text-0);
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  width: 110px;
  min-width: 60px;
  outline: none;
}
.fn-name:focus {
  background: var(--sf-bg-1);
  border-radius: var(--sf-radius-sm);
  box-shadow: 0 0 0 1px var(--sf-border-strong);
}
.fn-sig {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-3);
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.icon {
  padding: 3px;
  min-width: 20px;
  background: transparent;
  border: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--sf-text-2);
}
.icon:hover {
  background: var(--sf-bg-4);
  color: var(--sf-text-0);
}
.add-fn {
  font-size: 0.6875rem;
  padding: 4px 10px;
  background: transparent;
  border: 1px dashed var(--sf-border-strong);
  color: var(--sf-text-2);
}
.add-fn:hover {
  color: var(--sf-text-0);
  border-color: var(--sf-border-bright);
  background: var(--sf-bg-2);
}
.signature-editor {
  padding: 10px 14px;
  background: var(--sf-bg-1);
  border-top: 1px solid var(--sf-border);
  font-size: 0.6875rem;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.sig-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.sig-label {
  width: 60px;
  color: var(--sf-text-2);
  font-weight: 500;
  text-transform: uppercase;
  font-size: 0.625rem;
  letter-spacing: 0.5px;
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
  padding: 1px 1px 1px 4px;
  border-radius: var(--sf-radius-sm);
  border: 1px solid var(--sf-border);
}
.sig-param-name {
  width: 80px;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  background: transparent;
  border: none;
  padding: 3px 4px;
}
.sig-param-type {
  width: 100px;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  background: transparent;
  border: none;
  padding: 3px 20px 3px 4px;
}
</style>
