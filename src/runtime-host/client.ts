/**
 * Typed HTTP client for the SolFlow controller (Phase C C.2 c61).
 *
 * Wraps `fetch` so the editor never calls it directly. Three
 * disciplines this layer enforces:
 *
 *   1. **Typed surface** — every method takes / returns
 *      `host-spec`-mirrored types from `./types`. Misshapen
 *      controller responses raise a structured error, not a
 *      type-cast crash.
 *
 *   2. **Structured errors** — every failure mode produces a
 *      `ControllerClientError` with a discriminated `kind` so
 *      the UI can render distinct UX for network failure vs
 *      version mismatch vs HTTP 5xx vs decoder failure.
 *
 *   3. **Timeouts** — every request has an AbortSignal so a
 *      hung controller doesn't freeze the UI. Default 5s; the
 *      polling caller raises this for long-running run waits.
 *
 * Usage:
 *
 *   const client = controllerClient('http://127.0.0.1:3939');
 *   const health = await client.healthz();
 *   const wf = await client.submitWorkflow({ ... });
 *   const created = await client.createRun({ workflow_id, ... });
 *   const final = await client.pollRun(created.run_id);
 *
 * The client is stateless beyond the base URL — multiple instances
 * for different controllers are safe.
 */
import { HOST_SPEC_MAJOR } from './types';
import type {
  ActiveRunSummary,
  ConcurrencyMetrics,
  ConnectorMeta,
  Health,
  ProviderInfo,
  RunCreated,
  RunRecord,
  RunRequest,
  RunStatus,
  ScheduleCreate,
  ScheduleId,
  ScheduleRecord,
  WorkflowId,
  WorkflowSubmission,
  WorkflowSubmissionResponse,
} from './types';

// =============================================================
//  Error model
// =============================================================

/**
 * Discriminated client error. Every failure mode maps to one
 * `kind` so callers can render distinct UX.
 *
 *   - `network`     — request couldn't be sent / fetch threw /
 *                     CORS preflight failed / DNS / refused
 *   - `timeout`     — AbortSignal fired before the response
 *                     arrived (client-side timeout, not 504)
 *   - `http`        — server responded with non-2xx; carries
 *                     status + the controller's structured
 *                     `{ error: { code, message } }` body when
 *                     present
 *   - `decode`      — body couldn't be parsed as JSON, OR the
 *                     parsed shape didn't satisfy the expected
 *                     type contract (missing required field, etc.)
 *   - `version`     — controller's `host_spec_major` doesn't
 *                     match the editor's. Refused before any
 *                     workflow / run call.
 *   - `aborted`     — caller's external abort fired (e.g. user
 *                     cancelled). Distinct from `timeout` so the
 *                     UI doesn't show "timed out" for user action.
 */
export interface ControllerClientErrorBase {
  message: string;
}

export type ControllerClientError =
  | (ControllerClientErrorBase & { kind: 'network'; cause?: unknown })
  | (ControllerClientErrorBase & {
      kind: 'timeout';
      timeoutMs: number;
    })
  | (ControllerClientErrorBase & {
      kind: 'http';
      status: number;
      /** Controller's structured error code, when the body parsed. */
      code?: string;
    })
  | (ControllerClientErrorBase & { kind: 'decode'; cause?: unknown })
  | (ControllerClientErrorBase & {
      kind: 'version';
      controllerMajor: number;
      editorMajor: number;
    })
  /**
   * Phase C C.7 c99 — controller rejected the request because of
   * missing / malformed / mismatched bearer token. Distinct from
   * `http` so the editor can surface "set your token" UX without
   * pattern-matching status codes everywhere.
   *
   * `code` mirrors the controller's `auth_missing` /
   * `auth_malformed` / `auth_mismatch` discriminator so the modal
   * can render distinct guidance per case.
   */
  | (ControllerClientErrorBase & {
      kind: 'auth';
      status: number;
      code: 'auth_missing' | 'auth_malformed' | 'auth_mismatch' | 'unauthorized';
    })
  /**
   * Phase C C.7 c99 — URL didn't parse as a controller URL we'll
   * talk to. Carries a classification so the modal can render
   * the right correction prompt. Thrown by `controllerClient()`
   * construction, never from a request.
   */
  | (ControllerClientErrorBase & {
      kind: 'invalid_url';
      reason: 'no_scheme' | 'bad_scheme' | 'unparseable' | 'no_host';
    })
  | (ControllerClientErrorBase & { kind: 'aborted' });

export class ControllerClientErr extends Error {
  /**
   * Carries the discriminated error payload. Throw an instance
   * of this class so `try { … } catch (e) { if (e instanceof
   * ControllerClientErr) { e.payload.kind } }` works.
   */
  readonly payload: ControllerClientError;
  constructor(payload: ControllerClientError) {
    super(payload.message);
    this.name = 'ControllerClientErr';
    this.payload = payload;
  }
}

/** Convenience type-guard. */
export function isControllerClientError(
  e: unknown,
): e is ControllerClientErr {
  return e instanceof ControllerClientErr;
}

// =============================================================
//  Client configuration
// =============================================================

export interface ControllerClientOptions {
  /** Per-request default timeout in ms (excluding long-poll). */
  defaultTimeoutMs?: number;
  /**
   * Optional fetch override. Tests pass a fake. Default is
   * globalThis.fetch.
   */
  fetchImpl?: typeof fetch;
  /**
   * Phase C C.7 c99 — bearer token injected as
   * `Authorization: Bearer <token>` on every request. Empty
   * string / undefined → no header. /healthz still works
   * without a token (the controller leaves it open as the
   * "is this controller alive + what does it require" probe),
   * but every protected route requires this when the
   * controller has `auth_required=true`.
   */
  authToken?: string;
}

export interface ControllerClient {
  /** Normalized base URL (no trailing slash). */
  readonly baseUrl: string;
  healthz(opts?: RequestOpts): Promise<Health>;
  /** Health + host-spec major validation. Throws `version` on mismatch. */
  healthzChecked(opts?: RequestOpts): Promise<Health>;
  submitWorkflow(
    body: WorkflowSubmission,
    opts?: RequestOpts,
  ): Promise<WorkflowSubmissionResponse>;
  createRun(body: RunRequest, opts?: RequestOpts): Promise<RunCreated>;
  getRun(runId: string, opts?: RequestOpts): Promise<RunRecord>;
  listRuns(
    workflowId: WorkflowId,
    query?: { status?: RunStatus; limit?: number },
    opts?: RequestOpts,
  ): Promise<RunRecord[]>;
  /**
   * Cancel a run. C.2 returns NotImplemented from the controller;
   * the call exists so callers can use it unconditionally and the
   * UI can surface "cancel not yet supported" when it fires.
   */
  cancelRun(runId: string, opts?: RequestOpts): Promise<void>;
  /**
   * Poll `getRun` at `intervalMs` until status is terminal
   * (Succeeded / Failed / Cancelled) OR `overallTimeoutMs` elapses
   * OR `opts.signal` fires. Returns the final `RunRecord`.
   *
   * Default polling interval is 200ms (fast enough for sub-second
   * runs to feel snappy, slow enough that a 60s run doesn't burn
   * the controller with 300 requests).
   */
  pollRun(
    runId: string,
    pollOpts?: PollRunOptions,
  ): Promise<RunRecord>;

  // ---- Phase C C.3 — schedules + webhook ingress ----

  /** `POST /workflows/:id/schedules` */
  createSchedule(
    workflowId: WorkflowId,
    body: ScheduleCreate,
    opts?: RequestOpts,
  ): Promise<ScheduleRecord>;

  /** `GET /workflows/:id/schedules` */
  listSchedules(
    workflowId: WorkflowId,
    opts?: RequestOpts,
  ): Promise<ScheduleRecord[]>;

  /** `GET /schedules/:id` */
  getSchedule(scheduleId: ScheduleId, opts?: RequestOpts): Promise<ScheduleRecord>;

  /** `PATCH /schedules/:id` with `{ enabled }`. */
  setScheduleEnabled(
    scheduleId: ScheduleId,
    enabled: boolean,
    opts?: RequestOpts,
  ): Promise<ScheduleRecord>;

  /** `DELETE /schedules/:id`. */
  cancelSchedule(scheduleId: ScheduleId, opts?: RequestOpts): Promise<void>;

  /**
   * `POST /events/:path` — manually trigger any registered Event
   * schedules listening on `path` with the supplied body as
   * `inputs`. Editor uses this for a "test webhook" affordance.
   * Throws `http` 404 when no schedule matches.
   */
  triggerEvent(
    path: string,
    body: unknown,
    opts?: RequestOpts,
  ): Promise<RunRecord>;

  // ---- Phase C C.4 — connectors ----

  /** `GET /connectors` — list registered connector metadata. */
  listConnectors(opts?: RequestOpts): Promise<ConnectorMeta[]>;

  // ---- Phase 3 — real providers ----

  /** `GET /providers` — the real providers (module, url) the controller
   *  resolves `call("module.fn", …)` against. The honest "what will run
   *  for real" listing. Empty when none are configured. */
  listProviders(opts?: RequestOpts): Promise<ProviderInfo[]>;

  // ---- Phase C C.6 — orchestration introspection ----

  /** `GET /runs/active` — in-flight runs from the controller's
   *  RunManager active registry. */
  listActiveRuns(opts?: RequestOpts): Promise<ActiveRunSummary[]>;

  /** `GET /controller/concurrency` — saturation snapshot. */
  getConcurrencyMetrics(opts?: RequestOpts): Promise<ConcurrencyMetrics>;
}

export interface RequestOpts {
  /** Per-call timeout override. */
  timeoutMs?: number;
  /** Caller's external abort signal; e.g. user closed the modal. */
  signal?: AbortSignal;
}

export interface PollRunOptions extends RequestOpts {
  /** Poll cadence. Default 200ms. */
  intervalMs?: number;
  /** Hard overall ceiling. Default 30s. */
  overallTimeoutMs?: number;
}

// =============================================================
//  Factory
// =============================================================

const TERMINAL_STATUSES = new Set<RunStatus>([
  'Succeeded',
  'Failed',
  'Cancelled',
]);

export function controllerClient(
  url: string,
  options: ControllerClientOptions = {},
): ControllerClient {
  const baseUrl = normalizeBaseUrl(url);
  const defaultTimeoutMs = options.defaultTimeoutMs ?? 5_000;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const authToken = options.authToken && options.authToken.length > 0
    ? options.authToken
    : undefined;

  async function request<T>(
    method: string,
    path: string,
    body: unknown,
    opts: RequestOpts | undefined,
  ): Promise<T> {
    const timeoutMs = opts?.timeoutMs ?? defaultTimeoutMs;
    const url = `${baseUrl}${path}`;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(reasonTimeout(timeoutMs)), timeoutMs);
    // Caller can abort externally too — wire it in.
    const externalListener = () => controller.abort(reasonAborted());
    if (opts?.signal) {
      if (opts.signal.aborted) controller.abort(reasonAborted());
      else opts.signal.addEventListener('abort', externalListener);
    }
    // Phase C C.7 c99 — assemble headers explicitly so the auth
    // token rides on every request (including GETs with no body).
    const headers: Record<string, string> = {};
    if (body !== undefined) headers['content-type'] = 'application/json';
    if (authToken) headers['authorization'] = `Bearer ${authToken}`;
    let res: Response;
    try {
      res = await fetchImpl(url, {
        method,
        headers: Object.keys(headers).length > 0 ? headers : undefined,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal,
      });
    } catch (cause) {
      if (isAbortError(cause)) {
        // Discriminate timeout vs external abort by inspecting the
        // signal reason (set above).
        const reason = controller.signal.reason as { kind?: string } | null;
        if (reason?.kind === 'aborted') {
          throw new ControllerClientErr({
            kind: 'aborted',
            message: 'request aborted by caller',
          });
        }
        throw new ControllerClientErr({
          kind: 'timeout',
          timeoutMs,
          message: `request timed out after ${timeoutMs}ms`,
        });
      }
      throw new ControllerClientErr({
        kind: 'network',
        message:
          cause instanceof Error
            ? `network error: ${cause.message}`
            : 'network error',
        cause,
      });
    } finally {
      clearTimeout(timer);
      if (opts?.signal) opts.signal.removeEventListener('abort', externalListener);
    }

    // Try to parse the body even on non-2xx so we get the
    // controller's structured `{ error: { code, message } }`.
    let bodyJson: unknown = null;
    const text = await res.text().catch(() => '');
    if (text.length > 0) {
      try {
        bodyJson = JSON.parse(text);
      } catch {
        // Non-JSON body. Leave bodyJson null; treated per status below.
      }
    }

    if (!res.ok) {
      const err = extractHttpError(res.status, bodyJson);
      throw new ControllerClientErr(err);
    }

    // Empty 2xx (e.g. 204 from a future cancel) — caller's T must
    // accept this. We surface `null as unknown as T` only when T
    // is `void`; the caller's signature controls this.
    if (bodyJson === null && text.length === 0) {
      return undefined as unknown as T;
    }
    return bodyJson as T;
  }

  async function healthz(opts?: RequestOpts): Promise<Health> {
    const health = await request<Health>('GET', '/healthz', undefined, opts);
    if (typeof health !== 'object' || health === null) {
      throw new ControllerClientErr({
        kind: 'decode',
        message: 'health response is not an object',
      });
    }
    if (typeof health.host_spec_major !== 'number') {
      throw new ControllerClientErr({
        kind: 'decode',
        message: 'health response missing host_spec_major',
      });
    }
    return health;
  }

  async function healthzChecked(opts?: RequestOpts): Promise<Health> {
    const h = await healthz(opts);
    if (h.host_spec_major !== HOST_SPEC_MAJOR) {
      throw new ControllerClientErr({
        kind: 'version',
        controllerMajor: h.host_spec_major,
        editorMajor: HOST_SPEC_MAJOR,
        message: `host-spec major mismatch: controller=${h.host_spec_major}, editor=${HOST_SPEC_MAJOR}`,
      });
    }
    return h;
  }

  async function pollRun(
    runId: string,
    pollOpts?: PollRunOptions,
  ): Promise<RunRecord> {
    const interval = pollOpts?.intervalMs ?? 200;
    const overall = pollOpts?.overallTimeoutMs ?? 30_000;
    const startedAt = Date.now();
    while (true) {
      const elapsed = Date.now() - startedAt;
      if (elapsed >= overall) {
        throw new ControllerClientErr({
          kind: 'timeout',
          timeoutMs: overall,
          message: `pollRun exceeded ${overall}ms waiting for terminal status`,
        });
      }
      if (pollOpts?.signal?.aborted) {
        throw new ControllerClientErr({
          kind: 'aborted',
          message: 'pollRun aborted by caller',
        });
      }
      // Inner request timeout: never longer than the remaining
      // overall budget, so a slow GET can't drag us past the cap.
      const remaining = overall - elapsed;
      const r = await request<RunRecord>(
        'GET',
        `/runs/${encodeURIComponent(runId)}`,
        undefined,
        {
          timeoutMs: Math.max(500, Math.min(remaining, defaultTimeoutMs)),
          signal: pollOpts?.signal,
        },
      );
      if (TERMINAL_STATUSES.has(r.status)) return r;
      await sleep(interval, pollOpts?.signal);
    }
  }

  return {
    baseUrl,
    healthz,
    healthzChecked,
    submitWorkflow: (body, opts) =>
      request<WorkflowSubmissionResponse>(
        'POST',
        '/workflows',
        body,
        opts,
      ),
    createRun: (body, opts) =>
      request<RunCreated>('POST', '/runs', body, opts),
    getRun: (runId, opts) =>
      request<RunRecord>('GET', `/runs/${encodeURIComponent(runId)}`, undefined, opts),
    listRuns: (workflowId, query, opts) => {
      const qs = new URLSearchParams();
      if (query?.status) qs.set('status', query.status);
      if (query?.limit !== undefined) qs.set('limit', String(query.limit));
      const path =
        `/workflows/${encodeURIComponent(workflowId)}/runs`
        + (qs.toString() ? `?${qs}` : '');
      return request<RunRecord[]>('GET', path, undefined, opts);
    },
    cancelRun: async (runId, opts) => {
      await request<void>(
        'DELETE',
        `/runs/${encodeURIComponent(runId)}`,
        undefined,
        opts,
      );
    },
    pollRun,

    // ---- C.3 — schedules + webhook ingress ----

    createSchedule: (workflowId, body, opts) =>
      request<ScheduleRecord>(
        'POST',
        `/workflows/${encodeURIComponent(workflowId)}/schedules`,
        body,
        opts,
      ),
    listSchedules: (workflowId, opts) =>
      request<ScheduleRecord[]>(
        'GET',
        `/workflows/${encodeURIComponent(workflowId)}/schedules`,
        undefined,
        opts,
      ),
    getSchedule: (scheduleId, opts) =>
      request<ScheduleRecord>(
        'GET',
        `/schedules/${encodeURIComponent(scheduleId)}`,
        undefined,
        opts,
      ),
    setScheduleEnabled: (scheduleId, enabled, opts) =>
      request<ScheduleRecord>(
        'PATCH',
        `/schedules/${encodeURIComponent(scheduleId)}`,
        { enabled },
        opts,
      ),
    cancelSchedule: async (scheduleId, opts) => {
      await request<void>(
        'DELETE',
        `/schedules/${encodeURIComponent(scheduleId)}`,
        undefined,
        opts,
      );
    },
    triggerEvent: (path, body, opts) => {
      // `path` may contain slashes (matching the controller's
      // `/events/*path` wildcard). encodeURIComponent would
      // escape them and break the route, so we pass the path
      // verbatim — callers are trusted in editor context.
      const safePath = path.replace(/^\/+/, '');
      return request<RunRecord>(
        'POST',
        `/events/${safePath}`,
        body,
        opts,
      );
    },

    listConnectors: (opts) =>
      request<ConnectorMeta[]>('GET', '/connectors', undefined, opts),

    // ---- Phase 3 ----
    listProviders: (opts) =>
      request<ProviderInfo[]>('GET', '/providers', undefined, opts),

    // ---- C.6 ----
    listActiveRuns: (opts) =>
      request<ActiveRunSummary[]>('GET', '/runs/active', undefined, opts),
    getConcurrencyMetrics: (opts) =>
      request<ConcurrencyMetrics>(
        'GET',
        '/controller/concurrency',
        undefined,
        opts,
      ),
  };
}

// =============================================================
//  Internals
// =============================================================

function normalizeBaseUrl(url: string): string {
  const trimmed = url.trim();
  if (trimmed.length === 0) {
    throw new ControllerClientErr({
      kind: 'invalid_url',
      reason: 'unparseable',
      message: 'controller URL is empty',
    });
  }
  // Reject URLs without an explicit scheme — silently rewriting
  // them is a security footgun ("controller.example" is NOT the
  // same as http://controller.example). Schemes are recognized
  // by the `://` separator; bare-colon `host:port` lookalikes
  // are treated as missing scheme since that's the user's
  // common typo (and ambiguous with a future scheme).
  if (/^https?:\/\//i.test(trimmed)) {
    // fine — fall through to parse
  } else if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) {
    throw new ControllerClientErr({
      kind: 'invalid_url',
      reason: 'bad_scheme',
      message: `controller URL must use http:// or https:// (got "${url}")`,
    });
  } else {
    throw new ControllerClientErr({
      kind: 'invalid_url',
      reason: 'no_scheme',
      message: `controller URL must start with http:// or https:// (got "${url}")`,
    });
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch (cause) {
    throw new ControllerClientErr({
      kind: 'invalid_url',
      reason: 'unparseable',
      message: `controller URL did not parse (${(cause as Error).message ?? cause})`,
    });
  }
  if (!parsed.hostname) {
    throw new ControllerClientErr({
      kind: 'invalid_url',
      reason: 'no_host',
      message: `controller URL has no host (got "${url}")`,
    });
  }
  // URL.toString preserves any trailing slash on a pure-host URL
  // (`http://x` → `http://x/`). Strip trailing slashes consistently
  // so request joins produce a clean `<base><path>`.
  let normalized = parsed.toString();
  while (normalized.endsWith('/')) normalized = normalized.slice(0, -1);
  return normalized;
}

// =============================================================
//  Phase C C.7 c99 — URL classification helper
// =============================================================

/**
 * Static classification of a controller URL. The modal renders
 * intentional UX per class:
 *
 *   - `local`         — http://localhost / 127.0.0.1 / [::1] / *.local
 *                       Fine for local dev. No HTTPS expected.
 *   - `loopback_https`— HTTPS to localhost. Probably overkill, but
 *                       fine; we don't warn about it.
 *   - `https_remote`  — HTTPS to a non-loopback host. The good
 *                       remote case.
 *   - `unsafe_remote` — HTTP (not HTTPS) to a non-loopback host.
 *                       Editor surfaces a "credentials + bytecode
 *                       travel in cleartext" warning.
 *   - `invalid`       — URL doesn't parse / has no scheme / no host.
 *
 * Returns a `warnings` array the modal can render verbatim. Callers
 * MUST NOT silently upgrade `http://` to `https://` — the URL is
 * the user's typed intent.
 */
export interface UrlClassification {
  kind:
    | 'local'
    | 'loopback_https'
    | 'https_remote'
    | 'unsafe_remote'
    | 'invalid';
  url: string;
  warnings: string[];
  /** When `kind: 'invalid'`, why. */
  reason?: 'no_scheme' | 'bad_scheme' | 'unparseable' | 'no_host' | 'empty';
}

const LOOPBACK_HOSTS = new Set([
  'localhost',
  '127.0.0.1',
  '0.0.0.0',
  '::1',
  '[::1]',
]);

export function classifyControllerUrl(url: string): UrlClassification {
  const trimmed = url.trim();
  if (trimmed.length === 0) {
    return { kind: 'invalid', url: trimmed, warnings: [], reason: 'empty' };
  }
  if (!/^https?:\/\//i.test(trimmed)) {
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) {
      return {
        kind: 'invalid',
        url: trimmed,
        warnings: ['URL scheme must be http:// or https://'],
        reason: 'bad_scheme',
      };
    }
    return {
      kind: 'invalid',
      url: trimmed,
      warnings: ['URL must start with http:// or https://'],
      reason: 'no_scheme',
    };
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return {
      kind: 'invalid',
      url: trimmed,
      warnings: ['URL could not be parsed'],
      reason: 'unparseable',
    };
  }
  if (!parsed.hostname) {
    return {
      kind: 'invalid',
      url: trimmed,
      warnings: ['URL has no host'],
      reason: 'no_host',
    };
  }
  const host = parsed.hostname.toLowerCase();
  const isLoopback =
    LOOPBACK_HOSTS.has(host) || host.endsWith('.localhost') || host.endsWith('.local');
  const isHttps = parsed.protocol === 'https:';
  if (isLoopback && !isHttps) {
    return { kind: 'local', url: trimmed, warnings: [] };
  }
  if (isLoopback && isHttps) {
    return { kind: 'loopback_https', url: trimmed, warnings: [] };
  }
  if (!isHttps) {
    return {
      kind: 'unsafe_remote',
      url: trimmed,
      warnings: [
        'Plain HTTP to a remote host transports your token and workflow bytecode in cleartext. Use https:// or tunnel locally.',
      ],
    };
  }
  return { kind: 'https_remote', url: trimmed, warnings: [] };
}

interface StructuredHttpError {
  error?: { code?: string; message?: string };
}

const AUTH_CODES = new Set([
  'auth_missing',
  'auth_malformed',
  'auth_mismatch',
  'unauthorized',
]);

function extractHttpError(
  status: number,
  body: unknown,
): ControllerClientError {
  const wrapped = body as StructuredHttpError | null;
  const code = wrapped?.error?.code;
  const msg = wrapped?.error?.message;
  // Phase C C.7 c99 — 401 with a known auth code maps to the
  // discriminated `auth` kind so the UI can prompt the user to
  // set / fix their token without pattern-matching status codes.
  if (status === 401 && code && AUTH_CODES.has(code)) {
    return {
      kind: 'auth',
      status,
      code: code as 'auth_missing' | 'auth_malformed' | 'auth_mismatch' | 'unauthorized',
      message: msg
        ? `controller rejected auth (${code}): ${msg}`
        : `controller rejected auth (${code})`,
    };
  }
  // A bare 401 with no structured code still gets bucketed as
  // `auth` since "401 means auth" by convention; the editor's
  // fallback render path covers it.
  if (status === 401) {
    return {
      kind: 'auth',
      status,
      code: 'unauthorized',
      message: msg
        ? `controller rejected auth: ${msg}`
        : 'controller rejected auth',
    };
  }
  return {
    kind: 'http',
    status,
    code,
    message: msg
      ? `controller HTTP ${status} (${code ?? 'unknown'}): ${msg}`
      : `controller HTTP ${status}`,
  };
}

function isAbortError(e: unknown): boolean {
  if (!e || typeof e !== 'object') return false;
  const name = (e as { name?: string }).name;
  return name === 'AbortError' || name === 'TimeoutError';
}

/** Sentinel objects so the abort reason discriminates the cause. */
function reasonTimeout(ms: number): { kind: 'timeout'; ms: number } {
  return { kind: 'timeout', ms };
}
function reasonAborted(): { kind: 'aborted' } {
  return { kind: 'aborted' };
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(
        new ControllerClientErr({
          kind: 'aborted',
          message: 'sleep aborted by caller',
        }),
      );
      return;
    }
    const t = setTimeout(() => {
      if (signal) signal.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(t);
      reject(
        new ControllerClientErr({
          kind: 'aborted',
          message: 'sleep aborted by caller',
        }),
      );
    };
    if (signal) signal.addEventListener('abort', onAbort, { once: true });
  });
}
