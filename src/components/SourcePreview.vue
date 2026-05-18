<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { HighlightStyle, syntaxHighlighting, StreamLanguage } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { useGraphStore } from '@/stores/graph.store';

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
      Detached-edit banner. Surfaced honestly: this Phase-A build has
      no SOL → graph parser, so anything typed here lives only in the
      editor buffer. Phase B (WASM) will close this loop.
    -->
    <div v-if="isEditing" class="edit-banner" :class="{ dirty: isDetached }">
      <svg viewBox="0 0 16 16" width="11" height="11" fill="none" class="banner-icon">
        <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.3" />
        <path d="M8 5 V8.5 M8 10.5 V11.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
      </svg>
      <span class="banner-text">
        <strong>Code editing is detached from the visual graph.</strong>
        Your changes here don't sync back to nodes — graph sync needs
        the Phase B WASM parser. Copy or download your edited SOL, or
        reset to graph output.
      </span>
    </div>
    <div ref="editorContainer" class="editor" />
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
</style>
