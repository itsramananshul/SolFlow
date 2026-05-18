<script setup lang="ts">
/**
 * Expression assist popover.
 *
 * A small "⨁" button that lives next to an expression input. Clicking
 * opens a popover with three sections so the user doesn't stare at a
 * blank field wondering what to type:
 *
 *   - Values    — variables in scope, function params, trigger payload,
 *                 and common literals (true / false / 0 / "")
 *   - Operators — comparison, math, and logic operators each with a
 *                 plain-English label ("equals", "greater than", "and")
 *   - Examples  — context-specific examples from portMeta
 *
 * Clicking any chip emits an `insert` event with the snippet text.
 * The parent owns the input element and decides where to splice the
 * text in (at cursor position, with smart leading-space handling).
 *
 * No effort is made to PARSE existing expressions. This is a one-way
 * assistance tool — user types or clicks; the field is the source of
 * truth. Phase B's SOL parser will close the bidirectional loop.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import type { ScopeBinding } from '@/graph/scope';
import { portMeta } from '@/graph/portMeta';
import type { NodeKind } from '@/graph/schema';

const props = defineProps<{
  /** The node that owns the input being assisted (for scope walking). */
  nodeId: string;
  /** Node kind (used to pull examples from portMeta). */
  nodeKind: NodeKind;
  /** Port id this assist is attached to. */
  portId: string;
}>();
const emit = defineEmits<{
  (e: 'insert', text: string): void;
}>();

const graph = useGraphStore();
const open = ref(false);
const btnRef = ref<HTMLButtonElement | null>(null);
const popoverRef = ref<HTMLDivElement | null>(null);
const popoverPos = ref({ x: 0, y: 0 });
const POPOVER_W = 280;
const POPOVER_H_MAX = 380;

function toggle() {
  if (open.value) {
    open.value = false;
    return;
  }
  positionPopover();
  open.value = true;
}

function positionPopover() {
  const btn = btnRef.value;
  if (!btn) return;
  const rect = btn.getBoundingClientRect();
  // Prefer below + right-aligned with the button; flip up if it would
  // run past the bottom of the viewport.
  let x = rect.right - POPOVER_W;
  let y = rect.bottom + 4;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  if (x < 8) x = 8;
  if (x + POPOVER_W > vw - 8) x = vw - POPOVER_W - 8;
  if (y + POPOVER_H_MAX > vh - 8) {
    y = rect.top - POPOVER_H_MAX - 4;
    if (y < 8) y = 8;
  }
  popoverPos.value = { x, y };
}

function onDocPointer(e: PointerEvent) {
  if (!open.value) return;
  const t = e.target as HTMLElement;
  if (t.closest('.expr-helper-popover')) return;
  if (t.closest('.expr-helper-btn')) return;
  open.value = false;
}

function onKey(e: KeyboardEvent) {
  if (!open.value) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    open.value = false;
  }
}

onMounted(() => {
  document.addEventListener('pointerdown', onDocPointer, true);
  window.addEventListener('keydown', onKey);
  window.addEventListener('resize', () => {
    if (open.value) positionPopover();
  });
});
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocPointer, true);
  window.removeEventListener('keydown', onKey);
});

// Reposition if the button moves (e.g. content above grew/shrank).
watch(open, (now) => {
  if (now) nextTick(positionPopover);
});

// =============================================================
//  Content: variables / payload / operators / literals / examples
// =============================================================

const fnScope = computed<ScopeBinding[]>(() =>
  graph.getScopeBindings(props.nodeId),
);

interface ValueChip {
  insert: string;
  label: string;
  detail?: string;
  origin: 'param' | 'let' | 'forEach' | 'payload' | 'literal';
}

const valueChips = computed<ValueChip[]>(() => {
  const chips: ValueChip[] = [];
  // Function parameters and let/forEach bindings, in scope order.
  for (const b of fnScope.value) {
    chips.push({
      insert: b.name,
      label: b.name,
      detail:
        b.source === 'param'
          ? 'parameter'
          : b.source === 'forEach-iter'
            ? 'loop item'
            : 'variable',
      origin: b.source === 'param' ? 'param' : b.source === 'forEach-iter' ? 'forEach' : 'let',
    });
  }
  // Trigger payload — surfaced when the active function has any
  // trigger nodes. Users can write `payload.foo` after picking this.
  const fn = graph.activeFunction;
  const hasTrigger = !!fn?.nodes.some((n) => n.data.kind === 'trigger');
  if (hasTrigger) {
    chips.push({
      insert: 'payload',
      label: 'payload',
      detail: 'trigger event',
      origin: 'payload',
    });
  }
  return chips;
});

interface OpChip {
  sym: string;
  label: string;
}

const COMPARE_OPS: OpChip[] = [
  { sym: '==', label: 'equals' },
  { sym: '!=', label: 'is not' },
  { sym: '>', label: 'greater than' },
  { sym: '<', label: 'less than' },
  { sym: '>=', label: 'at least' },
  { sym: '<=', label: 'at most' },
];
const MATH_OPS: OpChip[] = [
  { sym: '+', label: 'plus' },
  { sym: '-', label: 'minus' },
  { sym: '*', label: 'times' },
  { sym: '/', label: 'divided by' },
];
const LOGIC_OPS: OpChip[] = [
  { sym: '&&', label: 'and' },
  { sym: '||', label: 'or' },
  { sym: '!', label: 'not' },
];

const LITERAL_CHIPS: ValueChip[] = [
  { insert: 'true', label: 'true', origin: 'literal' },
  { insert: 'false', label: 'false', origin: 'literal' },
  { insert: '0', label: '0', origin: 'literal' },
  { insert: '""', label: '""', detail: 'empty string', origin: 'literal' },
];

const examples = computed<string[]>(() => {
  return portMeta(props.nodeKind, props.portId).examples ?? [];
});

function insert(text: string) {
  emit('insert', text);
}
</script>

<template>
  <button
    ref="btnRef"
    type="button"
    class="expr-helper-btn"
    :class="{ open }"
    :title="open ? 'Close insert menu' : 'Insert a variable, operator, or example'"
    @click="toggle"
  >
    <svg viewBox="0 0 12 12" width="11" height="11" fill="none">
      <path d="M6 2 V10 M2 6 H10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
  </button>

  <Teleport to="body">
    <Transition name="helper-fade">
      <div
        v-if="open"
        ref="popoverRef"
        class="expr-helper-popover"
        :style="{ left: popoverPos.x + 'px', top: popoverPos.y + 'px' }"
      >
        <!-- Values -->
        <div class="section">
          <div class="section-head">Values</div>
          <div v-if="valueChips.length === 0" class="empty">
            No variables in scope yet. Declare one with a <code>let</code> node above this one.
          </div>
          <div v-else class="chip-row">
            <button
              v-for="c in valueChips"
              :key="`v:${c.insert}`"
              type="button"
              class="value-chip"
              :class="`origin-${c.origin}`"
              @click="insert(c.insert)"
            >
              <span class="chip-label">{{ c.label }}</span>
              <span v-if="c.detail" class="chip-detail">{{ c.detail }}</span>
            </button>
          </div>
        </div>

        <!-- Literals -->
        <div class="section">
          <div class="section-head">Quick literals</div>
          <div class="chip-row">
            <button
              v-for="c in LITERAL_CHIPS"
              :key="`l:${c.insert}`"
              type="button"
              class="value-chip origin-literal"
              @click="insert(c.insert)"
            >
              <span class="chip-label">{{ c.label }}</span>
              <span v-if="c.detail" class="chip-detail">{{ c.detail }}</span>
            </button>
          </div>
        </div>

        <!-- Operators -->
        <div class="section">
          <div class="section-head">Compare</div>
          <div class="chip-row">
            <button
              v-for="op in COMPARE_OPS"
              :key="`c:${op.sym}`"
              type="button"
              class="op-chip"
              @click="insert(op.sym)"
            >
              <code>{{ op.sym }}</code>
              <span>{{ op.label }}</span>
            </button>
          </div>
        </div>

        <div class="section">
          <div class="section-head">Math</div>
          <div class="chip-row">
            <button
              v-for="op in MATH_OPS"
              :key="`m:${op.sym}`"
              type="button"
              class="op-chip"
              @click="insert(op.sym)"
            >
              <code>{{ op.sym }}</code>
              <span>{{ op.label }}</span>
            </button>
          </div>
        </div>

        <div class="section">
          <div class="section-head">Logic</div>
          <div class="chip-row">
            <button
              v-for="op in LOGIC_OPS"
              :key="`g:${op.sym}`"
              type="button"
              class="op-chip"
              @click="insert(op.sym)"
            >
              <code>{{ op.sym }}</code>
              <span>{{ op.label }}</span>
            </button>
          </div>
        </div>

        <!-- Examples (port-specific) -->
        <div v-if="examples.length > 0" class="section">
          <div class="section-head">Examples</div>
          <div class="chip-row">
            <button
              v-for="ex in examples"
              :key="`x:${ex}`"
              type="button"
              class="example-chip"
              @click="insert(ex)"
            >{{ ex }}</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.expr-helper-btn {
  width: 22px;
  height: 22px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  color: var(--sf-text-2);
  cursor: pointer;
  padding: 0;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.expr-helper-btn:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.expr-helper-btn.open {
  background: var(--sf-accent-dim);
  color: var(--sf-accent);
  border-color: var(--sf-accent);
}

.expr-helper-popover {
  position: fixed;
  width: 280px;
  max-height: 380px;
  overflow-y: auto;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-3);
  z-index: var(--sf-z-popover);
  padding: 8px 10px 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  font-size: 0.6875rem;
}
.helper-fade-enter-active,
.helper-fade-leave-active {
  transition: opacity 0.1s ease, transform 0.1s ease;
}
.helper-fade-enter-from,
.helper-fade-leave-to {
  opacity: 0;
  transform: translateY(-3px);
}

.section {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.section-head {
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--sf-text-3);
  font-weight: 600;
}
.empty {
  font-size: 0.625rem;
  color: var(--sf-text-3);
  line-height: 1.4;
}
.empty code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-2);
}

.chip-row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.value-chip,
.op-chip,
.example-chip {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 10px;
  padding: 3px 8px;
  cursor: pointer;
  color: var(--sf-text-1);
  font-size: 0.6875rem;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
  display: inline-flex;
  align-items: baseline;
  gap: 5px;
  white-space: nowrap;
}
.value-chip:hover,
.op-chip:hover,
.example-chip:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.chip-label {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
}
.chip-detail {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  text-transform: lowercase;
}
.value-chip.origin-param .chip-label {
  color: var(--sf-cat-flow);
}
.value-chip.origin-payload .chip-label {
  color: var(--sf-cat-trigger);
}
.value-chip.origin-literal .chip-label {
  color: var(--sf-cat-literal);
}
.op-chip code {
  font-family: var(--sf-font-mono);
  color: var(--sf-cat-operator);
  font-size: 0.6875rem;
}
.op-chip span {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
}
.example-chip {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
}
</style>
