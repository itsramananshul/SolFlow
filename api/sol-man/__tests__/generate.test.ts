/**
 * Tests for the /api/sol-man/generate handler — the strict-retry +
 * repair + structured-error path that the reliability hardening pass
 * landed.
 *
 * Strategy: monkey-patch a single provider's `call()` in the
 * registry to return scripted text per attempt. The real handler
 * runs end-to-end (resolveProvider → call → repairJson →
 * validateSpec → response envelope). No real network.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import handler from '../generate';
import { PROVIDERS } from '../_providers';

interface FakeRes {
  statusCode: number;
  body: unknown;
  status(code: number): FakeRes;
  json(payload: unknown): FakeRes;
}

function mockRes(): FakeRes {
  const res: FakeRes = {
    statusCode: 0,
    body: null,
    status(code) {
      this.statusCode = code;
      return this;
    },
    json(payload) {
      this.body = payload;
      return this;
    },
  };
  return res;
}

interface FakeReq {
  method: string;
  body: unknown;
}
function mockReq(body: unknown): FakeReq {
  return { method: 'POST', body };
}

// Minimal valid GeneratedGraphSpec — a single Manual trigger.
const VALID_SPEC_JSON = JSON.stringify({
  meta: { name: 'unit-test', description: 'spec for the test' },
  nodes: [{ id: 'n1', kind: 'trigger', triggerKind: 'manual' }],
  edges: [],
});

let originalCall: typeof PROVIDERS.openai.call;
let scripted: string[];
let calls: number;

beforeEach(() => {
  scripted = [];
  calls = 0;
  // Replace OpenAI's call with a deterministic scripted version.
  originalCall = PROVIDERS.openai.call;
  PROVIDERS.openai.call = vi.fn().mockImplementation(async () => {
    const text = scripted[calls] ?? scripted[scripted.length - 1] ?? '';
    calls++;
    return { text };
  });
  // Provide a fake env so resolveProvider picks OpenAI without env contamination.
  process.env.OPENAI_API_KEY = 'test-key';
  process.env.ANTHROPIC_API_KEY = '';
  process.env.GEMINI_API_KEY = '';
  process.env.GROK_API_KEY = '';
  process.env.OPENROUTER_API_KEY = '';
  process.env.SOL_MAN_PROVIDER = 'openai';
  process.env.SOL_MAN_MODEL = 'gpt-4o';
  process.env.SOL_MAN_STRICT_RETRIES = '1';
});

afterEach(() => {
  PROVIDERS.openai.call = originalCall;
  delete process.env.OPENAI_API_KEY;
  delete process.env.SOL_MAN_PROVIDER;
  delete process.env.SOL_MAN_MODEL;
  delete process.env.SOL_MAN_STRICT_RETRIES;
});

describe('/api/sol-man/generate — success', () => {
  it('returns the validated spec when the provider returns clean JSON', async () => {
    scripted = [VALID_SPEC_JSON];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(200);
    const body = res.body as { ok: boolean; attempts: number; repairApplied: boolean };
    expect(body.ok).toBe(true);
    expect(body.attempts).toBe(1);
    expect(body.repairApplied).toBe(false);
    expect(calls).toBe(1);
  });

  it('marks repairApplied=true when the model returns fenced JSON', async () => {
    scripted = ['```json\n' + VALID_SPEC_JSON + '\n```'];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(200);
    const body = res.body as { ok: boolean; repairApplied: boolean };
    expect(body.ok).toBe(true);
    expect(body.repairApplied).toBe(true);
  });
});

describe('/api/sol-man/generate — strict retry path', () => {
  it('retries on unparseable output, then succeeds on the second attempt', async () => {
    scripted = [
      'I cannot generate that workflow. Sorry.',
      VALID_SPEC_JSON,
    ];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(200);
    const body = res.body as { ok: boolean; attempts: number };
    expect(body.ok).toBe(true);
    expect(body.attempts).toBe(2);
    expect(calls).toBe(2);
  });

  it('retries on validation failure and surfaces invalid_json after retries exhausted', async () => {
    // Both attempts produce JSON that fails schema validation
    // (missing trigger node).
    scripted = [
      JSON.stringify({
        meta: { name: 'x', description: '' },
        nodes: [{ id: 'n1', kind: 'print', value: '"hi"' }],
        edges: [],
      }),
      JSON.stringify({
        meta: { name: 'x', description: '' },
        nodes: [{ id: 'n1', kind: 'print', value: '"hi"' }],
        edges: [],
      }),
    ];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(502);
    const body = res.body as {
      ok: boolean;
      kind: string;
      attempts: number;
      retryable: boolean;
    };
    expect(body.ok).toBe(false);
    expect(body.kind).toBe('validation_failed');
    expect(body.attempts).toBe(2);
    expect(body.retryable).toBe(true);
  });

  it('returns 504 with kind=gateway_timeout when the provider keeps timing out', async () => {
    PROVIDERS.openai.call = vi.fn().mockImplementation(async () => {
      calls++;
      throw new Error('OpenAI timed out after 25s');
    });
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(504);
    const body = res.body as { ok: boolean; kind: string; retryable: boolean; attempts: number };
    expect(body.ok).toBe(false);
    expect(body.kind).toBe('gateway_timeout');
    expect(body.retryable).toBe(true);
    expect(body.attempts).toBe(2);
  });

  it('does NOT retry on auth-style provider errors (non-transient)', async () => {
    PROVIDERS.openai.call = vi.fn().mockImplementation(async () => {
      calls++;
      throw new Error('OpenAI 401: invalid_api_key');
    });
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(502);
    const body = res.body as { ok: boolean; kind: string; retryable: boolean; attempts: number };
    expect(body.ok).toBe(false);
    expect(body.kind).toBe('provider_error');
    expect(body.retryable).toBe(false);
    expect(body.attempts).toBe(1);
  });
});

describe('/api/sol-man/generate — request validation', () => {
  it('rejects empty prompt with kind=bad_request', async () => {
    const req = mockReq({ prompt: '' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(400);
    const body = res.body as { ok: boolean; kind: string };
    expect(body.kind).toBe('bad_request');
  });

  it('rejects non-POST methods', async () => {
    const req = { method: 'GET', body: {} };
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(405);
  });

  it('returns kind=config_missing when no provider key is set', async () => {
    delete process.env.OPENAI_API_KEY;
    delete process.env.SOL_MAN_PROVIDER;
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(503);
    const body = res.body as {
      ok: boolean;
      kind: string;
      configMissing: boolean;
      availableProviders: Array<{ id: string }>;
    };
    expect(body.kind).toBe('config_missing');
    expect(body.configMissing).toBe(true);
    expect(body.availableProviders.length).toBeGreaterThan(0);
  });
});

describe('/api/sol-man/generate — empty response', () => {
  it('retries on empty completion + bails with kind=empty_response if it persists', async () => {
    scripted = ['', ''];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(502);
    const body = res.body as { ok: boolean; kind: string; attempts: number };
    expect(body.kind).toBe('empty_response');
    expect(body.attempts).toBe(2);
  });

  it('succeeds when the retry yields content after an empty first attempt', async () => {
    scripted = ['', VALID_SPEC_JSON];
    const req = mockReq({ prompt: 'demo' });
    const res = mockRes();
    await handler(req as never, res as never);
    expect(res.statusCode).toBe(200);
    const body = res.body as { ok: boolean; attempts: number };
    expect(body.ok).toBe(true);
    expect(body.attempts).toBe(2);
  });
});
