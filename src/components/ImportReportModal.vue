<script setup lang="ts">
/**
 * Modal shown after an AST → graph import completes. Renders the
 * honest classification breakdown the importer produced:
 *   - top-level counts (structs, enums, imports, ext-fns)
 *   - per-function support tier + degraded statement count
 *   - every notice grouped by severity
 *
 * The user already sees the graph itself (the import committed
 * before this modal opens). This is the "what just happened" panel
 * so they understand what's full vs partial vs unsupported.
 */
import { computed, onBeforeUnmount, onMounted } from 'vue';
import type { ImportReport, ImportSupport } from '@/graph/import';

const props = defineProps<{
  report: ImportReport;
  /** True iff the parser produced a usable AST (and the workflow
   *  was loaded). False means parse failed and the report shows
   *  compiler diagnostics instead. */
  applied: boolean;
}>();
const emit = defineEmits<{
  (e: 'close'): void;
  /** User clicked a function summary row that has a known source
   *  line. Parent (SourcePreview) re-enters edit mode (if needed)
   *  and scrolls the editor to that line. */
  (e: 'focusSourceLine', line: number): void;
}>();

const headlineLabel = computed(() => (props.applied ? 'Imported' : 'Import failed'));
const headlineTone = computed(() => {
  if (!props.applied) return 'err';
  if (props.report.counts.unsupported > 0) return 'warn';
  if (props.report.counts.sourceOnly > 0) return 'warn';
  if (props.report.counts.partial > 0) return 'info';
  return 'ok';
});

function supportLabel(s: ImportSupport): string {
  if (s === 'full') return 'Full';
  if (s === 'partial') return 'Partial';
  if (s === 'source-only') return 'Source-only';
  return 'Unsupported';
}
function supportTone(s: ImportSupport): string {
  if (s === 'full') return 'ok';
  if (s === 'partial') return 'info';
  return 'warn';
}

// Prod c51 — Escape-key close, matching other modals.
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close');
}
onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => document.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="modal">
      <div class="header">
        <span class="title">{{ headlineLabel }}</span>
        <span class="badge" :class="headlineTone">
          <template v-if="applied">
            {{ report.counts.full + report.counts.partial }} statement{{
              report.counts.full + report.counts.partial === 1 ? '' : 's'
            }}
            graphed
          </template>
          <template v-else>parse error — workflow unchanged</template>
        </span>
        <button class="close" @click="emit('close')" aria-label="Close">✕</button>
      </div>

      <div class="body">
        <!-- Top-level summary chips -->
        <div v-if="applied" class="chips">
          <span class="chip">
            <strong>{{ report.functions.length }}</strong>
            function{{ report.functions.length === 1 ? '' : 's' }}
          </span>
          <span v-if="report.topLevel.structs" class="chip">
            <strong>{{ report.topLevel.structs }}</strong>
            struct{{ report.topLevel.structs === 1 ? '' : 's' }}
          </span>
          <span v-if="report.topLevel.enums" class="chip">
            <strong>{{ report.topLevel.enums }}</strong>
            enum{{ report.topLevel.enums === 1 ? '' : 's' }}
          </span>
          <span v-if="report.topLevel.imports" class="chip">
            <strong>{{ report.topLevel.imports }}</strong>
            import{{ report.topLevel.imports === 1 ? '' : 's' }}
          </span>
          <span v-if="report.topLevel.extFunctions" class="chip warn">
            <strong>{{ report.topLevel.extFunctions }}</strong>
            ext fn{{ report.topLevel.extFunctions === 1 ? '' : 's' }}
            (source-only)
          </span>
        </div>

        <!-- Per-function table -->
        <div v-if="applied && report.functions.length > 0" class="section">
          <h3>Functions</h3>
          <table class="fn-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Support</th>
                <th class="num">Statements</th>
                <th class="num">Degraded</th>
                <th class="num">Source</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="fn in report.functions"
                :key="fn.name"
                :class="{ clickable: fn.sourceLine !== undefined }"
                :title="
                  fn.sourceLine !== undefined
                    ? `Open source at line ${fn.sourceLine}`
                    : 'No source line attached'
                "
                @click="fn.sourceLine !== undefined && emit('focusSourceLine', fn.sourceLine)"
              >
                <td class="mono">{{ fn.name }}</td>
                <td>
                  <span class="pill" :class="supportTone(fn.support)">
                    {{ supportLabel(fn.support) }}
                  </span>
                </td>
                <td class="num mono">{{ fn.statementCount }}</td>
                <td class="num mono" :class="{ warn: fn.unsupportedCount > 0 }">
                  {{ fn.unsupportedCount }}
                </td>
                <td class="num mono src-col">
                  <span v-if="fn.sourceLine !== undefined" class="src-link">
                    line {{ fn.sourceLine }} →
                  </span>
                  <span v-else class="src-na">—</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Notice list -->
        <div v-if="report.notices.length > 0" class="section">
          <h3>
            Notices
            <span class="hint">({{ report.notices.length }})</span>
          </h3>
          <ul class="notices">
            <li
              v-for="(n, i) in report.notices"
              :key="i"
              class="notice"
              :class="n.severity"
            >
              <span class="dot" :class="n.severity" />
              <span class="msg">
                <strong v-if="n.functionName" class="fn-ref">{{ n.functionName }}:</strong>
                {{ n.message }}
              </span>
            </li>
          </ul>
        </div>

        <!-- Empty state -->
        <div
          v-if="applied && report.notices.length === 0 && report.counts.partial === 0"
          class="empty"
        >
          Every imported statement has full graph representation.
        </div>
      </div>

      <div class="footer">
        <button class="ghost" @click="emit('close')">Done</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--sf-z-modal, 1000);
}
.modal {
  background: var(--sf-bg-0);
  border: 1px solid var(--sf-border);
  border-radius: 6px;
  width: min(640px, 92vw);
  max-height: 84vh;
  display: flex;
  flex-direction: column;
  color: var(--sf-text-0);
}
.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 18px;
  border-bottom: 1px solid var(--sf-border);
}
.title {
  font-size: 0.875rem;
  font-weight: 600;
}
.badge {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
  padding: 2px 8px;
  border-radius: 3px;
}
.badge.ok    { background: rgba(0, 204, 136, 0.12); color: var(--sf-success); }
.badge.info  { background: rgba(98, 154, 220, 0.12); color: var(--sf-text-1); }
.badge.warn  { background: rgba(232, 166, 87, 0.16); color: var(--sf-warning); }
.badge.err   { background: rgba(217, 102, 102, 0.16); color: var(--sf-error, #d96666); }
.close {
  margin-left: auto;
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  font-size: 0.875rem;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 3px;
}
.close:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
.body {
  padding: 16px 18px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 18px;
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.chip {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  padding: 4px 10px;
  border: 1px solid var(--sf-border);
  border-radius: 12px;
  font-size: 0.625rem;
  color: var(--sf-text-2);
}
.chip.warn { border-color: rgba(232, 166, 87, 0.4); color: var(--sf-warning); }
.chip strong { color: var(--sf-text-0); font-family: var(--sf-font-mono); }
.section h3 {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
  margin: 0 0 8px 0;
}
.section h3 .hint { color: var(--sf-text-3); font-weight: 400; margin-left: 6px; }
.fn-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.6875rem;
}
.fn-table thead th {
  text-align: left;
  font-weight: 600;
  color: var(--sf-text-3);
  padding: 6px 8px;
  border-bottom: 1px solid var(--sf-border);
}
.fn-table tbody td {
  padding: 6px 8px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}
.fn-table tbody tr.clickable { cursor: pointer; }
.fn-table tbody tr.clickable:hover {
  background: rgba(98, 154, 220, 0.06);
}
.fn-table .num { text-align: right; }
.fn-table .num.warn { color: var(--sf-warning); }
.src-col { text-align: right; min-width: 80px; }
.src-link {
  color: var(--sf-text-1);
  text-decoration: none;
}
tr.clickable:hover .src-link {
  color: var(--sf-text-0);
  text-decoration: underline;
}
.src-na { color: var(--sf-text-3); }
.mono { font-family: var(--sf-font-mono); }
.pill {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
}
.pill.ok   { background: rgba(0, 204, 136, 0.12); color: var(--sf-success); }
.pill.info { background: rgba(98, 154, 220, 0.12); color: var(--sf-text-1); }
.pill.warn { background: rgba(232, 166, 87, 0.16); color: var(--sf-warning); }
.notices {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.notice {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  font-size: 0.6875rem;
  line-height: 1.45;
}
.notice .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  margin-top: 6px;
  flex-shrink: 0;
}
.notice .dot.info    { background: var(--sf-text-3); }
.notice .dot.warning { background: var(--sf-warning); }
.fn-ref {
  font-family: var(--sf-font-mono);
  color: var(--sf-text-2);
  margin-right: 4px;
}
.empty {
  font-size: 0.6875rem;
  color: var(--sf-text-3);
  text-align: center;
  padding: 12px;
}
.footer {
  border-top: 1px solid var(--sf-border);
  padding: 10px 18px;
  display: flex;
  justify-content: flex-end;
}
.ghost {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 5px 14px;
  border-radius: 4px;
  font-size: 0.6875rem;
  cursor: pointer;
}
.ghost:hover { background: var(--sf-bg-2); color: var(--sf-text-0); }
</style>
