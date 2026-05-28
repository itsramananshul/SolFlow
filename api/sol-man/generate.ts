/**
 * POST /api/sol-man/generate
 *
 * Provider-agnostic LLM call with reliability hardening:
 *   - JSON repair layer for fenced / prose-wrapped / trailing-comma
 *     / truncated responses.
 *   - One strict-retry on JSON-parse failure with a tighter prompt
 *     ("Your previous response was invalid JSON. Respond with ONLY
 *      the JSON object").
 *   - Per-provider AbortSignal timeout so we fail before Vercel's
 *     edge cuts us with an HTML 504.
 *   - Structured error envelope with `kind` / `stage` / `attempts`
 *     / `retryable` / `details` so the client can render specific
 *     guidance and decide whether to auto-retry.
 *   - Server-side log lines tagged for grep-ability.
 *
 * Honesty contract (unchanged):
 *   - No provider configured → 503 { configMissing }
 *   - Malformed LLM output  → 502 with structured kind/stage
 *   - Upstream API error    → 502 with provider's own message
 *   - No fake responses, no fallback templates, no demo mode
 *
 * Environment variables: see _providers.ts for the full list.
 * New in this pass:
 *   SOL_MAN_PROVIDER_TIMEOUT_MS  per-call timeout (default 25_000)
 *   SOL_MAN_STRICT_RETRIES       integer; 1 means one extra try on
 *                                 invalid-JSON / validation failure
 *                                 (default 1, max 2)
 */

import type { VercelRequest, VercelResponse } from '@vercel/node';
import type {
  GenerateErrorKind,
  GenerateRequestBody,
  GenerateResponseBody,
  GenerateStage,
} from '../../src/sol-man/types';
// .js extensions required: package.json has "type": "module" so Vercel
// ships these as ESM, and Node's ESM resolver rejects extensionless
// relative imports at runtime (ERR_MODULE_NOT_FOUND).
import { SYSTEM_PROMPT, strictRetryUserPromptPreamble } from './_prompt.js';
import { SpecValidationError, validateSpec } from './_validate.js';
import { providerSummaries, resolveProvider } from './_providers.js';
import { repairJson, type RepairResult } from './_jsonRepair.js';

const MAX_PROMPT_LEN = 4_000;

function strictRetryCount(): number {
  const raw = process.env.SOL_MAN_STRICT_RETRIES;
  if (!raw) return 1;
  const n = Number(raw);
  if (!Number.isFinite(n) || n < 0) return 1;
  return Math.min(2, Math.floor(n));
}

export default async function handler(req: VercelRequest, res: VercelResponse) {
  const startedAt = Date.now();

  if (req.method !== 'POST') {
    return send(res, 405, makeFailure({
      error: 'method not allowed — POST required',
      kind: 'bad_request',
      stage: 'request_validation',
    }));
  }

  const body = req.body as GenerateRequestBody | undefined;
  const prompt = (body?.prompt ?? '').toString().trim();
  if (prompt.length === 0) {
    return send(res, 400, makeFailure({
      error: 'prompt is empty — describe the workflow you want',
      kind: 'bad_request',
      stage: 'request_validation',
    }));
  }
  if (prompt.length > MAX_PROMPT_LEN) {
    return send(res, 400, makeFailure({
      error: `prompt is too long (${prompt.length} chars; max ${MAX_PROMPT_LEN})`,
      kind: 'bad_request',
      stage: 'request_validation',
    }));
  }

  // Pull optional inline (BYO-key) config from the request body so
  // the user's browser-stored provider settings take precedence over
  // server env vars. Never echo the key back; never log it.
  const inline = body?.config;
  const resolved = resolveProvider(inline ?? null);
  if (!resolved) {
    return send(res, 503, {
      ok: false,
      error:
        'No LLM provider is configured. Open Sol Man settings (gear icon) and enter an API key from any supported provider — or set one of the provider env vars on this deployment.',
      kind: 'config_missing',
      stage: 'provider_resolution',
      retryable: false,
      configMissing: true,
      availableProviders: providerSummaries(),
    });
  }
  if (!resolved.model) {
    return send(res, 400, makeFailure({
      error: `${resolved.provider.name} needs a model name. Open Sol Man settings and set a Model value (or override the default via SOL_MAN_MODEL on the server).`,
      kind: 'bad_request',
      stage: 'provider_resolution',
      details: { provider: resolved.provider.id },
    }));
  }

  const maxRetries = strictRetryCount();
  const repairLog: string[] = [];
  let attempts = 0;
  let lastFailure: GenerateResponseBody | null = null;
  let activeModel = resolved.model;

  // Single retry loop: first attempt is the normal call; subsequent
  // attempts use the strict-retry preamble + add the prior failure
  // reason into the user prompt so the model self-corrects.
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    attempts++;
    const userPrompt =
      attempt === 0
        ? prompt
        : strictRetryUserPromptPreamble(lastFailureReason(lastFailure)) +
          '\n\nOriginal request:\n' +
          prompt;

    let llmResult;
    const callStartedAt = Date.now();
    try {
      llmResult = await resolved.provider.call({
        systemPrompt: SYSTEM_PROMPT,
        userPrompt,
        apiKey: resolved.apiKey,
        model: resolved.model,
        baseUrl: resolved.baseUrl,
      });
    } catch (e) {
      const message = (e as Error).message;
      const isTimeout = /timed? ?out/i.test(message);
      logProviderCall({
        provider: resolved.provider.id,
        model: activeModel,
        attempt: attempt + 1,
        durationMs: Date.now() - callStartedAt,
        ok: false,
        errorKind: isTimeout ? 'gateway_timeout' : 'provider_error',
        errorMessage: message,
      });
      lastFailure = makeFailure({
        error: `${resolved.provider.name}: ${message}`,
        kind: isTimeout ? 'gateway_timeout' : 'provider_error',
        stage: 'provider_call',
        retryable: isTimeout,
        details: { provider: resolved.provider.id, model: activeModel },
      });
      // Don't burn extra retries on auth / 4xx; those are not
      // transient. Only retry on gateway timeouts.
      if (!isTimeout) break;
      continue;
    }

    if (llmResult.actualModel) {
      activeModel = llmResult.actualModel;
    }
    const text = llmResult.text;
    logProviderCall({
      provider: resolved.provider.id,
      model: activeModel,
      attempt: attempt + 1,
      durationMs: Date.now() - callStartedAt,
      ok: true,
      bytes: text?.length ?? 0,
    });
    if (!text || text.length === 0) {
      lastFailure = makeFailure({
        error: `${resolved.provider.name} returned an empty response`,
        kind: 'empty_response',
        stage: 'provider_call',
        retryable: true,
        details: { provider: resolved.provider.id, model: activeModel },
      });
      continue;
    }

    // JSON extraction + repair. We always run repairJson rather than
    // a bare JSON.parse so partial / prose-wrapped responses recover
    // cleanly without an extra round trip.
    const repaired = repairJson(text);
    if (!repaired.ok) {
      repairLog.push(...repaired.log);
      lastFailure = makeFailure({
        error: `${resolved.provider.name} returned text that didn't parse as JSON even after repair: ${repaired.error}`,
        kind: 'invalid_json',
        stage: 'json_extraction',
        retryable: true,
        details: {
          provider: resolved.provider.id,
          model: activeModel,
          rawExcerpt: repaired.rawExcerpt,
          repairLog: repaired.log,
        },
      });
      continue;
    }

    // Validate schema. Validation failure isn't always model-
    // dependent — sometimes the model emits a correctly-shaped
    // object that violates a SOL constraint — but a strict retry
    // with the validator message often fixes it.
    let spec;
    try {
      spec = validateSpec(repaired.value);
    } catch (e) {
      if (e instanceof SpecValidationError) {
        lastFailure = makeFailure({
          error: `${resolved.provider.name} output failed validation: ${e.message}`,
          kind: 'validation_failed',
          stage: 'spec_validation',
          retryable: true,
          details: {
            provider: resolved.provider.id,
            model: activeModel,
            repairLog: (repaired as RepairResult).log,
          },
        });
        continue;
      }
      // Unexpected — surface but don't retry; this is a bug in our
      // validator, not the model.
      logUnexpectedError(e);
      return send(res, 500, makeFailure({
        error: `Unexpected validation error: ${(e as Error).message}`,
        kind: 'unknown',
        stage: 'spec_validation',
        retryable: false,
      }));
    }

    // Success path.
    logSuccess({
      provider: resolved.provider.id,
      model: activeModel,
      attempts,
      repairApplied: repaired.modified,
      totalMs: Date.now() - startedAt,
    });
    return send(res, 200, {
      ok: true,
      spec,
      model: activeModel,
      provider: { id: resolved.provider.id, name: resolved.provider.name },
      usage: llmResult.usage,
      attempts,
      repairApplied: repaired.modified,
    });
  }

  // Exhausted retries. Surface the most recent failure with a stable
  // 502 — except for gateway_timeout which we surface as 504 so the
  // client's classifier triggers its retry-with-backoff path.
  const finalFailure: GenerateResponseBody =
    lastFailure ?? makeFailure({
      error: 'Sol Man failed for an unknown reason',
      kind: 'unknown',
      stage: 'unknown',
    });
  // Augment with the final attempt count + provider/model details.
  const enriched: GenerateResponseBody = {
    ...finalFailure,
    attempts,
  };
  const status =
    enriched.ok === false && enriched.kind === 'gateway_timeout' ? 504 : 502;
  logFailureExit({
    provider: resolved.provider.id,
    model: activeModel,
    attempts,
    finalKind: (enriched.ok === false && enriched.kind) || 'unknown',
    totalMs: Date.now() - startedAt,
  });
  return send(res, status, enriched);
}

// =============================================================
//  Helpers
// =============================================================

function send(
  res: VercelResponse,
  status: number,
  body: GenerateResponseBody,
): void {
  res.status(status).json(body);
}

interface FailureSeed {
  error: string;
  kind: GenerateErrorKind;
  stage: GenerateStage;
  retryable?: boolean;
  details?: {
    provider?: string;
    model?: string;
    httpStatus?: number;
    rawExcerpt?: string;
    repairLog?: string[];
  };
}
function makeFailure(seed: FailureSeed): GenerateResponseBody {
  return {
    ok: false,
    error: seed.error,
    kind: seed.kind,
    stage: seed.stage,
    retryable: seed.retryable ?? false,
    details: seed.details,
  };
}

function lastFailureReason(prev: GenerateResponseBody | null): string {
  if (!prev || prev.ok) return 'unspecified';
  const stage = prev.stage ?? 'unknown';
  return `${stage}: ${prev.error}`;
}

// =============================================================
//  Logging — structured server-side info. NEVER logs the prompt
//  body, API keys, baseUrl, or raw model response. Field set is
//  intentionally minimal so grep stays cheap.
// =============================================================

function logProviderCall(info: {
  provider: string;
  model: string;
  attempt: number;
  durationMs: number;
  ok: boolean;
  bytes?: number;
  errorKind?: GenerateErrorKind;
  errorMessage?: string;
}): void {
  if (info.ok) {
    console.info(
      `[sol-man] provider_call provider=${info.provider} model=${info.model} attempt=${info.attempt} duration_ms=${info.durationMs} ok=true bytes=${info.bytes ?? 0}`,
    );
  } else {
    // Truncate error message so we don't dump multi-line provider
    // bodies into the log.
    const truncated = (info.errorMessage ?? '').slice(0, 200);
    console.warn(
      `[sol-man] provider_call provider=${info.provider} model=${info.model} attempt=${info.attempt} duration_ms=${info.durationMs} ok=false kind=${info.errorKind ?? 'unknown'} err="${truncated}"`,
    );
  }
}

function logSuccess(info: {
  provider: string;
  model: string;
  attempts: number;
  repairApplied: boolean;
  totalMs: number;
}): void {
  console.info(
    `[sol-man] result ok=true provider=${info.provider} model=${info.model} attempts=${info.attempts} repair_applied=${info.repairApplied} total_ms=${info.totalMs}`,
  );
}

function logFailureExit(info: {
  provider: string;
  model: string;
  attempts: number;
  finalKind: string;
  totalMs: number;
}): void {
  console.warn(
    `[sol-man] result ok=false provider=${info.provider} model=${info.model} attempts=${info.attempts} kind=${info.finalKind} total_ms=${info.totalMs}`,
  );
}

function logUnexpectedError(e: unknown): void {
  console.error(
    `[sol-man] unexpected_error message="${(e as Error)?.message ?? String(e)}"`,
  );
}
