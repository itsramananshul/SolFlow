/**
 * SolFlow Phase A — simulation playback store.
 *
 * Animates an execution trace produced by recordTrace() over the canvas.
 * The interpreter ran synchronously and finished already; this store
 * handles the visual playback (highlighting the currently-executing
 * node, marking visited nodes, pulsing active edges) plus the new
 * debug-mode controls (pause / step / resume) and runtime-value
 * tracking pulled from the trace.
 *
 * Auto-clears state ~4 seconds after playback ends so the canvas
 * returns to its default look. Pausing keeps the state frozen
 * indefinitely so users can inspect.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { StepEvent, Trace } from '@/runtime/simulate';

const STEP_MS = 180; // pace per event during playback

export const useSimulationStore = defineStore('simulation', () => {
  // Currently-glowing node (the one mid-execution this tick).
  const runningNodeId = ref<string | null>(null);
  // Nodes that have entered & exited successfully this run.
  const visitedNodeIds = ref<Set<string>>(new Set());
  // Node whose execution errored.
  const failedNodeIds = ref<Set<string>>(new Set());
  // Currently-pulsing edges (cleared on next event).
  const activeEdgeIds = ref<Set<string>>(new Set());

  /**
   * Sticky per-node runtime summary. Survives between events so the
   * user sees what each visited node "produced" while scanning the
   * graph after a run.
   */
  const valueByNodeId = ref<Map<string, string>>(new Map());
  /**
   * For nodes with multiple control-outs (branch / while / forEach),
   * the path that was actually taken on the most recent visit. Used
   * by SolNode to dim the not-taken arm visually.
   */
  const takenPathByNodeId = ref<Map<string, string>>(new Map());
  /** Most recent error message per node id, if any. */
  const errorByNodeId = ref<Map<string, string>>(new Map());

  // Playback state
  const isPlaying = ref(false);
  const isPaused = ref(false);
  const totalSteps = ref(0);
  const stepIndex = ref(0);
  const loadedTrace = ref<Trace | null>(null);
  const speed = ref(1);

  let playTimer: number | undefined;
  let clearTimer: number | undefined;

  const isFinished = computed(
    () => !isPlaying.value && (visitedNodeIds.value.size > 0 || failedNodeIds.value.size > 0),
  );

  /** True when a trace is loaded — even if paused. */
  const hasTrace = computed(() => loadedTrace.value !== null);

  function reset() {
    if (playTimer !== undefined) {
      window.clearTimeout(playTimer);
      playTimer = undefined;
    }
    if (clearTimer !== undefined) {
      window.clearTimeout(clearTimer);
      clearTimer = undefined;
    }
    runningNodeId.value = null;
    visitedNodeIds.value = new Set();
    failedNodeIds.value = new Set();
    activeEdgeIds.value = new Set();
    valueByNodeId.value = new Map();
    takenPathByNodeId.value = new Map();
    errorByNodeId.value = new Map();
    isPlaying.value = false;
    isPaused.value = false;
    totalSteps.value = 0;
    stepIndex.value = 0;
    loadedTrace.value = null;
  }

  function play(trace: Trace, opts?: { speed?: number }) {
    reset();
    if (trace.events.length === 0) return;
    loadedTrace.value = trace;
    totalSteps.value = trace.events.length;
    speed.value = opts?.speed ?? 1;
    isPlaying.value = true;
    isPaused.value = false;
    scheduleNext();
  }

  /**
   * Schedule the next event after STEP_MS / speed. No-op if paused or
   * past the end. Each scheduled tick consumes ONE event so pausing
   * mid-flight stops cleanly without dropping or replaying anything.
   */
  function scheduleNext() {
    if (!loadedTrace.value) return;
    if (isPaused.value) return;
    if (stepIndex.value >= totalSteps.value) {
      finish();
      return;
    }
    const stepMs = STEP_MS / speed.value;
    playTimer = window.setTimeout(() => {
      playTimer = undefined;
      applyEvent(loadedTrace.value!.events[stepIndex.value]);
      stepIndex.value++;
      if (stepIndex.value >= totalSteps.value) {
        finish();
      } else {
        scheduleNext();
      }
    }, stepMs);
  }

  function pause() {
    if (!isPlaying.value || isPaused.value) return;
    isPaused.value = true;
    if (playTimer !== undefined) {
      window.clearTimeout(playTimer);
      playTimer = undefined;
    }
  }

  function resume() {
    if (!isPlaying.value || !isPaused.value) return;
    isPaused.value = false;
    scheduleNext();
  }

  /**
   * Manually advance one event. Works whether currently paused or
   * playing; if playing, the auto-advance is paused first so step
   * granularity is deterministic. Use to walk through a workflow
   * one statement at a time.
   */
  function stepOnce() {
    if (!loadedTrace.value) return;
    pause();
    if (stepIndex.value >= totalSteps.value) return;
    applyEvent(loadedTrace.value.events[stepIndex.value]);
    stepIndex.value++;
    if (stepIndex.value >= totalSteps.value) {
      // Stay paused at the end so the user can inspect; auto-fade
      // doesn't trigger until they cancel or resume.
      isPlaying.value = false;
    }
  }

  /** Restart playback from the beginning of the loaded trace. */
  function restart() {
    const trace = loadedTrace.value;
    if (!trace) return;
    const previousSpeed = speed.value;
    play(trace, { speed: previousSpeed });
  }

  function applyEvent(ev: StepEvent) {
    switch (ev.type) {
      case 'enter':
        runningNodeId.value = ev.id;
        break;
      case 'exit':
        if (runningNodeId.value === ev.id) runningNodeId.value = null;
        visitedNodeIds.value = new Set([...visitedNodeIds.value, ev.id]);
        break;
      case 'edge':
        // Pulse one edge at a time — clear after a brief moment.
        activeEdgeIds.value = new Set([...activeEdgeIds.value, ev.id]);
        window.setTimeout(() => {
          const next = new Set(activeEdgeIds.value);
          next.delete(ev.id);
          activeEdgeIds.value = next;
        }, STEP_MS * 1.6);
        break;
      case 'error':
        failedNodeIds.value = new Set([...failedNodeIds.value, ev.id]);
        errorByNodeId.value = new Map(errorByNodeId.value).set(ev.id, ev.message);
        runningNodeId.value = null;
        break;
      case 'value':
        valueByNodeId.value = new Map(valueByNodeId.value).set(ev.id, ev.summary);
        if (ev.takenPath !== undefined) {
          takenPathByNodeId.value = new Map(takenPathByNodeId.value).set(ev.id, ev.takenPath);
        }
        break;
    }
  }

  function finish() {
    runningNodeId.value = null;
    isPlaying.value = false;
    isPaused.value = false;
    activeEdgeIds.value = new Set();
    // Hold the visited state on screen briefly so the user can scan
    // the executed path, then fade out — UNLESS the run failed, in
    // which case keep the failure visible until the user dismisses.
    if (failedNodeIds.value.size === 0) {
      clearTimer = window.setTimeout(() => {
        reset();
      }, 4000);
    }
  }

  function cancel() {
    reset();
  }

  function getNodeStatus(
    id: string,
  ): 'running' | 'visited' | 'failed' | 'idle' {
    if (failedNodeIds.value.has(id)) return 'failed';
    if (runningNodeId.value === id) return 'running';
    if (visitedNodeIds.value.has(id)) return 'visited';
    return 'idle';
  }

  function isEdgeActive(id: string): boolean {
    return activeEdgeIds.value.has(id);
  }

  function getValueFor(id: string): string | undefined {
    return valueByNodeId.value.get(id);
  }

  function getTakenPath(id: string): string | undefined {
    return takenPathByNodeId.value.get(id);
  }

  function getErrorFor(id: string): string | undefined {
    return errorByNodeId.value.get(id);
  }

  return {
    // state
    runningNodeId,
    visitedNodeIds,
    failedNodeIds,
    activeEdgeIds,
    valueByNodeId,
    takenPathByNodeId,
    errorByNodeId,
    isPlaying,
    isPaused,
    isFinished,
    hasTrace,
    totalSteps,
    stepIndex,
    speed,
    loadedTrace,
    // ops
    play,
    pause,
    resume,
    stepOnce,
    restart,
    cancel,
    reset,
    getNodeStatus,
    isEdgeActive,
    getValueFor,
    getTakenPath,
    getErrorFor,
  };
});
