# Run events + observability

**Phase C C.5 (shipped 2026-05-27).** Every run on the controller
emits a stream of structured `RunEvent`s — persisted in SQLite
(`run_events` table) AND broadcast on an in-process channel the
editor's SSE client subscribes to for real-time UX.

## Event types

`RunEvent` is a serde-tagged union (`tag = "kind"`). All variants
carry `run_id` (the run), `seq` (monotonic per-run, starts at 0),
and `ts` (ms since UNIX epoch).

| Kind | Fired when | Extra fields |
|---|---|---|
| `Queued` | execute_run begins | — |
| `Started` | Running status written, before VM starts | — |
| `Print` | every `print(...)` instruction (each line) | `text`, `source_span?` |
| `ExtCallStarted` | before connector.invoke | `connector`, `fn_name` |
| `ExtCallCompleted` | after connector.invoke (success or error) | `connector`, `fn_name`, `ok: bool` |
| `Diagnostic` | reserved for future stream-time diagnostics | `diagnostic: SolDiagnostic` |
| `Completed` | run finished without runtime error | `output: RunOutput` |
| `Failed` | run finished with runtime error | `error: RuntimeErrorView`, `source_span?` |
| `Cancelled` | (C.6) cancellation acknowledged | — |

`Completed` / `Failed` / `Cancelled` are **terminal** — the SSE
endpoint closes the stream after sending one. The TS client's
`onDone('terminal')` fires.

## Architecture

```
   execute_run (async tokio task)
   ───────────────────────────────
   ctx.emit(Queued)
   record.status = Running; persist
   ctx.emit(Started)
        │
        ▼
   VM (spawn_blocking)            ExtCallHandler          tokio runtime
   ────────────────────           ──────────────          ─────────────
   PrintInt/PrintString/...
      → emit_print(line)
         → print_callback           ExtCallStarted spawn_emit
            spawn_emit(Print)         connector.invoke (await via block_on)
                                    ExtCallCompleted spawn_emit
        │                            │
        ▼                            ▼
                EventSink::emit(event)
                ────────────────────
                ├── persistence.append_event (INSERT)
                └── broadcast.send (in-process)
                       │
                       ▼
                SSE subscribers
```

Three emit paths, one shared `Arc<AtomicU64>` seq counter — events
are monotonically ordered across sources (VM print + handler
ExtCall + executor lifecycle).

### Persistence — `run_events` table

```sql
CREATE TABLE run_events (
    run_id        TEXT NOT NULL REFERENCES runs(id),
    seq           INTEGER NOT NULL,
    ts            INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    PRIMARY KEY (run_id, seq)
);
CREATE INDEX idx_run_events_ts ON run_events(ts DESC);
```

`payload_json` is the full serde-serialized event — denormalized
so new variants don't need schema migrations. Composite PK gives
the SSE replay query an ASC index for free.

### Broadcast — `tokio::sync::broadcast`

`PersistentEventSink` holds a 1024-event ring. Slow subscribers
get a `Lagged(count)` error from `recv()`; the SSE handler
recovers by re-querying the persistent log starting from the
last seq it sent.

The broadcast carries **every** run's events; the SSE handler
filters in-memory by `event.run_id() == run_id`. With one subscriber
per active run this is cheap.

## HTTP API

### `GET /runs/:id/events`

Server-Sent Events stream.

**Query params:**

| Param | Meaning |
|---|---|
| `after=N` | Resume strictly after `seq > N`. Omitted = replay from the very first event. |

**Response:** `Content-Type: text/event-stream`. One SSE message per event:

```
event: Print
id: 5
data: {"kind":"Print","run_id":"run_abc","seq":5,"ts":1779999999000,"text":"hello","source_span":null}
```

The `event:` field carries the event's `kind` (for `addEventListener` dispatch).
The `id:` field carries `seq` (so the browser's `EventSource` reconnect
auto-sends `Last-Event-ID` and the server resumes from there).

**Lifecycle:**

1. **Replay** — every persisted event with `seq > after` (or all
   if `after` is omitted), in ASC order.
2. **Live** — subscribe to the in-process broadcast and forward
   matching-run events as they emit.
3. **Terminal close** — after `Completed` / `Failed` / `Cancelled`,
   the stream ends.

**Lagged recovery:** if the broadcast ring overflows, the handler
re-queries `list_events(run_id, last_seen_seq)` and resumes —
clients never silently miss events.

**Keep-alive:** 15-second heartbeat (axum `KeepAlive::text("keep-alive")`)
so reverse proxies don't idle-close.

## TS client

```typescript
import { openRunEventStream } from '@/runtime-host/event-stream';

const handle = openRunEventStream({
  baseUrl: 'http://localhost:3939',
  runId: 'run_abc',
  onEvent: (ev) => console.log(ev.kind, ev.seq),
  onDone: (reason) => console.log('done:', reason), // 'terminal' | 'closed'
  onError: (err) => console.warn(err),
});

// Later:
handle.close();
```

Implementation registers `addEventListener` for each known kind
(server tags messages with `kind`); the `onEvent` callback fires
regardless of which kind arrived. Terminal events auto-close the
underlying `EventSource`. Test seam: `eventSourceCtor` option
lets tests inject a fake constructor.

## Editor UX

Two surfaces in the editor consume the stream:

### Run modal — Live tab (per-run, real-time)

When a controller-local run starts, the modal opens an SSE
stream. The Live tab renders each event as it arrives:

- Print rows show the text + a "show source" link when
  `source_span` is present (jumps to source or canvas node via
  the existing `findNodeForSpan` + `jumpToNode` machinery — same
  click-to-source UX the browser-sim Trace tab uses).
- ExtCallStarted / ExtCallCompleted show connector + fn_name +
  success/error tag.
- Completed / Failed show return value or error kind.

The modal also polls `GET /runs/:id` for the final RunRecord
(since events alone don't carry every record field like
`created_at`); SSE is the streaming UX layered on top of polling.

### Run History modal (cross-run, replay)

Reachable from the Toolbar (list-with-arrow icon). Filters runs
by workflow + status + limit; clicking "View events" on any row
opens an inline replay panel — same `openRunEventStream` client,
no `?after` so the entire persisted event log replays.

## Adding a new event kind

1. Add the variant to `RunEvent` in `host-spec/src/lib.rs` (don't
   forget `run_id`, `seq`, `ts`).
2. Update the helper methods (`kind()`, `is_terminal()` etc.)
   so persistence + SSE see the new kind.
3. Add it to `KNOWN_KINDS` in `src/runtime-host/event-stream.ts`
   so the client subscribes to it.
4. Emit it from `execute_run`, the print callback, or the
   ExtCall handler — wherever it logically belongs.

Existing persisted events with old kinds keep working — the table
stores the full JSON payload, and serde's deny-unknown-fields
behavior isn't used here.

## Failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| SSE stream connects but no events arrive for an active run | Run is between Queued and Started + emit was slow | Wait — Print/ExtCall events arrive as the VM runs |
| `id:` field skips numbers in the stream | ExtCall events emitted from a sync context interleave with async lifecycle events; seqs allocated by a shared atomic so they're contiguous in time but the SSE delivery order may compress consecutive ids when they share a `data:` block | Expected; clients sort by `seq` if order matters |
| `last_seq` never advances + browser shows `(broken)` | Reverse proxy idle-closed the connection | Confirm the proxy passes `Content-Type: text/event-stream` through + disables compression |
| Editor shows "No events yet" but run completed | Browser missed terminal event before SSE drained | Reconnect with `?after=N` where N is the last seen seq; the persistent log replays the gap |

## Related docs

- [Local Controller](./CONTROLLER_LOCAL.md) — boot + connect
- [Phase C Roadmap](./PHASE_C_ROADMAP.md) — milestone status
- `controller/src/event_sink.rs` — emit fan-out
- `controller/src/server.rs::get_run_events` — SSE handler
- `src/runtime-host/event-stream.ts` — client
