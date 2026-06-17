/**
 * Unit tests for the real OpenPrem controller client.
 *
 * Hermetic: a fake `fetch` simulates the controller's
 * POST /workflow + GET /workflow/:id protocol (verified live against
 * `openprem-controller-v2` during development). No network needed.
 */
import { describe, expect, it } from 'vitest';
import { opremClient, OpremClientErr } from '../opremClient';

/** Build a fake fetch that walks a workflow through running -> terminal. */
function fakeController(opts: {
  pollsBeforeTerminal?: number;
  terminal: { status: 'completed' | 'error'; result?: unknown; error?: string };
  submitStatus?: number;
  submitBody?: string;
}): typeof fetch {
  let polls = 0;
  const id = 'wf-123';
  return (async (url: string, init?: RequestInit) => {
    const u = String(url);
    const method = init?.method ?? 'GET';
    if (method === 'POST' && u.endsWith('/workflow')) {
      const status = opts.submitStatus ?? 200;
      const body =
        opts.submitBody ?? JSON.stringify({ ok: true, workflow_id: id, status: 'running' });
      return new Response(body, { status, headers: { 'content-type': 'application/json' } });
    }
    if (method === 'GET' && u.includes(`/workflow/${id}`)) {
      polls += 1;
      const done = polls > (opts.pollsBeforeTerminal ?? 0);
      const payload = done
        ? {
            status: opts.terminal.status,
            workflow_name: 'main',
            source: 'workflow "main" { return 0; }',
            result: opts.terminal.result ?? null,
            progress: '5/5',
            step_count: 5,
            error: opts.terminal.error ?? null,
          }
        : {
            status: 'running',
            workflow_name: 'main',
            source: 'workflow "main" { return 0; }',
            result: null,
            progress: '2/5',
            step_count: 2,
            error: null,
          };
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }
    return new Response('not found', { status: 404 });
  }) as unknown as typeof fetch;
}

describe('opremClient — submit + poll', () => {
  it('runs a workflow to completion and returns the result', async () => {
    const client = opremClient('http://127.0.0.1:17321', {
      fetchImpl: fakeController({
        pollsBeforeTerminal: 1,
        terminal: { status: 'completed', result: 42 },
      }),
    });
    const outcome = await client.runWorkflow('workflow "main" { return 42; }', 'main', {
      intervalMs: 1,
    });
    expect(outcome.status).toBe('completed');
    expect(outcome.result).toBe(42);
    expect(outcome.error).toBeNull();
    expect(outcome.stepCount).toBe(5);
    expect(outcome.workflowId).toBe('wf-123');
  });

  it('surfaces a controller run error as a terminal error outcome', async () => {
    const client = opremClient('http://127.0.0.1:17321', {
      fetchImpl: fakeController({
        terminal: { status: 'error', error: "capability 'x.y' not found" },
      }),
    });
    const outcome = await client.runWorkflow('workflow "main" { return 0; }', 'main', {
      intervalMs: 1,
    });
    expect(outcome.status).toBe('error');
    expect(outcome.error).toBe("capability 'x.y' not found");
    expect(outcome.result).toBeNull();
  });
});

describe('opremClient — submit failures', () => {
  it('throws a structured http error on a non-2xx submit', async () => {
    const client = opremClient('http://127.0.0.1:17321', {
      fetchImpl: fakeController({
        submitStatus: 400,
        submitBody: 'SOL analysis failed: no workflow found',
        terminal: { status: 'completed' },
      }),
    });
    await expect(
      client.submitWorkflow('not valid', 'main'),
    ).rejects.toMatchObject({
      payload: { kind: 'http', status: 400 },
    });
  });

  it('rejects an invalid base URL at construction', () => {
    expect(() => opremClient('not-a-url')).toThrow(OpremClientErr);
  });
});

describe('opremClient — submit response validation', () => {
  it('throws decode when submit omits workflow_id', async () => {
    const client = opremClient('http://127.0.0.1:17321', {
      fetchImpl: fakeController({
        submitBody: JSON.stringify({ ok: true, status: 'running' }),
        terminal: { status: 'completed' },
      }),
    });
    await expect(client.submitWorkflow('x', 'main')).rejects.toMatchObject({
      payload: { kind: 'decode' },
    });
  });
});
