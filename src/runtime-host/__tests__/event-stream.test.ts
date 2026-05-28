/**
 * Vitest coverage for the SSE event-stream client (C.5 c84).
 *
 * Strategy: inject a tiny FakeEventSource that records the URL
 * it was constructed with + lets the test push events
 * synchronously to the registered listeners. No real network,
 * no timing.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  openRunEventStream,
  type EventSourceCtor,
  type EventSourceLike,
  type MessageEventLike,
} from '../event-stream';
import type { RunEvent } from '../types';

class FakeEventSource implements EventSourceLike {
  /** Captures every constructed instance so tests can poke at them. */
  static last: FakeEventSource | null = null;
  static lastUrl: string | URL | null = null;
  readonly listeners = new Map<string, Array<(e: MessageEventLike) => void>>();
  readyState = 0;
  onerror: ((e: unknown) => void) | null = null;
  closed = false;
  constructor(url: string | URL) {
    FakeEventSource.lastUrl = url;
    FakeEventSource.last = this;
  }
  addEventListener(kind: string, handler: (e: MessageEventLike) => void): void {
    const list = this.listeners.get(kind) ?? [];
    list.push(handler);
    this.listeners.set(kind, list);
  }
  removeEventListener(kind: string, handler: (e: MessageEventLike) => void): void {
    const list = this.listeners.get(kind);
    if (!list) return;
    this.listeners.set(
      kind,
      list.filter((h) => h !== handler),
    );
  }
  close(): void {
    this.closed = true;
  }
  /** Test helper: push an event payload to every listener for `kind`. */
  emit(kind: string, data: unknown): void {
    const list = this.listeners.get(kind) ?? [];
    const msg: MessageEventLike = { data: JSON.stringify(data) };
    for (const h of list) h(msg);
  }
}

/** Class is constructable directly. Cast away the EventSource
 *  shape since FakeEventSource only implements the slice the
 *  client uses. */
const FakeCtor = FakeEventSource as unknown as EventSourceCtor;

afterEach(() => {
  FakeEventSource.lastUrl = null;
  FakeEventSource.last = null;
});

const sampleQueued = (runId: string, seq = 0): RunEvent => ({
  kind: 'Queued',
  run_id: runId,
  seq,
  ts: seq,
});

const sampleCompleted = (runId: string, seq: number): RunEvent => ({
  kind: 'Completed',
  run_id: runId,
  seq,
  ts: seq,
  output: { return_value: 0, output: [], steps: 1 },
});

describe('openRunEventStream', () => {
  it('constructs an EventSource for the right URL (no after)', () => {
    const handle = openRunEventStream({
      baseUrl: 'http://x.example/',
      runId: 'run_a',
      onEvent: vi.fn(),
      eventSourceCtor: FakeCtor,
    });
    expect(FakeEventSource.last).not.toBeNull();
    expect(FakeEventSource.lastUrl).toBe('http://x.example/runs/run_a/events');
    handle.close();
  });

  it('includes ?after=N when supplied', () => {
    openRunEventStream({
      baseUrl: 'http://x.example',
      runId: 'run_a',
      afterSeq: 12,
      onEvent: vi.fn(),
      eventSourceCtor: FakeCtor,
    });
    expect(FakeEventSource.lastUrl).toBe('http://x.example/runs/run_a/events?after=12');
  });

  it('forwards parsed events to onEvent', () => {
    const events: RunEvent[] = [];
    openRunEventStream({
      baseUrl: 'http://x.example',
      runId: 'run_a',
      onEvent: (e) => events.push(e),
      eventSourceCtor: FakeCtor,
    });
    const es = FakeEventSource.last!;
    es.emit('Queued', sampleQueued('run_a'));
    es.emit('Print', {
      kind: 'Print', run_id: 'run_a', seq: 1, ts: 1, text: 'hi',
    });
    expect(events).toHaveLength(2);
    expect(events[0].kind).toBe('Queued');
    expect(events[1].kind).toBe('Print');
  });

  it('fires onDone("terminal") + closes the source on a terminal event', () => {
    const onDone = vi.fn();
    const handle = openRunEventStream({
      baseUrl: 'http://x.example',
      runId: 'run_a',
      onEvent: vi.fn(),
      onDone,
      eventSourceCtor: FakeCtor,
    });
    const es = FakeEventSource.last!;
    es.emit('Started', {
      kind: 'Started', run_id: 'run_a', seq: 0, ts: 0,
    });
    expect(handle.isDone).toBe(false);
    expect(es.closed).toBe(false);
    es.emit('Completed', sampleCompleted('run_a', 1));
    expect(handle.isDone).toBe(true);
    expect(es.closed).toBe(true);
    expect(onDone).toHaveBeenCalledWith('terminal');
  });

  it('close() fires onDone("closed") and stops the source', () => {
    const onDone = vi.fn();
    const handle = openRunEventStream({
      baseUrl: 'http://x.example',
      runId: 'run_a',
      onEvent: vi.fn(),
      onDone,
      eventSourceCtor: FakeCtor,
    });
    const es = FakeEventSource.last!;
    handle.close();
    expect(handle.isDone).toBe(true);
    expect(es.closed).toBe(true);
    expect(onDone).toHaveBeenCalledWith('closed');
    // Idempotent.
    handle.close();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('onError fires on bad-JSON payload, then continues', () => {
    const onError = vi.fn();
    const events: RunEvent[] = [];
    openRunEventStream({
      baseUrl: 'http://x.example',
      runId: 'run_a',
      onEvent: (e) => events.push(e),
      onError,
      eventSourceCtor: FakeCtor,
    });
    const es = FakeEventSource.last!;
    // Bypass FakeEventSource.emit (which JSON.stringifies)
    // and push a raw bad payload directly.
    const handler = es.listeners.get('Print')![0];
    handler({ data: '{not json}' });
    expect(onError).toHaveBeenCalled();
    es.emit('Print', {
      kind: 'Print', run_id: 'run_a', seq: 0, ts: 0, text: 'ok',
    });
    expect(events).toHaveLength(1);
  });

  it('throws when no EventSource is available + none injected', () => {
    const prev = (globalThis as { EventSource?: unknown }).EventSource;
    delete (globalThis as { EventSource?: unknown }).EventSource;
    try {
      expect(() =>
        openRunEventStream({
          baseUrl: 'http://x.example',
          runId: 'run_a',
          onEvent: vi.fn(),
        }),
      ).toThrow(/EventSource not available/);
    } finally {
      if (prev !== undefined) {
        (globalThis as { EventSource?: unknown }).EventSource = prev;
      }
    }
  });
});
