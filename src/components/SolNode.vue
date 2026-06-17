<script setup lang="ts">
/**
 * Vue Flow custom node renderer. One component handles all 22 SolFlow node
 * kinds; the body switches on `data.data.kind`. Handles are derived from
 * `data.data.ports`. Unwired data inputs get an inline `<input>` directly
 * on the card — typing into it sets `node.expressions[portId]` and the
 * emitter uses it as the SOL expression for that port. Wired ports show
 * a "wired" pill instead. Click-stop prevents Vue Flow from dragging the
 * node when the user clicks into an input.
 */
import { computed, ref } from 'vue';
import { Handle, Position, useVueFlow } from '@vue-flow/core';

import type { GraphNode, NodeData, Port } from '@/graph/schema';
import { typeCssClass, typeLabel } from '@/graph/schema';
import { categoryColor, categoryForKind } from '@/graph/kinds';
import { portMeta } from '@/graph/portMeta';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { useSimulationStore } from '@/stores/simulation.store';

interface Props {
  id: string;
  data: GraphNode;
  selected?: boolean;
}

const props = defineProps<Props>();
const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();
const { getViewport } = useVueFlow();

const node = computed(() => props.data);
const category = computed(() => categoryForKind(node.value.data.kind));
const kindLabel = computed(() => labelForKind(node.value.data));
const categoryDot = computed(() => categoryColor(category.value));

// Per category line icons, so every node reads at a glance like the reference.
const CATEGORY_ICONS: Record<string, string> = {
  trigger: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2L3 14h7l-1 8 10-12h-7z"/></svg>',
  flow: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="12" r="2.5"/><path d="M8.5 6H13a2 2 0 0 1 2 2v2.5M8.5 18H13a2 2 0 0 0 2-2v-2.5"/></svg>',
  variable: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2.5"/><path d="M8 9l3 3-3 3M13 15h3"/></svg>',
  operator: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8h16M4 16h16"/><circle cx="9" cy="8" r="1.6" fill="currentColor" stroke="none"/><circle cx="15" cy="16" r="1.6" fill="currentColor" stroke="none"/></svg>',
  literal: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9h16M4 15h16M10 4l-2 16M16 4l-2 16"/></svg>',
  access: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h2M16 4h2a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-2"/></svg>',
  call: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="14" rx="3"/><circle cx="9" cy="11" r="1.4" fill="currentColor" stroke="none"/><circle cx="15" cy="11" r="1.4" fill="currentColor" stroke="none"/><path d="M12 4V2M9 22h6"/></svg>',
  io: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2.5"/><path d="m4 7 8 6 8-6"/></svg>',
  annotation: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H8l-4 4V5a2 2 0 0 1 2-2h13a2 2 0 0 1 2 2z"/></svg>',
  entry: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 4 20 12 6 20 6 4"/></svg>',
};
const categoryIcon = computed(() => CATEGORY_ICONS[category.value] || CATEGORY_ICONS.flow);
const simStatus = computed(() => sim.getNodeStatus(node.value.id));
/**
 * Most recent runtime summary for this node, if a trace is loaded.
 * Surfaces as a pill in the header so the user can scan "what each
 * node did" without opening the Inspector. Cleared by sim.reset().
 */
const simValueSummary = computed<string | undefined>(() =>
  sim.getValueFor(node.value.id),
);
/** Most recent error message for this node (failed nodes only). */
const simError = computed<string | undefined>(() =>
  sim.getErrorFor(node.value.id),
);
/**
 * For branch / while / forEach: the control-out port that was taken
 * on the most recent visit. Used to dim the not-taken control arms
 * so users see WHY a path was chosen at a glance.
 */
const simTakenPath = computed<string | undefined>(() =>
  sim.getTakenPath(node.value.id),
);

// Role glyph: a single mono character that hints at what the node DOES at
// a glance. Branch = directional, loops = iterative, trigger = entry,
// terminal = end. Kept tiny + monospace so it reads as type, not decoration.
const roleGlyph = computed<string | null>(() => {
  const k = node.value.data.kind;
  if (k === 'branch') return '⌥';
  if (k === 'while' || k === 'forEach') return '↻';
  if (k === 'trigger') return '⚡';
  if (k === 'return') return '⏎';
  if (k === 'start') return '▸';
  return null;
});

// Tiny header badge used for triggers — communicates "this is an
// entrypoint" without leaning on a color-only differentiator.
const headerBadge = computed<string | null>(() => {
  const d = node.value.data;
  if (d.kind === 'trigger') return d.triggerKind.toUpperCase();
  return null;
});

// Plain-English explanations shown on a hover tooltip. The point is to
// teach concepts without forcing the user to open the Inspector or read
// docs. Kept under ~120 chars per entry so the tooltip stays scan-able.
function explainKind(d: NodeData): string {
  switch (d.kind) {
    case 'note':
      return 'A sticky note for your team. Doesn\'t affect execution.';
    case 'frame':
      return 'Visually groups a region of nodes. Doesn\'t affect execution.';
    case 'start':
      return 'Entry point of this function. Execution starts here and follows the wires below.';
    case 'trigger': {
      const k = d.triggerKind;
      if (k === 'webhook') return 'Starts the workflow when someone POSTs to your webhook URL.';
      if (k === 'timer')   return 'Starts the workflow on a schedule (every N minutes / hours / days).';
      if (k === 'event')   return `Starts the workflow when the event "${d.eventName}" happens in your system.`;
      if (k === 'http')    return 'Starts the workflow when a specific HTTP request is received.';
      return 'Starts the workflow when someone clicks "Trigger Event ▷" — useful for testing.';
    }
    case 'let':
      return 'Stores a value in a named variable. Downstream nodes can read this name.';
    case 'assign':
      return 'Changes the value of an existing variable.';
    case 'print':
      return 'Outputs a value (string, number, etc.) to the run log.';
    case 'return':
      return d.hasValue ? 'Ends the function and gives back a value.' : 'Ends the function with no return value.';
    case 'branch':
      return 'Sends execution one of two ways based on a condition: then (true) or else (false).';
    case 'while':
      return 'Repeats the body steps while the condition stays true.';
    case 'forEach':
      return 'Walks through each item in an array, running the body once per item.';
    case 'binaryOp':
      return `Combines two values with an operator (${d.op}). Math, comparison, or logic.`;
    case 'unaryOp':
      return `Applies a one-sided operator to a value (${d.op}).`;
    case 'varGet':
      return 'Reads the current value of a variable.';
    case 'literal':
      return `A fixed value of type ${d.litType}.`;
    case 'arrayLiteral':
      return 'A fixed-length array — each slot gets its own value.';
    case 'structLiteral':
      return 'Constructs a struct by filling in each of its fields.';
    case 'fieldAccess':
      return 'Reads a single field out of a struct value.';
    case 'fieldSet':
      return 'Writes a new value into a struct field.';
    case 'indexRead':
      return 'Reads the array element at a given index.';
    case 'indexSet':
      return 'Writes a value into the array element at a given index.';
    case 'enumVariant':
      return 'A specific value of an enum (e.g. Status::Active).';
    case 'call':
      return 'Calls another function defined in this workflow.';
  }
}

const explanation = computed(() => explainKind(node.value.data));
const tooltipVisible = ref(false);
let tooltipTimer: number | undefined;
function showTooltip() {
  if (tooltipTimer !== undefined) window.clearTimeout(tooltipTimer);
  // Delay so quick mouse passes don't flash the tooltip.
  tooltipTimer = window.setTimeout(() => {
    tooltipVisible.value = true;
  }, 600);
}
function hideTooltip() {
  if (tooltipTimer !== undefined) {
    window.clearTimeout(tooltipTimer);
    tooltipTimer = undefined;
  }
  tooltipVisible.value = false;
}

// =============================================================
//  Frame resize handle
//  Drag the south-east corner to resize. Screen-pixel deltas
//  are divided by the current zoom factor so the resize feels
//  1:1 with the cursor at any viewport scale.
// =============================================================
const MIN_FRAME_W = 200;
const MIN_FRAME_H = 140;
let resizeStart: { x: number; y: number; w: number; h: number } | null = null;

function onFrameResizeStart(e: MouseEvent) {
  if (node.value.data.kind !== 'frame') return;
  e.stopPropagation();
  e.preventDefault();
  resizeStart = {
    x: e.clientX,
    y: e.clientY,
    w: node.value.data.width,
    h: node.value.data.height,
  };
  window.addEventListener('mousemove', onFrameResizeMove);
  window.addEventListener('mouseup', onFrameResizeEnd);
}

function onFrameResizeMove(e: MouseEvent) {
  if (!resizeStart) return;
  if (node.value.data.kind !== 'frame') return;
  const zoom = getViewport().zoom || 1;
  const dx = (e.clientX - resizeStart.x) / zoom;
  const dy = (e.clientY - resizeStart.y) / zoom;
  const newW = Math.max(MIN_FRAME_W, Math.round(resizeStart.w + dx));
  const newH = Math.max(MIN_FRAME_H, Math.round(resizeStart.h + dy));
  graph.updateNodeData(node.value.id, {
    width: newW,
    height: newH,
  } as Partial<NodeData>);
}

function onFrameResizeEnd() {
  resizeStart = null;
  window.removeEventListener('mousemove', onFrameResizeMove);
  window.removeEventListener('mouseup', onFrameResizeEnd);
}

const dataIns = computed<Port[]>(() =>
  node.value.ports.in.filter((p) => p.kind === 'data'),
);
const dataOuts = computed<Port[]>(() =>
  node.value.ports.out.filter((p) => p.kind === 'data'),
);
const controlIns = computed<Port[]>(() =>
  node.value.ports.in.filter((p) => p.kind === 'control'),
);
const controlOuts = computed<Port[]>(() =>
  node.value.ports.out.filter((p) => p.kind === 'control'),
);
// Multi-out flow nodes (branch/while/forEach) carry directional labels
// (then/else/after/body) that the user genuinely needs to read. Every
// other node has a single `next` out — the label is noise. Suppress it
// so the canvas reads cleanly when many statements stack vertically.
const showControlOutLabels = computed(
  () => controlOuts.value.length > 1,
);

function inlineExprFor(portId: string): string {
  return node.value.expressions?.[portId] ?? '';
}

function isPortWired(portId: string): boolean {
  const fn = graph.activeFunction;
  if (!fn) return false;
  return fn.edges.some(
    (e) =>
      e.kind === 'data' &&
      e.target.node === node.value.id &&
      e.target.port === portId,
  );
}

/** Short label of the source node for a wired data input. */
function wiredSourceLabel(portId: string): string {
  const src = wiredSourceNode(portId);
  if (!src) return 'connected';
  const d = src.data;
  switch (d.kind) {
    case 'varGet':      return d.varName || 'var';
    case 'literal':     return d.value || d.litType;
    case 'fieldAccess': return `.${d.fieldName}`;
    case 'indexRead':   return 'arr[i]';
    case 'binaryOp':    return d.op;
    case 'enumVariant': return `${d.enumName}::${d.variantName}`;
    case 'call':        return 'fn()';
    case 'trigger':     return 'payload';
    case 'let':         return d.varName;
    case 'forEach':     return d.iteratorName;
    default:            return d.kind;
  }
}

function wiredSourceNode(portId: string): GraphNode | undefined {
  const fn = graph.activeFunction;
  if (!fn) return undefined;
  const edge = fn.edges.find(
    (e) =>
      e.kind === 'data' &&
      e.target.node === node.value.id &&
      e.target.port === portId,
  );
  if (!edge) return undefined;
  return fn.nodes.find((n) => n.id === edge.source.node);
}

/** Click handler on the "from X" chip: pan the canvas to the source. */
function jumpToSourcePort(portId: string, ev: MouseEvent) {
  ev.stopPropagation();
  const src = wiredSourceNode(portId);
  if (!src) return;
  ui.requestFocus(src.id);
}

function onExprInput(portId: string, e: Event) {
  const text = (e.target as HTMLInputElement).value;
  graph.updateNodeExpression(node.value.id, portId, text);
}

function placeholderFor(portId: string, kind: string): string {
  // Defer to the canonical portMeta table; fall back to the port id.
  const meta = portMeta(kind as NodeData['kind'], portId);
  return meta.placeholder ?? meta.label ?? portId;
}

// canDelete reflects the store's entry-point rule: a function must
// keep at least one entry (Start OR Trigger). For non-entry nodes this
// is always true; for entries it's true iff at least one OTHER entry
// remains. We compute it locally so the button can show as disabled
// with a clear tooltip — better UX than a button that silently no-ops.
const canDelete = computed<boolean>(() => {
  const k = node.value.data.kind;
  if (k !== 'start' && k !== 'trigger') return true;
  const fn = graph.activeFunction;
  if (!fn) return false;
  return fn.nodes.some(
    (n) =>
      n.id !== node.value.id &&
      (n.data.kind === 'start' || n.data.kind === 'trigger'),
  );
});

const deleteTooltip = computed<string>(() => {
  if (canDelete.value) return 'Delete';
  if (node.value.data.kind === 'start') {
    return 'Add a Trigger first — every function needs an entrypoint.';
  }
  return 'A function needs at least one entry — add another Trigger or a Start before deleting this one.';
});

// Duplicate is hidden on Start: only one Start per function by design.
const canDuplicate = computed<boolean>(() => node.value.data.kind !== 'start');

function handleDelete() {
  graph.removeNode(node.value.id);
  if (ui.selectedNodeId === node.value.id) ui.selectNode(null);
}

function handleDuplicate() {
  if (!canDuplicate.value) return;
  const dup = graph.duplicateNode(node.value.id);
  if (dup) ui.selectNode(dup.id);
}

function labelForKind(data: NodeData): string {
  switch (data.kind) {
    case 'note':
      return 'note';
    case 'frame':
      return data.title || 'Section';
    case 'start':
      return 'start()';
    case 'trigger': {
      const k = data.triggerKind;
      if (k === 'webhook') return `webhook · ${data.webhookPath ?? ''}`;
      if (k === 'timer') return `timer · ${data.cronExpr ?? ''}`;
      if (k === 'http') return `${data.httpMethod ?? 'POST'} ${data.httpPath ?? ''}`;
      if (k === 'manual') return `manual · ${data.eventName}`;
      return `event · ${data.eventName}`;
    }
    case 'let':
      return `let ${data.varName || '_'}: ${typeLabel(data.varType)}`;
    case 'assign':
      return `${data.varName || '_'} =`;
    case 'print':
      return 'print';
    case 'return':
      return 'return';
    case 'branch':
      return data.hasElse ? 'if / else' : 'if';
    case 'while':
      return 'while';
    case 'forEach':
      return `for ${data.iteratorName || 'item'} in`;
    case 'binaryOp':
      return data.op;
    case 'unaryOp':
      return `${data.op}x`;
    case 'varGet':
      return data.varName || 'var';
    case 'literal':
      return formatLiteralPreview(data.litType, data.value);
    case 'arrayLiteral':
      return `[${data.length}] ${typeLabel(data.itemType)}`;
    case 'structLiteral':
      return `${data.structName || 'struct'} {}`;
    case 'fieldAccess':
      return `.${data.fieldName || 'field'}`;
    case 'fieldSet':
      return `.${data.fieldName || 'field'} =`;
    case 'indexRead':
      return '[i]';
    case 'indexSet':
      return '[i] =';
    case 'enumVariant':
      return `${data.enumName || '?'}::${data.variantName || '?'}`;
    case 'call': {
      const fn = graph.workflow.functions.find((f) => f.id === data.functionId);
      return `${fn?.name ?? 'call'}()`;
    }
  }
}

function formatLiteralPreview(t: string, v: string): string {
  if (t === 'str') {
    const s = v ?? '';
    return `"${s.length > 20 ? s.slice(0, 20) + '…' : s}"`;
  }
  if (t === 'char') return `'${(v ?? ' ')[0] ?? ' '}'`;
  return v || '0';
}
</script>

<template>
  <!--
    Annotation nodes (note / frame) get their own minimal renderers.
    They have no ports, no execution semantics — just visual aids for
    large workflows. Falls through to the normal node renderer below.
  -->
  <div
    v-if="node.data.kind === 'frame'"
    class="sf-frame"
    :class="{ selected }"
    :style="{
      width: node.data.width + 'px',
      height: node.data.height + 'px',
    }"
  >
    <div class="frame-title">
      <input
        class="frame-title-input nodrag nopan"
        :value="node.data.title"
        placeholder="Section title…"
        spellcheck="false"
        @click.stop
        @mousedown.stop
        @input="(e) => graph.updateNodeData(node.id, { title: (e.target as HTMLInputElement).value } as Partial<NodeData>)"
      />
    </div>
    <!--
      South-east corner resize handle. nodrag+nopan prevent Vue Flow
      from intercepting the mousedown; @mousedown.stop keeps the frame
      itself from starting a drag-move. Visible only on hover/select.
    -->
    <div
      class="frame-resize nodrag nopan"
      @mousedown="onFrameResizeStart"
    >
      <svg viewBox="0 0 10 10" width="10" height="10" fill="none">
        <path d="M2 9 L9 2 M5 9 L9 5 M8 9 L9 8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
      </svg>
    </div>
  </div>
  <div
    v-else-if="node.data.kind === 'note'"
    class="sf-note"
    :class="{ selected }"
    @mouseenter="ui.setHovered(node.id)"
    @mouseleave="ui.setHovered(null)"
  >
    <textarea
      class="note-body nodrag nopan"
      :value="node.data.text"
      placeholder="Add a note…"
      spellcheck="false"
      @click.stop
      @mousedown.stop
      @input="(e) => graph.updateNodeData(node.id, { text: (e.target as HTMLTextAreaElement).value } as Partial<NodeData>)"
    />
  </div>
  <div
    v-else
    :class="[
      'sf-node',
      `cat-${category}`,
      { selected, 'is-running': simStatus === 'running', 'is-visited': simStatus === 'visited', 'is-failed': simStatus === 'failed' },
    ]"
    @mouseenter="ui.setHovered(node.id)"
    @mouseleave="ui.setHovered(null)"
  >
    <div
      class="header"
      @mouseenter="showTooltip"
      @mouseleave="hideTooltip"
    >
      <span class="node-icon" :style="{ color: categoryDot }" v-html="categoryIcon" />
      <span class="title" :title="kindLabel">{{ kindLabel }}</span>
      <span v-if="headerBadge" class="header-badge">{{ headerBadge }}</span>
      <Transition name="tip">
        <div v-if="tooltipVisible" class="node-tooltip">
          <div class="tip-title">{{ kindLabel }}</div>
          <div class="tip-body">{{ explanation }}</div>
          <div v-if="simValueSummary" class="tip-runtime">
            <span class="tip-runtime-label">Last run:</span>
            <code>{{ simValueSummary }}</code>
          </div>
          <div v-if="simError" class="tip-error">
            <span class="tip-runtime-label">Error:</span>
            <code>{{ simError }}</code>
          </div>
        </div>
      </Transition>

      <!--
        Runtime value pill — visible only while a trace is loaded. Sits
        under the header so it doesn't push other chrome around. Shows
        what the node DID on the most recent simulated run (let value,
        branch decision, printed text, etc.). Tooltip on hover gives
        the longer detail.
      -->
      <div
        v-if="simValueSummary"
        class="runtime-pill"
        :class="{ failed: simStatus === 'failed' }"
        :title="simError || simValueSummary"
      >
        <span class="runtime-glyph">▸</span>
        <span class="runtime-text">{{ simValueSummary }}</span>
      </div>
      <div class="quick-actions nodrag">
        <button
          v-if="canDuplicate"
          class="qa-btn"
          title="Duplicate"
          aria-label="Duplicate node"
          @click.stop="handleDuplicate"
          @mousedown.stop
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none" aria-hidden="true">
            <rect x="2.5" y="2.5" width="5" height="5" rx="1" stroke="currentColor" stroke-width="1.2" />
            <rect x="4.5" y="4.5" width="5" height="5" rx="1" stroke="currentColor" stroke-width="1.2" />
          </svg>
        </button>
        <button
          class="qa-btn"
          :disabled="!canDelete"
          :title="deleteTooltip"
          :aria-label="canDelete ? 'Delete node' : deleteTooltip"
          @click.stop="handleDelete"
          @mousedown.stop
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none" aria-hidden="true">
            <path
              d="M3 3 9 9 M9 3 3 9"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>
    </div>

    <div v-if="dataIns.length > 0 || dataOuts.length > 0" class="body">
      <!-- Data inputs (left side) -->
      <div v-if="dataIns.length > 0" class="ports in">
        <div v-for="p in dataIns" :key="`in:${p.id}`" class="port-row">
          <Handle
            :id="p.id"
            type="target"
            :position="Position.Left"
            :class="['handle', typeCssClass(p.type)]"
          />
          <div class="port-cell">
            <div class="port-meta">
              <span class="port-label">{{ p.name }}</span>
              <button
                v-if="isPortWired(p.id) && inlineExprFor(p.id).trim() !== ''"
                class="pill override nodrag nopan"
                title="An inline expression overrides the connected source. Click to jump to source."
                @click.stop="(ev) => jumpToSourcePort(p.id, ev)"
                @mousedown.stop
              >override</button>
              <button
                v-else-if="isPortWired(p.id)"
                class="pill wire nodrag nopan"
                :title="`Connected from ${wiredSourceLabel(p.id)} — click to jump`"
                @click.stop="(ev) => jumpToSourcePort(p.id, ev)"
                @mousedown.stop
              >from {{ wiredSourceLabel(p.id) }} →</button>
              <span v-else class="port-type">{{ p.type ? typeLabel(p.type) : '' }}</span>
            </div>
            <!-- Inline input is always editable when not purely wired.
                 The Inspector handles the "override a wired input"
                 case in a richer panel; on the node card we just show
                 the source chip so the surface stays compact. -->
            <input
              v-if="!isPortWired(p.id) || inlineExprFor(p.id).trim() !== ''"
              class="port-input nodrag nopan"
              :value="inlineExprFor(p.id)"
              :placeholder="placeholderFor(p.id, node.data.kind)"
              spellcheck="false"
              @click.stop
              @mousedown.stop
              @input="onExprInput(p.id, $event)"
            />
          </div>
        </div>
      </div>

      <!-- Data outputs (right side) -->
      <div v-if="dataOuts.length > 0" class="ports out">
        <div v-for="p in dataOuts" :key="`out:${p.id}`" class="port-row">
          <div class="port-meta right">
            <span class="port-type">{{ p.type ? typeLabel(p.type) : '' }}</span>
            <span class="port-label">{{ p.name }}</span>
          </div>
          <Handle
            :id="p.id"
            type="source"
            :position="Position.Right"
            :class="['handle', typeCssClass(p.type)]"
          />
        </div>
      </div>
    </div>

    <!-- Control flow handles -->
    <Handle
      v-for="p in controlIns"
      :key="`cin:${p.id}`"
      :id="p.id"
      type="target"
      :position="Position.Top"
      class="handle control"
    />
    <div v-if="controlOuts.length > 0" class="control-out-row">
      <Handle
        v-for="(p, i) in controlOuts"
        :key="`cout:${p.id}`"
        :id="p.id"
        type="source"
        :position="Position.Bottom"
        :style="{ left: `${((i + 0.5) / controlOuts.length) * 100}%` }"
        class="handle control"
        :class="{
          'taken-path': simTakenPath === p.id,
          'not-taken-path': simTakenPath !== undefined && simTakenPath !== p.id,
        }"
      />
    </div>
    <!--
      Inline footer band for multi-out flow nodes. Labels live INSIDE the
      card so they never collide with downstream nodes when graphs pack
      tight. Suppressed for single-out nodes (most statements) — `next`
      would just add noise.
    -->
    <div v-if="showControlOutLabels" class="control-out-labels">
      <div
        v-for="(p, i) in controlOuts"
        :key="`coutlbl:${p.id}`"
        class="control-out-label"
        :class="{
          'taken-path': simTakenPath === p.id,
          'not-taken-path': simTakenPath !== undefined && simTakenPath !== p.id,
        }"
        :style="{ left: `${((i + 0.5) / controlOuts.length) * 100}%` }"
      >
        {{ p.name }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.sf-node {
  /* Clean white elevated card, like the reference. Local token overrides flip
     the whole node subtree to a light surface with dark content, so the card
     reads as white even on the dark canvas. */
  --sf-bg-1: #f3f5fa;
  --sf-bg-2: #ffffff;
  --sf-bg-3: #eef1f7;
  --sf-bg-4: #e6e9f2;
  --sf-text-0: #20222e;
  --sf-text-1: #474a5b;
  --sf-text-2: #8b90a3;
  --sf-text-3: #b4b8c6;
  --sf-border: #edeef4;
  --sf-border-strong: #e2e4ee;
  background: #ffffff;
  color: var(--sf-text-0);
  border: 1px solid rgba(20, 22, 34, 0.06);
  border-radius: 16px;
  min-width: 220px;
  max-width: 400px;
  font-size: 0.6875rem;
  position: relative;
  user-select: none;
  cursor: grab;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.26), 0 2px 6px rgba(0, 0, 0, 0.14);
  transition:
    border-color 0.12s ease,
    box-shadow 0.14s ease,
    transform 0.12s ease,
    background 0.12s ease;
}
.sf-node:active {
  cursor: grabbing;
}
.sf-node:hover {
  border-color: rgba(20, 22, 34, 0.10);
  box-shadow: 0 18px 40px rgba(0, 0, 0, 0.32), 0 3px 8px rgba(0, 0, 0, 0.18);
}
.sf-node.selected {
  border-color: var(--sf-accent);
  box-shadow:
    0 0 0 2px var(--sf-accent-ring),
    0 18px 40px rgba(0, 0, 0, 0.32);
}

.header {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 12px;
  border-bottom: 1px solid var(--sf-border);
}
.node-icon {
  width: 24px;
  height: 24px;
  border-radius: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--sf-bg-3);
  flex-shrink: 0;
}
.node-icon :deep(svg) { width: 14px; height: 14px; }
.role-glyph {
  font-family: var(--sf-font-mono);
  font-size: 0.75rem;
  line-height: 1;
  color: var(--sf-text-2);
  flex-shrink: 0;
}
.title {
  flex: 1;
  color: var(--sf-text-0);
  font-family: var(--sf-font-sans, system-ui, -apple-system, "Segoe UI", sans-serif);
  font-size: 0.82rem;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: -0.2px;
}
.header-badge {
  font-family: var(--sf-font-mono);
  font-size: 0.5rem;
  letter-spacing: 0.6px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 3px;
  background: rgba(232, 166, 87, 0.14);
  color: var(--sf-cat-trigger);
  flex-shrink: 0;
}
.quick-actions {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.sf-node:hover .quick-actions,
.sf-node.selected .quick-actions {
  opacity: 1;
}
.qa-btn {
  background: transparent;
  border: none;
  padding: 3px;
  border-radius: 2px;
  color: var(--sf-text-3);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.qa-btn:hover {
  color: var(--sf-text-0);
  background: var(--sf-bg-4);
}
.qa-btn:last-child:hover:not(:disabled) {
  color: var(--sf-error);
  background: rgba(255, 77, 79, 0.12);
}
.qa-btn:disabled {
  opacity: 0.32;
  cursor: not-allowed;
}

.body {
  padding: 6px 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ports {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.port-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  position: relative;
  min-height: 18px;
}
.ports.in .port-row {
  padding-left: 10px;
  padding-right: 8px;
}
.ports.out .port-row {
  padding-left: 8px;
  padding-right: 10px;
  justify-content: flex-end;
}
.port-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}
.port-meta {
  display: flex;
  align-items: center;
  gap: 6px;
}
.port-meta.right {
  justify-content: flex-end;
}
.port-label {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-1);
}
.port-type {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  color: var(--sf-text-3);
}
.pill.wire,
.pill.override {
  font-family: var(--sf-font-mono);
  font-size: 0.5625rem;
  padding: 1px 5px;
  border-radius: 2px;
  border: none;
  cursor: pointer;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: background 0.12s ease, color 0.12s ease;
}
.pill.wire {
  color: var(--sf-accent);
  background: var(--sf-accent-dim);
  letter-spacing: 0.3px;
}
.pill.wire:hover {
  background: rgba(108, 92, 231, 0.24);
  color: var(--sf-text-0);
}
.pill.override {
  color: var(--sf-cat-trigger);
  background: rgba(232, 166, 87, 0.16);
  letter-spacing: 0.4px;
  text-transform: uppercase;
}
.pill.override:hover {
  background: rgba(232, 166, 87, 0.28);
  color: #1a1208;
}
.port-input {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  color: var(--sf-text-0);
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: 3px;
  padding: 3px 6px;
  outline: none;
  width: 100%;
  transition: border-color 0.12s ease;
}
.port-input:hover {
  border-color: var(--sf-border-strong);
}
.port-input:focus {
  border-color: var(--sf-accent);
  background: var(--sf-bg-2);
  box-shadow: 0 0 0 1px var(--sf-accent-dim);
}
.port-input::placeholder {
  color: var(--sf-text-3);
  font-style: italic;
}

.control-out-row {
  position: relative;
  height: 10px;
}
.control-out-labels {
  position: relative;
  height: 16px;
  border-top: 1px solid var(--sf-border);
  background: var(--sf-bg-1);
  border-bottom-left-radius: var(--sf-radius-md);
  border-bottom-right-radius: var(--sf-radius-md);
}
.control-out-label {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
  font-size: 0.5625rem;
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  white-space: nowrap;
  pointer-events: none;
  letter-spacing: 0.5px;
  text-transform: lowercase;
}

.handle.control {
  background: var(--sf-text-2);
  border-color: var(--sf-bg-2);
  border-radius: 2px;
}
.handle.data-int { background: var(--sf-type-int); }
.handle.data-float { background: var(--sf-type-float); }
.handle.data-bool { background: var(--sf-type-bool); }
.handle.data-str { background: var(--sf-type-str); }
.handle.data-char { background: var(--sf-type-char); }
.handle.data-array { background: var(--sf-type-array); }
.handle.data-struct { background: var(--sf-type-struct); }
.handle.data-enum { background: var(--sf-type-enum); }
.handle.data-any { background: var(--sf-type-any); }

/* =================================================================
 *  Frame annotation — translucent region wrapper for big workflows
 * ================================================================= */
.sf-frame {
  position: relative;
  background: rgba(255, 255, 255, 0.018);
  border: 1px dashed rgba(255, 255, 255, 0.14);
  border-radius: var(--sf-radius-lg);
  cursor: grab;
  transition: border-color 0.12s ease, background 0.12s ease;
}
.sf-frame:active {
  cursor: grabbing;
}
.sf-frame:hover {
  background: rgba(255, 255, 255, 0.03);
  border-color: rgba(255, 255, 255, 0.22);
}
.sf-frame.selected {
  border-color: var(--sf-accent);
  background: rgba(108, 92, 231, 0.04);
}
.frame-title {
  position: absolute;
  top: -12px;
  left: 14px;
  background: var(--sf-canvas-bg);
  padding: 0 6px;
  display: flex;
}
.frame-title-input {
  background: transparent;
  border: none;
  color: var(--sf-text-1);
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  padding: 0;
  outline: none;
  width: max-content;
  min-width: 80px;
}
.frame-title-input::placeholder {
  color: var(--sf-text-3);
}
.frame-resize {
  position: absolute;
  bottom: 0;
  right: 0;
  width: 18px;
  height: 18px;
  display: flex;
  align-items: flex-end;
  justify-content: flex-end;
  padding: 3px;
  cursor: nwse-resize;
  color: var(--sf-text-3);
  opacity: 0;
  transition: opacity 0.12s ease, color 0.12s ease;
  border-bottom-right-radius: var(--sf-radius-lg);
}
.sf-frame:hover .frame-resize,
.sf-frame.selected .frame-resize {
  opacity: 1;
}
.frame-resize:hover {
  color: var(--sf-accent);
}

/* =================================================================
 *  Note annotation — sticky-note for free-text annotation
 * ================================================================= */
.sf-note {
  background: rgba(245, 200, 90, 0.07);
  border: 1px solid rgba(245, 200, 90, 0.28);
  border-radius: var(--sf-radius-md);
  padding: 8px 10px;
  min-width: 200px;
  max-width: 320px;
  cursor: grab;
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.sf-note:active {
  cursor: grabbing;
}
.sf-note:hover {
  border-color: rgba(245, 200, 90, 0.4);
}
.sf-note.selected {
  border-color: rgba(245, 200, 90, 0.7);
  box-shadow: 0 0 0 1px rgba(245, 200, 90, 0.18);
}
.note-body {
  width: 100%;
  background: transparent;
  border: none;
  resize: vertical;
  min-height: 60px;
  max-height: 260px;
  color: #f5c85a;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  line-height: 1.45;
  padding: 0;
  outline: none;
  font-feature-settings: 'cv11';
}
.note-body::placeholder {
  color: rgba(245, 200, 90, 0.4);
  font-style: italic;
}

/* =================================================================
 *  Runtime value pill — shown when a trace has been recorded
 * ================================================================= */
.runtime-pill {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0 8px 6px;
  padding: 3px 8px;
  background: rgba(0, 204, 136, 0.08);
  border: 1px solid rgba(0, 204, 136, 0.25);
  border-radius: var(--sf-radius-sm);
  font-size: 0.625rem;
  color: var(--sf-success);
  font-family: var(--sf-font-mono);
  max-width: calc(100% - 16px);
}
.runtime-pill.failed {
  background: rgba(255, 77, 79, 0.08);
  border-color: rgba(255, 77, 79, 0.35);
  color: var(--sf-error);
}
.runtime-glyph {
  font-size: 0.625rem;
  flex-shrink: 0;
}
.runtime-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: 0.1px;
}

/* Tooltip extension lines for runtime info. */
.tip-runtime,
.tip-error {
  margin-top: 4px;
  padding-top: 4px;
  border-top: 1px solid var(--sf-border);
  font-size: 0.625rem;
  color: var(--sf-text-2);
}
.tip-runtime-label {
  font-family: var(--sf-font-mono);
  font-size: 0.5rem;
  letter-spacing: 0.4px;
  color: var(--sf-text-3);
  text-transform: uppercase;
  margin-right: 5px;
}
.tip-runtime code,
.tip-error code {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-0);
}
.tip-runtime code {
  color: var(--sf-success);
}
.tip-error {
  color: var(--sf-error);
}
.tip-error code {
  color: var(--sf-error);
}

/* =================================================================
 *  Branch / loop arm dimming
 *  The control-out handle + footer label for the path that was taken
 *  on the most recent run is highlighted. Arms that were NOT taken
 *  fade so users can visually answer "why did it go this way?"
 *  at a glance. Only relevant when sim.takenPathByNodeId has an
 *  entry for this node.
 * ================================================================= */
.handle.control.taken-path {
  background: var(--sf-success) !important;
  box-shadow: 0 0 0 3px rgba(0, 204, 136, 0.18);
}
.handle.control.not-taken-path {
  background: var(--sf-text-3) !important;
  opacity: 0.45;
}
.control-out-label.taken-path {
  color: var(--sf-success);
  font-weight: 600;
}
.control-out-label.not-taken-path {
  opacity: 0.4;
}

/* Hover-help tooltip floating above the node header. Teaches what a
   kind does in one sentence so users don't need to open the Inspector
   to learn. Pointer-events: none so it doesn't intercept node drags. */
.node-tooltip {
  position: absolute;
  bottom: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-2);
  padding: 8px 10px;
  width: max-content;
  max-width: 280px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  line-height: 1.4;
  z-index: 10;
  pointer-events: none;
}
.node-tooltip .tip-title {
  font-family: var(--sf-font-mono);
  font-weight: 600;
  color: var(--sf-text-0);
  font-size: 0.625rem;
  letter-spacing: 0.3px;
  margin-bottom: 3px;
}
.node-tooltip .tip-body {
  color: var(--sf-text-2);
  white-space: normal;
}
.tip-enter-active,
.tip-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.tip-enter-from,
.tip-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(4px);
}
</style>
