/**
 * Shared state for the floating Trace (debug) panel.
 *
 * The Run modal computes the execution-trace rows for a run and pushes them
 * here; the standalone `TracePanel` window renders them. Kept separate so
 * the Trace panel can float independently of the Run panel and stay open
 * while the user works the canvas.
 */
import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface TraceRowVM {
  index: number;
  kind: 'stmt' | 'call' | 'return' | 'extcall' | 'extresult' | 'error';
  fn: string;
  depth: number;
  line: number | null;
  snippet: string;
  /** Canvas node id this step maps to, if any. */
  nodeId: string | null;
  fnName: string | null;
}

export type TraceDock = 'float' | 'right';

export const useTracePanelStore = defineStore('tracePanel', () => {
  const rows = ref<TraceRowVM[]>([]);
  const steps = ref(0);
  const truncated = ref(false);
  const open = ref(false);
  // Default to a right-docked sidebar so the trace never spawns on top of
  // the Run panel's controls; the user can switch it to Floating.
  const dock = ref<TraceDock>('right');

  /** Replace the trace with a fresh run's rows. Opens the panel when there
   *  are rows; closes it (kept) when a run produced none. */
  function setTrace(next: TraceRowVM[], meta: { steps: number; truncated: boolean }) {
    rows.value = next;
    steps.value = meta.steps;
    truncated.value = meta.truncated;
    if (next.length > 0) open.value = true;
  }

  function clear() {
    rows.value = [];
    steps.value = 0;
    truncated.value = false;
  }

  function openPanel() { if (rows.value.length > 0) open.value = true; }
  function close() { open.value = false; }
  function setDock(d: TraceDock) { dock.value = d; }

  return { rows, steps, truncated, open, dock, setTrace, clear, openPanel, close, setDock };
});
