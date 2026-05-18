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
            <div class="row"><span>Undo</span><kbd>{{ modKey }}</kbd><kbd>Z</kbd></div>
            <div class="row"><span>Redo</span><kbd>{{ modKey }}</kbd><kbd>⇧</kbd><kbd>Z</kbd></div>
            <div class="row"><span>Close modal / drawer / deselect</span><kbd>Esc</kbd></div>
            <div class="row"><span>Show this help</span><kbd>?</kbd></div>
          </section>
          <section>
            <div class="section-title">Triggers &amp; entry points</div>
            <div class="row">
              <span>A function always has one entry: a <code>Start</code> (manual) or a <code>Trigger</code> (event-driven). Adding the first Trigger to an empty function removes the placeholder Start automatically.</span>
            </div>
            <div class="row">
              <span>Trigger kinds: Manual / Webhook / Timer / Event / HTTP. Configure them in the Inspector.</span>
            </div>
            <div class="row">
              <span>Edit a trigger's sample payload, then press <strong>Trigger Event ▷</strong> in the Inspector to simulate an inbound event.</span>
            </div>
            <div class="row">
              <span>You can delete Start once a Trigger exists (or vice versa). You can't delete the last entry — that would orphan the function.</span>
            </div>
          </section>
          <section>
            <div class="section-title">Canvas</div>
            <div class="row"><span>Quick-add node at cursor</span><kbd>Space</kbd></div>
            <div class="row"><span>Quick-add (alt)</span><kbd>{{ modKey }}</kbd><kbd>K</kbd></div>
            <div class="row"><span>Quick-add at click</span><kbd>Double-click</kbd></div>
            <div class="row"><span>Add node + auto-connect</span><kbd>Drag edge to empty</kbd></div>
            <div class="row"><span>Duplicate selection</span><kbd>{{ modKey }}</kbd><kbd>D</kbd></div>
            <div class="row"><span>Copy selection</span><kbd>{{ modKey }}</kbd><kbd>C</kbd></div>
            <div class="row"><span>Paste at cursor</span><kbd>{{ modKey }}</kbd><kbd>V</kbd></div>
            <div class="row"><span>Drag node from palette</span><kbd>Click + Drag</kbd></div>
            <div class="row"><span>Multi-select</span><kbd>⇧</kbd><kbd>Click</kbd></div>
            <div class="row"><span>Marquee select</span><kbd>⇧</kbd><kbd>Drag</kbd></div>
            <div class="row"><span>Pan canvas</span><kbd>Drag empty canvas</kbd></div>
            <div class="row"><span>Delete selection</span><kbd>Del</kbd></div>
            <div class="row"><span>Right-click for actions</span><kbd>R-Click</kbd></div>
            <div class="row"><span>Drop workflow JSON</span><kbd>Drag from desktop</kbd></div>
          </section>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.12s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
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
