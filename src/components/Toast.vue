<script setup lang="ts">
/**
 * Toast stack — fixed bottom-right of the viewport. Renders the
 * currently-pending toasts from useToastStore. Each toast is a small
 * card with a level-colored left rail, title, optional body, optional
 * action button, and a close button.
 *
 * Stacking order: newest at the bottom. Each toast animates in from
 * the right and out to the right.
 */
import { useToastStore, type Toast } from '@/stores/toast.store';

const toasts = useToastStore();

function runAction(t: Toast) {
  if (t.action) {
    try { t.action.onClick(); } catch { /* swallow — action shouldn't crash the toast */ }
  }
  toasts.dismiss(t.id);
}

function levelLabel(t: Toast): string {
  switch (t.level) {
    case 'info':    return 'Note';
    case 'success': return 'Done';
    case 'warning': return 'Warning';
    case 'error':   return 'Error';
  }
}
</script>

<template>
  <Teleport to="body">
    <div class="toast-stack" aria-live="polite" aria-atomic="false">
      <TransitionGroup name="toast">
        <div
          v-for="t in toasts.toasts"
          :key="t.id"
          class="toast"
          :class="['level-' + t.level]"
          role="status"
        >
          <div class="rail" aria-hidden="true" />
          <div class="content">
            <div class="header">
              <span class="level-label">{{ levelLabel(t) }}</span>
              <button
                type="button"
                class="close"
                :aria-label="'Dismiss ' + levelLabel(t) + ' toast'"
                @click="toasts.dismiss(t.id)"
              >
                <svg viewBox="0 0 12 12" width="9" height="9" fill="none" aria-hidden="true">
                  <path d="M3 3 L9 9 M9 3 L3 9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
              </button>
            </div>
            <div class="title">{{ t.title }}</div>
            <div v-if="t.body" class="body">{{ t.body }}</div>
            <button
              v-if="t.action"
              type="button"
              class="action"
              @click="runAction(t)"
            >{{ t.action.label }}</button>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  right: 16px;
  bottom: 36px; /* clear of the status bar */
  z-index: var(--sf-z-toast, 1100);
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none; /* let the canvas through; toasts re-enable on themselves */
  max-width: min(360px, calc(100vw - 32px));
}
.toast {
  pointer-events: auto;
  display: flex;
  background: var(--sf-bg-1);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-2);
  overflow: hidden;
  min-width: 240px;
}
.rail {
  width: 3px;
  flex-shrink: 0;
}
.level-info .rail    { background: var(--sf-accent); }
.level-success .rail { background: var(--sf-success); }
.level-warning .rail { background: var(--sf-warning); }
.level-error .rail   { background: var(--sf-error); }

.content {
  padding: 8px 10px 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.level-label {
  font-size: 0.5625rem;
  letter-spacing: 0.5px;
  text-transform: uppercase;
  font-weight: 600;
  color: var(--sf-text-3);
}
.level-info .level-label    { color: var(--sf-accent); }
.level-success .level-label { color: var(--sf-success); }
.level-warning .level-label { color: var(--sf-warning); }
.level-error .level-label   { color: var(--sf-error); }

.close {
  background: transparent;
  border: none;
  color: var(--sf-text-3);
  cursor: pointer;
  padding: 2px;
  border-radius: 3px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background 0.12s ease, color 0.12s ease;
}
.close:hover {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
}
.title {
  font-size: 0.75rem;
  color: var(--sf-text-0);
  font-weight: 500;
  line-height: 1.35;
  word-wrap: break-word;
}
.body {
  font-size: 0.6875rem;
  color: var(--sf-text-2);
  line-height: 1.5;
  margin-top: 2px;
  word-wrap: break-word;
}
.action {
  margin-top: 6px;
  align-self: flex-start;
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border: 1px solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  font-size: 0.6875rem;
  padding: 4px 10px;
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease;
}
.action:hover {
  background: var(--sf-accent);
  color: white;
  border-color: var(--sf-accent);
}

/* Slide-in from the right, fade-out in place. */
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(20px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
</style>
