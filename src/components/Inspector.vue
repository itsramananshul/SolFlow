<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import { useGraphStore } from '@/stores/graph.store';
import { useUIStore } from '@/stores/ui.store';
import { useSimulationStore } from '@/stores/simulation.store';
import ExpressionHelper from './ExpressionHelper.vue';
import BranchConditionBuilder from './BranchConditionBuilder.vue';
import {
  BINARY_OPS,
  UNARY_OPS,
  type BinaryOpSymbol,
  type HttpMethod,
  type TriggerKind,
  type UnaryOpSymbol,
  type SolPrimitive,
  type SolType,
} from '@/graph/schema';
import { bindingsInScope } from '@/graph/scope';
import { recordTrace } from '@/runtime/simulate';
import { portMeta } from '@/graph/portMeta';
import type { GraphEdge, GraphNode } from '@/graph/schema';

const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
// Friendlier labels for triggers — backend kind stays the same, the UI
// stops shouting jargon at first-time users.
interface TriggerKindOption {
  value: TriggerKind;
  label: string;
  blurb: string;
}
const TRIGGER_KIND_OPTIONS: TriggerKindOption[] = [
  { value: 'manual',  label: 'Run manually',     blurb: 'Someone clicks a button to start it.' },
  { value: 'webhook', label: 'Webhook (URL)',    blurb: 'Anyone POSTs to a URL to start it.' },
  { value: 'timer',   label: 'On a schedule',    blurb: 'Runs every minute, hour, day…' },
  { value: 'event',   label: 'When something happens', blurb: 'Reacts to a named event in your system.' },
  { value: 'http',    label: 'HTTP request',     blurb: 'A REST endpoint people can call.' },
];

// Timer presets — most users want "every 5 minutes", not a cron expression.
// "Custom" reveals the raw cron input behind the Advanced toggle.
interface TimerPreset {
  label: string;
  cron: string;
}
const TIMER_PRESETS: TimerPreset[] = [
  { label: 'Every minute',     cron: '* * * * *' },
  { label: 'Every 5 minutes',  cron: '*/5 * * * *' },
  { label: 'Every 15 minutes', cron: '*/15 * * * *' },
  { label: 'Every hour',       cron: '0 * * * *' },
  { label: 'Every day at 9am', cron: '0 9 * * *' },
  { label: 'Custom schedule',  cron: '' },
];
function presetForCron(cron: string): string {
  const match = TIMER_PRESETS.find((p) => p.cron && p.cron === cron);
  return match ? match.label : 'Custom schedule';
}
function applyTimerPreset(label: string) {
  const preset = TIMER_PRESETS.find((p) => p.label === label);
  if (!preset) return;
  if (preset.cron) update({ cronExpr: preset.cron });
  // Custom: keep whatever the user already had, advanced field becomes editable.
}

// Per-trigger "Advanced" disclosure state. Keyed by node id so each
// trigger remembers its own toggle.
const advancedOpen = ref<Record<string, boolean>>({});
function toggleAdvanced(nodeId: string) {
  advancedOpen.value = {
    ...advancedOpen.value,
    [nodeId]: !advancedOpen.value[nodeId],
  };
}
function isAdvancedOpen(nodeId: string): boolean {
  return !!advancedOpen.value[nodeId];
}

// Template type narrowing doesn't follow v-if into find() callbacks, so
// expose the current trigger blurb as a computed with explicit guarding.
const currentTriggerBlurb = computed<string>(() => {
  const d = selectedNode.value?.data;
  if (!d || d.kind !== 'trigger') return '';
  return (
    TRIGGER_KIND_OPTIONS.find((o) => o.value === d.triggerKind)?.blurb ?? ''
  );
});

const graph = useGraphStore();
const ui = useUIStore();
const sim = useSimulationStore();

const copyMsg = ref<string | null>(null);
function copyToClipboard(text: string) {
  if (typeof navigator !== 'undefined' && navigator.clipboard) {
    navigator.clipboard.writeText(text).catch(() => {
      /* ignore */
    });
  }
  copyMsg.value = 'Copied';
  setTimeout(() => (copyMsg.value = null), 1200);
}

function triggerEvent() {
  if (!selectedNode.value || selectedNode.value.data.kind !== 'trigger') return;
  const trace = recordTrace(graph.workflow, {
    entryTriggerId: selectedNode.value.id,
  });
  sim.play(trace);
}

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

/**
 * One-line summary of what the selected node *currently* does, with
 * actual configured values (event name, cron, variable, etc.) baked in.
 * Different from the SolNode hover tooltip (which explains the kind in
 * the abstract). This is "what does THIS instance do, right now?"
 */
const nodeSummary = computed<string>(() => {
  const n = selectedNode.value;
  if (!n) return '';
  const d = n.data;
  switch (d.kind) {
    case 'start':
      return 'Function entry — execution starts here.';
    case 'trigger': {
      if (d.triggerKind === 'webhook') return `Runs when someone POSTs to ${d.webhookPath || 'your webhook URL'}.`;
      if (d.triggerKind === 'timer') {
        const preset = describeCron(d.cronExpr ?? '');
        return preset ? `Runs ${preset.toLowerCase()}.` : `Runs on schedule "${d.cronExpr}".`;
      }
      if (d.triggerKind === 'event') return `Runs when "${d.eventName}" happens.`;
      if (d.triggerKind === 'http') return `Runs on ${d.httpMethod ?? 'POST'} ${d.httpPath || '/'}.`;
      return 'Runs when manually triggered.';
    }
    case 'let':
      return `Declares variable "${d.varName}" of type ${typeAsString(d.varType)}.`;
    case 'assign':
      return d.varName ? `Updates variable "${d.varName}".` : 'Assigns a value to a variable.';
    case 'print':
      return 'Writes a value to the run log.';
    case 'return':
      return d.hasValue ? 'Returns a value from this function.' : 'Ends this function with no value.';
    case 'branch':
      return d.hasElse
        ? 'Goes one of two ways: then if true, else if false.'
        : 'Continues only if the condition is true.';
    case 'while':
      return 'Repeats the body while the condition stays true.';
    case 'forEach':
      return `Walks through each item as "${d.iteratorName}".`;
    case 'binaryOp':
      return `Computes lhs ${d.op} rhs.`;
    case 'unaryOp':
      return `Computes ${d.op}operand.`;
    case 'varGet':
      return d.varName ? `Reads the current value of "${d.varName}".` : 'Reads a variable.';
    case 'literal':
      return `A constant ${d.litType} value: ${d.value}.`;
    case 'arrayLiteral':
      return `Builds a ${d.length}-item array of ${typeAsString(d.itemType)}.`;
    case 'structLiteral':
      return d.structName ? `Constructs a ${d.structName}.` : 'Constructs a struct (pick which one).';
    case 'fieldAccess':
      return d.fieldName ? `Reads .${d.fieldName} from a ${d.structName}.` : 'Reads a struct field.';
    case 'fieldSet':
      return d.fieldName ? `Writes into .${d.fieldName} of a ${d.structName}.` : 'Writes a struct field.';
    case 'indexRead':
      return 'Reads an array element at an index.';
    case 'indexSet':
      return 'Writes an array element at an index.';
    case 'enumVariant':
      return d.enumName && d.variantName ? `The "${d.enumName}::${d.variantName}" variant.` : 'An enum variant.';
    case 'call':
      return d.functionId
        ? `Calls function "${graph.workflow.functions.find((f) => f.id === d.functionId)?.name ?? '?'}".`
        : 'Calls another function (pick which one).';
    case 'note':
      return 'A sticky note. Does not affect execution.';
    case 'frame':
      return `Visually groups nodes under "${d.title || 'this section'}".`;
  }
});

function describeCron(cron: string): string | null {
  const map: Record<string, string> = {
    '* * * * *': 'Every minute',
    '*/5 * * * *': 'Every 5 minutes',
    '*/15 * * * *': 'Every 15 minutes',
    '0 * * * *': 'Every hour',
    '0 9 * * *': 'Every day at 9am',
  };
  return map[cron] ?? null;
}

function exprFor(portId: string): string {
  if (!selectedNode.value) return '';
  return selectedNode.value.expressions?.[portId] ?? '';
}

function setExpr(portId: string, text: string) {
  if (!selectedNode.value) return;
  graph.updateNodeExpression(selectedNode.value.id, portId, text);
}

function onExprInput(portId: string, e: Event) {
  const ta = e.target as HTMLTextAreaElement;
  setExpr(portId, ta.value);
  ta.style.height = 'auto';
  ta.style.height = Math.min(160, ta.scrollHeight) + 'px';
}

function isPortWired(portId: string): boolean {
  return wiredEdgeFor(portId) !== undefined;
}

/**
 * Find the data edge feeding into this node's `portId`, if any. Used by
 * the wired-input panel to surface "Connected from [source]" + Jump /
 * Disconnect actions.
 */
function wiredEdgeFor(portId: string): GraphEdge | undefined {
  if (!selectedNode.value || !graph.activeFunction) return undefined;
  return graph.activeFunction.edges.find(
    (e) =>
      e.kind === 'data' &&
      e.target.node === selectedNode.value!.id &&
      e.target.port === portId,
  );
}

function sourceNodeFor(portId: string): GraphNode | undefined {
  const edge = wiredEdgeFor(portId);
  if (!edge) return undefined;
  return graph.activeFunction?.nodes.find((n) => n.id === edge.source.node);
}

/** Short human description of the source — used in the "from X" chip. */
function sourceLabel(portId: string): string {
  const src = sourceNodeFor(portId);
  if (!src) return 'another node';
  const d = src.data;
  switch (d.kind) {
    case 'varGet':       return d.varName || 'variable';
    case 'literal':      return `literal ${d.value}`;
    case 'binaryOp':     return `${d.op} operator`;
    case 'unaryOp':      return `${d.op}x operator`;
    case 'fieldAccess':  return `.${d.fieldName}`;
    case 'indexRead':    return 'arr[i]';
    case 'enumVariant':  return `${d.enumName}::${d.variantName}`;
    case 'call':         return 'function result';
    case 'trigger':      return `${d.triggerKind} trigger payload`;
    case 'arrayLiteral': return `[${d.length}]`;
    case 'structLiteral': return `${d.structName} {}`;
    case 'let':          return `let ${d.varName}`;
    case 'forEach':      return `for-each item (${d.iteratorName})`;
    default:             return String(d.kind);
  }
}

function disconnectInput(portId: string) {
  const edge = wiredEdgeFor(portId);
  if (!edge) return;
  graph.removeEdge(edge.id);
}

function jumpToSource(portId: string) {
  const src = sourceNodeFor(portId);
  if (!src) return;
  ui.requestFocus(src.id);
}

/** True when the user has typed something into the inline field. */
function hasInlineExpr(portId: string): boolean {
  return exprFor(portId).trim() !== '';
}

function clearInlineOverride(portId: string) {
  setExpr(portId, '');
}

// =============================================================
//  Outbound connections — "this node's output is used by ___"
// =============================================================
// The inbound side ("connected from") tells the user where values
// come into a node. The outbound side tells them what consumes the
// node's outputs — equally important for imported workflows where a
// `let amount` deep inside a frame might feed three branches.

interface OutboundUse {
  /** Stable Vue key. */
  edgeId: string;
  /** Output port on the selected node (for labeling). */
  fromPortId: string;
  /** Target node + port. */
  targetNodeId: string;
  targetLabel: string;
  targetPortName: string;
}

function describeNodeShort(n: GraphNode): string {
  const d = n.data;
  switch (d.kind) {
    case 'start':         return 'start()';
    case 'trigger':       return `${d.triggerKind} trigger`;
    case 'let':           return `let ${d.varName}`;
    case 'assign':        return `${d.varName} =`;
    case 'print':         return 'print';
    case 'return':        return 'return';
    case 'branch':        return 'branch';
    case 'while':         return 'while';
    case 'forEach':       return `for ${d.iteratorName}`;
    case 'binaryOp':      return `op ${d.op}`;
    case 'unaryOp':       return `op ${d.op}`;
    case 'varGet':        return d.varName || 'varGet';
    case 'literal':       return `${d.value}`;
    case 'arrayLiteral':  return `array[${d.length}]`;
    case 'structLiteral': return d.structName || 'struct';
    case 'fieldAccess':   return `.${d.fieldName}`;
    case 'fieldSet':      return `.${d.fieldName} =`;
    case 'indexRead':     return 'arr[i]';
    case 'indexSet':      return 'arr[i] =';
    case 'enumVariant':   return `${d.enumName}::${d.variantName}`;
    case 'call':          return 'call()';
    case 'note':          return 'note';
    case 'frame':         return d.title || 'Section';
  }
}

const outboundUses = computed<OutboundUse[]>(() => {
  const sel = selectedNode.value;
  const fn = graph.activeFunction;
  if (!sel || !fn) return [];
  const out: OutboundUse[] = [];
  for (const e of fn.edges) {
    if (e.kind !== 'data') continue;
    if (e.source.node !== sel.id) continue;
    const target = fn.nodes.find((n) => n.id === e.target.node);
    if (!target) continue;
    const port = target.ports.in.find((p) => p.id === e.target.port);
    out.push({
      edgeId: e.id,
      fromPortId: e.source.port,
      targetNodeId: target.id,
      targetLabel: describeNodeShort(target),
      targetPortName: port?.name ?? e.target.port,
    });
  }
  return out;
});

function disconnectOutbound(edgeId: string) {
  graph.removeEdge(edgeId);
}

function jumpToTarget(nodeId: string) {
  ui.requestFocus(nodeId);
}

// =============================================================
//  Expression-helper-driven inserts
// =============================================================
// The ExpressionHelper button next to each input emits an `insert`
// event with the snippet text. We splice it into the current value at
// the cursor position (or append, smartly spacing), then restore
// focus + cursor placement after Vue updates the DOM.

const exprRefs = ref<Record<string, HTMLInputElement | null>>({});
function setExprRef(portId: string, el: Element | null) {
  exprRefs.value[portId] = el as HTMLInputElement | null;
}
function insertIntoPort(portId: string, snippet: string) {
  const el = exprRefs.value[portId];
  const current = exprFor(portId);
  let next: string;
  let caret: number;
  if (el && document.activeElement === el && el.selectionStart !== null) {
    const start = el.selectionStart ?? current.length;
    const end = el.selectionEnd ?? start;
    const before = current.slice(0, start);
    const after = current.slice(end);
    const needLeadingSpace = before.length > 0 && !/\s$/.test(before);
    const inserted = needLeadingSpace ? ` ${snippet}` : snippet;
    next = before + inserted + after;
    caret = (before + inserted).length;
  } else {
    const needLeadingSpace = current.length > 0 && !/\s$/.test(current);
    next = current + (needLeadingSpace ? ` ${snippet}` : snippet);
    caret = next.length;
  }
  setExpr(portId, next);
  nextTick(() => {
    const e = exprRefs.value[portId];
    if (!e) return;
    e.focus();
    e.setSelectionRange(caret, caret);
  });
}

function setBranchCond(nodeId: string, cond: string) {
  graph.updateNodeExpression(nodeId, 'cond', cond);
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
      <!--
        Plain-English summary banner — shown FIRST so users immediately
        know what the selected node does *with its current settings*,
        before they have to read individual fields. Different from the
        hover tooltip (which explains the kind in the abstract).
      -->
      <section class="section summary-section">
        <p class="summary">{{ nodeSummary }}</p>
      </section>

      <!--
        Inputs section.
        Each input rendered as one of:

          (a) Not wired, no inline yet     →  plain expression input
                                              with examples + helper.

          (b) Not wired, inline filled in  →  same input, "Inline" tag.

          (c) Wired, no inline override    →  rich "Connected from ___"
                                              card with Jump / Disconnect
                                              / Override actions.

          (d) Wired AND inline filled in   →  "Inline override active"
                                              banner over the editable
                                              input, with "Use wired
                                              instead" + Jump + Disconnect.

        The emitter already prefers non-empty inline expressions over
        wired edges, so all four states behave consistently downstream.
      -->
      <section v-if="dataInPorts.length > 0" class="section">
        <div class="section-header">
          <span>Inputs</span>
          <span class="hint">type a SOL expression, or wire from another node</span>
        </div>

        <div v-for="p in dataInPorts" :key="p.id" class="field input-field">
          <!-- Friendly label + state tag -->
          <div class="input-label">
            <span class="input-label-text">{{ portMeta(selectedNode.data.kind, p.id).label ?? p.name }}</span>
            <span v-if="isPortWired(p.id) && hasInlineExpr(p.id)" class="state-tag override">
              Inline override
            </span>
            <span v-else-if="isPortWired(p.id)" class="state-tag wired">
              Connected
            </span>
            <span v-else-if="hasInlineExpr(p.id)" class="state-tag inline">
              Inline
            </span>
          </div>

          <!--
            Wired source card. Shown whenever a data edge feeds this
            port. The connection summary + jump/disconnect actions live
            here. The override expression input is rendered BELOW the
            card, always visible — no toggle to manage; the emitter
            already gives non-empty inline expressions priority over
            wired edges, so this is a clean "empty = use connection /
            non-empty = override wins" model.
          -->
          <div
            v-if="isPortWired(p.id)"
            class="wired-card"
            :class="{ 'override-active': hasInlineExpr(p.id) }"
          >
            <div class="wired-summary">
              <svg viewBox="0 0 16 16" width="11" height="11" class="wire-glyph" fill="none">
                <path
                  d="M2 8 H6 M10 8 H14"
                  stroke="currentColor"
                  stroke-width="1.4"
                  stroke-linecap="round"
                />
                <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.4" />
              </svg>
              <span>
                Connected from <strong>{{ sourceLabel(p.id) }}</strong>
              </span>
            </div>
            <div v-if="hasInlineExpr(p.id)" class="override-banner">
              Inline override active — your typed value below wins over the connection.
            </div>
            <div class="wired-actions">
              <button type="button" class="wired-btn" @click="jumpToSource(p.id)">
                Jump to source →
              </button>
              <button type="button" class="wired-btn" @click="disconnectInput(p.id)">
                Disconnect
              </button>
              <button
                v-if="hasInlineExpr(p.id)"
                type="button"
                class="wired-btn"
                @click="clearInlineOverride(p.id)"
              >
                Clear override
              </button>
            </div>
          </div>

          <!--
            Expression input. ALWAYS visible:
              - Unwired port → primary entry, friendly placeholder.
              - Wired port  → secondary "override" entry. Empty means
                              "use the connection above"; typing anything
                              flips on override mode (banner appears in
                              the card above).
            One field, two behaviors — no toggle. The ⨁ helper button
            on the right opens a picker (variables / operators / quick
            literals / examples) so users don't have to memorize SOL
            syntax to fill the field.
          -->
          <div class="expr-row">
            <input
              :ref="(el) => setExprRef(p.id, el as Element | null)"
              class="expr-input"
              :class="{ 'override-input': isPortWired(p.id) }"
              :value="exprFor(p.id)"
              :placeholder="
                isPortWired(p.id)
                  ? 'Override with expression — leave empty to use the connection'
                  : (portMeta(selectedNode.data.kind, p.id).placeholder ?? p.name)
              "
              spellcheck="false"
              @input="(e) => setExpr(p.id, (e.target as HTMLInputElement).value)"
            />
            <ExpressionHelper
              :node-id="selectedNode.id"
              :node-kind="selectedNode.data.kind"
              :port-id="p.id"
              @insert="(text: string) => insertIntoPort(p.id, text)"
            />
          </div>

          <!-- Example chips: click to fill the input. Hidden once the
               user has typed anything (no need to keep nagging) or
               when the port is wired (the connection is the example). -->
          <div
            v-if="
              !hasInlineExpr(p.id) &&
              !isPortWired(p.id) &&
              (portMeta(selectedNode.data.kind, p.id).examples?.length ?? 0) > 0
            "
            class="example-chips"
          >
            <span class="example-prefix">e.g.</span>
            <button
              v-for="ex in portMeta(selectedNode.data.kind, p.id).examples"
              :key="ex"
              type="button"
              class="example-chip"
              @click="setExpr(p.id, ex)"
            >{{ ex }}</button>
          </div>

          <!-- Helper blurb -->
          <span
            v-if="portMeta(selectedNode.data.kind, p.id).helper"
            class="help-blurb input-helper"
          >
            {{ portMeta(selectedNode.data.kind, p.id).helper }}
          </span>
        </div>
      </section>

      <!--
        Outbound connections — "this node's outputs are read by ___".
        Symmetric counterpart to the wired-card inbound view. Shown only
        when the selected node has at least one wired data output. Each
        row gives a Jump + Disconnect action so a user reading an
        imported workflow can quickly answer "what uses this?" and
        unwire it if they want to replace its consumers.
      -->
      <section v-if="outboundUses.length > 0" class="section">
        <div class="section-header">
          <span>Used by</span>
          <span class="hint">{{ outboundUses.length }} consumer{{ outboundUses.length === 1 ? '' : 's' }}</span>
        </div>
        <div
          v-for="use in outboundUses"
          :key="use.edgeId"
          class="outbound-row"
        >
          <div class="outbound-summary">
            <svg viewBox="0 0 16 16" width="11" height="11" class="wire-glyph out" fill="none">
              <path
                d="M2 8 H6 M10 8 H14"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
              <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.4" />
            </svg>
            <span>
              <code class="port-name">{{ use.fromPortId }}</code>
              →
              <strong>{{ use.targetLabel }}</strong>
              <span class="dim"> · {{ use.targetPortName }}</span>
            </span>
          </div>
          <div class="outbound-actions">
            <button type="button" class="wired-btn" @click="jumpToTarget(use.targetNodeId)">
              Jump to consumer →
            </button>
            <button type="button" class="wired-btn" @click="disconnectOutbound(use.edgeId)">
              Disconnect
            </button>
          </div>
        </div>
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
          <p class="help-blurb">
            A branch checks a condition and goes one of two ways. The
            <code>then</code> path runs when the condition is true; the
            <code>else</code> path runs when it's false.
          </p>
          <!--
            Guided condition builder. "Build it" mode composes a SOL
            expression from three slots (left value · operator · right
            value); "Type it" mode is the raw expression input. Either
            mode writes to the `cond` port's inline expression — the
            same field the Inputs section above edits, so changes here
            and in the inputs section stay in sync via the store.
          -->
          <div class="field">
            <span class="field-label">Condition</span>
            <BranchConditionBuilder
              :node-id="selectedNode.id"
              :node-kind="selectedNode.data.kind"
              :model-value="exprFor('cond')"
              @update:model-value="(v: string) => setBranchCond(selectedNode!.id, v)"
            />
          </div>
          <label class="field checkbox-row">
            <input
              type="checkbox"
              :checked="selectedNode.data.hasElse"
              @change="(e) => update({ hasElse: (e.target as HTMLInputElement).checked })"
            />
            <span>Also handle the "else" case</span>
          </label>
        </template>

        <template v-else-if="selectedNode.data.kind === 'while'">
          <p class="help-blurb">
            A while loop repeats the steps connected to its
            <code>body</code> output for as long as the condition stays
            true. Type the condition above, then connect what should
            repeat to the body port.
          </p>
        </template>

        <template v-else-if="selectedNode.data.kind === 'forEach'">
          <p class="help-blurb">
            For-each walks through every item in an array. Each item is
            handed to the steps connected to <code>body</code>, then the
            loop moves on to the next item.
          </p>
          <label class="field">
            <span class="field-label">Name for each item</span>
            <input
              :value="selectedNode.data.iteratorName"
              placeholder="item"
              @input="(e) => update({ iteratorName: (e.target as HTMLInputElement).value })"
            />
            <span class="help-blurb">
              Inside the body, you'll refer to the current array element by
              this name (e.g. <code>order</code>, <code>row</code>).
            </span>
          </label>
          <label class="field">
            <span class="field-label">What kind of item is it?</span>
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

        <template v-else-if="selectedNode.data.kind === 'trigger'">
          <!-- Plain-English headline. The trigger kind drives the rest of
               the form, so it's the first decision and reads as a question. -->
          <label class="field">
            <span class="field-label">What starts this workflow?</span>
            <select
              :value="selectedNode.data.triggerKind"
              @change="(e) => update({ triggerKind: (e.target as HTMLSelectElement).value as TriggerKind })"
            >
              <option v-for="opt in TRIGGER_KIND_OPTIONS" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </option>
            </select>
            <span class="help-blurb">{{ currentTriggerBlurb }}</span>
          </label>

          <!-- Per-kind primary controls. Each has a friendly first label and
               a sub-hint; raw infra (event name, schema, cron, etc.) is
               tucked into the Advanced disclosure below. -->

          <!-- Webhook: URL is the only thing users actually need to copy. -->
          <label v-if="selectedNode.data.triggerKind === 'webhook'" class="field">
            <span class="field-label">Your webhook URL</span>
            <div class="copy-row">
              <input
                readonly
                :value="selectedNode.data.webhookPath"
                class="copy-input"
              />
              <button
                type="button"
                class="copy-btn"
                @click="copyToClipboard(selectedNode.data.webhookPath ?? '')"
              >Copy</button>
            </div>
            <span class="help-blurb">
              Anyone who sends a POST request to this URL will start the workflow.
            </span>
          </label>

          <!-- Timer: preset picker keeps cron behind Advanced. -->
          <label v-if="selectedNode.data.triggerKind === 'timer'" class="field">
            <span class="field-label">Run every…</span>
            <select
              :value="presetForCron(selectedNode.data.cronExpr ?? '')"
              @change="(e) => applyTimerPreset((e.target as HTMLSelectElement).value)"
            >
              <option v-for="p in TIMER_PRESETS" :key="p.label" :value="p.label">
                {{ p.label }}
              </option>
            </select>
            <span class="help-blurb">
              Pick how often you want this workflow to run. Choose "Custom
              schedule" to write a cron expression in Advanced settings.
            </span>
          </label>

          <!-- Event: one human-friendly field. -->
          <label v-if="selectedNode.data.triggerKind === 'event'" class="field">
            <span class="field-label">When does it run?</span>
            <input
              :value="selectedNode.data.eventName"
              placeholder="e.g. invoice.created, user.signed_up"
              spellcheck="false"
              @input="(e) => update({ eventName: (e.target as HTMLInputElement).value })"
            />
            <span class="help-blurb">
              The name of the event your system emits. Other workflows or
              services emit this name to fire this trigger.
            </span>
          </label>

          <!-- HTTP: method + path together. -->
          <template v-if="selectedNode.data.triggerKind === 'http'">
            <label class="field">
              <span class="field-label">Which HTTP request triggers it?</span>
              <div class="http-row">
                <select
                  class="http-method"
                  :value="selectedNode.data.httpMethod"
                  @change="(e) => update({ httpMethod: (e.target as HTMLSelectElement).value as HttpMethod })"
                >
                  <option v-for="m in HTTP_METHODS" :key="m" :value="m">{{ m }}</option>
                </select>
                <input
                  class="http-path"
                  :value="selectedNode.data.httpPath"
                  placeholder="/api/orders"
                  spellcheck="false"
                  @input="(e) => update({ httpPath: (e.target as HTMLInputElement).value })"
                />
              </div>
              <span class="help-blurb">
                The HTTP method and path that, when called, start the workflow.
              </span>
            </label>
          </template>

          <!-- Manual: nothing extra to configure; just clarify what it means. -->
          <p v-if="selectedNode.data.triggerKind === 'manual'" class="help-blurb">
            A manual trigger runs only when someone clicks "Trigger Event ▷"
            below. Useful for testing or one-off workflows.
          </p>

          <!-- Sample data: kept primary because Trigger Event uses it. -->
          <label class="field">
            <span class="field-label">Sample data (used for testing)</span>
            <textarea
              class="mono-area"
              rows="4"
              :value="selectedNode.data.samplePayload"
              spellcheck="false"
              @input="(e) => update({ samplePayload: (e.target as HTMLTextAreaElement).value })"
            />
            <span class="help-blurb">
              When you click "Trigger Event ▷", this JSON is bound to the
              trigger's <code>payload</code> output so the rest of the workflow
              can read it.
            </span>
          </label>

          <div class="trigger-actions">
            <button type="button" class="trigger-btn" @click="triggerEvent">
              Trigger Event ▷
            </button>
            <span v-if="copyMsg" class="copy-msg">{{ copyMsg }}</span>
          </div>

          <!-- Advanced disclosure: raw infra fields live here so first-time
               users don't see four scary fields up-front. Power users get
               everything they need by opening it. -->
          <div class="advanced-toggle">
            <button
              type="button"
              class="advanced-btn"
              @click="toggleAdvanced(selectedNode.id)"
            >
              <span class="caret">{{ isAdvancedOpen(selectedNode.id) ? '▾' : '▸' }}</span>
              Advanced settings
            </button>
          </div>
          <div v-if="isAdvancedOpen(selectedNode.id)" class="advanced-body">
            <label class="field">
              <span class="field-label">Event name <span class="dim">(internal)</span></span>
              <input
                :value="selectedNode.data.eventName"
                spellcheck="false"
                @input="(e) => update({ eventName: (e.target as HTMLInputElement).value })"
              />
              <span class="help-blurb">
                A unique identifier used by the runtime to route this event.
              </span>
            </label>
            <label v-if="selectedNode.data.triggerKind === 'timer'" class="field">
              <span class="field-label">Custom cron expression</span>
              <input
                :value="selectedNode.data.cronExpr"
                placeholder="*/5 * * * *"
                spellcheck="false"
                class="mono-input"
                @input="(e) => update({ cronExpr: (e.target as HTMLInputElement).value })"
              />
              <span class="help-blurb">
                Standard cron format: minute, hour, day, month, weekday. e.g.
                <code>0 9 * * 1-5</code> = 9am on weekdays.
              </span>
            </label>
            <label class="field">
              <span class="field-label">Expected payload shape</span>
              <textarea
                class="mono-area"
                rows="3"
                :value="selectedNode.data.payloadSchema"
                spellcheck="false"
                @input="(e) => update({ payloadSchema: (e.target as HTMLTextAreaElement).value })"
              />
              <span class="help-blurb">
                Describes the structure of incoming events. Used for
                validation and downstream type-checking.
              </span>
            </label>
          </div>
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

        <template v-else-if="selectedNode.data.kind === 'frame'">
          <label class="field">
            <span class="field-label">Section title</span>
            <input
              :value="selectedNode.data.title"
              placeholder="e.g. Payment Processing"
              @input="(e) => update({ title: (e.target as HTMLInputElement).value })"
            />
          </label>
          <div class="size-row">
            <label class="field size-field">
              <span class="field-label">Width</span>
              <input
                type="number"
                min="200"
                step="20"
                :value="selectedNode.data.width"
                @input="(e) => update({ width: Math.max(200, Number((e.target as HTMLInputElement).value) || 200) })"
              />
            </label>
            <label class="field size-field">
              <span class="field-label">Height</span>
              <input
                type="number"
                min="140"
                step="20"
                :value="selectedNode.data.height"
                @input="(e) => update({ height: Math.max(140, Number((e.target as HTMLInputElement).value) || 140) })"
              />
            </label>
          </div>
          <p class="help-blurb">
            Drag the corner of the frame to resize visually, or type
            exact dimensions here. Resizing doesn't move the nodes
            inside — dragging the frame body does.
          </p>
        </template>

        <template v-else-if="selectedNode.data.kind === 'note'">
          <label class="field">
            <span class="field-label">Note text</span>
            <textarea
              class="mono-area"
              rows="5"
              :value="selectedNode.data.text"
              placeholder="Add a note for your team…"
              @input="(e) => update({ text: (e.target as HTMLTextAreaElement).value })"
            />
          </label>
          <p class="help-blurb">
            Notes are for humans only — they're never emitted as SOL and
            don't affect execution.
          </p>
        </template>

        <template v-else>
          <p class="help-blurb">
            This node has no settings — connect its ports above to use it.
          </p>
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
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.kind-tag {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-2);
  padding: 2px 6px;
  border-radius: 3px;
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border);
}
.empty {
  padding: 20px 16px;
  color: var(--sf-text-3);
  font-size: 0.75rem;
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
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.6px;
  text-transform: uppercase;
}
.section-header .hint {
  font-size: 0.5625rem;
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
  font-size: 0.6875rem;
  color: var(--sf-text-1);
}
.port-name {
  font-family: var(--sf-font-mono);
}
.wire-pill,
.inline-pill {
  font-size: 0.5625rem;
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
  font-size: 0.75rem;
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
  font-size: 0.6875rem;
  margin: 0;
}
.mono-area {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  resize: vertical;
  min-height: 48px;
}
.copy-row {
  display: flex;
  gap: 4px;
}
.copy-input {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
  flex: 1;
  background: var(--sf-bg-1);
  cursor: text;
}
.copy-btn {
  background: var(--sf-bg-3);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  color: var(--sf-text-1);
  font-size: 0.625rem;
  padding: 2px 8px;
  cursor: pointer;
}
.copy-btn:hover {
  background: var(--sf-bg-4);
  color: var(--sf-text-0);
}
.trigger-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}
.trigger-btn {
  background: var(--sf-cat-trigger);
  color: #1a1208;
  border: none;
  border-radius: var(--sf-radius-sm);
  padding: 6px 12px;
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  letter-spacing: 0.2px;
}
.trigger-btn:hover {
  filter: brightness(1.08);
}
.copy-msg {
  font-size: 0.625rem;
  color: var(--sf-success, #5fd97a);
  letter-spacing: 0.4px;
}
.help-blurb {
  display: block;
  margin-top: 4px;
  font-size: 0.625rem;
  line-height: 1.5;
  color: var(--sf-text-3);
}
.help-blurb code {
  font-family: var(--sf-font-mono);
  background: var(--sf-bg-3);
  padding: 1px 4px;
  border-radius: 2px;
  color: var(--sf-text-2);
}
.http-row {
  display: flex;
  gap: 4px;
}
.http-method {
  flex: 0 0 88px;
}
.http-path {
  flex: 1;
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.mono-input {
  font-family: var(--sf-font-mono);
  font-size: 0.6875rem;
}
.advanced-toggle {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px dashed var(--sf-border);
}
.advanced-btn {
  background: transparent;
  border: none;
  color: var(--sf-text-2);
  font-size: 0.6875rem;
  padding: 4px 0;
  cursor: pointer;
  letter-spacing: 0.2px;
}
.advanced-btn:hover {
  color: var(--sf-text-0);
  background: transparent;
}
.advanced-btn .caret {
  font-family: var(--sf-font-mono);
  margin-right: 4px;
}
.advanced-body {
  margin-top: 6px;
  padding: 10px 12px;
  background: var(--sf-bg-1);
  border-radius: var(--sf-radius-sm);
  border: 1px solid var(--sf-border);
}
.dim {
  color: var(--sf-text-3);
  font-weight: 400;
  font-size: 0.5625rem;
  letter-spacing: 0.3px;
}
.summary-section {
  background: var(--sf-bg-1);
}
.summary {
  margin: 0;
  font-size: 0.75rem;
  line-height: 1.5;
  color: var(--sf-text-1);
}
.size-row {
  display: flex;
  gap: 10px;
}
.size-field {
  flex: 1;
}

/* =================================================================
 *  Inputs section: wired-card, state tags, example chips
 * ================================================================= */
.input-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 14px;
}
.input-field:last-child {
  margin-bottom: 0;
}
.input-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  font-weight: 500;
}
.input-label-text {
  flex: 1;
  min-width: 0;
}
.state-tag {
  font-family: var(--sf-font-mono);
  font-size: 0.5rem;
  letter-spacing: 0.5px;
  text-transform: uppercase;
  padding: 2px 6px;
  border-radius: 3px;
  font-weight: 600;
  flex-shrink: 0;
}
.state-tag.wired {
  background: rgba(50, 145, 255, 0.14);
  color: var(--sf-accent);
}
.state-tag.inline {
  background: rgba(255, 255, 255, 0.06);
  color: var(--sf-text-2);
}
.state-tag.override {
  background: rgba(232, 166, 87, 0.18);
  color: var(--sf-cat-trigger);
}

.wired-card {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-left: 2px solid var(--sf-accent);
  border-radius: var(--sf-radius-sm);
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.wired-card.override-active {
  border-left-color: var(--sf-cat-trigger);
}
.wired-summary {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
}
.wired-summary strong {
  color: var(--sf-text-0);
  font-weight: 600;
}
.wire-glyph {
  color: var(--sf-accent);
  flex-shrink: 0;
}
.wired-card.override-active .wire-glyph {
  color: var(--sf-cat-trigger);
}
.override-banner {
  font-size: 0.625rem;
  color: var(--sf-cat-trigger);
  background: rgba(232, 166, 87, 0.08);
  padding: 4px 6px;
  border-radius: 3px;
  line-height: 1.4;
}
.wired-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.wired-btn {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 3px 8px;
  border-radius: 3px;
  font-size: 0.6875rem;
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.wired-btn:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}

.example-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
}
.example-prefix {
  font-size: 0.5625rem;
  color: var(--sf-text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
  margin-right: 2px;
}
.example-chip {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  color: var(--sf-text-2);
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  padding: 2px 7px;
  border-radius: 10px;
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.example-chip:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.input-helper {
  /* Aligned via help-blurb base style but tightened spacing here. */
  margin-top: 0;
}

.expr-row {
  display: flex;
  gap: 6px;
  align-items: stretch;
}
.expr-row .expr-input {
  flex: 1;
  min-width: 0;
}

.expr-input.override-input {
  /* Faint amber tint so the override field is visually distinct from
     the primary entry when the port is also wired. */
  border-color: rgba(232, 166, 87, 0.28);
  background: rgba(232, 166, 87, 0.04);
}
.expr-input.override-input:focus {
  border-color: var(--sf-cat-trigger);
  background: rgba(232, 166, 87, 0.08);
  box-shadow: 0 0 0 1px rgba(232, 166, 87, 0.32);
}

.outbound-row {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-left: 2px solid var(--sf-cat-flow);
  border-radius: var(--sf-radius-sm);
  padding: 8px 10px;
  margin-bottom: 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.outbound-row:last-child {
  margin-bottom: 0;
}
.outbound-summary {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 0.6875rem;
  color: var(--sf-text-1);
}
.outbound-summary strong {
  color: var(--sf-text-0);
  font-weight: 600;
}
.wire-glyph.out {
  color: var(--sf-cat-flow);
}
.outbound-actions {
  display: flex;
  gap: 4px;
}
</style>
