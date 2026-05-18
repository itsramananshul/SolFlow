<script setup lang="ts">
/**
 * Floating play/pause/step controls. Appears at the bottom-center of
 * the canvas while a trace is loaded. Auto-disappears 4s after a
 * successful run completes; stays put on failure until the user
 * resets so they can read the failure state.
 */
import { computed } from 'vue';
import { useSimulationStore } from '@/stores/simulation.store';

const sim = useSimulationStore();

const visible = computed(() => sim.hasTrace);
const atEnd = computed(() => sim.stepIndex >= sim.totalSteps);
const progressPct = computed(() => {
  if (sim.totalSteps === 0) return 0;
  return Math.round((sim.stepIndex / sim.totalSteps) * 100);
});

function onTogglePlay() {
  if (atEnd.value) {
    sim.restart();
    return;
  }
  if (sim.isPaused) {
    sim.resume();
  } else if (sim.isPlaying) {
    sim.pause();
  } else {
    // Trace loaded but not playing — happens after stepOnce reached the end.
    sim.restart();
  }
}

function onStep() {
  sim.stepOnce();
}

function onReset() {
  sim.restart();
}

function onDismiss() {
  sim.cancel();
}
</script>

<template>
  <Transition name="exec-fade">
    <div v-if="visible" class="exec-controls">
      <button
        type="button"
        class="ctl-btn primary"
        :title="atEnd ? 'Restart from the beginning' : sim.isPaused || !sim.isPlaying ? 'Resume' : 'Pause'"
        :aria-label="atEnd ? 'Restart simulation' : sim.isPaused || !sim.isPlaying ? 'Resume simulation' : 'Pause simulation'"
        @click="onTogglePlay"
      >
        <span v-if="atEnd">↻</span>
        <span v-else-if="sim.isPaused || !sim.isPlaying">▶</span>
        <span v-else>⏸</span>
      </button>
      <button
        type="button"
        class="ctl-btn"
        :disabled="atEnd"
        title="Step one event"
        aria-label="Step one event"
        @click="onStep"
      >
        ⏵|
      </button>
      <button
        type="button"
        class="ctl-btn"
        title="Restart from the beginning"
        aria-label="Restart simulation"
        @click="onReset"
      >
        ↻
      </button>

      <div class="progress">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: progressPct + '%' }" />
        </div>
        <span class="progress-count">
          {{ sim.stepIndex }} / {{ sim.totalSteps }}
        </span>
      </div>

      <button
        type="button"
        class="ctl-btn close"
        title="Dismiss / cancel simulation"
        aria-label="Dismiss simulation"
        @click="onDismiss"
      >
        ✕
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.exec-controls {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  bottom: 24px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  background: rgba(17, 17, 17, 0.92);
  border: 1px solid var(--sf-border-strong);
  border-radius: 999px;
  box-shadow: var(--sf-shadow-3);
  backdrop-filter: blur(8px);
  z-index: var(--sf-z-popover);
  font-size: 0.6875rem;
  color: var(--sf-text-1);
  min-width: 320px;
}
.exec-fade-enter-active,
.exec-fade-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.exec-fade-enter-from,
.exec-fade-leave-to {
  opacity: 0;
  transform: translate(-50%, 6px);
}
.ctl-btn {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid var(--sf-border);
  border-radius: 50%;
  color: var(--sf-text-1);
  cursor: pointer;
  padding: 0;
  font-size: 0.75rem;
  font-family: var(--sf-font-mono);
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.ctl-btn:hover:not(:disabled) {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
  border-color: var(--sf-border-strong);
}
.ctl-btn:disabled {
  opacity: 0.32;
  cursor: not-allowed;
}
.ctl-btn.primary {
  background: var(--sf-accent);
  border-color: var(--sf-accent);
  color: white;
  width: 32px;
  height: 32px;
  font-size: 0.875rem;
}
.ctl-btn.primary:hover {
  background: var(--sf-accent-hover);
  border-color: var(--sf-accent-hover);
}
.ctl-btn.close {
  border-color: transparent;
  font-size: 0.625rem;
  color: var(--sf-text-3);
}
.ctl-btn.close:hover {
  background: rgba(255, 77, 79, 0.12);
  color: var(--sf-error);
  border-color: rgba(255, 77, 79, 0.3);
}
.progress {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 120px;
  padding: 0 4px;
}
.progress-bar {
  flex: 1;
  height: 3px;
  background: var(--sf-bg-3);
  border-radius: 2px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: var(--sf-accent);
  transition: width 0.12s ease;
}
.progress-count {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-3);
  letter-spacing: 0.3px;
  flex-shrink: 0;
  min-width: 56px;
  text-align: right;
}
</style>
