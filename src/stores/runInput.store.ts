/**
 * Test-payload state for manual runs.
 *
 * When a workflow reads event data (`payload`) from a trigger/webhook,
 * a manual run needs that data provided. This store holds the editable
 * JSON the user supplies in the Run panel and the sample default to reset
 * to. Browser Simulation injects it as `payload`; the Local Controller
 * sends it as the run's `inputs` (also bound to `payload`).
 */
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

/** Free names that, when referenced by a workflow, indicate it expects
 *  event/test data from a trigger or webhook. */
const PAYLOAD_NAMES = ['payload', 'event', 'input', 'request', 'body'];

/** True when the emitted SOL references a payload-like free name. */
export function usesPayload(source: string): boolean {
  if (!source) return false;
  // Word-boundary match so `payloadX` / `myinput` don't false-positive.
  return PAYLOAD_NAMES.some((n) => new RegExp(`\\b${n}\\b`).test(source));
}

export interface PayloadValidity {
  ok: boolean;
  /** Parse error message when not ok. */
  message: string;
}

export const useRunInputStore = defineStore('runInput', () => {
  /** The current editable payload JSON text. */
  const payloadText = ref<string>('');
  /** The sample-provided default, used by "Reset to sample payload". */
  const samplePayload = ref<string>('');

  /** Seed from a sample's default payload (called on sample load). */
  function loadSamplePayload(text: string | undefined) {
    samplePayload.value = text ?? '';
    payloadText.value = text ?? '';
  }

  /** Reset the editor back to the sample's default payload. */
  function resetToSample() {
    payloadText.value = samplePayload.value;
  }

  function setPayload(text: string) {
    payloadText.value = text;
  }

  const hasSamplePayload = computed(() => samplePayload.value.trim().length > 0);

  /** JSON validity of the current payload text. Empty is treated as
   *  "no payload" (valid; the run simply binds nothing). */
  const validity = computed<PayloadValidity>(() => {
    const t = payloadText.value.trim();
    if (t === '') return { ok: true, message: '' };
    try {
      JSON.parse(t);
      return { ok: true, message: '' };
    } catch (e) {
      return { ok: false, message: e instanceof Error ? e.message : 'Invalid JSON' };
    }
  });

  /** The payload JSON to inject, or '' when empty/invalid. */
  const injectable = computed(() => {
    const t = payloadText.value.trim();
    return t !== '' && validity.value.ok ? t : '';
  });

  return {
    payloadText,
    samplePayload,
    hasSamplePayload,
    validity,
    injectable,
    loadSamplePayload,
    resetToSample,
    setPayload,
  };
});
