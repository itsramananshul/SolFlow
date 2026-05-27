/**
 * `useControllerRunHistoryStore` coverage (Phase C C.2 c66).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useControllerRunHistoryStore } from '../controller-run-history.store';

beforeEach(() => {
  setActivePinia(createPinia());
  const mem = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => mem.get(k) ?? null,
    setItem: (k: string, v: string) => mem.set(k, v),
    removeItem: (k: string) => mem.delete(k),
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const URL_A = 'http://a.example';
const URL_B = 'http://b.example';

describe('useControllerRunHistoryStore', () => {
  it('listFor returns empty for unknown URLs', () => {
    const h = useControllerRunHistoryStore();
    expect(h.listFor(URL_A)).toEqual([]);
  });

  it('record prepends most-recent first', () => {
    const h = useControllerRunHistoryStore();
    h.record({
      controllerUrl: URL_A,
      workflowId: 'wf_1', runId: 'run_1', workflowName: 'a',
      status: 'Succeeded', durationMs: 10, submittedAt: 1,
    });
    h.record({
      controllerUrl: URL_A,
      workflowId: 'wf_2', runId: 'run_2', workflowName: 'b',
      status: 'Failed', durationMs: 20, submittedAt: 2,
    });
    const got = h.listFor(URL_A);
    expect(got).toHaveLength(2);
    expect(got[0].runId).toBe('run_2');
    expect(got[1].runId).toBe('run_1');
  });

  it('record de-dupes on runId (latest replaces)', () => {
    const h = useControllerRunHistoryStore();
    h.record({
      controllerUrl: URL_A,
      workflowId: 'wf_1', runId: 'run_1', workflowName: 'a',
      status: 'Queued', durationMs: null, submittedAt: 1,
    });
    h.record({
      controllerUrl: URL_A,
      workflowId: 'wf_1', runId: 'run_1', workflowName: 'a',
      status: 'Succeeded', durationMs: 42, submittedAt: 1,
    });
    const got = h.listFor(URL_A);
    expect(got).toHaveLength(1);
    expect(got[0].status).toBe('Succeeded');
    expect(got[0].durationMs).toBe(42);
  });

  it('record caps at 20 entries per URL (FIFO eviction)', () => {
    const h = useControllerRunHistoryStore();
    for (let i = 0; i < 25; i++) {
      h.record({
        controllerUrl: URL_A,
        workflowId: `wf_${i}`,
        runId: `run_${i}`,
        workflowName: `n${i}`,
        status: 'Succeeded',
        durationMs: i,
        submittedAt: i,
      });
    }
    const got = h.listFor(URL_A);
    expect(got).toHaveLength(20);
    // Most-recent first: run_24 head, run_5 tail.
    expect(got[0].runId).toBe('run_24');
    expect(got[19].runId).toBe('run_5');
  });

  it('URLs are isolated from each other', () => {
    const h = useControllerRunHistoryStore();
    h.record({
      controllerUrl: URL_A, workflowId: 'wf_a', runId: 'run_a',
      workflowName: 'a', status: 'Succeeded', durationMs: 1, submittedAt: 1,
    });
    h.record({
      controllerUrl: URL_B, workflowId: 'wf_b', runId: 'run_b',
      workflowName: 'b', status: 'Failed', durationMs: 1, submittedAt: 1,
    });
    expect(h.listFor(URL_A)).toHaveLength(1);
    expect(h.listFor(URL_B)).toHaveLength(1);
    expect(h.listFor(URL_A)[0].runId).toBe('run_a');
  });

  it('update patches only the matching runId', () => {
    const h = useControllerRunHistoryStore();
    h.record({
      controllerUrl: URL_A, workflowId: 'wf_1', runId: 'run_1',
      workflowName: 'a', status: 'Queued', durationMs: null, submittedAt: 1,
    });
    h.record({
      controllerUrl: URL_A, workflowId: 'wf_2', runId: 'run_2',
      workflowName: 'b', status: 'Queued', durationMs: null, submittedAt: 2,
    });
    h.update(URL_A, 'run_1', { status: 'Succeeded', durationMs: 99 });
    const got = h.listFor(URL_A);
    const r1 = got.find((e) => e.runId === 'run_1')!;
    const r2 = got.find((e) => e.runId === 'run_2')!;
    expect(r1.status).toBe('Succeeded');
    expect(r1.durationMs).toBe(99);
    expect(r2.status).toBe('Queued');
  });

  it('clearFor removes only the targeted URL', () => {
    const h = useControllerRunHistoryStore();
    h.record({
      controllerUrl: URL_A, workflowId: 'wf_a', runId: 'run_a',
      workflowName: 'a', status: 'Succeeded', durationMs: 1, submittedAt: 1,
    });
    h.record({
      controllerUrl: URL_B, workflowId: 'wf_b', runId: 'run_b',
      workflowName: 'b', status: 'Succeeded', durationMs: 1, submittedAt: 1,
    });
    h.clearFor(URL_A);
    expect(h.listFor(URL_A)).toEqual([]);
    expect(h.listFor(URL_B)).toHaveLength(1);
  });

  it('survives a fresh store instance via localStorage', () => {
    const h1 = useControllerRunHistoryStore();
    h1.record({
      controllerUrl: URL_A, workflowId: 'wf_1', runId: 'run_1',
      workflowName: 'a', status: 'Succeeded', durationMs: 10, submittedAt: 1,
    });
    // Tear down pinia + rebuild; localStorage is the only carrier.
    setActivePinia(createPinia());
    const h2 = useControllerRunHistoryStore();
    expect(h2.listFor(URL_A)).toHaveLength(1);
    expect(h2.listFor(URL_A)[0].runId).toBe('run_1');
  });
});
