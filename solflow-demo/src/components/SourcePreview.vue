<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { defaultKeymap } from '@codemirror/commands';
import { HighlightStyle, syntaxHighlighting, StreamLanguage } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { useGraphStore } from '@/stores/graph.store';

const graph = useGraphStore();

const editorContainer = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;
const editableCompartment = new Compartment();

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
  { tag: t.keyword, color: '#c678dd' },
  { tag: t.string, color: '#98c379' },
  { tag: t.comment, color: '#5c6370', fontStyle: 'italic' },
  { tag: t.number, color: '#d19a66' },
  { tag: t.typeName, color: '#e5c07b' },
  { tag: t.variableName, color: '#abb2bf' },
]);

const baseTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--sf-bg-1)',
      color: 'var(--sf-text-0)',
      fontFamily: 'var(--sf-font-mono)',
      fontSize: '12px',
    },
    '.cm-content': {
      caretColor: 'var(--sf-accent)',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--sf-bg-0)',
      color: 'var(--sf-text-3)',
      borderRight: '1px solid var(--sf-border)',
    },
    '.cm-activeLine': {
      backgroundColor: 'var(--sf-bg-2)',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'var(--sf-bg-2)',
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
      keymap.of(defaultKeymap),
      solLang,
      syntaxHighlighting(solHighlight),
      baseTheme,
      editableCompartment.of(EditorState.readOnly.of(true)),
    ],
  });
  view = new EditorView({ state, parent: editorContainer.value });

  watch(
    () => graph.emitted.source,
    (newSrc) => {
      if (!view) return;
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
</script>

<template>
  <div class="source-preview">
    <div class="header">
      <span class="title">SOL Source (live preview)</span>
      <span class="muted" v-if="graph.emitted.warnings.length > 0">
        {{ graph.emitted.warnings.length }} warning{{
          graph.emitted.warnings.length === 1 ? '' : 's'
        }}
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
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--sf-bg-0);
  border-bottom: 1px solid var(--sf-border);
}
.title {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: var(--sf-text-1);
}
.editor {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
</style>
