/**
 * SolFlow Phase A — toast notification store.
 *
 * Single global surface for ephemeral non-blocking notifications.
 * Replaces native `alert()` calls (which feel cheap and vary across
 * browsers) and gives a place for silent operations — dropped edges,
 * Sol Man repair-pass warnings, autosave failures — to surface
 * visibly without forcing a modal.
 *
 * Toasts auto-dismiss after `durationMs` (default 4000) unless their
 * level is `error`, which stay until explicitly dismissed so the user
 * can read them. Maximum 4 toasts on screen at once; older ones fade
 * out as new ones arrive.
 */

import { defineStore } from 'pinia';
import { ref } from 'vue';

export type ToastLevel = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: number;
  level: ToastLevel;
  title: string;
  /** Optional second-line body. Keep short — one sentence at most. */
  body?: string;
  /** ms before auto-dismiss. `error` defaults to never (0). */
  durationMs: number;
  /** Optional action button. Click runs the callback then dismisses. */
  action?: { label: string; onClick: () => void };
}

const MAX_TOASTS = 4;
let nextId = 1;

export const useToastStore = defineStore('toast', () => {
  const toasts = ref<Toast[]>([]);

  function add(
    level: ToastLevel,
    title: string,
    options?: {
      body?: string;
      durationMs?: number;
      action?: { label: string; onClick: () => void };
    },
  ): number {
    const defaultDuration = level === 'error' ? 0 : 4000;
    const t: Toast = {
      id: nextId++,
      level,
      title,
      body: options?.body,
      durationMs: options?.durationMs ?? defaultDuration,
      action: options?.action,
    };
    toasts.value = [...toasts.value, t].slice(-MAX_TOASTS);
    if (t.durationMs > 0) {
      window.setTimeout(() => dismiss(t.id), t.durationMs);
    }
    return t.id;
  }

  function dismiss(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }

  function clear() {
    toasts.value = [];
  }

  // Convenience helpers — call these from any store/component instead
  // of `useToastStore().add('info', ...)`.
  function info(title: string, body?: string): number {
    return add('info', title, body ? { body } : undefined);
  }
  function success(title: string, body?: string): number {
    return add('success', title, body ? { body } : undefined);
  }
  function warning(title: string, body?: string): number {
    return add('warning', title, body ? { body } : undefined);
  }
  function error(title: string, body?: string): number {
    return add('error', title, body ? { body } : undefined);
  }

  return { toasts, add, dismiss, clear, info, success, warning, error };
});
