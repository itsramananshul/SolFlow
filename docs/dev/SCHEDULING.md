# Scheduling — Timer + Event triggers

**Phase C C.3 (shipped 2026-05-27).** Schedules let workflows run
without an editor session — either on a cron cadence (Timer) or in
response to an HTTP webhook (Event).

## Triggers in C.3

| Trigger | Fires when | Persisted | Survives controller restart? |
|---|---|---|---|
| `Manual` | IDE submits a run | n/a | n/a |
| `Timer { schedule_id, cron }` | cron expression matches | yes | yes (the scheduler tick resumes from the persisted `next_fire_at`) |
| `Event { source }` | `POST /events/:source` with any body | yes | yes |

`schedule_id` is filled by the controller on registration — pass
an empty string when creating, then read it back from the response.

## Cron syntax

The controller accepts the conventional 5-field cron form
(`min hour dom mon dow`). Internally it normalizes to the
7-field form (seconds=0, year=*) the [cron](https://docs.rs/cron)
crate expects.

Examples:

| Cron | Fires |
|---|---|
| `* * * * *` | every minute |
| `*/5 * * * *` | every 5 minutes |
| `0 9 * * 1-5` | 09:00 weekdays |
| `0 0 1 * *` | midnight on the 1st of each month |

Times are evaluated in UTC. Invalid expressions fail the
`POST /workflows/:id/schedules` request with `400 bytecode_invalid`;
no broken schedules ever land in the DB.

## HTTP API

| Method | Path | Body | Returns |
|---|---|---|---|
| `POST` | `/workflows/:id/schedules` | `ScheduleCreate` | `201 ScheduleRecord` |
| `GET` | `/workflows/:id/schedules` | — | `ScheduleRecord[]` |
| `GET` | `/schedules/:id` | — | `ScheduleRecord` |
| `PATCH` | `/schedules/:id` | `{ enabled: bool }` | `ScheduleRecord` |
| `DELETE` | `/schedules/:id` | — | `204 NoContent` (idempotent) |
| `POST` | `/events/*path` | any JSON | `202 RunRecord` (first matching schedule's run) |

`ScheduleCreate`:

```ts
{
  trigger:
    | { kind: 'Timer'; schedule_id: ''; cron: string }
    | { kind: 'Event'; source: string },
  enabled: boolean,    // default true
}
```

`ScheduleRecord`:

```ts
{
  id: string,
  workflow_id: string,
  trigger: RunTrigger,
  enabled: boolean,
  next_fire_at: number | null,   // ms since epoch; null for Event
  created_at: number,            // ms since epoch
}
```

## Examples

### Register a 5-minute Timer

```bash
curl -X POST http://127.0.0.1:3939/workflows/wf_abc/schedules \
  -H 'content-type: application/json' \
  -d '{"trigger":{"kind":"Timer","schedule_id":"","cron":"*/5 * * * *"},"enabled":true}'
```

Response (excerpt):

```json
{
  "id": "sch_abc",
  "workflow_id": "wf_abc",
  "trigger": { "kind": "Timer", "schedule_id": "", "cron": "*/5 * * * *" },
  "enabled": true,
  "next_fire_at": 1779999900000,
  "created_at": 1779999600000
}
```

The scheduler tick (1s cadence) will fire the workflow when
`next_fire_at <= now`, mint a fresh `RunRecord` with
`trigger.kind = "Timer"` carrying the schedule's id + cron, and
recompute `next_fire_at` from the cron expression.

### Register an Event (webhook) trigger

```bash
curl -X POST http://127.0.0.1:3939/workflows/wf_abc/schedules \
  -H 'content-type: application/json' \
  -d '{"trigger":{"kind":"Event","source":"ci/build"},"enabled":true}'
```

Fire it:

```bash
curl -X POST http://127.0.0.1:3939/events/ci/build \
  -H 'content-type: application/json' \
  -d '{"ref":"main","sha":"abc123"}'
```

Returns the created `RunRecord` with `inputs: {"ref":"main","sha":"abc123"}`
and `trigger.kind = "Event"`. The body lands on the run as `inputs`
verbatim — workflows can read it via the future inputs API (a
C.7-era language addition; for now the body is recorded for audit).

`*path` in the route is a wildcard, so multi-segment paths like
`ci/build` work as a single trigger source.

### Disable / re-enable a schedule

```bash
curl -X PATCH http://127.0.0.1:3939/schedules/sch_abc \
  -H 'content-type: application/json' \
  -d '{"enabled":false}'
```

Disabled schedules stay in the DB but skip both the timer tick and
event ingress. Re-enable with `{"enabled":true}`. A Timer whose
`next_fire_at` is now in the past fires on the next tick after enable.

### Delete a schedule

```bash
curl -X DELETE http://127.0.0.1:3939/schedules/sch_abc
```

Idempotent — deleting an already-removed schedule returns `204`.
Existing runs that the schedule fired stay in the DB; the
`runs.trigger_json` retains the schedule id for audit, even though
the schedule itself is gone.

## How the scheduler tick works

```
tokio task (started by LocalController::new)
  every 1s:
    rows = SELECT * FROM schedules
            WHERE enabled = 1
              AND next_fire_at IS NOT NULL
              AND next_fire_at <= ?    -- bind now_ms()
            ORDER BY next_fire_at ASC
    for each row:
      run = RunRecord { trigger: Timer { schedule_id, cron }, ... }
      INSERT INTO runs (...)                    -- as Queued
      tokio::spawn(execute_run(...))            -- async
      UPDATE schedules SET next_fire_at = cron.next_after(now)
```

The 1s cadence is fixed in C.3. Sub-second triggers aren't a
target — Phase C is for orchestration, not realtime. If a cron
expression's next fire falls exactly on the same second as a tick,
the tick will fire it on this pass; if it falls during the 999ms
between ticks, the next tick picks it up.

## Editor UX

Open Schedules from the Toolbar's clock icon. The modal:

- Defaults the workflow-id field to the most recently submitted
  workflow (read from the run-history store)
- Lists existing schedules with toggle-enable / delete
- Has a create form with separate Timer / Event subforms
- Has a manual webhook-trigger pane for testing `POST /events/:path`
  without an external sender

## Failure modes

| Symptom | Cause | Fix |
|---|---|---|
| `400 bytecode_invalid: invalid cron "..."` on POST | cron expression doesn't parse | check 5-field syntax; quote properly in shell |
| `404 workflow_not_found` on POST | workflow id doesn't exist on this controller | submit the workflow first (or copy a known id) |
| `404 schedule_not_found` on `POST /events/:path` | no enabled Event schedule listens on that path | register one, or check the path matches verbatim |
| Schedule's `next_fire_at` cleared (None) without an obvious reason | cron expression was rejected on advancement | controller stderr logs `schedule X: invalid cron "..."`; edit / delete the schedule |
| Workflow was deleted but schedule keeps firing | the schedule still references the workflow id | the scheduler logs `scheduled workflow X not found; disabling schedule Y` and auto-disables it on the next tick |

## Related docs

- [Local Controller](./CONTROLLER_LOCAL.md) — boot + connect
- [Phase C Roadmap](./PHASE_C_ROADMAP.md) — C.3 status + next milestones
- `controller/src/scheduler.rs` — implementation
- `controller/migrations/0002_schedules.sql` — schema
