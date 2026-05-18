<script setup lang="ts">
import { computed } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import {
  BINARY_OPS,
  UNARY_OPS,
  type BinaryOpSymbol,
  type UnaryOpSymbol,
  type SolPrimitive,
  type SolType,
} from '@/graph/schema';
import { bindingsInScope } from '@/graph/scope';

const graph = useGraphStore();
const ui = useUIStore();

const selectedNode = computed(() => {
  const fn = graph.activeFunction;
  if (!fn || !ui.selectedNodeId) return null;
  return fn.nodes.find((n) => n.id === ui.selectedNodeId) ?? null;
});

const PRIMS: SolPrimitive[] = ['int', 'float', 'bool', 'str', 'char'];

function namedTypeOptions(): string[] {
  return [
    ...PRIMS,
    ...graph.workflow.structs.map((s) => s.name),
    ...graph.workflow.enums.map((e) => e.name),
  ];
}

function typeAsString(t: SolType): string {
  if (t.kind === 'named') return t.name;
  return t.kind;
}

function typeFromString(s: string): SolType {
  if (PRIMS.includes(s as SolPrimitive)) return { kind: s as SolPrimitive };
  return { kind: 'named', name: s };
}

const inScopeVars = computed(() => {
  if (!selectedNode.value || !graph.activeFunction) return [];
  return bindingsInScope(graph.activeFunction, selectedNode.value.id);
});

const structOptions = computed(() => graph.workflow.structs);
const enumOptions = computed(() => graph.workflow.enums);
const functionOptions = computed(() =>
  graph.workflow.functions.filter((f) => f.id !== graph.activeFunctionId),
);

// Narrowed accessors so the template stays TypeScript-clean.
const selectedStructFields = computed(() => {
  const d = selectedNode.value?.data;
  if (!d) return [];
  if (d.kind !== 'fieldAccess' && d.kind !== 'fieldSet' && d.kind !== 'structLiteral')
    return [];
  return structOptions.value.find((s) => s.name === d.structName)?.fields ?? [];
});

const selectedEnumVariants = computed(() => {
  const d = selectedNode.value?.data;
  if (!d) return [];
  if (d.kind !== 'enumVariant') return [];
  return enumOptions.value.find((e) => e.name === d.enumName)?.variants ?? [];
});

function update<T extends object>(patch: T) {
  if (!selectedNode.value) return;
  graph.updateNodeData(
    selectedNode.value.id,
    patch as Partial<typeof selectedNode.value.data>,
  );
}

function onVarGetChange(e: Event) {
  const newName = (e.target as HTMLSelectElement).value;
  const v = inScopeVars.value.find((b) => b.name === newName);
  update({ varName: newName, resolvedType: v?.type ?? { kind: 'any' as const } });
}
</script>

<template>
  <div class="inspector">
    <div class="inspector-header">
      <span class="title">Inspector</span>
      <span class="muted" v-if="selectedNode">{{ selectedNode.data.kind }}</span>
    </div>
    <div v-if="!selectedNode" class="empty">Select a node to edit its parameters.</div>

    <div v-else class="body">
      <!-- Let -->
      <template v-if="selectedNode.data.kind === 'let'">
        <label>
          <span>Name</span>
          <input
            :value="selectedNode.data.varName"
            @input="(e) => update({ varName: (e.target as HTMLInputElement).value })"
          />
        </label>
        <label>
          <span>Type</span>
          <select
            :value="typeAsString(selectedNode.data.varType)"
            @change="(e) => update({ varType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
      </template>

      <!-- Assign -->
      <template v-else-if="selectedNode.data.kind === 'assign'">
        <label>
          <span>Target variable</span>
          <select
            :value="selectedNode.data.varName"
            @change="(e) => update({ varName: (e.target as HTMLSelectElement).value })"
          >
            <option value="">— pick variable —</option>
            <option v-for="v in inScopeVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- Return -->
      <template v-else-if="selectedNode.data.kind === 'return'">
        <label class="checkbox-row">
          <input
            type="checkbox"
            :checked="selectedNode.data.hasValue"
            @change="(e) => update({ hasValue: (e.target as HTMLInputElement).checked })"
          />
          <span>Returns a value</span>
        </label>
      </template>

      <!-- Branch -->
      <template v-else-if="selectedNode.data.kind === 'branch'">
        <label class="checkbox-row">
          <input
            type="checkbox"
            :checked="selectedNode.data.hasElse"
            @change="(e) => update({ hasElse: (e.target as HTMLInputElement).checked })"
          />
          <span>Include `else` branch</span>
        </label>
      </template>

      <!-- ForEach -->
      <template v-else-if="selectedNode.data.kind === 'forEach'">
        <label>
          <span>Iterator name</span>
          <input
            :value="selectedNode.data.iteratorName"
            @input="(e) => update({ iteratorName: (e.target as HTMLInputElement).value })"
          />
        </label>
        <label>
          <span>Item type</span>
          <select
            :value="typeAsString(selectedNode.data.iteratorType)"
            @change="(e) => update({ iteratorType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
      </template>

      <!-- BinaryOp -->
      <template v-else-if="selectedNode.data.kind === 'binaryOp'">
        <label>
          <span>Operator</span>
          <select
            :value="selectedNode.data.op"
            @change="(e) => update({ op: (e.target as HTMLSelectElement).value as BinaryOpSymbol })"
          >
            <option v-for="op in BINARY_OPS" :key="op" :value="op">{{ op }}</option>
          </select>
        </label>
        <label>
          <span>Operand type</span>
          <select
            :value="typeAsString(selectedNode.data.valueType)"
            @change="(e) => update({ valueType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
      </template>

      <!-- UnaryOp -->
      <template v-else-if="selectedNode.data.kind === 'unaryOp'">
        <label>
          <span>Operator</span>
          <select
            :value="selectedNode.data.op"
            @change="(e) => update({ op: (e.target as HTMLSelectElement).value as UnaryOpSymbol })"
          >
            <option v-for="op in UNARY_OPS" :key="op" :value="op">{{ op }}</option>
          </select>
        </label>
        <label>
          <span>Operand type</span>
          <select
            :value="typeAsString(selectedNode.data.valueType)"
            @change="(e) => update({ valueType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
      </template>

      <!-- VarGet -->
      <template v-else-if="selectedNode.data.kind === 'varGet'">
        <label>
          <span>Variable</span>
          <select :value="selectedNode.data.varName" @change="onVarGetChange">
            <option value="">— pick variable —</option>
            <option v-for="v in inScopeVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- Literal -->
      <template v-else-if="selectedNode.data.kind === 'literal'">
        <label>
          <span>Literal type</span>
          <select
            :value="selectedNode.data.litType"
            @change="(e) => update({ litType: (e.target as HTMLSelectElement).value as SolPrimitive })"
          >
            <option v-for="t in PRIMS" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
        <label v-if="selectedNode.data.litType === 'bool'">
          <span>Value</span>
          <select
            :value="selectedNode.data.value"
            @change="(e) => update({ value: (e.target as HTMLSelectElement).value })"
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        </label>
        <label v-else>
          <span>Value</span>
          <input
            :value="selectedNode.data.value"
            @input="(e) => update({ value: (e.target as HTMLInputElement).value })"
          />
        </label>
      </template>

      <!-- ArrayLiteral -->
      <template v-else-if="selectedNode.data.kind === 'arrayLiteral'">
        <label>
          <span>Item type</span>
          <select
            :value="typeAsString(selectedNode.data.itemType)"
            @change="(e) => update({ itemType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
        <label>
          <span>Length</span>
          <input
            type="number"
            min="0"
            :value="selectedNode.data.length"
            @input="(e) => update({ length: Math.max(0, Number((e.target as HTMLInputElement).value)) })"
          />
        </label>
      </template>

      <!-- StructLiteral -->
      <template v-else-if="selectedNode.data.kind === 'structLiteral'">
        <label>
          <span>Struct</span>
          <select
            :value="selectedNode.data.structName"
            @change="(e) => update({ structName: (e.target as HTMLSelectElement).value })"
          >
            <option value="">— pick struct —</option>
            <option v-for="s in structOptions" :key="s.id" :value="s.name">{{ s.name }}</option>
          </select>
        </label>
      </template>

      <!-- FieldAccess / FieldSet -->
      <template
        v-else-if="
          selectedNode.data.kind === 'fieldAccess' ||
          selectedNode.data.kind === 'fieldSet'
        "
      >
        <label>
          <span>Struct</span>
          <select
            :value="selectedNode.data.structName"
            @change="(e) => update({ structName: (e.target as HTMLSelectElement).value, fieldName: '' })"
          >
            <option value="">— pick struct —</option>
            <option v-for="s in structOptions" :key="s.id" :value="s.name">{{ s.name }}</option>
          </select>
        </label>
        <label>
          <span>Field</span>
          <select
            :value="selectedNode.data.fieldName"
            @change="(e) => update({ fieldName: (e.target as HTMLSelectElement).value })"
          >
            <option value="">— pick field —</option>
            <option v-for="f in selectedStructFields" :key="f.name" :value="f.name">
              {{ f.name }}: {{ typeAsString(f.type) }}
            </option>
          </select>
        </label>
      </template>

      <!-- IndexRead / IndexSet -->
      <template
        v-else-if="
          selectedNode.data.kind === 'indexRead' ||
          selectedNode.data.kind === 'indexSet'
        "
      >
        <label>
          <span>Element type</span>
          <select
            :value="typeAsString(selectedNode.data.elementType)"
            @change="(e) => update({ elementType: typeFromString((e.target as HTMLSelectElement).value) })"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
      </template>

      <!-- EnumVariant -->
      <template v-else-if="selectedNode.data.kind === 'enumVariant'">
        <label>
          <span>Enum</span>
          <select
            :value="selectedNode.data.enumName"
            @change="(e) => update({ enumName: (e.target as HTMLSelectElement).value, variantName: '' })"
          >
            <option value="">— pick enum —</option>
            <option v-for="en in enumOptions" :key="en.id" :value="en.name">{{ en.name }}</option>
          </select>
        </label>
        <label>
          <span>Variant</span>
          <select
            :value="selectedNode.data.variantName"
            @change="(e) => update({ variantName: (e.target as HTMLSelectElement).value })"
          >
            <option value="">— pick variant —</option>
            <option v-for="v in selectedEnumVariants" :key="v.name" :value="v.name">
              {{ v.name }}<template v-if="v.value !== null"> = {{ v.value }}</template>
            </option>
          </select>
        </label>
      </template>

      <!-- Call -->
      <template v-else-if="selectedNode.data.kind === 'call'">
        <label>
          <span>Function</span>
          <select
            :value="selectedNode.data.functionId"
            @change="(e) => update({ functionId: (e.target as HTMLSelectElement).value })"
          >
            <option value="">— pick function —</option>
            <option v-for="f in functionOptions" :key="f.id" :value="f.id">
              {{ f.name }}({{ f.params.map((p) => p.name).join(', ') }})
            </option>
          </select>
        </label>
      </template>

      <!-- Print / Start / fallthrough -->
      <template v-else>
        <p class="muted-note">No parameters for this node.</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.inspector {
  flex: 1;
  display: flex;
  flex-direction: column;
  border-bottom: 1px solid var(--sf-border);
  overflow: hidden;
  min-height: 0;
}
.inspector-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.title {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.empty {
  padding: 16px;
  color: var(--sf-text-3);
  font-size: 12px;
  text-align: center;
}
.body {
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}
label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--sf-text-1);
}
label.checkbox-row {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}
label.checkbox-row input {
  width: auto;
}
.muted-note {
  color: var(--sf-text-3);
  font-size: 11px;
  margin: 0;
}
</style>
