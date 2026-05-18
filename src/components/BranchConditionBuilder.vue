<script setup lang="ts">
/**
 * Guided branch-condition builder.
 *
 * Two-tab control. "Build it" is the assisted mode: three controls
 * (left value · operator · right value) composed into a SOL boolean
 * expression. "Type it" is the existing raw-expression input.
 *
 * Builder state lives in the component only. We don't try to parse
 * arbitrary expressions back into LHS/op/RHS — the Phase A grammar is
 * richer than three slots and round-tripping would lie to the user.
 * Instead, switching INTO Build mode after typing raw clears the
 * builder fields so the user sees a known-clean starting state, and
 * switching FROM Build to Type leaves the raw expression intact.
 *
 * Either mode writes to the same string output (the branch's `cond`
 * inline expression). The parent owns persistence.
 */
import { computed, ref, watch } from 'vue';
import ExpressionHelper from './ExpressionHelper.vue';
import type { NodeKind } from '@/graph/schema';
import { useGraphStore } from '@/stores/graph.store';

const props = defineProps<{
  /** Node id (for scope lookup). */
  nodeId: string;
  /** Node kind — used by ExpressionHelper. */
  nodeKind: NodeKind;
  /** Currently-stored condition expression. */
  modelValue: string;
}>();
const emit = defineEmits<{
  (e: 'update:modelValue', text: string): void;
}>();

type Mode = 'build' | 'raw';
const mode = ref<Mode>('build');

// =============================================================
//  Build mode — LHS / op / RHS
// =============================================================

const COMPARE_OPS: { sym: string; label: string }[] = [
  { sym: '==', label: 'equals' },
  { sym: '!=', label: 'is not' },
  { sym: '>',  label: 'greater than' },
  { sym: '<',  label: 'less than' },
  { sym: '>=', label: 'at least' },
  { sym: '<=', label: 'at most' },
  { sym: '&&', label: 'and' },
  { sym: '||', label: 'or' },
];

const graph = useGraphStore();
const scope = computed(() => graph.getScopeBindings(props.nodeId));
const hasTrigger = computed(() => {
  const fn = graph.activeFunction;
  return !!fn?.nodes.some((n) => n.data.kind === 'trigger');
});

// "Picker options" for either LHS or RHS: variables, payload, plus a
// "Type a value…" sentinel that flips the slot to raw text entry.
interface PickerOpt {
  value: string;        // what gets inserted into the slot
  label: string;
  detail?: string;
  kind: 'var' | 'payload' | 'custom' | 'true' | 'false' | 'empty';
}

const CUSTOM_SENTINEL = '__custom__';

const lhsOptions = computed<PickerOpt[]>(() => {
  const out: PickerOpt[] = [];
  for (const b of scope.value) {
    out.push({
      value: b.name,
      label: b.name,
      detail:
        b.source === 'param'
          ? 'parameter'
          : b.source === 'forEach-iter'
            ? 'loop item'
            : 'variable',
      kind: 'var',
    });
  }
  if (hasTrigger.value) {
    out.push({ value: 'payload', label: 'payload', detail: 'trigger event', kind: 'payload' });
  }
  out.push({ value: CUSTOM_SENTINEL, label: 'Custom expression…', kind: 'custom' });
  return out;
});

const rhsOptions = computed<PickerOpt[]>(() => {
  return [
    ...lhsOptions.value.filter((o) => o.kind !== 'custom'),
    { value: 'true', label: 'true', kind: 'true' },
    { value: 'false', label: 'false', kind: 'false' },
    { value: '0', label: '0', kind: 'empty' },
    { value: '""', label: 'empty string', kind: 'empty' },
    { value: CUSTOM_SENTINEL, label: 'Custom value…', kind: 'custom' },
  ];
});

// Internal builder state. Defaults to the first available variable so
// the user can see a complete (if dummy) expression appear immediately.
const lhsPick = ref<string>(CUSTOM_SENTINEL);
const lhsCustom = ref<string>('');
const op = ref<string>('>');
const rhsPick = ref<string>(CUSTOM_SENTINEL);
const rhsCustom = ref<string>('');

// On first mount, seed the builder with a sensible default (first
// scope variable, '>', literal value placeholder).
watch(
  scope,
  (s) => {
    if (lhsPick.value === CUSTOM_SENTINEL && lhsCustom.value === '' && s.length > 0) {
      lhsPick.value = s[0].name;
    }
  },
  { immediate: true },
);

const lhsExpr = computed(() =>
  lhsPick.value === CUSTOM_SENTINEL ? lhsCustom.value.trim() : lhsPick.value,
);
const rhsExpr = computed(() =>
  rhsPick.value === CUSTOM_SENTINEL ? rhsCustom.value.trim() : rhsPick.value,
);
const composed = computed(() => {
  const L = lhsExpr.value;
  const R = rhsExpr.value;
  if (!L || !R) return '';
  return `${L} ${op.value} ${R}`;
});

// Push the composed expression into the model whenever the build
// fields change AND we're in build mode.
watch(composed, (next) => {
  if (mode.value !== 'build') return;
  emit('update:modelValue', next);
});

// Switching to Build clears local builder state so the user sees a
// fresh "first-variable > _" starting point rather than a stale
// roundtrip-faking attempt. The raw expression stays in modelValue
// until the next builder change overwrites it.
function setMode(next: Mode) {
  if (next === mode.value) return;
  if (next === 'build') {
    lhsPick.value = scope.value[0]?.name ?? CUSTOM_SENTINEL;
    lhsCustom.value = '';
    op.value = '>';
    rhsPick.value = CUSTOM_SENTINEL;
    rhsCustom.value = '';
  }
  mode.value = next;
}

// =============================================================
//  Raw mode — pass through to the parent with helper button
// =============================================================

function onRawInput(e: Event) {
  const v = (e.target as HTMLInputElement).value;
  emit('update:modelValue', v);
}

// Helper-driven smart insertion: append with leading space if needed.
function onHelperInsert(snippet: string) {
  const current = props.modelValue;
  const needSpace = current.length > 0 && !/\s$/.test(current);
  const next = needSpace ? `${current} ${snippet}` : current + snippet;
  emit('update:modelValue', next);
}

// In Build mode the LHS-custom and RHS-custom slots get their own
// little helper buttons too, so users can drop a variable inside a
// custom expression (e.g. "order.amount").
function insertIntoLhs(snippet: string) {
  const cur = lhsCustom.value;
  const needSpace = cur.length > 0 && !/\s$/.test(cur);
  lhsCustom.value = needSpace ? `${cur} ${snippet}` : cur + snippet;
}
function insertIntoRhs(snippet: string) {
  const cur = rhsCustom.value;
  const needSpace = cur.length > 0 && !/\s$/.test(cur);
  rhsCustom.value = needSpace ? `${cur} ${snippet}` : cur + snippet;
}
</script>

<template>
  <div class="branch-builder">
    <!-- Mode tabs -->
    <div class="mode-tabs">
      <button
        type="button"
        class="mode-tab"
        :class="{ active: mode === 'build' }"
        @click="setMode('build')"
      >
        Build it
      </button>
      <button
        type="button"
        class="mode-tab"
        :class="{ active: mode === 'raw' }"
        @click="setMode('raw')"
      >
        Type it
      </button>
    </div>

    <!-- Build mode -->
    <div v-if="mode === 'build'" class="build-body">
      <div class="build-row">
        <!-- LHS -->
        <div class="slot">
          <select v-model="lhsPick" class="slot-pick">
            <option v-for="o in lhsOptions" :key="o.value" :value="o.value">
              {{ o.label }}<template v-if="o.detail"> · {{ o.detail }}</template>
            </option>
          </select>
          <div v-if="lhsPick === CUSTOM_SENTINEL" class="slot-custom">
            <input
              v-model="lhsCustom"
              type="text"
              class="slot-input"
              placeholder="e.g. order.amount"
              spellcheck="false"
            />
            <ExpressionHelper
              :node-id="nodeId"
              :node-kind="nodeKind"
              port-id="cond"
              @insert="insertIntoLhs"
            />
          </div>
        </div>

        <!-- Operator -->
        <select v-model="op" class="slot-op">
          <option v-for="o in COMPARE_OPS" :key="o.sym" :value="o.sym">
            {{ o.label }} ({{ o.sym }})
          </option>
        </select>

        <!-- RHS -->
        <div class="slot">
          <select v-model="rhsPick" class="slot-pick">
            <option v-for="o in rhsOptions" :key="o.value" :value="o.value">
              {{ o.label }}<template v-if="o.detail"> · {{ o.detail }}</template>
            </option>
          </select>
          <div v-if="rhsPick === CUSTOM_SENTINEL" class="slot-custom">
            <input
              v-model="rhsCustom"
              type="text"
              class="slot-input"
              placeholder="e.g. 1000"
              spellcheck="false"
            />
            <ExpressionHelper
              :node-id="nodeId"
              :node-kind="nodeKind"
              port-id="cond"
              @insert="insertIntoRhs"
            />
          </div>
        </div>
      </div>

      <div class="composed">
        <span class="composed-label">Composed:</span>
        <code v-if="composed">{{ composed }}</code>
        <code v-else class="composed-empty">finish both sides to compose…</code>
      </div>
    </div>

    <!-- Raw mode -->
    <div v-else class="raw-body">
      <div class="raw-row">
        <input
          class="raw-input"
          :value="modelValue"
          placeholder="e.g. amount > 1000"
          spellcheck="false"
          @input="onRawInput"
        />
        <ExpressionHelper
          :node-id="nodeId"
          :node-kind="nodeKind"
          port-id="cond"
          @insert="onHelperInsert"
        />
      </div>
      <p class="raw-hint">
        Type any SOL boolean expression. Switch to "Build it" for a guided three-slot composer
        — heads up: switching back clears the builder fields.
      </p>
    </div>
  </div>
</template>

<style scoped>
.branch-builder {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.mode-tabs {
  display: inline-flex;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  width: max-content;
}
.mode-tab {
  background: transparent;
  border: none;
  color: var(--sf-text-2);
  padding: 4px 12px;
  font-size: 0.625rem;
  letter-spacing: 0.3px;
  cursor: pointer;
  border-radius: 0;
}
.mode-tab:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
}
.mode-tab.active {
  background: var(--sf-accent);
  color: white;
}

.build-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.build-row {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  gap: 6px;
  align-items: start;
}
.slot {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.slot-pick {
  width: 100%;
  font-size: 0.6875rem;
}
.slot-op {
  font-size: 0.6875rem;
  background: var(--sf-bg-2);
  border-color: var(--sf-border-strong);
  color: var(--sf-cat-operator);
  font-weight: 600;
}
.slot-custom {
  display: flex;
  gap: 4px;
  align-items: stretch;
}
.slot-input {
  flex: 1;
  min-width: 0;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}

.composed {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  background: var(--sf-bg-1);
  border: 1px dashed var(--sf-border);
  border-radius: var(--sf-radius-sm);
}
.composed-label {
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-3);
  flex-shrink: 0;
}
.composed code {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  color: var(--sf-text-0);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.composed code.composed-empty {
  color: var(--sf-text-3);
  font-style: italic;
}

.raw-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.raw-row {
  display: flex;
  gap: 6px;
  align-items: stretch;
}
.raw-input {
  flex: 1;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.raw-hint {
  margin: 0;
  font-size: 0.625rem;
  color: var(--sf-text-3);
  line-height: 1.4;
}
</style>
