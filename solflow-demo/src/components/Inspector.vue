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

const dataInPorts = computed(() => {
  if (!selectedNode.value) return [];
  return selectedNode.value.ports.in.filter((p) => p.kind === 'data');
});

function exprFor(portId: string): string {
  if (!selectedNode.value) return '';
  return selectedNode.value.expressions?.[portId] ?? '';
}

function setExpr(portId: string, text: string) {
  if (!selectedNode.value) return;
  graph.updateNodeExpression(selectedNode.value.id, portId, text);
}

function isPortWired(portId: string): boolean {
  if (!selectedNode.value || !graph.activeFunction) return false;
  return graph.activeFunction.edges.some(
    (e) =>
      e.kind === 'data' &&
      e.target.node === selectedNode.value!.id &&
      e.target.port === portId,
  );
}

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

const placeholderFor = (portId: string, kind: string): string => {
  if (portId === 'cond') return 'e.g. counter < 4';
  if (portId === 'value' && kind === 'print') return 'e.g. "hello, world"';
  if (portId === 'value' && kind === 'return') return 'e.g. 0';
  if (portId === 'value' && kind === 'let') return 'e.g. 42 or Person { name: "evan", age: 19 }';
  if (portId === 'value' && kind === 'assign') return 'e.g. counter + 1';
  if (portId === 'array') return 'e.g. arr';
  if (portId === 'index') return 'e.g. i';
  if (portId === 'target') return 'e.g. node';
  if (portId.startsWith('arg:')) return `e.g. ${portId.slice(4)}_value`;
  return '';
};
</script>

<template>
  <div class="inspector">
    <div class="inspector-header">
      <span class="title">Inspector</span>
      <span class="kind-tag" v-if="selectedNode">{{ selectedNode.data.kind }}</span>
    </div>

    <div v-if="!selectedNode" class="empty">
      <p>Select a node to edit its parameters.</p>
      <p class="muted-note">Or drag a node from the palette to begin.</p>
    </div>

    <div v-else class="body">
      <!-- Inline expressions section — shown FIRST if the node has data inputs. -->
      <section v-if="dataInPorts.length > 0" class="section">
        <div class="section-header">
          <span>Inputs</span>
          <span class="hint">type SOL expression, or wire the port</span>
        </div>
        <label v-for="p in dataInPorts" :key="p.id" class="field">
          <span class="field-label">
            <span class="port-name">{{ p.name }}</span>
            <span v-if="isPortWired(p.id)" class="wire-pill">wired</span>
            <span v-else-if="exprFor(p.id)" class="inline-pill">inline</span>
          </span>
          <input
            class="expr-input"
            :value="exprFor(p.id)"
            :placeholder="placeholderFor(p.id, selectedNode.data.kind)"
            :disabled="isPortWired(p.id)"
            spellcheck="false"
            @input="(e) => setExpr(p.id, (e.target as HTMLInputElement).value)"
          />
        </label>
      </section>

      <!-- Kind-specific properties. -->
      <section class="section">
        <div class="section-header">
          <span>Properties</span>
        </div>

        <template v-if="selectedNode.data.kind === 'let'">
          <label class="field">
            <span class="field-label">Name</span>
            <input
              :value="selectedNode.data.varName"
              @input="(e) => update({ varName: (e.target as HTMLInputElement).value })"
            />
          </label>
          <label class="field">
            <span class="field-label">Type</span>
            <select
              :value="typeAsString(selectedNode.data.varType)"
              @change="(e) => update({ varType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'assign'">
          <label class="field">
            <span class="field-label">Target variable</span>
            <select
              :value="selectedNode.data.varName"
              @change="(e) => update({ varName: (e.target as HTMLSelectElement).value })"
            >
              <option value="">— pick variable —</option>
              <option v-for="v in inScopeVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'return'">
          <label class="field checkbox-row">
            <input
              type="checkbox"
              :checked="selectedNode.data.hasValue"
              @change="(e) => update({ hasValue: (e.target as HTMLInputElement).checked })"
            />
            <span>Returns a value</span>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'branch'">
          <label class="field checkbox-row">
            <input
              type="checkbox"
              :checked="selectedNode.data.hasElse"
              @change="(e) => update({ hasElse: (e.target as HTMLInputElement).checked })"
            />
            <span>Include `else` branch</span>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'while'">
          <p class="muted-note">Condition above, body via the body control out.</p>
        </template>

        <template v-else-if="selectedNode.data.kind === 'forEach'">
          <label class="field">
            <span class="field-label">Iterator name</span>
            <input
              :value="selectedNode.data.iteratorName"
              @input="(e) => update({ iteratorName: (e.target as HTMLInputElement).value })"
            />
          </label>
          <label class="field">
            <span class="field-label">Item type</span>
            <select
              :value="typeAsString(selectedNode.data.iteratorType)"
              @change="(e) => update({ iteratorType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'binaryOp'">
          <label class="field">
            <span class="field-label">Operator</span>
            <select
              :value="selectedNode.data.op"
              @change="(e) => update({ op: (e.target as HTMLSelectElement).value as BinaryOpSymbol })"
            >
              <option v-for="op in BINARY_OPS" :key="op" :value="op">{{ op }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Operand type</span>
            <select
              :value="typeAsString(selectedNode.data.valueType)"
              @change="(e) => update({ valueType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'unaryOp'">
          <label class="field">
            <span class="field-label">Operator</span>
            <select
              :value="selectedNode.data.op"
              @change="(e) => update({ op: (e.target as HTMLSelectElement).value as UnaryOpSymbol })"
            >
              <option v-for="op in UNARY_OPS" :key="op" :value="op">{{ op }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Operand type</span>
            <select
              :value="typeAsString(selectedNode.data.valueType)"
              @change="(e) => update({ valueType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'varGet'">
          <label class="field">
            <span class="field-label">Variable</span>
            <select :value="selectedNode.data.varName" @change="onVarGetChange">
              <option value="">— pick variable —</option>
              <option v-for="v in inScopeVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'literal'">
          <label class="field">
            <span class="field-label">Literal type</span>
            <select
              :value="selectedNode.data.litType"
              @change="(e) => update({ litType: (e.target as HTMLSelectElement).value as SolPrimitive })"
            >
              <option v-for="t in PRIMS" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
          <label v-if="selectedNode.data.litType === 'bool'" class="field">
            <span class="field-label">Value</span>
            <select
              :value="selectedNode.data.value"
              @change="(e) => update({ value: (e.target as HTMLSelectElement).value })"
            >
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          </label>
          <label v-else class="field">
            <span class="field-label">Value</span>
            <input
              :value="selectedNode.data.value"
              @input="(e) => update({ value: (e.target as HTMLInputElement).value })"
            />
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'arrayLiteral'">
          <label class="field">
            <span class="field-label">Item type</span>
            <select
              :value="typeAsString(selectedNode.data.itemType)"
              @change="(e) => update({ itemType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Length</span>
            <input
              type="number"
              min="0"
              :value="selectedNode.data.length"
              @input="(e) => update({ length: Math.max(0, Number((e.target as HTMLInputElement).value)) })"
            />
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'structLiteral'">
          <label class="field">
            <span class="field-label">Struct</span>
            <select
              :value="selectedNode.data.structName"
              @change="(e) => update({ structName: (e.target as HTMLSelectElement).value })"
            >
              <option value="">— pick struct —</option>
              <option v-for="s in structOptions" :key="s.id" :value="s.name">{{ s.name }}</option>
            </select>
          </label>
        </template>

        <template
          v-else-if="
            selectedNode.data.kind === 'fieldAccess' ||
            selectedNode.data.kind === 'fieldSet'
          "
        >
          <label class="field">
            <span class="field-label">Struct</span>
            <select
              :value="selectedNode.data.structName"
              @change="(e) => update({ structName: (e.target as HTMLSelectElement).value, fieldName: '' })"
            >
              <option value="">— pick struct —</option>
              <option v-for="s in structOptions" :key="s.id" :value="s.name">{{ s.name }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Field</span>
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

        <template
          v-else-if="
            selectedNode.data.kind === 'indexRead' ||
            selectedNode.data.kind === 'indexSet'
          "
        >
          <label class="field">
            <span class="field-label">Element type</span>
            <select
              :value="typeAsString(selectedNode.data.elementType)"
              @change="(e) => update({ elementType: typeFromString((e.target as HTMLSelectElement).value) })"
            >
              <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
            </select>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'enumVariant'">
          <label class="field">
            <span class="field-label">Enum</span>
            <select
              :value="selectedNode.data.enumName"
              @change="(e) => update({ enumName: (e.target as HTMLSelectElement).value, variantName: '' })"
            >
              <option value="">— pick enum —</option>
              <option v-for="en in enumOptions" :key="en.id" :value="en.name">{{ en.name }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Variant</span>
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

        <template v-else-if="selectedNode.data.kind === 'call'">
          <label class="field">
            <span class="field-label">Function</span>
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

        <template v-else>
          <p class="muted-note">No additional parameters.</p>
        </template>
      </section>
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
  padding: 10px 14px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.kind-tag {
  font-family: var(--sf-font-mono);
  font-size: 10px;
  color: var(--sf-text-2);
  padding: 2px 6px;
  border-radius: 3px;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
}
.empty {
  padding: 20px 16px;
  color: var(--sf-text-3);
  font-size: 12px;
}
.empty p {
  margin: 0 0 6px 0;
}
.body {
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}
.section {
  padding: 12px 14px;
  border-bottom: 1px solid var(--sf-border);
}
.section:last-child {
  border-bottom: none;
}
.section-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 8px;
  color: var(--sf-text-2);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.6px;
  text-transform: uppercase;
}
.section-header .hint {
  font-size: 9px;
  text-transform: none;
  letter-spacing: 0;
  color: var(--sf-text-3);
  font-weight: 400;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 10px;
}
.field:last-child {
  margin-bottom: 0;
}
.field-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--sf-text-1);
}
.port-name {
  font-family: var(--sf-font-mono);
}
.wire-pill,
.inline-pill {
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 1px 4px;
  border-radius: 2px;
}
.wire-pill {
  background: rgba(50, 145, 255, 0.12);
  color: var(--sf-accent);
}
.inline-pill {
  background: rgba(255, 255, 255, 0.06);
  color: var(--sf-text-2);
}
.expr-input {
  font-family: var(--sf-font-mono);
  font-size: 12px;
}
.expr-input:disabled {
  opacity: 0.4;
  background: var(--sf-bg-1);
}
.field.checkbox-row {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}
.field.checkbox-row input {
  width: auto;
  margin: 0;
}
.muted-note {
  color: var(--sf-text-3);
  font-size: 11px;
  margin: 0;
}
</style>
