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
      <div class="header-left">
        <span class="title">SOL</span>
        <span class="hint">live preview</span>
      </div>
      <span class="warnings" v-if="graph.emitted.warnings.length > 0">
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
.warnings {
  font-size: 0.625rem;
  color: var(--sf-warning);
  font-family: var(--sf-font-mono);
}
.editor {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
</style>
