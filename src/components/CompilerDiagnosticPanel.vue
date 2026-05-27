<script setup lang="ts">
/**
 * Rich compiler-diagnostic panel.
 *
 * Replaces the simple inline list that used to live in
 * SourcePreview. Diagnostics are grouped by phase, each row is
 * clickable, and clicking emits a `focus` event with the source
 * range so the parent (SourcePreview) can scroll the CodeMirror
 * view to that line.
 *
 * Why split this out:
 *   - SourcePreview was getting crowded
 *   - the panel is reusable later for the import/run flows
 *   - testing diagnostic rendering in isolation is easier
 */
import { computed } from 'vue';
import type { DiagnosticPhase, SolDiagnostic } from '@/compiler/types';

const props = defineProps<{
  diagnostics: SolDiagnostic[];
  /** Editor state surfaced for status display only. */
  state: 'idle' | 'loading' | 'ready' | 'error';
  /** When state === 'error', the load-failure message. */
  errorMessage: string | null;
  /** Source text — used to derive line/col from byte spans. */
  source: string;
}>();

const emit = defineEmits<{
  /** Fired when the user clicks a diagnostic row. The parent
   *  should focus the CodeMirror cursor at this position. */
  (e: 'focus', loc: { start: number; end: number; line: number; col: number }): void;
}>();

const errorCount = computed(
  () => props.diagnostics.filter((d) => d.severity === 'Error').length,
);
const warningCount = computed(
  () => props.diagnostics.filter((d) => d.severity === 'Warning').length,
);

/** Diagnostics grouped by phase, ordered as they appear in the
 *  pipeline. Empty groups are filtered out at render time. */
const PHASE_ORDER: DiagnosticPhase[] = [
  'Lexer',
  'Parser',
  'Analyzer',
  'Codegen',
  'Runtime',
  'Internal',
];

interface PhaseGroup {
  phase: DiagnosticPhase;
  items: SolDiagnostic[];
}

const grouped = computed<PhaseGroup[]>(() => {
  const byPhase = new Map<DiagnosticPhase, SolDiagnostic[]>();
  for (const d of props.diagnostics) {
    const arr = byPhase.get(d.phase) ?? [];
    arr.push(d);
    byPhase.set(d.phase, arr);
  }
  // Stable order: errors first within each phase, then by source
  // position. Diagnostics without spans sort to the end.
  for (const arr of byPhase.values()) {
    arr.sort((a, b) => {
      // severity: Error < Warning < Note
      const sevOrder = { Error: 0, Warning: 1, Note: 2 } as const;
      const sa = sevOrder[a.severity];
      const sb = sevOrder[b.severity];
      if (sa !== sb) return sa - sb;
      // position
      const pa = a.span?.start ?? Number.MAX_SAFE_INTEGER;
      const pb = b.span?.start ?? Number.MAX_SAFE_INTEGER;
      return pa - pb;
    });
  }
  return PHASE_ORDER.filter((p) => byPhase.has(p)).map((p) => ({
    phase: p,
    items: byPhase.get(p)!,
  }));
});

/** Derive (line, col) — both 1-indexed — from a byte offset in
 *  the source. Mirrors `SourceSpan::to_line_col` in the Rust
 *  compiler. Assumes ASCII-ish source (multi-byte UTF-8 chars
 *  may produce off-by-one columns in literal strings; not a
 *  problem in practice for SOL today). */
function lineColAt(source: string, offset: number): { line: number; col: number } {
  let line = 1;
  let col = 1;
  for (let i = 0; i < offset && i < source.length; i++) {
    if (source.charCodeAt(i) === 10) {
      line++;
      col = 1;
    } else {
      col++;
    }
  }
  return { line, col };
}

function onClick(d: SolDiagnostic) {
  if (!d.span) return;
  const { line, col } = lineColAt(props.source, d.span.start);
  emit('focus', { start: d.span.start, end: d.span.end, line, col });
}

function phaseLabel(p: DiagnosticPhase): string {
  if (p === 'Internal') return 'internal compiler error';
  return p.toLowerCase();
}

function rowHasSpan(d: SolDiagnostic): boolean {
  return d.span !== null;
}
</script>

<template>
  <div class="panel" :class="{ erred: errorCount > 0 }">
    <div class="panel-header">
      <span class="label">Compiler</span>
      <span v-if="state === 'loading'" class="status loading">loading WASM…</span>
      <span v-else-if="state === 'error'" class="status err">
        load failed: {{ errorMessage }}
      </span>
      <span v-else-if="errorCount > 0" class="status err">
        {{ errorCount }} error{{ errorCount === 1 ? '' : 's' }}
      </span>
      <span v-else-if="warningCount > 0" class="status warn">
        {{ warningCount }} warning{{ warningCount === 1 ? '' : 's' }}
      </span>
      <span v-else-if="state === 'ready'" class="status ok">clean</span>
    </div>

    <div v-if="grouped.length > 0" class="groups">
      <div v-for="group in grouped" :key="group.phase" class="group">
        <div class="group-label">{{ phaseLabel(group.phase) }}</div>
        <ul class="rows">
          <li
            v-for="(d, i) in group.items"
            :key="`${group.phase}-${i}`"
            class="row"
            :class="[d.severity.toLowerCase(), { clickable: rowHasSpan(d) }]"
            :title="rowHasSpan(d) ? 'Click to focus source' : 'No source location'"
            @click="onClick(d)"
          >
            <span class="dot" :class="d.severity.toLowerCase()" />
            <span class="code">{{ d.code }}</span>
            <span class="msg">{{ d.message }}</span>
            <span v-if="d.help" class="help">{{ d.help }}</span>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
  max-height: 180px;
  overflow-y: auto;
}
.panel.erred { background: rgba(220, 80, 80, 0.04); }

.panel-header {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 14px;
  border-bottom: 1px solid var(--sf-border);
  position: sticky;
  top: 0;
  background: inherit;
  z-index: 1;
}
.label {
  font-size: 0.5625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
}
.status { font-size: 0.625rem; font-family: var(--sf-font-mono); }
.status.loading { color: var(--sf-text-3); }
.status.ok      { color: var(--sf-success); }
.status.warn    { color: var(--sf-warning); }
.status.err     { color: var(--sf-error, #d96666); }

.groups {
  display: flex;
  flex-direction: column;
}
.group { border-top: 1px solid rgba(255, 255, 255, 0.03); }
.group:first-child { border-top: none; }
.group-label {
  font-size: 0.5625rem;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-3);
  padding: 4px 14px 2px;
  background: rgba(255, 255, 255, 0.02);
}
.rows { list-style: none; padding: 0; margin: 0; }
.row {
  display: grid;
  grid-template-columns: 8px 60px 1fr;
  gap: 8px;
  padding: 4px 14px;
  font-size: 0.6875rem;
  font-family: var(--sf-font-mono);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  align-items: baseline;
  cursor: default;
}
.row.clickable { cursor: pointer; }
.row.clickable:hover {
  background: rgba(255, 255, 255, 0.03);
}
.row .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  align-self: center;
}
.dot.error   { background: var(--sf-error, #d96666); }
.dot.warning { background: var(--sf-warning); }
.dot.note    { background: var(--sf-text-3); }
.code {
  font-weight: 600;
  color: var(--sf-text-2);
}
.row.error   .code { color: var(--sf-error, #d96666); }
.row.warning .code { color: var(--sf-warning); }
.msg {
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--sf-text-0);
}
.help {
  display: block;
  margin-top: 2px;
  font-size: 0.625rem;
  color: var(--sf-text-3);
  font-style: italic;
  /* Re-align under the message column, not the dot/code columns. */
  grid-column: 3;
}
</style>
