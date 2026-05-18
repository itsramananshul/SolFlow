/**
 * SolFlow Phase A — simulation playback store.
 *
 * Animates an execution trace produced by recordTrace() over the canvas.
 * The interpreter ran synchronously and finished already; this store
 * only handles the visual playback (highlighting the currently-executing
 * node, marking visited nodes, pulsing active edges).
 *
 * Auto-clears state 4 seconds after playback ends so the canvas returns
 * to its default look. Esc / new Run / new selection cancel playback.
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

  const isPlaying = ref(false);
  const totalSteps = ref(0);
  const stepIndex = ref(0);

  let playTimer: number | undefined;
  let clearTimer: number | undefined;

  const isFinished = computed(
    () => !isPlaying.value && (visitedNodeIds.value.size > 0 || failedNodeIds.value.size > 0),
  );

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
    isPlaying.value = false;
    totalSteps.value = 0;
    stepIndex.value = 0;
  }

  function play(trace: Trace, opts?: { speed?: number }) {
    reset();
    const events = trace.events;
    if (events.length === 0) return;
    const stepMs = STEP_MS / (opts?.speed ?? 1);

    isPlaying.value = true;
    totalSteps.value = events.length;

    const step = (i: number) => {
      stepIndex.value = i;
      if (i >= events.length) {
        finish();
        return;
      }
      applyEvent(events[i]);
      playTimer = window.setTimeout(() => step(i + 1), stepMs);
    };

    step(0);
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
        // schedule clear for this specific edge
        window.setTimeout(() => {
          const next = new Set(activeEdgeIds.value);
          next.delete(ev.id);
          activeEdgeIds.value = next;
        }, STEP_MS * 1.6);
        break;
      case 'error':
        failedNodeIds.value = new Set([...failedNodeIds.value, ev.id]);
        runningNodeId.value = null;
        break;
    }
  }

  function finish() {
    runningNodeId.value = null;
    isPlaying.value = false;
    activeEdgeIds.value = new Set();
    // Hold the visited state on screen briefly so the user can scan
    // the executed path, then fade out.
    clearTimer = window.setTimeout(() => {
      reset();
    }, 4000);
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

  return {
    // state
    runningNodeId,
    visitedNodeIds,
    failedNodeIds,
    activeEdgeIds,
    isPlaying,
    isFinished,
    totalSteps,
    stepIndex,
    // ops
    play,
    cancel,
    reset,
    getNodeStatus,
    isEdgeActive,
  };
});
