<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { HighlightStyle, syntaxHighlighting, StreamLanguage } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { useGraphStore } from '@/stores/graph.store';
import { useToastStore } from '@/stores/toast.store';
import { analyzeSource } from '@/compiler/api';
import type { SolDiagnostic } from '@/compiler/types';
import type { ImportReport } from '@/graph/import';
import ImportReportModal from './ImportReportModal.vue';

const graph = useGraphStore();

const editorContainer = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;
const editableCompartment = new Compartment();

/**
 * Edit mode is HONESTLY DETACHED. Phase A has no SOL parser, so any
 * code the user types here cannot be reflected back into the visual
 * graph. We surface that loudly via a banner instead of pretending
 * sync exists. The detached buffer is preserved in component state
 * until the user explicitly resets it.
 *
 * When editing is on, the watcher that mirrors graph.emitted.source
 * into the editor is suppressed — otherwise every graph change would
 * stomp the user's edits.
 */
const isEditing = ref(false);
const editedBuffer = ref<string>('');
function enterEdit() {
  if (!view) return;
  editedBuffer.value = view.state.doc.toString();
  isEditing.value = true;
  view.dispatch({
    effects: editableCompartment.reconfigure(EditorState.readOnly.of(false)),
  });
  view.focus();
}
function exitEdit() {
  isEditing.value = false;
  if (!view) return;
  view.dispatch({
    effects: editableCompartment.reconfigure(EditorState.readOnly.of(true)),
  });
}
function resetToGraph() {
  if (!view) return;
  const src = graph.emitted.source;
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: src },
  });
  editedBuffer.value = src;
}

/** Source the export buttons & clipboard read from. */
function currentSource(): string {
  if (view) return view.state.doc.toString();
  return graph.emitted.source;
}

const isDetached = computed(
  () => isEditing.value && editedBuffer.value !== graph.emitted.source,
);

// ---------- Phase B.5: live compiler diagnostics ----------
//
// When the user is in edit mode we run the REAL SOL compiler
// (compiled to WASM in compiler-wasm/) against their buffer and
// surface its diagnostics inline. Phase A only had graph-derived
// warnings; this is the first time the editor calls canonical
// compiler logic.
//
// Sync flow:
//   - watch `editedBuffer` (only changes while in edit mode)
//   - debounce 250ms (analyzeSource on every keystroke is wasteful
//     on long files — empirically the parser is fast but JSON
//     round-trip dominates)
//   - cancel any in-flight call on new input (epoch check)
//   - never throw out the user's editing; only display diagnostics
type CompilerState = 'idle' | 'loading' | 'ready' | 'error';
const compilerDiagnostics = ref<SolDiagnostic[]>([]);
const compilerState = ref<CompilerState>('idle');
const compilerError = ref<string | null>(null);
let analyzeEpoch = 0;
let debounceHandle: number | null = null;

async function runAnalyzeNow(source: string) {
  const myEpoch = ++analyzeEpoch;
  // First call after WASM load takes longer; mark explicitly so the
  // UI can show "loading compiler…" instead of looking dead.
  if (compilerState.value === 'idle') compilerState.value = 'loading';
  try {
    const result = await analyzeSource(source);
    // Stale-response guard: a later edit's analysis won the race.
    if (myEpoch !== analyzeEpoch) return;
    compilerDiagnostics.value = result.diagnostics;
    compilerState.value = 'ready';
    compilerError.value = null;
  } catch (e) {
    if (myEpoch !== analyzeEpoch) return;
    // WASM failed to load or the bridge threw before catch_unwind
    // could catch (shouldn't happen in practice, but defensive).
    compilerState.value = 'error';
    compilerError.value = e instanceof Error ? e.message : String(e);
    compilerDiagnostics.value = [];
  }
}

function scheduleAnalyze(source: string) {
  if (debounceHandle !== null) window.clearTimeout(debounceHandle);
  debounceHandle = window.setTimeout(() => {
    debounceHandle = null;
    void runAnalyzeNow(source);
  }, 250);
}

watch(editedBuffer, (src) => {
  if (!isEditing.value) return;
  scheduleAnalyze(src);
});

// When the user enters edit mode, kick off an immediate analyze so
// they see compiler diagnostics for the graph-derived source even
// before they touch a key.
watch(isEditing, (editing) => {
  if (editing) {
    void runAnalyzeNow(editedBuffer.value);
  } else {
    // Leaving edit mode: clear diagnostics so the next entry starts
    // fresh and we don't keep stale state around.
    compilerDiagnostics.value = [];
    compilerState.value = 'idle';
    compilerError.value = null;
    if (debounceHandle !== null) {
      window.clearTimeout(debounceHandle);
      debounceHandle = null;
    }
  }
});

const compilerErrorCount = computed(
  () => compilerDiagnostics.value.filter((d) => d.severity === 'Error').length,
);
const compilerWarningCount = computed(
  () => compilerDiagnostics.value.filter((d) => d.severity === 'Warning').length,
);

// ---------- B.7: AST → graph import ----------
//
// "Import to graph" turns the edited SOL into a fresh workflow,
// replacing the current one. We block the action when the compiler
// reports errors — importing a broken AST is guaranteed to produce
// a degraded graph, and the user is better served fixing source first.
//
// Result of a successful import opens the ImportReportModal so the
// user can see what landed as full / partial / source-only /
// unsupported. We don't toast on success — the modal is the
// canonical UX surface for the report.
const importReport = ref<ImportReport | null>(null);
const importApplied = ref(false);
const importInFlight = ref(false);

async function runImport() {
  if (!view) return;
  if (importInFlight.value) return;
  if (compilerErrorCount.value > 0) {
    // Importing a broken AST yields garbage. Surface the gate
    // honestly rather than silently doing something useless.
    useToastStore().warning(
      'Fix compiler errors first',
      'Import to graph parses + walks the AST; a parse error means there is no AST to walk.',
    );
    return;
  }
  const source = view.state.doc.toString();
  importInFlight.value = true;
  try {
    const result = await graph.importFromSource(source);
    importReport.value = result.report;
    importApplied.value = result.ok;
    if (result.ok) {
      // Leave edit mode — the source pane will re-mirror the new
      // graph's emitted source automatically.
      exitEdit();
    }
  } catch (e) {
    useToastStore().error(
      'Import failed',
      e instanceof Error ? e.message : 'Unknown error',
    );
  } finally {
    importInFlight.value = false;
  }
}

function closeImportReport() {
  importReport.value = null;
}

// Minimal SOL StreamLanguage — keyword + literal + comment highlighting.
const solLang = StreamLanguage.define({
  startState: () => ({ inBlock: false }),
  token(stream, state) {
    if (state.inBlock) {
      if (stream.match(/.*?\*\//)) {
        state.inBlock = false;
        return 'comment';
      }
      stream.skipToEnd();
      return 'comment';
    }
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }
    if (stream.match('/*')) {
      state.inBlock = true;
      stream.skipToEnd();
      return 'comment';
    }
    if (stream.match(/"(?:[^"\\]|\\.)*"/)) return 'string';
    if (stream.match(/'(?:[^'\\]|\\.)*'/)) return 'string';
    if (stream.match(/\b(?:function|let|if|else|while|for|in|return|struct|enum|import|as|true|false)\b/)) return 'keyword';
    if (stream.match(/\b(?:int|float|bool|str|char|void)\b/)) return 'type';
    if (stream.match(/\b\d+\.\d+\b/)) return 'number';
    if (stream.match(/\b\d+\b/)) return 'number';
    if (stream.match(/\b[A-Z][A-Za-z0-9_]*\b/)) return 'type-name';
    if (stream.match(/\b[a-z_][a-zA-Z0-9_]*\b/)) return 'identifier';
    stream.next();
    return null;
  },
  tokenTable: {
    'type-name': t.typeName,
    identifier: t.variableName,
  },
});

const solHighlight = HighlightStyle.define([
  { tag: t.keyword, color: '#a07ec3' },
  { tag: t.string, color: '#7ea66a' },
  { tag: t.comment, color: '#555', fontStyle: 'italic' },
  { tag: t.number, color: '#d4a76a' },
  { tag: t.typeName, color: '#5d8acf' },
  { tag: t.variableName, color: '#cfcfcf' },
]);

const baseTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'transparent',
      color: 'var(--sf-text-0)',
      fontFamily: 'var(--sf-font-mono)',
      fontSize: '12px',
    },
    '.cm-content': {
      caretColor: 'var(--sf-accent)',
      padding: '8px 0',
    },
    '.cm-gutters': {
      backgroundColor: 'transparent',
      color: 'var(--sf-text-3)',
      borderRight: '1px solid var(--sf-border)',
      paddingRight: '4px',
    },
    '.cm-lineNumbers .cm-gutterElement': {
      fontFamily: 'var(--sf-font-mono)',
      fontSize: '10px',
    },
    '.cm-activeLine': {
      backgroundColor: 'rgba(255, 255, 255, 0.025)',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'rgba(255, 255, 255, 0.025)',
    },
  },
  { dark: true },
);

onMounted(() => {
  if (!editorContainer.value) return;
  const state = EditorState.create({
    doc: graph.emitted.source,
    extensions: [
      lineNumbers(),
      highlightActiveLine(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      solLang,
      syntaxHighlighting(solHighlight),
      baseTheme,
      editableCompartment.of(EditorState.readOnly.of(true)),
      // Track buffer changes while editing so the "detached vs graph"
      // banner stays accurate without re-rendering the whole view.
      EditorView.updateListener.of((u) => {
        if (!isEditing.value) return;
        if (u.docChanged) editedBuffer.value = u.state.doc.toString();
      }),
    ],
  });
  view = new EditorView({ state, parent: editorContainer.value });

  watch(
    () => graph.emitted.source,
    (newSrc) => {
      if (!view) return;
      // CRITICAL: when the user is editing manually, never overwrite
      // their buffer with the live graph output. They'd lose changes
      // mid-keystroke. Sync is honestly one-way: graph → preview only
      // when NOT editing.
      if (isEditing.value) return;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: newSrc },
      });
    },
  );
});

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});

const copyState = ref<'idle' | 'copied'>('idle');
async function copySource() {
  try {
    await navigator.clipboard.writeText(currentSource());
    copyState.value = 'copied';
    setTimeout(() => (copyState.value = 'idle'), 1200);
  } catch {
    /* clipboard refused */
  }
}

function downloadEdited() {
  const blob = new Blob([currentSource()], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${graph.workflow.meta.name || 'workflow'}.sol`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
</script>

<template>
  <div class="source-preview">
    <div class="header">
      <div class="header-left">
        <span class="title">SOL</span>
        <span v-if="!isEditing" class="hint">live preview</span>
        <span v-else class="hint editing">edit mode · detached</span>
      </div>
      <div class="header-right">
        <span
          v-if="graph.emitted.warnings.length > 0 && !isEditing"
          class="warnings"
        >
          {{ graph.emitted.warnings.length }} warning{{
            graph.emitted.warnings.length === 1 ? '' : 's'
          }}
        </span>
        <button
          v-if="isEditing"
          class="copy-btn"
          @click="resetToGraph"
          title="Discard your edits and restore the graph-derived source"
        >Reset to graph</button>
        <button
          v-if="isEditing"
          class="copy-btn import-btn"
          :disabled="importInFlight || compilerErrorCount > 0"
          :title="
            compilerErrorCount > 0
              ? 'Fix compiler errors before importing'
              : 'Parse this source and replace the current workflow with the imported graph'
          "
          @click="runImport"
        >
          {{ importInFlight ? 'Importing…' : 'Import to graph' }}
        </button>
        <button
          v-if="isEditing"
          class="copy-btn"
          @click="downloadEdited"
        >Download .sol</button>
        <button
          class="copy-btn"
          :class="{ active: isEditing }"
          @click="isEditing ? exitEdit() : enterEdit()"
        >
          {{ isEditing ? 'Done editing' : 'Edit' }}
        </button>
        <button
          class="copy-btn"
          :class="{ copied: copyState === 'copied' }"
          @click="copySource"
        >
          {{ copyState === 'copied' ? '✓ Copied' : 'Copy' }}
        </button>
      </div>
    </div>
    <!--
      Edit-mode banner. Phase B.4 wired up real compiler-backed
      diagnostics (the lexer + parser + analyzer now run in-browser
      via WASM), but the AST → graph importer is still pending, so
      edits don't yet flow back into nodes.
    -->
    <div v-if="isEditing" class="edit-banner" :class="{ dirty: isDetached }">
      <svg viewBox="0 0 16 16" width="11" height="11" fill="none" class="banner-icon">
        <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.3" />
        <path d="M8 5 V8.5 M8 10.5 V11.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
      </svg>
      <span class="banner-text">
        <strong>Editing in detached mode.</strong>
        Compiler diagnostics below are live. Click <em>Import to graph</em>
        to parse this source and replace the visual graph with what
        you typed — partial/unsupported constructs are preserved
        explicitly. Or copy/download/reset without importing.
      </span>
    </div>
    <!--
      Live compiler-diagnostics panel. Only rendered while editing so
      we don't add chrome to the read-only preview path. Empty list
      shows "clean" so the panel never looks broken when the source
      is valid.
    -->
    <div v-if="isEditing" class="compiler-panel" :class="{ erred: compilerErrorCount > 0 }">
      <div class="compiler-panel-header">
        <span class="compiler-label">Compiler</span>
        <span v-if="compilerState === 'loading'" class="compiler-status loading">
          loading WASM…
        </span>
        <span v-else-if="compilerState === 'error'" class="compiler-status err">
          load failed: {{ compilerError }}
        </span>
        <span v-else-if="compilerErrorCount > 0" class="compiler-status err">
          {{ compilerErrorCount }} error{{ compilerErrorCount === 1 ? '' : 's' }}
        </span>
        <span v-else-if="compilerWarningCount > 0" class="compiler-status warn">
          {{ compilerWarningCount }} warning{{ compilerWarningCount === 1 ? '' : 's' }}
        </span>
        <span v-else-if="compilerState === 'ready'" class="compiler-status ok">
          clean
        </span>
      </div>
      <ul v-if="compilerDiagnostics.length > 0" class="compiler-list">
        <li
          v-for="(d, i) in compilerDiagnostics"
          :key="i"
          class="compiler-row"
          :class="d.severity.toLowerCase()"
        >
          <span class="diag-code">{{ d.code }}</span>
          <span class="diag-phase">{{ d.phase }}</span>
          <span class="diag-msg">{{ d.message }}</span>
        </li>
      </ul>
    </div>
    <div ref="editorContainer" class="editor" />
    <ImportReportModal
      v-if="importReport"
      :report="importReport"
      :applied="importApplied"
      @close="closeImportReport"
    />
  </div>
</template>

<style scoped>
.source-preview {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--sf-bg-1);
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.header-left {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.title {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.hint {
  font-size: 0.625rem;
  color: var(--sf-text-3);
}
.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.warnings {
  font-size: 0.625rem;
  color: var(--sf-warning);
  font-family: var(--sf-font-mono);
}
.copy-btn {
  background: transparent;
  border: 1px solid var(--sf-border);
  color: var(--sf-text-1);
  padding: 3px 8px;
  border-radius: 3px;
  font-size: 0.625rem;
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.copy-btn:hover {
  background: var(--sf-bg-2);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.copy-btn.copied {
  background: rgba(0, 204, 136, 0.1);
  border-color: rgba(0, 204, 136, 0.3);
  color: var(--sf-success);
}
.copy-btn.active {
  background: rgba(232, 166, 87, 0.12);
  border-color: rgba(232, 166, 87, 0.32);
  color: var(--sf-cat-trigger);
}
.copy-btn.import-btn {
  background: rgba(98, 154, 220, 0.10);
  border-color: rgba(98, 154, 220, 0.32);
  color: var(--sf-text-0);
}
.copy-btn.import-btn:hover:not(:disabled) {
  background: rgba(98, 154, 220, 0.18);
  border-color: rgba(98, 154, 220, 0.5);
}
.copy-btn.import-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.hint.editing {
  color: var(--sf-cat-trigger);
  font-family: var(--sf-font-mono);
  letter-spacing: 0.4px;
  text-transform: uppercase;
  font-size: 0.5625rem;
}
.edit-banner {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 14px;
  background: rgba(232, 166, 87, 0.08);
  border-bottom: 1px solid rgba(232, 166, 87, 0.22);
  color: var(--sf-text-1);
  font-size: 0.6875rem;
  line-height: 1.45;
}
.edit-banner.dirty {
  background: rgba(232, 166, 87, 0.14);
  border-bottom-color: rgba(232, 166, 87, 0.36);
}
.banner-icon {
  color: var(--sf-cat-trigger);
  flex-shrink: 0;
  margin-top: 2px;
}
.banner-text strong {
  color: var(--sf-text-0);
  font-weight: 600;
}
.editor {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

/* ---------- Live compiler diagnostics (B.5) ---------- */
.compiler-panel {
  display: flex;
  flex-direction: column;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
  max-height: 140px;
  overflow-y: auto;
}
.compiler-panel.erred {
  background: rgba(220, 80, 80, 0.04);
}
.compiler-panel-header {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 14px;
  border-bottom: 1px solid var(--sf-border);
}
.compiler-label {
  font-size: 0.5625rem;
  font-weight: 600;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  color: var(--sf-text-2);
}
.compiler-status {
  font-size: 0.625rem;
  font-family: var(--sf-font-mono);
}
.compiler-status.loading { color: var(--sf-text-3); }
.compiler-status.ok      { color: var(--sf-success); }
.compiler-status.warn    { color: var(--sf-warning); }
.compiler-status.err     { color: var(--sf-error, #d96666); }
.compiler-list {
  list-style: none;
  padding: 0;
  margin: 0;
}
.compiler-row {
  display: grid;
  grid-template-columns: 56px 70px 1fr;
  gap: 8px;
  padding: 4px 14px;
  font-size: 0.6875rem;
  font-family: var(--sf-font-mono);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  align-items: baseline;
}
.compiler-row.error    { color: var(--sf-text-0); }
.compiler-row.warning  { color: var(--sf-text-1); }
.compiler-row.note     { color: var(--sf-text-2); }
.diag-code {
  font-weight: 600;
  color: var(--sf-text-2);
}
.compiler-row.error   .diag-code { color: var(--sf-error, #d96666); }
.compiler-row.warning .diag-code { color: var(--sf-warning); }
.diag-phase {
  color: var(--sf-text-3);
  text-transform: lowercase;
  font-size: 0.625rem;
}
.diag-msg {
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
