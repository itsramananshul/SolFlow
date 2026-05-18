<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const modKey = ref<'⌘' | 'Ctrl'>('Ctrl');
onMounted(() => {
  if (typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)) {
    modKey.value = '⌘';
  }
  document.addEventListener('keydown', onKey);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
});

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close');
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}
</script>

<template>
  <Transition name="fade">
    <div v-if="open" class="backdrop" @click="onBackdrop">
      <div class="modal" @click.stop>
        <header class="modal-header">
          <span class="title">Keyboard shortcuts</span>
          <button class="ghost" @click="$emit('close')" title="Close (Esc)">
            <svg viewBox="0 0 12 12" width="11" height="11" fill="none">
              <path d="M3 3 9 9 M9 3 3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        </header>
        <div class="body">
          <section>
            <div class="section-title">Editor</div>
            <div class="row"><span>Save workflow JSON</span><kbd>{{ modKey }}</kbd><kbd>S</kbd></div>
            <div class="row"><span>Export .sol</span><kbd>{{ modKey }}</kbd><kbd>E</kbd></div>
            <div class="row"><span>Run workflow</span><kbd>{{ modKey }}</kbd><kbd>↵</kbd></div>
            <div class="row"><span>Open Sol Man (AI)</span><kbd>{{ modKey }}</kbd><kbd>J</kbd></div>
            <div class="row"><span>Toggle presentation mode</span><kbd>P</kbd></div>
            <div class="row"><span>Undo</span><kbd>{{ modKey }}</kbd><kbd>Z</kbd></div>
            <div class="row"><span>Redo</span><kbd>{{ modKey }}</kbd><kbd>⇧</kbd><kbd>Z</kbd></div>
            <div class="row"><span>Close modal / drawer / deselect</span><kbd>Esc</kbd></div>
            <div class="row"><span>Show this help</span><kbd>?</kbd></div>
          </section>
          <section>
            <div class="section-title">Navigation</div>
            <div class="row"><span>Search nodes in this function</span><kbd>{{ modKey }}</kbd><kbd>F</kbd></div>
            <div class="row"><span>Fit selection (or whole graph)</span><kbd>1</kbd></div>
            <div class="row"><span>Fit whole graph</span><kbd>Home</kbd></div>
            <div class="row"><span>Reset zoom to 100%</span><kbd>{{ modKey }}</kbd><kbd>0</kbd></div>
            <div class="row"><span>Zoom in</span><kbd>{{ modKey }}</kbd><kbd>=</kbd></div>
            <div class="row"><span>Zoom out</span><kbd>{{ modKey }}</kbd><kbd>-</kbd></div>
            <div class="row"><span>Select all nodes</span><kbd>{{ modKey }}</kbd><kbd>A</kbd></div>
          </section>
          <section>
            <div class="section-title">Entry points</div>
            <div class="row">
              <span><strong>Start</strong> = classic function entry (manual run). <strong>Trigger</strong> = event-driven entry (webhook / timer / event / HTTP).</span>
            </div>
            <div class="row">
              <span>A function can have either, both, or multiple Triggers — but only one Start.</span>
            </div>
            <div class="row">
              <span>Drag Start from the palette's <code>Entry</code> section to add it back after deleting. Adding it twice selects the existing one.</span>
            </div>
            <div class="row">
              <span>Edit a trigger's sample payload, then press <strong>Trigger Event ▷</strong> in the Inspector to simulate an inbound event.</span>
            </div>
            <div class="row">
              <span>You can't delete the last entry — that would leave the function with no way to start.</span>
            </div>
          </section>
          <section>
            <div class="section-title">Canvas</div>
            <div class="row"><span>Quick-add node at cursor</span><kbd>Space</kbd></div>
            <div class="row"><span>Quick-add (alt)</span><kbd>{{ modKey }}</kbd><kbd>K</kbd></div>
            <div class="row"><span>Quick-add at click</span><kbd>Double-click</kbd></div>
            <div class="row"><span>Add node + auto-connect</span><kbd>Drag edge to empty</kbd></div>
            <div class="row"><span>Duplicate selection (keeps wiring)</span><kbd>{{ modKey }}</kbd><kbd>D</kbd></div>
            <div class="row"><span>Copy selection</span><kbd>{{ modKey }}</kbd><kbd>C</kbd></div>
            <div class="row"><span>Paste at cursor</span><kbd>{{ modKey }}</kbd><kbd>V</kbd></div>
            <div class="row"><span>Drag node from palette</span><kbd>Click + Drag</kbd></div>
            <div class="row"><span>Multi-select</span><kbd>⇧</kbd><kbd>Click</kbd></div>
            <div class="row"><span>Marquee select</span><kbd>⇧</kbd><kbd>Drag</kbd></div>
            <div class="row"><span>Pan canvas</span><kbd>Drag empty canvas</kbd></div>
            <div class="row"><span>Delete selection (nodes / edges)</span><kbd>Del</kbd></div>
            <div class="row"><span>Right-click for actions</span><kbd>R-Click</kbd></div>
            <div class="row"><span>Drop workflow JSON</span><kbd>Drag from desktop</kbd></div>
          </section>
          <section>
            <div class="section-title">Reusable blocks</div>
            <div class="row">
              <span>Drag built-in patterns or your saved blocks from the <code>Blocks</code> sidebar tab. Multi-node blocks auto-wrap in a Frame named after the block.</span>
            </div>
            <div class="row">
              <span>Save a marquee selection as a reusable block via right-click → <strong>Save N nodes as reusable block…</strong>.</span>
            </div>
            <div class="row">
              <span>Quick Add (Space / {{ modKey }}K) searches blocks + nodes together. Block entries land at the top when the query is empty.</span>
            </div>
          </section>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: scale(0.985);
}
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  z-index: var(--sf-z-modal-top);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}
.modal {
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-lg);
  box-shadow: var(--sf-shadow-3);
  width: min(640px, 100%);
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--sf-border);
  background: var(--sf-bg-0);
}
.title {
  font-size: 0.8125rem;
  font-weight: 600;
}
.body {
  flex: 1;
  overflow: auto;
  padding: 16px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}
@media (max-width: 600px) {
  .body {
    grid-template-columns: 1fr;
  }
}
.section-title {
  font-size: 0.5625rem;
  font-weight: 600;
  letter-spacing: 0.8px;
  text-transform: uppercase;
  color: var(--sf-text-3);
  margin-bottom: 10px;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 5px 0;
  font-size: 0.75rem;
  color: var(--sf-text-1);
  gap: 8px;
  border-bottom: 1px solid var(--sf-border);
}
.row:last-child {
  border-bottom: none;
}
.row span {
  flex: 1;
}
kbd {
  display: inline-block;
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  padding: 2px 6px;
  background: var(--sf-bg-3);
  border: 1px solid var(--sf-border-strong);
  border-radius: 3px;
  color: var(--sf-text-0);
  margin-left: 4px;
}
</style>
