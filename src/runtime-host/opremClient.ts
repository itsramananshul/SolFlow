/**
 * Typed HTTP client for the real OpenPrem controller
 * (reference/open-prem-cleaning/controller, binary
 * `openprem-controller-v2`).
 *
 * This is a different protocol from SolFlow's legacy `client.ts`
 * (which targets SolFlow's own Phase C `controller/` crate). The real
 * OpenPrem controller is simpler and source based:
 *
 *   submit:  POST /workflow      { source, workflow }
 *               -> { ok, workflow_id, status }
 *   poll:    GET  /workflow/:id
 *               -> { status, workflow_name, source, result,
 *                    progress, step_count, error }
 *
 * The controller compiles + runs the SOL source itself (no client side
 * bytecode), so the editor just sends the source it already has. Print
 * output is not returned over HTTP; the controller exposes the
 * workflow's return value as `result` plus a status and step count.
 *
 * The controller serves a permissive CORS layer, so the browser can
 * call it cross origin. Auth is optional (Ed25519 HTTP signatures);
 * unsigned requests pass through, so the editor sends none.
 */

// =============================================================
//  Wire types (mirror controller/src/api.rs)
// =============================================================

/** Response of `POST /workflow`. */
export interface OpremSubmitResponse {
  ok: boolean;
  workflow_id: string;
  status: string;
}

/** Response of `GET /workflow/:id`. */
export interface OpremWorkflowStatus {
  /** "running" | "completed" | "error". */
  status: string;
  workflow_name: string | null;
  source: string | null;
  /** Return value (canonical SOL value as JSON) when completed. */
  result: unknown | null;
  /** "<pc>/<step_count>" progress string. */
  progress: string | null;
  step_count: number | null;
  error: string | null;
}

/** Terminal outcome of a submit + poll cycle. */
export interface OpremRunOutcome {
  workflowId: string;
  /** "completed" | "error". */
  status: string;
  result: unknown | null;
  error: string | null;
  stepCount: number | null;
}

// =============================================================
//  Error model
// =============================================================

export type OpremClientError =
  | { kind: 'network'; message: string; cause?: unknown }
  | { kind: 'timeout'; message: string; timeoutMs: number }
  | { kind: 'http'; message: string; status: number; body?: string }
  | { kind: 'decode'; message: string; cause?: unknown }
  | { kind: 'invalid_url'; message: string }
  | { kind: 'aborted'; message: string };

export class OpremClientErr extends Error {
  readonly payload: OpremClientError;
  constructor(payload: OpremClientError) {
    super(payload.message);
    this.name = 'OpremClientErr';
    this.payload = payload;
  }
}

export function isOpremClientError(e: unknown): e is OpremClientErr {
  return e instanceof OpremClientErr;
}

// =============================================================
//  Client
// =============================================================

export interface OpremClientOptions {
  /** Per request default timeout (ms). Default 5000. */
  defaultTimeoutMs?: number;
  /** Fetch override (tests inject a fake). Default globalThis.fetch. */
  fetchImpl?: typeof fetch;
}

export interface OpremRequestOpts {
  timeoutMs?: number;
  signal?: AbortSignal;
}

export interface OpremPollOpts extends OpremRequestOpts {
  /** Poll cadence (ms). Default 250. */
  intervalMs?: number;
  /** Overall ceiling (ms). Default 30000. */
  overallTimeoutMs?: number;
}

export interface OpremClient {
  readonly baseUrl: string;
  /** `POST /workflow` — submit SOL source for a named workflow. */
  submitWorkflow(
    source: string,
    workflow: string,
    opts?: OpremRequestOpts,
  ): Promise<OpremSubmitResponse>;
  /** `GET /workflow/:id` — current status + result. */
  getWorkflow(id: string, opts?: OpremRequestOpts): Promise<OpremWorkflowStatus>;
  /**
   * Submit, then poll until the workflow reaches a terminal status
   * ("completed" or "error") or the overall timeout elapses.
   */
  runWorkflow(
    source: string,
    workflow: string,
    opts?: OpremPollOpts,
  ): Promise<OpremRunOutcome>;
}

const TERMINAL = new Set(['completed', 'error']);

export function opremClient(url: string, options: OpremClientOptions = {}): OpremClient {
  const baseUrl = normalizeBaseUrl(url);
  const defaultTimeoutMs = options.defaultTimeoutMs ?? 5_000;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);

  async function request<T>(
    method: string,
    path: string,
    body: unknown,
    opts: OpremRequestOpts | undefined,
  ): Promise<T> {
    const timeoutMs = opts?.timeoutMs ?? defaultTimeoutMs;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(reason('timeout')), timeoutMs);
    const onExternalAbort = () => controller.abort(reason('aborted'));
    if (opts?.signal) {
      if (opts.signal.aborted) controller.abort(reason('aborted'));
      else opts.signal.addEventListener('abort', onExternalAbort);
    }
    const headers: Record<string, string> = {};
    if (body !== undefined) headers['content-type'] = 'application/json';
    let res: Response;
    try {
      res = await fetchImpl(`${baseUrl}${path}`, {
        method,
        headers: Object.keys(headers).length > 0 ? headers : undefined,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal,
      });
    } catch (cause) {
      if (isAbortError(cause)) {
        const r = controller.signal.reason as { kind?: string } | null;
        if (r?.kind === 'aborted') {
          throw new OpremClientErr({ kind: 'aborted', message: 'request aborted by caller' });
        }
        throw new OpremClientErr({
          kind: 'timeout',
          timeoutMs,
          message: `request timed out after ${timeoutMs}ms`,
        });
      }
      throw new OpremClientErr({
        kind: 'network',
        message: cause instanceof Error ? `network error: ${cause.message}` : 'network error',
        cause,
      });
    } finally {
      clearTimeout(timer);
      if (opts?.signal) opts.signal.removeEventListener('abort', onExternalAbort);
    }

    const text = await res.text().catch(() => '');
    if (!res.ok) {
      // The controller returns plain text error bodies (axum
      // (StatusCode, String)); surface them verbatim.
      throw new OpremClientErr({
        kind: 'http',
        status: res.status,
        body: text || undefined,
        message: text
          ? `controller HTTP ${res.status}: ${text}`
          : `controller HTTP ${res.status}`,
      });
    }
    if (text.length === 0) return undefined as unknown as T;
    try {
      return JSON.parse(text) as T;
    } catch (cause) {
      throw new OpremClientErr({
        kind: 'decode',
        message: 'controller response was not valid JSON',
        cause,
      });
    }
  }

  async function getWorkflow(id: string, opts?: OpremRequestOpts): Promise<OpremWorkflowStatus> {
    return request<OpremWorkflowStatus>('GET', `/workflow/${encodeURIComponent(id)}`, undefined, opts);
  }

  async function submitWorkflow(
    source: string,
    workflow: string,
    opts?: OpremRequestOpts,
  ): Promise<OpremSubmitResponse> {
    const r = await request<OpremSubmitResponse>('POST', '/workflow', { source, workflow }, opts);
    if (!r || typeof r.workflow_id !== 'string') {
      throw new OpremClientErr({
        kind: 'decode',
        message: 'submit response missing workflow_id',
      });
    }
    return r;
  }

  async function runWorkflow(
    source: string,
    workflow: string,
    opts?: OpremPollOpts,
  ): Promise<OpremRunOutcome> {
    const interval = opts?.intervalMs ?? 250;
    const overall = opts?.overallTimeoutMs ?? 30_000;
    const submitted = await submitWorkflow(source, workflow, opts);
    const id = submitted.workflow_id;
    const startedAt = nowMs();
    while (true) {
      if (opts?.signal?.aborted) {
        throw new OpremClientErr({ kind: 'aborted', message: 'run aborted by caller' });
      }
      const elapsed = nowMs() - startedAt;
      if (elapsed >= overall) {
        throw new OpremClientErr({
          kind: 'timeout',
          timeoutMs: overall,
          message: `run exceeded ${overall}ms waiting for terminal status`,
        });
      }
      const remaining = overall - elapsed;
      const st = await getWorkflow(id, {
        timeoutMs: Math.max(500, Math.min(remaining, defaultTimeoutMs)),
        signal: opts?.signal,
      });
      if (TERMINAL.has(st.status)) {
        return {
          workflowId: id,
          status: st.status,
          result: st.result ?? null,
          error: st.error ?? null,
          stepCount: st.step_count ?? null,
        };
      }
      await sleep(interval, opts?.signal);
    }
  }

  return { baseUrl, submitWorkflow, getWorkflow, runWorkflow };
}

// =============================================================
//  Internals
// =============================================================

function nowMs(): number {
  return Date.now();
}

function normalizeBaseUrl(url: string): string {
  const trimmed = url.trim();
  if (!/^https?:\/\//i.test(trimmed)) {
    throw new OpremClientErr({
      kind: 'invalid_url',
      message: `controller URL must start with http:// or https:// (got "${url}")`,
    });
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch (cause) {
    throw new OpremClientErr({
      kind: 'invalid_url',
      message: `controller URL did not parse (${(cause as Error).message ?? cause})`,
    });
  }
  if (!parsed.hostname) {
    throw new OpremClientErr({ kind: 'invalid_url', message: `controller URL has no host (got "${url}")` });
  }
  let normalized = parsed.toString();
  while (normalized.endsWith('/')) normalized = normalized.slice(0, -1);
  return normalized;
}

function isAbortError(e: unknown): boolean {
  if (!e || typeof e !== 'object') return false;
  const name = (e as { name?: string }).name;
  return name === 'AbortError' || name === 'TimeoutError';
}

function reason(kind: 'timeout' | 'aborted'): { kind: string } {
  return { kind };
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new OpremClientErr({ kind: 'aborted', message: 'sleep aborted by caller' }));
      return;
    }
    const t = setTimeout(() => {
      if (signal) signal.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(t);
      reject(new OpremClientErr({ kind: 'aborted', message: 'sleep aborted by caller' }));
    };
    if (signal) signal.addEventListener('abort', onAbort, { once: true });
  });
}
