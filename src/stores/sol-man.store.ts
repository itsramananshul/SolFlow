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
import type {
  GeneratedGraphSpec,
  GenerateErrorKind,
  GenerateResponseBody,
  GenerateStage,
  ProviderSummary,
} from '@/sol-man/types';
import { useGraphStore } from '@/stores/graph.store';
import { useSolManConfigStore } from '@/stores/sol-man-config.store';
import { useToastStore } from '@/stores/toast.store';
import { validateWorkflow, type Diagnostic } from '@/graph/validate';

export type SolManStatus = 'idle' | 'generating' | 'preview' | 'error';

export const useSolManStore = defineStore('solMan', () => {
  const prompt = ref('');
  const status = ref<SolManStatus>('idle');
  const errorMessage = ref<string | null>(null);
  /** Structured failure classification — surfaced by the modal so
   *  we can render code-specific guidance instead of just dumping
   *  `errorMessage`. Reliability hardening pass. */
  const errorKind = ref<GenerateErrorKind | null>(null);
  const errorStage = ref<GenerateStage | null>(null);
  const errorRetryable = ref<boolean>(false);
  const errorAttempts = ref<number>(0);
  /** Tagged details (provider, model, raw excerpt, repair log)
   *  surfaced via Copy Error Details. Never contains keys. */
  const errorDetails = ref<{
    provider?: string;
    model?: string;
    httpStatus?: number;
    rawExcerpt?: string;
    repairLog?: string[];
  } | null>(null);
  const configMissing = ref(false);
  const availableProviders = ref<ProviderSummary[]>([]);
  const spec = ref<GeneratedGraphSpec | null>(null);
  const lastModel = ref<string | null>(null);
  const lastProvider = ref<{ id: string; name: string } | null>(null);
  /** Did the server's strict-retry path kick in for the current
   *  preview? Surfaced as a soft notice ("recovered after retry"). */
  const lastAttempts = ref<number>(0);
  const lastRepairApplied = ref<boolean>(false);
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
    errorKind.value = null;
    errorStage.value = null;
    errorRetryable.value = false;
    errorAttempts.value = 0;
    errorDetails.value = null;
    configMissing.value = false;
    availableProviders.value = [];
    spec.value = null;
    translationWarnings.value = [];
    previewDiagnostics.value = [];
    previewWarnings.value = [];
    lastModel.value = null;
    lastProvider.value = null;
    lastAttempts.value = 0;
    lastRepairApplied.value = false;
  }

  function clearPrompt() {
    prompt.value = '';
  }

  /**
   * Submit the current prompt. Resolves with the new status so the
   * caller can react synchronously after `await`. Re-running with the
   * same prompt is allowed — overwrites the prior preview.
   *
   * Reliability-hardening pass: on a transient failure (gateway
   * timeout, empty response, invalid JSON, validation failed) we
   * AUTOMATICALLY retry once with the same prompt. The server's
   * strict-retry path covers JSON-shape errors WITHIN one request
   * lifecycle; this layer covers gateway/transport errors BETWEEN
   * requests (the user wouldn't otherwise see a retry happen for a
   * Vercel-edge 504). The retry is silent — the modal shows
   * "Recovered after retry" on success rather than flashing an
   * error state in between.
   *
   * Pass `{ silent: true }` to skip the auto-retry (used by the
   * Retry button so a manual press doesn't compound into 2 calls).
   */
  async function generate(
    opts: { autoRetry?: boolean } = { autoRetry: true },
  ): Promise<SolManStatus> {
    const text = prompt.value.trim();
    if (!text) {
      errorMessage.value = 'Describe the workflow you want first.';
      errorKind.value = 'bad_request';
      errorStage.value = 'request_validation';
      errorRetryable.value = false;
      status.value = 'error';
      return status.value;
    }
    status.value = 'generating';
    errorMessage.value = null;
    errorKind.value = null;
    errorStage.value = null;
    errorRetryable.value = false;
    errorAttempts.value = 0;
    errorDetails.value = null;
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

    let resp = await callSolMan(text, cfg);
    let clientAttempts = 1;

    // Auto-retry ONCE on a retryable failure. Skipped when the
    // caller opted out (manual Retry button — we don't want a button
    // press to become 2 calls). Auth / bad_request / config_missing
    // are NEVER retried regardless of the retryable flag — the
    // server marks them non-retryable but a defensive double-check
    // doesn't hurt.
    if (
      !resp.ok
      && resp.retryable === true
      && opts.autoRetry !== false
      && resp.kind !== 'config_missing'
      && resp.kind !== 'bad_request'
    ) {
      // Tiny backoff to avoid immediately re-hitting a hot gateway.
      await new Promise<void>((r) => setTimeout(r, 400));
      resp = await callSolMan(text, cfg);
      clientAttempts = 2;
    }

    if (!resp.ok) {
      surfaceFailure(resp, clientAttempts);
      return status.value;
    }
    rememberPrompt(text);
    spec.value = resp.spec;
    lastModel.value = resp.model;
    lastProvider.value = resp.provider ?? null;
    lastAttempts.value = (resp.attempts ?? 1) * clientAttempts;
    lastRepairApplied.value = !!resp.repairApplied;

    // Run the SAME validation pipeline the live canvas runs against
    // the prospective workflow. If anything's off, we surface it in
    // the preview and gate the Apply buttons. This is the line that
    // keeps a broken graph off the user's canvas.
    runPreviewValidation();

    status.value = 'preview';
    return status.value;
  }

  function surfaceFailure(
    resp: Extract<GenerateResponseBody, { ok: false }>,
    clientAttempts: number,
  ) {
    errorMessage.value = resp.error;
    errorKind.value = resp.kind ?? 'unknown';
    errorStage.value = resp.stage ?? 'unknown';
    errorRetryable.value = !!resp.retryable;
    errorAttempts.value = (resp.attempts ?? 1) * clientAttempts;
    errorDetails.value = resp.details ?? null;
    configMissing.value = !!resp.configMissing;
    availableProviders.value = resp.availableProviders ?? [];
    status.value = 'error';
  }

  /**
   * Manual retry from the modal's Retry button. Skips the
   * auto-retry (we got HERE because the user clicked the button)
   * so a single press = a single call, with the same prompt
   * preserved.
   */
  function retry(): Promise<SolManStatus> {
    return generate({ autoRetry: false });
  }

  /**
   * Build a copy-pastable diagnostic blob for the Copy details
   * button on the error banner. Includes everything an operator
   * would need to file a bug: provider/model/stage/kind/attempts/
   * raw excerpt / repair log. EXPLICITLY excludes the prompt text
   * (privacy) and any API key (security).
   */
  function buildErrorDetailsBlob(): string {
    const lines = [
      'Sol Man generation failure',
      `kind:      ${errorKind.value ?? 'unknown'}`,
      `stage:     ${errorStage.value ?? 'unknown'}`,
      `retryable: ${errorRetryable.value}`,
      `attempts:  ${errorAttempts.value}`,
      `message:   ${errorMessage.value ?? ''}`,
    ];
    const d = errorDetails.value;
    if (d) {
      if (d.provider) lines.push(`provider:  ${d.provider}`);
      if (d.model) lines.push(`model:     ${d.model}`);
      if (d.httpStatus !== undefined) lines.push(`status:    ${d.httpStatus}`);
      if (d.repairLog && d.repairLog.length > 0) {
        lines.push(`repair:    ${d.repairLog.join(' → ')}`);
      }
      if (d.rawExcerpt) {
        lines.push('raw_excerpt:');
        lines.push('  ' + d.rawExcerpt);
      }
    }
    return lines.join('\n');
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
    // Surface the repair-pass warnings via a toast after the modal
    // closes — without this they vanish along with the modal preview.
    // Users who don't read the preview carefully would otherwise miss
    // important details about what Sol Man had to substitute.
    if (warnings.length > 0) {
      useToastStore().add('warning', `Sol Man made ${warnings.length} ${warnings.length === 1 ? 'adjustment' : 'adjustments'}`, {
        body: warnings[0] + (warnings.length > 1 ? ` (+${warnings.length - 1} more)` : ''),
      });
    } else {
      useToastStore().success('Workflow applied');
    }
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
    if (warnings.length > 0) {
      useToastStore().add('warning', `Sol Man made ${warnings.length} ${warnings.length === 1 ? 'adjustment' : 'adjustments'}`, {
        body: warnings[0] + (warnings.length > 1 ? ` (+${warnings.length - 1} more)` : ''),
      });
    } else {
      useToastStore().success('Inserted into workflow');
    }
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
    errorKind,
    errorStage,
    errorRetryable,
    errorAttempts,
    errorDetails,
    configMissing,
    availableProviders,
    spec,
    lastModel,
    lastProvider,
    lastAttempts,
    lastRepairApplied,
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
    retry,
    buildErrorDetailsBlob,
    runPreviewValidation,
    applyAsNewWorkflow,
    insertIntoCurrent,
    cancel,
    clearPrompt,
    rememberPrompt,
    reset,
  };
});
