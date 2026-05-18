/**
 * User-supplied Sol Man provider configuration.
 *
 * Each user enters their own API key in the SolFlow UI. The key
 * lives in localStorage on the user's browser; SolFlow's API
 * forwards it to the chosen LLM provider on each request and never
 * persists or logs it server-side.
 *
 * Server-side env vars remain a valid fallback for self-hosted
 * deployments where the deployer wants to provide a shared key (so
 * end-users don't need their own). When both are present, the
 * user's browser-stored config wins.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

const STORAGE_KEY = 'solflow.solMan.config';

export interface SolManConfigPayload {
  providerId: string;
  apiKey: string;
  /** Optional model override (uses provider's default if blank). */
  model?: string;
  /** Required for openai-compatible; otherwise ignored. */
  baseUrl?: string;
}

interface StoredConfig extends SolManConfigPayload {
  /** When the config was last saved — surfaced in the settings UI. */
  savedAt?: string;
}

export const useSolManConfigStore = defineStore('solManConfig', () => {
  const providerId = ref('');
  const apiKey = ref('');
  const model = ref('');
  const baseUrl = ref('');
  const savedAt = ref<string | null>(null);

  function bootstrap() {
    if (typeof localStorage === 'undefined') return;
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    try {
      const c = JSON.parse(raw) as StoredConfig;
      providerId.value = c.providerId ?? '';
      apiKey.value = c.apiKey ?? '';
      model.value = c.model ?? '';
      baseUrl.value = c.baseUrl ?? '';
      savedAt.value = c.savedAt ?? null;
    } catch {
      /* corrupted; leave defaults */
    }
  }

  function save() {
    if (typeof localStorage === 'undefined') return;
    const now = new Date().toISOString();
    const payload: StoredConfig = {
      providerId: providerId.value,
      apiKey: apiKey.value,
      model: model.value || undefined,
      baseUrl: baseUrl.value || undefined,
      savedAt: now,
    };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
    savedAt.value = now;
  }

  function clear() {
    providerId.value = '';
    apiKey.value = '';
    model.value = '';
    baseUrl.value = '';
    savedAt.value = null;
    if (typeof localStorage !== 'undefined') {
      localStorage.removeItem(STORAGE_KEY);
    }
  }

  const isConfigured = computed(
    () => providerId.value.trim() !== '' && apiKey.value.trim() !== '',
  );

  function toRequestConfig(): SolManConfigPayload | null {
    if (!isConfigured.value) return null;
    return {
      providerId: providerId.value,
      apiKey: apiKey.value,
      model: model.value || undefined,
      baseUrl: baseUrl.value || undefined,
    };
  }

  /** Masked key for display ("sk-…abcd"). Never expose the full key. */
  function maskedKey(): string {
    const k = apiKey.value;
    if (k.length < 8) return k ? '••••' : '';
    return `${k.slice(0, 3)}…${k.slice(-4)}`;
  }

  return {
    providerId,
    apiKey,
    model,
    baseUrl,
    savedAt,
    isConfigured,
    bootstrap,
    save,
    clear,
    toRequestConfig,
    maskedKey,
  };
});
