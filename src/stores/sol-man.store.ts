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
import { validateWorkflow, type Diagnostic } from '@/graph/validate';

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

  // Preview-time validation. We translate the generated spec into a
  // prospective workflow as soon as the LLM responds, run the same
  // diagnostics the live canvas runs, and gate the Apply buttons. This
  // is the guard against silently dropping broken workflows onto the
  // user's canvas.
  const previewDiagnostics = ref<Diagnostic[]>([]);
  const previewWarnings = ref<string[]>([]);

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
    previewDiagnostics.value = [];
    previewWarnings.value = [];
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
    previewDiagnostics.value = [];
    previewWarnings.value = [];

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

    // Run the SAME validation pipeline the live canvas runs against
    // the prospective workflow. If anything's off, we surface it in
    // the preview and gate the Apply buttons. This is the line that
    // keeps a broken graph off the user's canvas.
    runPreviewValidation();

    status.value = 'preview';
    return status.value;
  }

  /**
   * Build the prospective workflow from the current spec and run
   * graph validation against it. Stores diagnostics + repair warnings
   * for the modal to render. Idempotent and cheap — re-runs whenever
   * we want to refresh preview state.
   */
  function runPreviewValidation() {
    previewDiagnostics.value = [];
    previewWarnings.value = [];
    if (!spec.value) return;
    const { workflow, warnings } = specToWorkflow(spec.value);
    previewWarnings.value = warnings;
    previewDiagnostics.value = validateWorkflow(workflow);
  }

  /**
   * Diagnostic codes that NEVER get bypassed via `force: true`. These
   * are categories where letting the workflow through produces SOL
   * that is silently dangerous rather than loudly wrong:
   *
   *   - `missing-input`        — required port has no edge and no
   *                              inline expression. Emitter would insert
   *                              `__UNRESOLVED_INPUT__`; that fails at
   *                              SOL parse, but for `print()` the
   *                              bytecode silently no-ops (T9020).
   *   - `bad-inline-expression`— inline expression failed lint. Either
   *                              the SOL parser rejects it or — worse —
   *                              it's JavaScript that the simulator
   *                              would execute (T9029).
   *
   * Apply-anyway is still allowed for softer errors (e.g.
   * type-mismatch warnings).
   */
  const NON_BYPASSABLE_CODES = new Set([
    'missing-input',
    'bad-inline-expression',
  ]);

  const previewBlockingErrors = computed(() =>
    previewErrors.value.filter((d) => NON_BYPASSABLE_CODES.has(d.code)),
  );
  const hasBlockingErrors = computed(
    () => previewBlockingErrors.value.length > 0,
  );

  /**
   * Replace the user's workflow with the generated spec. Goes through
   * graph.loadWorkflow so undo/redo / autosave / port-rebuild all
   * fire normally.
   *
   * Pass `{ force: true }` to apply even when preview validation found
   * errors — wired to the "Apply draft with errors" override button.
   * `force` does NOT bypass codes in NON_BYPASSABLE_CODES; those are
   * always refused.
   */
  function applyAsNewWorkflow(
    opts: { force?: boolean } = {},
  ): { ok: boolean; warnings: string[]; blocked?: boolean } {
    if (!spec.value) return { ok: false, warnings: [] };
    if (hasBlockingErrors.value) {
      return { ok: false, warnings: previewWarnings.value, blocked: true };
    }
    if (hasErrors.value && !opts.force) {
      return { ok: false, warnings: previewWarnings.value, blocked: true };
    }
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
   *
   * Same `force` semantics as applyAsNewWorkflow — non-bypassable
   * codes are always refused.
   */
  function insertIntoCurrent(
    flowPos: { x: number; y: number },
    opts: { force?: boolean } = {},
  ): {
    ok: boolean;
    warnings: string[];
    newIds: string[];
    blocked?: boolean;
  } {
    if (!spec.value) return { ok: false, warnings: [], newIds: [] };
    if (hasBlockingErrors.value) {
      return {
        ok: false,
        warnings: previewWarnings.value,
        newIds: [],
        blocked: true,
      };
    }
    if (hasErrors.value && !opts.force) {
      return {
        ok: false,
        warnings: previewWarnings.value,
        newIds: [],
        blocked: true,
      };
    }
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
  const previewErrors = computed(() =>
    previewDiagnostics.value.filter((d) => d.severity === 'error'),
  );
  const previewWarningsDiagnostics = computed(() =>
    previewDiagnostics.value.filter((d) => d.severity === 'warning'),
  );
  const hasErrors = computed(() => previewErrors.value.length > 0);
  const hasWarnings = computed(
    () =>
      previewWarningsDiagnostics.value.length > 0 ||
      previewWarnings.value.length > 0,
  );

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
    previewDiagnostics,
    previewWarnings,
    history,
    // derived
    isPreviewing,
    isGenerating,
    isError,
    previewErrors,
    previewWarningsDiagnostics,
    previewBlockingErrors,
    hasErrors,
    hasBlockingErrors,
    hasWarnings,
    // ops
    generate,
    runPreviewValidation,
    applyAsNewWorkflow,
    insertIntoCurrent,
    cancel,
    clearPrompt,
    rememberPrompt,
    reset,
  };
});
