/**
 * Sol Man UI state store.
 *
 * Tracks:
 *   - the in-progress prompt the user is typing
 *   - the generation lifecycle (idle / generating / preview / error)
 *   - the most recent GeneratedGraphSpec for preview
 *   - any spec-to-graph translation warnings to surface
 *
 * The actual graph mutations (loadWorkflow / insertBlock) happen
 * through the graph store; this store only owns the conversation
 * with the LLM and the preview state.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { callSolMan } from '@/sol-man/client';
import {
  specToInsertSnapshot,
  specToWorkflow,
} from '@/sol-man/applyGraph';
import type { GeneratedGraphSpec, ProviderSummary } from '@/sol-man/types';
import { useGraphStore } from '@/stores/graph.store';
import { useSolManConfigStore } from '@/stores/sol-man-config.store';

export type SolManStatus = 'idle' | 'generating' | 'preview' | 'error';

export const useSolManStore = defineStore('solMan', () => {
  const prompt = ref('');
  const status = ref<SolManStatus>('idle');
  const errorMessage = ref<string | null>(null);
  const configMissing = ref(false);
  const availableProviders = ref<ProviderSummary[]>([]);
  const spec = ref<GeneratedGraphSpec | null>(null);
  const lastModel = ref<string | null>(null);
  const lastProvider = ref<{ id: string; name: string } | null>(null);
  const translationWarnings = ref<string[]>([]);

  // Recent prompts for quick re-use. Held in memory only — localStorage
  // could come later. Most-recent first; capped at 8.
  const history = ref<string[]>([]);
  function rememberPrompt(p: string) {
    const trimmed = p.trim();
    if (!trimmed) return;
    history.value = [trimmed, ...history.value.filter((x) => x !== trimmed)].slice(0, 8);
  }

  function reset() {
    status.value = 'idle';
    errorMessage.value = null;
    configMissing.value = false;
    availableProviders.value = [];
    spec.value = null;
    translationWarnings.value = [];
    lastModel.value = null;
    lastProvider.value = null;
  }

  function clearPrompt() {
    prompt.value = '';
  }

  /**
   * Submit the current prompt. Resolves with the new status so the
   * caller can react synchronously after `await`. Re-running with the
   * same prompt is allowed — overwrites the prior preview.
   */
  async function generate(): Promise<SolManStatus> {
    const text = prompt.value.trim();
    if (!text) {
      errorMessage.value = 'Describe the workflow you want first.';
      status.value = 'error';
      return status.value;
    }
    status.value = 'generating';
    errorMessage.value = null;
    configMissing.value = false;
    availableProviders.value = [];
    spec.value = null;
    translationWarnings.value = [];

    // BYO-key: forward the user's locally-stored provider config so
    // the server uses it instead of any deployer env vars. When the
    // user has no local config saved, this is null and the server
    // falls back to env vars.
    const cfg = useSolManConfigStore().toRequestConfig();
    const resp = await callSolMan(text, cfg);
    if (!resp.ok) {
      errorMessage.value = resp.error;
      configMissing.value = !!resp.configMissing;
      availableProviders.value = resp.availableProviders ?? [];
      status.value = 'error';
      return status.value;
    }
    rememberPrompt(text);
    spec.value = resp.spec;
    lastModel.value = resp.model;
    lastProvider.value = resp.provider ?? null;
    status.value = 'preview';
    return status.value;
  }

  /**
   * Replace the user's workflow with the generated spec. Goes through
   * graph.loadWorkflow so undo/redo / autosave / port-rebuild all
   * fire normally.
   */
  function applyAsNewWorkflow(): { ok: boolean; warnings: string[] } {
    if (!spec.value) return { ok: false, warnings: [] };
    const graph = useGraphStore();
    const { workflow, warnings } = specToWorkflow(spec.value);
    graph.loadWorkflow(workflow);
    translationWarnings.value = warnings;
    reset();
    return { ok: true, warnings };
  }

  /**
   * Insert the generated spec into the active function as a cluster
   * of nodes (auto-wrapped in a frame when multi-node, like any other
   * block insertion).
   */
  function insertIntoCurrent(flowPos: { x: number; y: number }): {
    ok: boolean;
    warnings: string[];
    newIds: string[];
  } {
    if (!spec.value) return { ok: false, warnings: [], newIds: [] };
    const graph = useGraphStore();
    const ctx = graph.ctx;
    const { snapshot, warnings } = specToInsertSnapshot(spec.value, ctx, flowPos);
    const newIds = graph.insertBlock(snapshot, flowPos);
    translationWarnings.value = warnings;
    reset();
    return { ok: true, warnings, newIds };
  }

  function cancel() {
    reset();
  }

  const isPreviewing = computed(() => status.value === 'preview' && !!spec.value);
  const isGenerating = computed(() => status.value === 'generating');
  const isError = computed(() => status.value === 'error');

  return {
    // state
    prompt,
    status,
    errorMessage,
    configMissing,
    availableProviders,
    spec,
    lastModel,
    lastProvider,
    translationWarnings,
    history,
    // derived
    isPreviewing,
    isGenerating,
    isError,
    // ops
    generate,
    applyAsNewWorkflow,
    insertIntoCurrent,
    cancel,
    clearPrompt,
    rememberPrompt,
    reset,
  };
});
