/**
 * Vitest coverage for the typed controller client (Phase C C.2 c61).
 *
 * Strategy: inject a fake `fetchImpl` per test. That keeps tests
 * hermetic (no network, no port, no live controller) while still
 * exercising the full client surface — including timeout, abort,
 * structured error decoding, and host-spec major check.
 *
 * The fake returns precooked `Response` objects from the global
 * `Response` (Node 18+ provides one). When the test asserts
 * timeout / network failure, the fake throws instead.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ControllerClientErr,
  controllerClient,
  isControllerClientError,
} from '../client';
import { HOST_SPEC_MAJOR } from '../types';
import type {
  Health,
  RunCreated,
  RunRecord,
  WorkflowSubmissionResponse,
} from '../types';

// ----- test helpers -----

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function fakeFetch(handler: (input: RequestInfo | URL, init?: RequestInit) => Response | Promise<Response>): typeof fetch {
  return ((input: RequestInfo | URL, init?: RequestInit) =>
    Promise.resolve(handler(input, init))) as typeof fetch;
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

// ----- normalization -----

describe('controllerClient — base URL normalization', () => {
  it('strips a single trailing slash', () => {
    const c = controllerClient('http://x.example/', { fetchImpl: fakeFetch(() => jsonResponse({})) });
    expect(c.baseUrl).toBe('http://x.example');
  });

  it('strips multiple trailing slashes', () => {
    const c = controllerClient('http://x.example///', { fetchImpl: fakeFetch(() => jsonResponse({})) });
    expect(c.baseUrl).toBe('http://x.example');
  });

  it('rejects URLs without a scheme', () => {
    expect(() => controllerClient('127.0.0.1:3939')).toThrow(/http:\/\/ or https:\/\//);
  });

  it('rejects file:// URLs', () => {
    expect(() => controllerClient('file:///tmp/x')).toThrow(/http:\/\/ or https:\/\//);
  });
});

// ----- happy paths -----

describe('controllerClient — happy paths', () => {
  const sampleHealth: Health = {
    ok: true,
    controller_version: '0.1.0',
    host_spec_major: HOST_SPEC_MAJOR,
  };

  it('healthz returns the parsed body', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(sampleHealth)),
    });
    const h = await c.healthz();
    expect(h.host_spec_major).toBe(HOST_SPEC_MAJOR);
    expect(h.controller_version).toBe('0.1.0');
  });

  it('healthzChecked passes on matching major', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(sampleHealth)),
    });
    await expect(c.healthzChecked()).resolves.toMatchObject({ ok: true });
  });

  it('submitWorkflow POSTs JSON and returns the parsed response', async () => {
    let captured: RequestInit | undefined;
    const resp: WorkflowSubmissionResponse = {
      workflow_id: 'wf_abc',
      content_hash: 'deadbeef',
    };
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((_url, init) => {
        captured = init;
        return jsonResponse(resp);
      }),
    });
    const got = await c.submitWorkflow({
      name: 'test',
      bytecode: [1, 2],
      instruction_spans: [3],
    });
    expect(got.workflow_id).toBe('wf_abc');
    expect(captured?.method).toBe('POST');
    expect(captured?.body).toBe(
      JSON.stringify({ name: 'test', bytecode: [1, 2], instruction_spans: [3] }),
    );
  });

  it('createRun returns the parsed response', async () => {
    const created: RunCreated = { run_id: 'run_a', status: 'Queued' };
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(created)),
    });
    const r = await c.createRun({
      workflow_id: 'wf_abc',
      trigger: { kind: 'Manual' },
    });
    expect(r.run_id).toBe('run_a');
  });

  it('listRuns serializes status + limit into the query string', async () => {
    let capturedUrl = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((url) => {
        capturedUrl = String(url);
        return jsonResponse([]);
      }),
    });
    await c.listRuns('wf_abc', { status: 'Failed', limit: 20 });
    expect(capturedUrl).toBe('http://x.example/workflows/wf_abc/runs?status=Failed&limit=20');
  });
});

// ----- error mapping -----

describe('controllerClient — error mapping', () => {
  it('rejects HTTP 4xx with structured `kind: "http"` error', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() =>
        jsonResponse(
          { error: { code: 'bytecode_invalid', message: 'empty bytecode' } },
          400,
        ),
      ),
    });
    try {
      await c.submitWorkflow({
        name: 'x',
        bytecode: [],
        instruction_spans: [],
      });
      expect.fail('should have thrown');
    } catch (e) {
      expect(isControllerClientError(e)).toBe(true);
      const err = e as ControllerClientErr;
      expect(err.payload.kind).toBe('http');
      if (err.payload.kind === 'http') {
        expect(err.payload.status).toBe(400);
        expect(err.payload.code).toBe('bytecode_invalid');
      }
    }
  });

  it('rejects 404 cleanly when no body parses', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => new Response('Not Found', { status: 404 })),
    });
    await expect(c.getRun('run_missing')).rejects.toMatchObject({
      payload: { kind: 'http', status: 404 },
    });
  });

  it('maps network failure to `kind: "network"`', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: (() =>
        Promise.reject(new TypeError('fetch failed: ECONNREFUSED'))) as typeof fetch,
    });
    await expect(c.healthz()).rejects.toMatchObject({
      payload: { kind: 'network' },
    });
  });

  it('rejects host-spec major mismatch with `kind: "version"`', async () => {
    const wrongMajor: Health = {
      ok: true,
      controller_version: '0.99.0',
      host_spec_major: 999,
    };
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(wrongMajor)),
    });
    try {
      await c.healthzChecked();
      expect.fail('should have thrown');
    } catch (e) {
      expect(isControllerClientError(e)).toBe(true);
      const err = e as ControllerClientErr;
      expect(err.payload.kind).toBe('version');
      if (err.payload.kind === 'version') {
        expect(err.payload.controllerMajor).toBe(999);
        expect(err.payload.editorMajor).toBe(HOST_SPEC_MAJOR);
      }
    }
  });

  it('rejects misshapen health responses with `kind: "decode"`', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse({ controller_version: 'x' })),
    });
    await expect(c.healthz()).rejects.toMatchObject({
      payload: { kind: 'decode' },
    });
  });
});

// ----- timeout + abort -----

describe('controllerClient — timeout + abort', () => {
  it('times out a slow request with `kind: "timeout"`', async () => {
    const c = controllerClient('http://x.example', {
      defaultTimeoutMs: 25,
      fetchImpl: ((_url: RequestInfo | URL, init?: RequestInit) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            const err = new Error('aborted');
            err.name = 'AbortError';
            reject(err);
          });
        })) as typeof fetch,
    });
    const promise = c.healthz();
    // Attach the rejection handler BEFORE advancing timers so the
    // rejection isn't briefly unhandled (Vitest flags those even
    // though the test ultimately passes).
    const expectation = expect(promise).rejects.toMatchObject({
      payload: { kind: 'timeout', timeoutMs: 25 },
    });
    await vi.advanceTimersByTimeAsync(50);
    await expectation;
  });

  it('honors an external abort signal with `kind: "aborted"`', async () => {
    const ctrl = new AbortController();
    const c = controllerClient('http://x.example', {
      fetchImpl: ((_url: RequestInfo | URL, init?: RequestInit) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            const err = new Error('aborted');
            err.name = 'AbortError';
            reject(err);
          });
        })) as typeof fetch,
    });
    const promise = c.healthz({ signal: ctrl.signal });
    ctrl.abort();
    await expect(promise).rejects.toMatchObject({
      payload: { kind: 'aborted' },
    });
  });
});

// ----- pollRun -----

describe('controllerClient — pollRun', () => {
  it('returns when status hits a terminal value', async () => {
    let n = 0;
    const sequence: RunRecord[] = [
      buildRun('Queued'),
      buildRun('Running'),
      buildRun('Succeeded'),
    ];
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(sequence[Math.min(n++, sequence.length - 1)])),
    });
    const promise = c.pollRun('run_a', { intervalMs: 10, overallTimeoutMs: 500 });
    await vi.advanceTimersByTimeAsync(50);
    const r = await promise;
    expect(r.status).toBe('Succeeded');
    expect(n).toBeGreaterThanOrEqual(3);
  });

  it('times out a run that never reaches terminal status', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() => jsonResponse(buildRun('Running'))),
    });
    const promise = c.pollRun('run_a', { intervalMs: 5, overallTimeoutMs: 30 });
    const expectation = expect(promise).rejects.toMatchObject({
      payload: { kind: 'timeout' },
    });
    await vi.advanceTimersByTimeAsync(100);
    await expectation;
  });
});

function buildRun(status: RunRecord['status']): RunRecord {
  return {
    id: 'run_a',
    workflow_id: 'wf_a',
    status,
    trigger: { kind: 'Manual' },
    inputs: {},
    diagnostics: [],
    created_at: 0,
  };
}

// ----- C.3 — schedules + event ingress -----

describe('controllerClient — schedules (C.3)', () => {
  const sampleSchedule = {
    id: 'sch_abc',
    workflow_id: 'wf_a',
    trigger: { kind: 'Timer', schedule_id: 'sch_abc', cron: '*/5 * * * *' },
    enabled: true,
    next_fire_at: 1_700_000_001_000,
    created_at: 1_700_000_000_000,
  };

  it('createSchedule POSTs the body to the workflow-scoped endpoint', async () => {
    let url = '';
    let method = '';
    let body = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((u, init) => {
        url = String(u);
        method = init?.method ?? '';
        body = String(init?.body ?? '');
        return jsonResponse(sampleSchedule, 201);
      }),
    });
    const got = await c.createSchedule('wf_a', {
      trigger: { kind: 'Timer', schedule_id: '', cron: '*/5 * * * *' },
      enabled: true,
    });
    expect(method).toBe('POST');
    expect(url).toBe('http://x.example/workflows/wf_a/schedules');
    expect(JSON.parse(body).trigger.cron).toBe('*/5 * * * *');
    expect(got.id).toBe('sch_abc');
  });

  it('listSchedules GETs the workflow-scoped endpoint', async () => {
    let url = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((u) => {
        url = String(u);
        return jsonResponse([sampleSchedule]);
      }),
    });
    const got = await c.listSchedules('wf_a');
    expect(url).toBe('http://x.example/workflows/wf_a/schedules');
    expect(got).toHaveLength(1);
  });

  it('setScheduleEnabled PATCHes the schedule-scoped endpoint', async () => {
    let method = '';
    let body = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((_u, init) => {
        method = init?.method ?? '';
        body = String(init?.body ?? '');
        return jsonResponse({ ...sampleSchedule, enabled: false });
      }),
    });
    const got = await c.setScheduleEnabled('sch_abc', false);
    expect(method).toBe('PATCH');
    expect(JSON.parse(body).enabled).toBe(false);
    expect(got.enabled).toBe(false);
  });

  it('cancelSchedule DELETEs and returns void', async () => {
    let url = '';
    let method = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((u, init) => {
        url = String(u);
        method = init?.method ?? '';
        return new Response(null, { status: 204 });
      }),
    });
    await c.cancelSchedule('sch_abc');
    expect(method).toBe('DELETE');
    expect(url).toBe('http://x.example/schedules/sch_abc');
  });

  it('triggerEvent POSTs to /events/:path verbatim (preserves slashes)', async () => {
    let url = '';
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((u) => {
        url = String(u);
        return jsonResponse(buildRun('Queued'));
      }),
    });
    await c.triggerEvent('ci/build', { ref: 'main' });
    expect(url).toBe('http://x.example/events/ci/build');
  });

  it('triggerEvent surfaces 404 as kind=http when no schedule matches', async () => {
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch(() =>
        jsonResponse(
          { error: { code: 'schedule_not_found', message: 'no match' } },
          404,
        ),
      ),
    });
    await expect(c.triggerEvent('nowhere', {})).rejects.toMatchObject({
      payload: { kind: 'http', status: 404 },
    });
  });
});

// ----- C.4 — connectors -----

describe('controllerClient — connectors (C.4)', () => {
  it('listConnectors returns the parsed list', async () => {
    let url = '';
    const payload = [
      {
        name: 'http',
        description: 'HTTP reference',
        version: '0.1.0',
        default_policy: {
          timeout_ms: 10_000,
          retry_attempts: 0,
          backoff_base_ms: 100,
          max_response_bytes: 1024 * 1024,
        },
      },
    ];
    const c = controllerClient('http://x.example', {
      fetchImpl: fakeFetch((u) => {
        url = String(u);
        return jsonResponse(payload);
      }),
    });
    const got = await c.listConnectors();
    expect(url).toBe('http://x.example/connectors');
    expect(got).toHaveLength(1);
    expect(got[0].name).toBe('http');
    expect(got[0].default_policy.timeout_ms).toBe(10_000);
  });
});
