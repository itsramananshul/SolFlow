# Run lifecycle + orchestration semantics

**Phase C C.6 (shipped 2026-05-28).** This is the authoritative
reference for how a run moves through the controller — from the
HTTP `POST /runs` that creates it to the terminal `Cancelled` /
`Succeeded` / `Failed` / `TimedOut` / `Rejected` row in SQLite.
If implementation and this document disagree, the implementation
is the bug.

## State machine

```
                                 ┌─────────────┐
                          ┌─────►│   Rejected  │  saturation = Reject + queue full
                          │      └─────────────┘
                          │
   POST /runs ────► Queued ─────► Starting ─────► Running ────► Succeeded
                          │           │             │
                          │           │             │
                          │           ▼             ▼
                          └────► Cancelled ◄── Cancelling ◄─── (DELETE /runs/:id)
                                      ▲             │
                                      │             │
                                      │             ▼
                                      │         TimedOut    (wall-clock fired)
                                      │             │
                                      │             ▼
                                      │           Failed    (runtime error)
                                      │
                              (user cancel during
                               any post-Queued state
                               wins over the
                               natural terminal)
```

### Valid transitions

The transition map is encoded once in
`host_spec::RunStatus::can_transition_to`. Tests
(`run_status_valid_transitions`, `run_status_rejects_invalid_transitions`)
pin every entry.

| From | Allowed Next |
|---|---|
| `Queued` | `Starting`, `Cancelled`, `Rejected` |
| `Starting` | `Running`, `Cancelled`, `Failed` |
| `Running` | `Succeeded`, `Failed`, `Cancelling`, `TimedOut` |
| `Cancelling` | `Cancelled`, `Failed` |
| Terminals (`Succeeded` / `Failed` / `Cancelled` / `TimedOut` / `Rejected`) | none — terminals are sinks |

`RunStatus::transition_to(next)` returns
`Err(InvalidTransition { from, to })` for any pair not in this
table. Internal call sites that move status (`RunManager`,
`executor`) always go through this helper, never direct
mutation.

### State semantics

| State | Meaning |
|---|---|
| `Queued` | Accepted by the controller; row persisted; sitting in the FIFO mpsc queue. `cancel_requested` may be set (sticky bit across restart). |
| `Starting` | Dispatcher dequeued + acquired a worker permit + wrote `started_at`. VM hasn't ticked yet. Window is small (~ms) but visible in SSE. |
| `Running` | VM is executing instructions on a `spawn_blocking` thread. |
| `Cancelling` | A cancel landed via `DELETE /runs/:id`. The VM is being given up to 5s to honor its cancel callback + return cleanly. |
| `Succeeded` | VM ran to completion without runtime error. Terminal. |
| `Failed` | VM hit a runtime error (`DivByZero` / `OOB` / `ExtCallFailed` / `ResourceLimit` / etc.). Terminal. |
| `Cancelled` | User cancellation completed. Terminal. May be reached via several paths (see below). |
| `TimedOut` | Wall-clock budget exhausted. The VM exited via the timeout flag; status reflects budget overrun rather than runtime error. Terminal. |
| `Rejected` | Controller refused at enqueue time (saturation = `Reject` + queue full). The VM was never invoked. Terminal. |

## Paths to each terminal

### `Succeeded` (the happy path)
1. `Queued` → `Starting` → `Running` → VM returns Ok → `Succeeded`.
2. Single `RunEvent::Completed { output }` emitted; SSE stream
   closes after the client receives it.

### `Failed`
1. The VM returns `Err(RunError::*)` (DivByZero / OOB / ExtCallFailed / ResourceLimit / etc.).
2. `executor::run_error_to_view` maps the variant to
   `RuntimeErrorView`; `RunEvent::Failed { error, source_span? }`
   is emitted.
3. Persisted row's `output_json` carries the captured print buffer
   up to the point of failure.

### `Cancelled` (user-initiated)
1. Editor / client sends `DELETE /runs/:id`.
2. `RunManager::cancel` finds the active entry, flips
   `cancel_flag`, and persists `cancel_requested = 1`.
3. Status transitions `Running → Cancelling` (event emitted).
4. The VM's cancel callback returns true on its next poll
   between instructions → `RunError::Cancelled` → `Failed`
   row written by executor.
5. `RunManager::reconcile_post_execution` runs after the worker
   returns. Because `cancel_flag` is set, the terminal gets
   promoted: persisted status changes from whatever it landed
   on to `Cancelled`; a single `RunEvent::Cancelled` is emitted.
   This also catches the case where cancel arrived during a
   slow `ExtCall` and the handler returned Failed — the user's
   intent wins.

### `Cancelled` (cancel-before-dispatch)
1. Cancel arrives while the run is still `Queued` or sitting
   in the worker's permit-wait phase.
2. Dispatcher's pre-dispatch check (and the worker's pre-Starting
   check) observes `cancel_flag` or `cancel_requested` and
   short-circuits: `Queued → Cancelled` directly, with one
   `RunEvent::Cancelled` and **no** `Starting` / `Cancelling`
   intermediates (the VM was never invoked).

### `TimedOut`
1. Wall-clock budget elapses; executor's `tokio::select!`
   sets the internal `timeout_flag` (distinct from the user
   `cancel_flag`).
2. VM cancel callback observes the combined flag, returns
   `RunError::Cancelled`.
3. Executor's post-VM mapping sees `timed_out = true` and writes
   `RunStatus::TimedOut`; emits `RunEvent::TimedOut { wall_clock_secs }`.
4. If user cancel ALSO fired, reconcile promotes `TimedOut →
   Cancelled` (user intent wins).
5. **Grace window:** if the VM ignores cancel for > 5s after
   the budget fires, executor calls `finalize_timed_out` to
   record the terminal anyway. The orphaned `spawn_blocking`
   thread eventually exits via `step_limit` at worst.

### `Rejected`
1. Controller's saturation policy is `Reject`.
2. `enqueue` sees `queue_depth >= max_queued_runs`.
3. Row persisted with `status = Rejected, completed_at = now`;
   one `RunEvent::Rejected { reason }` emitted.
4. Client's `POST /runs` still returns `202 Accepted` with the
   run_id + status — by the time the response lands the row is
   already terminal. (Future: surface this as 503 with a
   distinct code; deferred to C.7.)

### `QueueFull` (different from `Rejected`)
- Saturation policy is `Queue` (default): `enqueue` returns
  `EnqueueOutcome::QueueFull` → controller maps to
  `ControllerError::QueueFull` → HTTP **503** with
  `code: "queue_full"`.
- **No row is persisted** — the run never existed from
  persistence's perspective. The client retries on its own
  cadence.
- Editor distinguishes this from generic 503 via the `code`
  field; renders a "controller busy" message rather than a
  fatal error.

## Concurrency policy

`ConcurrencyPolicy` (controller-wide, set at boot via
`LocalController::with_concurrency_policy`):

| Field | Default | Meaning |
|---|---|---|
| `max_concurrent_runs` | 8 | Workers gating `Starting → Running` via `tokio::sync::Semaphore` |
| `max_queued_runs` | 64 | mpsc channel capacity backing the FIFO queue |
| `on_saturation` | `Queue` | `Queue` (default) returns `EnqueueOutcome::QueueFull` / `Reject` finalizes Rejected |

The semaphore is acquired by the dispatcher BEFORE spawning
a worker, so we never have more than `max_concurrent_runs`
running. Drops on permit release pump the queue.

## Cancellation architecture

Two flags coexist:

- **`user_cancel: Arc<AtomicBool>`** — flipped by
  `RunManager::cancel(run_id)` when the editor / client sends
  `DELETE /runs/:id`. Sticky persisted bit
  (`runs.cancel_requested`) survives restarts.
- **`timeout_flag: Arc<AtomicBool>`** — flipped by the
  executor's wall-clock `tokio::select!` when budget elapses.
  Per-run, in-memory only.

The VM's `cancel_callback` is the OR of both:

```rust
let cb = Arc::new(move || {
    user_cancel.load(Relaxed) || timeout_flag.load(Relaxed)
});
```

The VM polls between every instruction. Cost is one branch +
one Relaxed atomic load — negligible.

For connectors: `ConnectorInvocation` carries an
`Option<Arc<AtomicBool>>` mirror of the same union (computed
by `ControllerExtCallHandler`). The HTTP connector:
- checks the flag before every retry attempt
- `tokio::select!`-races each in-flight request against a 50ms
  cancel poll
- returns `ConnectorError::Cancelled` when triggered

Reconcile's "user cancel wins" rule (in
`RunManager::reconcile_post_execution`) only consults
`user_cancel` — `timeout_flag` is intentionally hidden so
TimedOut isn't mis-promoted to Cancelled.

## Resource limits

`RunPolicy` per-run:

| Field | Default | Enforcement |
|---|---|---|
| `step_limit` | 10,000,000 | VM checks before each instruction; fires `RunError::StepLimit` → Failed |
| `wall_clock_timeout` | 600 s | Executor `tokio::select!`; fires TimedOut path |
| `max_output_lines` | 100,000 | VM checks in every Print instruction; fires `RunError::ResourceLimit { resource: "output_lines" }` → Failed |
| `max_events_per_run` | 1,000,000 | `RunEventCtx::is_capped()` checked before each emit; fires one terminal `RunEvent::Failed { ResourceLimit { resource: "events" } }` marker; subsequent emits drop silently |

The `events` cap protects the SSE stream + `run_events` table
from runaway logging. The cap-reached marker counts as one of
the N events, so with `cap=N` you see N-1 normal events + 1
marker.

## Boot recovery

`LocalController::recover_runs()` runs at binary startup
BEFORE the scheduler tick starts:

1. `persistence::list_recoverable_runs()` — fetches every row
   with `status IN (Queued, Starting, Running, Cancelling)`
   ordered by `created_at ASC`.
2. `reset_non_terminal_to_queued()` — bulk UPDATE sets all
   four statuses to `Queued`, clears `started_at` +
   `completed_at`.
3. For each: `RunManager::reattach(record)` pushes into the
   dispatcher channel without re-persisting (status is already
   Queued) or re-emitting a duplicate `Queued` event (the
   original event is still in the log for SSE replay).

### At-least-once semantics

A run that was `Running` when the controller crashed gets a
**fresh attempt** on restart. Side-effects already performed
(ExtCalls fired, external state changed) may execute again.
Workflow authors are responsible for idempotency. Phase C does
not implement exactly-once delivery; that's a C.7+ concern
when we add distributed coordination.

### Sticky cancel across restart

`runs.cancel_requested` is a persistent INTEGER column set by
`RunManager::cancel`. The dispatcher's pre-Starting check
consults both the in-memory flag AND the persistent bit, so a
mid-cancel run that survived a crash finalizes as `Cancelled`
on the first post-reboot dispatch — without re-running the VM.

## Scheduler coordination

`TokioScheduler` (the cron + webhook handler) routes through
`RunManager.enqueue(...)` too — Timer-fired runs and webhook
ingress runs go through the same queue + concurrency caps as
manual runs from `POST /runs`. The three enqueue outcomes:

| Outcome | Scheduler tick behavior | Webhook behavior |
|---|---|---|
| `Accepted` | Schedule's `next_fire_at` advances normally; new run executes | HTTP 202 with the run record |
| `Rejected` | Log warning; schedule's `next_fire_at` still advances (no retry on this tick); the Rejected record persists | HTTP 202 with the Rejected record |
| `QueueFull` | Log warning; schedule's `next_fire_at` advances; **no record persisted** (next tick will try again) | HTTP 503 |

The scheduler never holds the dispatcher hostage — if the queue
is full, the schedule just misses one fire. Next tick re-tries.

## Observability

Every state transition emits a `RunEvent`:

| Event | When | Carries |
|---|---|---|
| `Queued` | `enqueue` accepted | — |
| `Starting` | dispatcher promoted Queued→Starting | — |
| `Started` | VM about to begin (inside executor) | — |
| `Print` | VM Print instruction | `text`, `source_span?` |
| `ExtCallStarted` / `ExtCallCompleted` | handler dispatch | connector + fn_name + ok |
| `Cancelling` | user cancel arrived for an active run | — |
| `Cancelled` | terminal cancel | — |
| `Completed` | terminal success | `output` |
| `Failed` | terminal runtime error | `error: RuntimeErrorView`, `source_span?` |
| `TimedOut` | terminal wall-clock | `wall_clock_secs` |
| `Rejected` | saturation refusal | `reason` |

All events have monotonic `seq` per run. SSE clients use
`?after=N` to resume past gaps; the controller's broadcast
ring uses `tokio::sync::broadcast` with capacity 1024 and the
`Lagged` recovery path falls back to re-querying the persistent
log.

## HTTP API summary

| Endpoint | Method | Returns | Notes |
|---|---|---|---|
| `/runs` | POST | `202 RunCreated` / `503` | Status may be `Queued` or `Rejected` (saturation = Reject) |
| `/runs/:id` | GET | `200 RunRecord` / `404` | |
| `/runs/:id` | DELETE | `204` / `404` | Real cancel; idempotent (terminal runs return 204 too) |
| `/runs/:id/events` | GET (SSE) | event stream | Replay + live; terminal event closes the stream |
| `/runs/active` | GET | `[ActiveRunSummary]` | RunManager in-memory snapshot |
| `/controller/concurrency` | GET | `ConcurrencyMetrics` | Caps + current depth + policy |
| `/workflows/:id/runs` | GET | `[RunRecord]` | Filter by `?status=` (any of the 9 lifecycle states) |

## Failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| Run sits in `Queued` indefinitely | Worker pool starved (all `max_concurrent_runs` busy) OR controller crashed pre-Starting | Check `/controller/concurrency`; restart triggers boot recovery |
| `POST /runs` returns `503 queue_full` repeatedly | Sustained throughput > `max_concurrent_runs` × inverse VM duration | Raise `SOLFLOW_CONTROLLER_MAX_QUEUED_RUNS` / `MAX_CONCURRENT_RUNS` env vars; or accept backpressure |
| Cancel doesn't seem to work | The cancel arrived; the VM was waiting on a slow Print/Heap op that doesn't poll cancel. Wait — VM exits on next instruction (~µs) | If genuinely stuck > 5s, the executor synthesizes `TimedOut` via the grace path |
| `Failed` row says "wall-clock timeout" but status is `Failed` not `TimedOut` | You're looking at a row from before C.6 c94 (pre-2026-05-28). New rows correctly land `TimedOut` |
| SSE stream shows `Failed { ResourceLimit: "events" }` mid-run | `max_events_per_run` cap hit. Run kept executing but events were dropped to protect the log + stream | Raise `SOLFLOW_CONTROLLER_MAX_EVENTS_PER_RUN` if legitimate; otherwise reduce event spam in workflow |
| Recovered run shows duplicate ExtCall events in history | At-least-once recovery: the ExtCall fired pre-crash; re-execution fires it again | Document idempotency requirements; long-term: exactly-once needs C.7's distributed coordination |

## Related docs

- [Local Controller](./CONTROLLER_LOCAL.md) — boot + env vars
- [Connectors](./CONNECTORS.md) — ExtCall surface + URL grammar
- [Scheduling](./SCHEDULING.md) — Timer + Event triggers
- [Events](./EVENTS.md) — SSE protocol + run_events table
- [Phase C Architecture](./PHASE_C_ARCHITECTURE.md) — canonical design
- `controller/src/run_manager.rs` — orchestration source
- `controller/src/executor.rs` — VM driver + timeout path
- `host-spec/src/lib.rs` — `RunStatus::can_transition_to` (the
  state-machine source of truth)
