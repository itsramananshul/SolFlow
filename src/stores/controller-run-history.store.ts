/**
 * Lightweight client-side run-history index (Phase C C.2 c64).
 *
 * Tracks the most recent N controller runs the editor has
 * submitted, keyed by controller URL. Persisted to localStorage
 * so reloading the editor keeps history visible without a
 * round-trip.
 *
 * Why client-side?
 *   The controller's `GET /workflows/:id/runs` is the source of
 *   truth and survives controller restarts (the SQLite DB does),
 *   but the editor doesn't keep a stable workflow-id mapping yet
 *   — every "Re-run" mints a new workflow (C.2 doesn't dedupe by
 *   content hash). So the editor remembers "I just submitted
 *   these runs in this session"; reopening fetches the latest
 *   state from the controller. C.7+ adds proper workflow naming
 *   and this layer either goes away or becomes a cache index.
 *
 * Bound at 20 entries per controller URL (FIFO eviction). Older
 * entries stay queryable on the controller via list_runs even
 * after they age out of this index.
 */
import { ref } from 'vue';
import { defineStore } from 'pinia';
import type { RunStatus } from '@/runtime-host/types';

const STORAGE_KEY = 'solflow.controller.run_history';
const MAX_PER_URL = 20;

export interface ControllerRunHistoryEntry {
  /** Controller base URL the run was submitted to (for keying). */
  controllerUrl: string;
  workflowId: string;
  runId: string;
  /** Editor-side display name (e.g. `editor:start`). */
  workflowName: string;
  status: RunStatus;
  /** Final wall-clock duration in ms, when known. */
  durationMs: number | null;
  /** ms since epoch when the editor submitted the run. */
  submittedAt: number;
}

type Index = Record<string, ControllerRunHistoryEntry[]>;

export const useControllerRunHistoryStore = defineStore('controllerRunHistory', () => {
  const index = ref<Index>(load());

  function listFor(controllerUrl: string): ControllerRunHistoryEntry[] {
    return index.value[controllerUrl] ?? [];
  }

  function record(entry: ControllerRunHistoryEntry): void {
    const key = entry.controllerUrl;
    const existing = index.value[key] ?? [];
    // De-dupe on runId — if we already saw this run, replace
    // (latest status / duration wins). Otherwise prepend.
    const filtered = existing.filter((e) => e.runId !== entry.runId);
    const next = [entry, ...filtered].slice(0, MAX_PER_URL);
    index.value = { ...index.value, [key]: next };
    save(index.value);
  }

  /** Patch an existing entry's status / duration (e.g. after re-poll). */
  function update(
    controllerUrl: string,
    runId: string,
    patch: Partial<Pick<ControllerRunHistoryEntry, 'status' | 'durationMs'>>,
  ): void {
    const existing = index.value[controllerUrl];
    if (!existing) return;
    const next = existing.map((e) =>
      e.runId === runId ? { ...e, ...patch } : e,
    );
    index.value = { ...index.value, [controllerUrl]: next };
    save(index.value);
  }

  function clearFor(controllerUrl: string): void {
    if (!index.value[controllerUrl]) return;
    const next = { ...index.value };
    delete next[controllerUrl];
    index.value = next;
    save(index.value);
  }

  return { index, listFor, record, update, clearFor };
});

function load(): Index {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object') return parsed as Index;
    return {};
  } catch {
    return {};
  }
}

function save(value: Index): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // ignore quota / disabled storage
  }
}
