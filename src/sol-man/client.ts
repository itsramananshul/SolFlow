/**
 * Sol Man API client.
 *
 * Reliability hardening pass (post "non-JSON response (HTTP 504)"):
 *
 *   - Adds a client-side AbortController so we fail at a known time
 *     (default 45s) instead of letting the fetch hang or letting
 *     Vercel cut us with its own HTML 504 page.
 *   - Classifies non-2xx responses by Content-Type. HTML responses
 *     (the typical shape of platform-edge timeouts) get a
 *     `kind: 'gateway_timeout'` envelope so the modal can render
 *     a real message + a retry button instead of "non-JSON response
 *     (HTTP 504)".
 *   - Surfaces `kind` / `stage` / `attempts` / `retryable` /
 *     `details` from the server so the modal can route on cause.
 *   - Never echoes the user prompt or any provider key back through
 *     the error message.
 *
 * Keep the signature stable: callers still receive a
 * GenerateResponseBody and switch on `.ok`. The new fields are
 * additive.
 */

import type {
  GenerateRequestBody,
  GenerateResponseBody,
  GenerateErrorKind,
  GenerateStage,
  InlineProviderConfig,
} from './types';

/** Default client-side timeout. Chosen to be SHORTER than the
 *  Vercel function maxDuration (60s) so we always reach our own
 *  error path before the platform cuts us with its HTML 504. */
export const DEFAULT_CLIENT_TIMEOUT_MS = 45_000;

export interface CallSolManOptions {
  /** Per-call timeout in ms. Defaults to DEFAULT_CLIENT_TIMEOUT_MS. */
  timeoutMs?: number;
  /** Optional fetch override. Tests pass a fake. */
  fetchImpl?: typeof fetch;
  /** External abort signal (e.g. user closed the modal). */
  signal?: AbortSignal;
}

export async function callSolMan(
  prompt: string,
  config?: InlineProviderConfig | null,
  options: CallSolManOptions = {},
): Promise<GenerateResponseBody> {
  const request: GenerateRequestBody = config ? { prompt, config } : { prompt };
  const timeoutMs = options.timeoutMs ?? DEFAULT_CLIENT_TIMEOUT_MS;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);

  // Combine our timeout signal with the caller's signal so an
  // external cancel (user closes modal) wins over the natural
  // timeout — without losing the timeout if the caller didn't pass one.
  const timeoutController = new AbortController();
  const timeoutId = setTimeout(
    () => timeoutController.abort(new DOMException('timeout', 'TimeoutError')),
    timeoutMs,
  );
  const externalListener = () =>
    timeoutController.abort(new DOMException('external', 'AbortError'));
  if (options.signal) {
    if (options.signal.aborted) externalListener();
    else options.signal.addEventListener('abort', externalListener);
  }

  let resp: Response;
  try {
    resp = await fetchImpl('/api/sol-man/generate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
      signal: timeoutController.signal,
    });
  } catch (e) {
    clearTimeout(timeoutId);
    if (options.signal) options.signal.removeEventListener('abort', externalListener);
    // Classify the failure.
    const name = (e as Error)?.name;
    const message = (e as Error)?.message ?? String(e);
    if (name === 'TimeoutError') {
      return makeFailure({
        kind: 'gateway_timeout',
        stage: 'provider_call',
        retryable: true,
        error: `Sol Man timed out after ${(timeoutMs / 1000).toFixed(0)}s. The provider didn't return in time. Press Retry to try again — the prompt is kept.`,
      });
    }
    if (name === 'AbortError') {
      return makeFailure({
        kind: 'unknown',
        stage: 'unknown',
        retryable: false,
        error: 'Sol Man request cancelled.',
      });
    }
    return makeFailure({
      kind: 'network',
      stage: 'provider_call',
      retryable: true,
      error: `Network error reaching Sol Man: ${message}. Check your connection or whether the dev server (vercel dev) is running.`,
    });
  }
  clearTimeout(timeoutId);
  if (options.signal) options.signal.removeEventListener('abort', externalListener);

  // Inspect content-type BEFORE attempting json(). When Vercel's edge
  // cuts us at maxDuration it returns an HTML page, not our JSON
  // envelope — calling resp.json() on that throws and produces the
  // old "non-JSON response (HTTP 504)" message.
  const contentType = (resp.headers.get('content-type') ?? '').toLowerCase();
  const isJson = contentType.includes('application/json');

  if (!isJson) {
    // Read the body once for diagnostic excerpt.
    const text = await resp.text().catch(() => '');
    const excerpt = text.slice(0, 200).replace(/\s+/g, ' ').trim();
    if (resp.status === 504 || /timeout|gateway/i.test(text)) {
      return makeFailure({
        kind: 'gateway_timeout',
        stage: 'provider_call',
        retryable: true,
        error: `Sol Man timed out at the platform gateway (HTTP ${resp.status}). The provider didn't respond in time. Press Retry to try again.`,
        details: { httpStatus: resp.status, rawExcerpt: excerpt },
      });
    }
    if (resp.status >= 500 && resp.status < 600) {
      return makeFailure({
        kind: 'gateway_timeout',
        stage: 'provider_call',
        retryable: true,
        error: `Sol Man's server returned HTTP ${resp.status}. Press Retry — this is usually transient.`,
        details: { httpStatus: resp.status, rawExcerpt: excerpt },
      });
    }
    return makeFailure({
      kind: 'unknown',
      stage: 'unknown',
      retryable: resp.status >= 500,
      error: `Sol Man returned a non-JSON response (HTTP ${resp.status}). This usually means the server is misconfigured or behind a proxy that's intercepting requests.`,
      details: { httpStatus: resp.status, rawExcerpt: excerpt },
    });
  }

  let parsed: unknown;
  try {
    parsed = await resp.json();
  } catch (e) {
    return makeFailure({
      kind: 'unknown',
      stage: 'unknown',
      retryable: resp.status >= 500,
      error: `Sol Man returned a response that claimed to be JSON but didn't parse: ${(e as Error).message}`,
      details: { httpStatus: resp.status },
    });
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return makeFailure({
      kind: 'unknown',
      stage: 'unknown',
      retryable: false,
      error: 'Sol Man returned a malformed response.',
      details: { httpStatus: resp.status },
    });
  }
  // Server speaks our envelope — pass it through unchanged.
  return parsed as GenerateResponseBody;
}

interface FailureSeed {
  kind: GenerateErrorKind;
  stage: GenerateStage;
  retryable: boolean;
  error: string;
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
    retryable: seed.retryable,
    details: seed.details,
  };
}
