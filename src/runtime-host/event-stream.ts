/**
 * Typed `RunEvent` stream client (Phase C C.5 c84).
 *
 * Thin wrapper around the browser `EventSource` API for the
 * controller's `GET /runs/:id/events` Server-Sent Events
 * endpoint. Handles:
 *
 *   - per-kind `addEventListener` registration (the server tags
 *     each message with the event's `kind` so clients can
 *     dispatch typed listeners; we register one listener per
 *     kind and call back through a single `onEvent` callback)
 *   - auto-resume via `Last-Event-ID` (EventSource sets this
 *     header automatically across reconnects; the server's
 *     `?after=N` query takes precedence)
 *   - explicit close + done signal when a terminal event
 *     (Completed / Failed / Cancelled) arrives
 *
 * The implementation is dep-free and Vue-agnostic so the
 * editor's stores + components can compose it freely.
 *
 * Browser-only by default — pass a mock `EventSourceCtor` in
 * tests (the Node EventSource shape matches the browser's).
 */
import type { RunEvent } from './types';

/** Every RunEvent.kind the controller emits today. The SSE
 *  client adds a listener for each so callbacks fire regardless
 *  of which kind was tagged. Update this list when adding new
 *  variants. */
const KNOWN_KINDS = [
  'Queued',
  'Started',
  'Print',
  'ExtCallStarted',
  'ExtCallCompleted',
  'Diagnostic',
  'Completed',
  'Failed',
  'Cancelled',
  // Phase C C.6 — lifecycle expansion.
  'Starting',
  'Cancelling',
  'Rejected',
  'TimedOut',
] as const;
type KnownKind = (typeof KNOWN_KINDS)[number];

const TERMINAL_KINDS: ReadonlySet<KnownKind> = new Set([
  'Completed',
  'Failed',
  'Cancelled',
  // Terminals added in C.6.
  'Rejected',
  'TimedOut',
]);

/** Minimal type-shape we need from the EventSource constructor
 *  — exposing this lets tests inject a fake. */
export type EventSourceCtor = new (
  url: string | URL,
  init?: EventSourceInit,
) => EventSourceLike;

/** Subset of the DOM `EventSource` we actually use. */
export interface EventSourceLike {
  addEventListener(
    kind: string,
    handler: (e: MessageEventLike) => void,
  ): void;
  removeEventListener(
    kind: string,
    handler: (e: MessageEventLike) => void,
  ): void;
  readonly readyState: number;
  onerror: ((e: unknown) => void) | null;
  close(): void;
}

/** Subset of the DOM `MessageEvent`. */
export interface MessageEventLike {
  readonly data: string;
  readonly lastEventId?: string;
}

export interface OpenRunEventStreamOptions {
  /** Controller base URL (no trailing slash). */
  baseUrl: string;
  /** Run id to subscribe to. */
  runId: string;
  /** Optional `?after=N` — server replays events with `seq > N`.
   *  Omitted = replay everything from start. */
  afterSeq?: number;
  /** Fires for every received event, after parsing + kind tag
   *  reconciliation. */
  onEvent: (event: RunEvent) => void;
  /** Fires on transport error (network drop, parse failure).
   *  EventSource auto-reconnects; this callback is informational. */
  onError?: (err: Error) => void;
  /** Fires exactly once when the stream ends — either on a
   *  terminal event (Completed/Failed/Cancelled) or because the
   *  caller invoked `close()`. */
  onDone?: (reason: 'terminal' | 'closed') => void;
  /** Test seam: inject a mock EventSource constructor. */
  eventSourceCtor?: EventSourceCtor;
}

export interface RunEventStreamHandle {
  /** Stop the stream + fire `onDone('closed')` if not already done. */
  close(): void;
  /** True after a terminal event was received OR `close()` was called. */
  readonly isDone: boolean;
}

/**
 * Open a run-event stream. Returns a handle the caller can use
 * to stop the stream explicitly (e.g. on Vue component unmount).
 */
export function openRunEventStream(
  opts: OpenRunEventStreamOptions,
): RunEventStreamHandle {
  const base = opts.baseUrl.replace(/\/+$/, '');
  const params = new URLSearchParams();
  if (opts.afterSeq !== undefined) {
    params.set('after', String(opts.afterSeq));
  }
  const url =
    `${base}/runs/${encodeURIComponent(opts.runId)}/events`
    + (params.toString() ? `?${params}` : '');

  const ctor: EventSourceCtor =
    opts.eventSourceCtor
    ?? (globalThis as { EventSource?: EventSourceCtor }).EventSource!;
  if (typeof ctor !== 'function') {
    throw new Error('EventSource not available — pass eventSourceCtor');
  }

  let done = false;
  let closed = false;
  const es = new ctor(url);

  const finish = (reason: 'terminal' | 'closed') => {
    if (done) return;
    done = true;
    try {
      es.close();
    } catch {
      /* ignore */
    }
    opts.onDone?.(reason);
  };

  // Per-kind listeners. EventSource dispatches on the SSE
  // `event:` line value; we register one per known kind so any
  // tagged event reaches `onEvent`.
  const listeners = new Map<string, (e: MessageEventLike) => void>();
  for (const kind of KNOWN_KINDS) {
    const handler = (e: MessageEventLike) => {
      let parsed: RunEvent;
      try {
        parsed = JSON.parse(e.data) as RunEvent;
      } catch (err) {
        opts.onError?.(
          new Error(
            `failed to parse SSE event payload (kind=${kind}): ${err instanceof Error ? err.message : String(err)}`,
          ),
        );
        return;
      }
      opts.onEvent(parsed);
      if (TERMINAL_KINDS.has(kind)) {
        finish('terminal');
      }
    };
    es.addEventListener(kind, handler);
    listeners.set(kind, handler);
  }

  es.onerror = (e) => {
    // The server closes the connection on terminal events; for
    // EventSource that surfaces as a transport error. Treat it
    // as benign if we already saw a terminal event (done=true);
    // otherwise pipe through.
    if (done) return;
    opts.onError?.(
      e instanceof Error
        ? e
        : new Error('SSE transport error'),
    );
  };

  return {
    close() {
      if (closed) return;
      closed = true;
      finish('closed');
    },
    get isDone() {
      return done;
    },
  };
}
