/**
 * Tests for the Sol Man API client.
 *
 * Coverage: every classification path the user can hit. The headline
 * regression test is the HTML-504 case — the original bug was "Sol
 * Man returned a non-JSON response (HTTP 504)", which collapsed
 * gateway timeouts, HTML error pages, and provider failures into
 * one unhelpful message.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { callSolMan } from '../client';
import type { GenerateResponseBody } from '../types';

function htmlResponse(body: string, status = 504): Response {
  return new Response(body, {
    status,
    headers: { 'content-type': 'text/html; charset=utf-8' },
  });
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('callSolMan — happy path', () => {
  it('returns the parsed envelope when the server speaks JSON', async () => {
    const happy: GenerateResponseBody = {
      ok: true,
      spec: {
        meta: { name: 'demo', description: '' },
        nodes: [{ id: 'n1', kind: 'trigger' }],
        edges: [],
      },
      model: 'gpt-4o',
    };
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse(happy));
    const out = await callSolMan('test', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(true);
    if (out.ok) {
      expect(out.spec.nodes[0].id).toBe('n1');
      expect(out.model).toBe('gpt-4o');
    }
  });

  it('forwards inline BYO config in the request body', async () => {
    let captured: RequestInit | undefined;
    const fetchImpl = vi.fn().mockImplementation((_url, init) => {
      captured = init;
      return Promise.resolve(jsonResponse({ ok: false, error: 'x' }));
    });
    await callSolMan(
      'test',
      { providerId: 'anthropic', apiKey: 'k', model: 'claude' },
      { fetchImpl: fetchImpl as unknown as typeof fetch },
    );
    const body = JSON.parse(String(captured?.body));
    expect(body.config.providerId).toBe('anthropic');
    expect(body.config.apiKey).toBe('k');
  });
});

describe('callSolMan — HTML 504 from the platform edge (the headline bug)', () => {
  it('classifies HTML-504 as gateway_timeout, NOT "non-JSON response"', async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      htmlResponse(
        '<!DOCTYPE html><html><body><h1>Gateway Timeout</h1></body></html>',
        504,
      ),
    );
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('gateway_timeout');
      expect(out.retryable).toBe(true);
      expect(out.stage).toBe('provider_call');
      // The user-visible message is informative, not "non-JSON".
      expect(out.error).not.toMatch(/non-JSON/);
      expect(out.error).toMatch(/timed out|gateway|retry/i);
      expect(out.details?.httpStatus).toBe(504);
    }
  });

  it('classifies an HTML 502 (provider down) as retryable gateway_timeout-ish', async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      htmlResponse('<html>Bad Gateway</html>', 502),
    );
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('gateway_timeout');
      expect(out.retryable).toBe(true);
      expect(out.details?.httpStatus).toBe(502);
    }
  });

  it('classifies non-2xx HTML below 500 as non-retryable unknown', async () => {
    const fetchImpl = vi.fn().mockResolvedValue(htmlResponse('<html>nope</html>', 404));
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.retryable).toBe(false);
      expect(out.details?.httpStatus).toBe(404);
    }
  });
});

describe('callSolMan — timeouts + network errors', () => {
  it('returns kind=gateway_timeout when the client-side AbortController fires', async () => {
    // Fake fetch that listens to the abort signal but never resolves.
    const fetchImpl = vi.fn().mockImplementation((_url, init) =>
      new Promise((_resolve, reject) => {
        const signal = (init as RequestInit | undefined)?.signal;
        signal?.addEventListener('abort', () => {
          const err = new Error('aborted');
          (err as Error & { name: string }).name = 'TimeoutError';
          reject(err);
        });
      }),
    );
    const promise = callSolMan('demo', null, {
      fetchImpl: fetchImpl as unknown as typeof fetch,
      timeoutMs: 25,
    });
    // Attach the assertion before advancing timers so the rejection
    // isn't briefly unhandled.
    const expectation = expect(promise).resolves.toMatchObject({
      ok: false,
      kind: 'gateway_timeout',
      retryable: true,
    });
    await vi.advanceTimersByTimeAsync(50);
    await expectation;
  });

  it('maps real network failure to kind=network', async () => {
    const fetchImpl = vi.fn().mockRejectedValue(new TypeError('failed to fetch'));
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('network');
      expect(out.retryable).toBe(true);
    }
  });

  it('respects an external abort signal with kind=unknown / retryable=false', async () => {
    const ctl = new AbortController();
    const fetchImpl = vi.fn().mockImplementation((_url, init) =>
      new Promise((_resolve, reject) => {
        const signal = (init as RequestInit | undefined)?.signal;
        signal?.addEventListener('abort', () => {
          const err = new Error('aborted');
          (err as Error & { name: string }).name = 'AbortError';
          reject(err);
        });
      }),
    );
    const promise = callSolMan('demo', null, {
      fetchImpl: fetchImpl as unknown as typeof fetch,
      signal: ctl.signal,
    });
    ctl.abort();
    const out = await promise;
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('unknown');
      expect(out.retryable).toBe(false);
    }
  });
});

describe('callSolMan — server error envelope passthrough', () => {
  it('passes through a structured error envelope unchanged (config_missing)', async () => {
    const envelope: GenerateResponseBody = {
      ok: false,
      error: 'no provider configured',
      kind: 'config_missing',
      stage: 'provider_resolution',
      retryable: false,
      configMissing: true,
      availableProviders: [],
    };
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse(envelope, 503));
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('config_missing');
      expect(out.configMissing).toBe(true);
    }
  });

  it('passes through validation_failed envelope from the server', async () => {
    const envelope: GenerateResponseBody = {
      ok: false,
      error: 'spec must include at least one trigger',
      kind: 'validation_failed',
      stage: 'spec_validation',
      retryable: true,
      attempts: 2,
    };
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse(envelope, 502));
    const out = await callSolMan('demo', null, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.kind).toBe('validation_failed');
      expect(out.attempts).toBe(2);
    }
  });
});
