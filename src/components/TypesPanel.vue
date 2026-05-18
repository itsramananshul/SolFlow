<script setup lang="ts">
import { useGraphStore } from '@/stores/graph.store';
import type { SolType, StructField, EnumVariant, SolPrimitive } from '@/graph/schema';

const graph = useGraphStore();

const PRIMS: SolPrimitive[] = ['int', 'float', 'bool', 'str', 'char'];

function typeFromKindString(k: string): SolType {
  if (PRIMS.includes(k as SolPrimitive)) return { kind: k as SolPrimitive };
  // Treat anything else as a named ref (struct/enum) — for v0.1 we don't
  // surface array/tuple types in the struct-field type dropdown.
  return { kind: 'named', name: k };
}

function typeAsString(t: SolType): string {
  if (t.kind === 'named') return t.name;
  if (t.kind === 'array') return `array:${t.size ?? ''}:${typeAsString(t.inner)}`;
  return t.kind;
}

function namedTypeOptions(): string[] {
  return [
    ...PRIMS,
    ...graph.workflow.structs.map((s) => s.name),
    ...graph.workflow.enums.map((e) => e.name),
  ];
}

function addField(structId: string) {
  const s = graph.workflow.structs.find((s) => s.id === structId);
  if (!s) return;
  const newField: StructField = { name: 'field', type: { kind: 'int' } };
  graph.updateStruct(structId, { fields: [...s.fields, newField] });
}
function removeField(structId: string, idx: number) {
  const s = graph.workflow.structs.find((s) => s.id === structId);
  if (!s) return;
  const fields = s.fields.filter((_, i) => i !== idx);
  graph.updateStruct(structId, { fields });
}
function updateFieldName(structId: string, idx: number, name: string) {
  const s = graph.workflow.structs.find((s) => s.id === structId);
  if (!s) return;
  const fields = s.fields.map((f, i) => (i === idx ? { ...f, name } : f));
  graph.updateStruct(structId, { fields });
}
function updateFieldType(structId: string, idx: number, typeStr: string) {
  const s = graph.workflow.structs.find((s) => s.id === structId);
  if (!s) return;
  const fields = s.fields.map((f, i) =>
    i === idx ? { ...f, type: typeFromKindString(typeStr) } : f,
  );
  graph.updateStruct(structId, { fields });
}

function addVariant(enumId: string) {
  const e = graph.workflow.enums.find((e) => e.id === enumId);
  if (!e) return;
  const newVariant: EnumVariant = { name: 'Variant', value: null };
  graph.updateEnum(enumId, { variants: [...e.variants, newVariant] });
}
function removeVariant(enumId: string, idx: number) {
  const e = graph.workflow.enums.find((e) => e.id === enumId);
  if (!e) return;
  const variants = e.variants.filter((_, i) => i !== idx);
  graph.updateEnum(enumId, { variants });
}
function updateVariantName(enumId: string, idx: number, name: string) {
  const e = graph.workflow.enums.find((e) => e.id === enumId);
  if (!e) return;
  const variants = e.variants.map((v, i) => (i === idx ? { ...v, name } : v));
  graph.updateEnum(enumId, { variants });
}
function updateVariantValue(enumId: string, idx: number, value: string) {
  const e = graph.workflow.enums.find((e) => e.id === enumId);
  if (!e) return;
  const variants = e.variants.map((v, i) =>
    i === idx ? { ...v, value: value.trim() === '' ? null : Number(value) } : v,
  );
  graph.updateEnum(enumId, { variants });
}
</script>

<template>
  <div class="types">
    <div class="section">
      <div class="section-header">
        <span>Structs</span>
        <button class="ghost" @click="graph.addStruct()">+ Struct</button>
      </div>
      <div v-if="graph.workflow.structs.length === 0" class="empty">
        No structs yet.
      </div>
      <div
        v-for="s in graph.workflow.structs"
        :key="s.id"
        class="card"
      >
        <div class="card-header">
          <input
            class="name"
            :value="s.name"
            @input="(e) => graph.updateStruct(s.id, { name: (e.target as HTMLInputElement).value })"
          />
          <button class="ghost danger" @click="graph.deleteStruct(s.id)">×</button>
        </div>
        <div
          v-for="(f, idx) in s.fields"
          :key="idx"
          class="row field-row"
        >
          <input
            class="field-name"
            :value="f.name"
            @input="(e) => updateFieldName(s.id, idx, (e.target as HTMLInputElement).value)"
          />
          <select
            class="field-type"
            :value="typeAsString(f.type)"
            @change="(e) => updateFieldType(s.id, idx, (e.target as HTMLSelectElement).value)"
          >
            <option v-for="t in namedTypeOptions()" :key="t" :value="t">{{ t }}</option>
          </select>
          <button class="ghost" @click="removeField(s.id, idx)">×</button>
        </div>
        <button class="ghost add-field" @click="addField(s.id)">+ field</button>
      </div>
    </div>

    <div class="section">
      <div class="section-header">
        <span>Enums</span>
        <button class="ghost" @click="graph.addEnum()">+ Enum</button>
      </div>
      <div v-if="graph.workflow.enums.length === 0" class="empty">
        No enums yet.
      </div>
      <div
        v-for="e in graph.workflow.enums"
        :key="e.id"
        class="card"
      >
        <div class="card-header">
          <input
            class="name"
            :value="e.name"
            @input="(ev) => graph.updateEnum(e.id, { name: (ev.target as HTMLInputElement).value })"
          />
          <button class="ghost danger" @click="graph.deleteEnum(e.id)">×</button>
        </div>
        <div
          v-for="(v, idx) in e.variants"
          :key="idx"
          class="row variant-row"
        >
          <input
            class="variant-name"
            :value="v.name"
            @input="(ev) => updateVariantName(e.id, idx, (ev.target as HTMLInputElement).value)"
          />
          <input
            class="variant-value"
            type="text"
            :value="v.value ?? ''"
            placeholder="auto"
            @input="(ev) => updateVariantValue(e.id, idx, (ev.target as HTMLInputElement).value)"
          />
          <button class="ghost" @click="removeVariant(e.id, idx)">×</button>
        </div>
        <button class="ghost add-field" @click="addVariant(e.id)">+ variant</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.types {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 16px;
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
.empty {
  color: var(--sf-text-3);
  padding: 4px;
}
.card {
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  padding: 6px;
  margin-bottom: 6px;
}
.card-header {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: 4px;
}
.card-header .name {
  flex: 1;
  font-weight: 600;
}
.field-row,
.variant-row {
  margin-bottom: 4px;
  gap: 4px;
}
.field-name,
.variant-name {
  flex: 1;
}
.field-type {
  flex: 1;
}
.variant-value {
  width: 70px;
}
.add-field {
  width: 100%;
  margin-top: 2px;
  font-size: 0.625rem;
  padding: 3px;
  color: var(--sf-text-2);
}
.danger:hover {
  color: var(--sf-error);
}
</style>
