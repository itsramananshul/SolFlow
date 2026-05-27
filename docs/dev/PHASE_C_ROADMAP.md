# Phase C — Phased Roadmap

Companion to [`PHASE_C_ARCHITECTURE.md`](./PHASE_C_ARCHITECTURE.md).
This document is the milestone delivery plan; the architecture
doc is the locked design.

## Cadence

Each milestone:

- Ships independently with passing tests
- Doesn't break browser-sim (the editor's local-execution path
  stays usable throughout Phase C)
- Includes its own README + migration notes
- Closes by updating this document with a ✅ tag + commit refs

Milestones are sized to be coherent commit batches (small days,
not weeks). When a milestone's surface area looks larger than
that, split it.

## Milestones

### C.1 — Architecture + scaffolding ✅ complete (c54–c58)

**Goal.** Lock the architecture. Land scaffolding so subsequent
milestones build against a stable contract.

**Deliverables.**
- `PHASE_C_ARCHITECTURE.md` (c54) ✅
- `PHASE_C_ROADMAP.md` (this doc; c55) ✅
- `host-spec/` Rust crate — wire types + serde, no transport
  (c56) ✅
- `controller/` Rust crate — Controller / Connector / Scheduler /
  Persistence traits + stub impls (c57) ✅
- TS mirror of `host-spec` types in `src/runtime-host/types.ts`
  (c56) ✅
- Editor `ControllerSettingsModal.vue` stub — display-only,
  no networking (c58) ✅

**Success criteria.**
- `cargo test --workspace` green including the two new crates
- `npm run check` green; TS mirror types compile clean
- Editor builds with the new modal accessible from Toolbar

**Non-goals.**
- No working controller binary (C.2)
- No persistence (C.2)
- No HTTP server (C.2)
- No actual editor → controller submission flow (C.2)

---

### C.2 — Controller MVP (local) ✅ SHIPPED

**Goal.** A running controller binary on `localhost` that accepts
workflow submissions + creates + executes runs via the canonical
SOL VM.

**Delivered (c59 – c66).**
- `solflow_controller` binary (`cargo run -p solflow_controller`)
  serving HTTP on `127.0.0.1:3939` by default, configurable via
  env vars (`SOLFLOW_CONTROLLER_BIND` / `_DB` / `_STEP_LIMIT` /
  `_TIMEOUT_SECS`); graceful ctrl-c + SIGTERM shutdown
- `SqlitePersistence` (sqlx + `migrations/0001_initial.sql`)
  implementing the `Persistence` trait, with in-memory variant
  for tests
- `LocalController` glues persistence + executor; mints workflow
  / run IDs, content-hashes bytecode for replay + audit
- HTTP API: `GET /healthz`, `POST /workflows`, `POST /runs`,
  `GET /runs/:id`, `GET /workflows/:id/runs`,
  `DELETE /runs/:id` (501 until C.6), uniform JSON error
  envelopes, permissive CORS
- `Inst` serde derive (c59) + `host-spec` wire-encoding helpers
  (`encode_bytecode` / `encode_instruction_spans`) and TS-side
  `compile_for_wire_json` entry point
- `src/runtime-host/client.ts` — typed `controllerClient(url)`
  with structured `ControllerClientError` (kinds: network /
  timeout / http / decode / version / aborted), AbortSignal
  timeouts, host-spec major check via `healthzChecked()`,
  `pollRun` with overall-timeout
- `useControllerStore` Pinia store + revised
  `ControllerSettingsModal` with live connect / disconnect /
  retry + distinct UX for each error kind + persisted URL +
  silent reconnect on app mount
- RunModal mode selector (browser-sim / controller-local) with
  unified result rendering; controller path: compile → POST
  /workflows → POST /runs → poll; meta footer with workflow_id
  / run_id / status / duration
- Per-controller run history (`useControllerRunHistoryStore`):
  collapsible "Recent runs" with Reopen — re-fetches via
  `GET /runs/:id` proving persistence survives controller
  restarts
- `docs/dev/CONTROLLER_LOCAL.md` — how-to-run + env vars +
  troubleshooting + API quick reference

**Success criteria — met.**
- ✅ Editor can submit the Hello sample to a controller and see
  the same output it gets in browser-sim
- ✅ Editor can see run history across restarts of both editor
  and controller (Reopen exercises this)
- ✅ Step-limit + wall-clock-timeout enforced (covered by
  `executor::tests::execute_run_step_limit_enforced` +
  `RunPolicy`)

**Test coverage.**
- Rust workspace: 77 tests across compiler / runtime / host-spec
  / controller (20 controller tests cover persistence, executor,
  LocalController end-to-end, axum server)
- TypeScript: 97 vitest including 18-test client-suite
  (normalization, every method, every error kind, timeout vs
  abort discrimination, pollRun terminal + timeout)

**Non-goals (deferred as planned).**
- No real ExtCall — connectors land in C.4; until then,
  controller's ExtCall returns the same `ExtCallBlocked`
  structured error the browser-sim does
- No scheduling — manual runs only
- No real-time event stream — clients poll `GET /runs/:id` for
  status; structured runtime-error details + execution trace
  stream from controller land in C.5

---

### C.3 — Scheduling MVP ✅ SHIPPED

**Goal.** Timer + event triggers actually fire and create runs.

**Delivered (c67 – c73).**
- `migrations/0002_schedules.sql` — schedules table + partial
  index on `(enabled, next_fire_at)` for the tick hot path
- Persistence trait gains 8 schedule methods (put / get /
  delete / list_for_workflow / list_due_timer /
  list_enabled_event / update_next_fire / set_enabled)
- `TokioScheduler` (controller/src/scheduler.rs) — 1s tick loop
  that fires due Timer schedules, advances `next_fire_at` from
  the cron expression, and handles webhook ingress via
  `ingress_event(path, body)`
- `LocalController::new()` starts the scheduler tick on boot;
  `create_schedule` impl + non-trait helpers
  (list/cancel/set_enabled/ingress_event) for the HTTP layer
- HTTP routes: `POST` / `GET` `/workflows/:id/schedules`,
  `GET` / `DELETE` / `PATCH` `/schedules/:id`,
  `POST /events/*path` (wildcard for multi-segment paths)
- TS `ControllerClient` gains createSchedule / listSchedules /
  getSchedule / setScheduleEnabled / cancelSchedule /
  triggerEvent
- `SchedulesModal` (Toolbar → clock icon) — workflow-scoped
  schedule list with enable/disable/delete, create form
  (Timer cron / Event path), manual webhook-trigger pane for
  testing
- `docs/dev/SCHEDULING.md` — cron syntax + HTTP API examples
  + failure-mode table

**Success criteria — met.**
- ✅ A Timer-triggered workflow with `*/5 * * * *` fires
  automatically (live-tested: a `* * * * *` Timer registered at
  T+0 produced two `Timer`-trigger runs by T+20s)
- ✅ A schedule survives controller restart — same SQLite path,
  schedules persist; scheduler tick resumes from `next_fire_at`
- ✅ A webhook POST creates a new run with the request body as
  inputs — `POST /events/ci/build` with `{"ref":"main"}`
  produces a run with `inputs: {"ref":"main"}` and
  `trigger.kind="Event"`

**Test coverage.**
- 41 controller tests (+14 over C.2): 7 persistence
  (schedule CRUD + due filtering + enable/disable + delete),
  9 scheduler (cron parse + register/cancel + ingress_event
  match/no-match + end-to-end tick), 5 server route
  (post_schedule happy + invalid-cron, full lifecycle,
  unmatched path 404, matched event with inputs)
- 124 vitest (+6): client suite covers all 6 new HTTP methods
  including 404-no-match path

**Non-goals (deferred as planned).**
- No backfill / catchup semantics — if the controller is down
  when a Timer should fire, the run doesn't backfill; next
  scheduled fire continues normally
- No timezone support — cron evaluates in UTC
- No sub-second tick — fixed 1s cadence; sub-second scheduling
  isn't a Phase C target

---

### C.4 — Connector framework

**Goal.** Real external calls work via a typed Connector trait.

**Deliverables.**
- `Connector` trait in `controller/` with at-least-once
  semantics, timeout, retry policy
- `http` reference connector implementation
- ExtCall URL parsing: `connector://name?key=value`
- Connector config loaded from controller config file
  (environment variables for secrets)
- Tests: an `ext function fetch_user(url: str) -> str` workflow
  hits a real local HTTP server and gets a response

**Success criteria.**
- Workflows that previously failed in browser-sim with
  `ExtCallBlocked` now succeed when run via a controller with
  the `http` connector enabled
- Connector errors surface as `RunError::ExtCallFailed { connector,
  message }` — no silent successes

---

### C.5 — Event log + observability UI

**Goal.** Real-time + persistent execution visibility.

**Deliverables.**
- WebSocket endpoint `/runs/:id/events`
- `run_events` table populated for every run
- Editor "Run Log" panel listening to the stream
- Editor "Run history" panel querying past runs
- Existing source-span / node-mapping (c42 / c43 / c44) plumbs
  through controller events; click-to-source + click-to-node
  works in the Run Log

**Success criteria.**
- Long-running workflows show live output streaming in the
  editor without polling
- Past runs queryable by status / time range / workflow

---

### C.6 — Multi-run management

**Goal.** Production-ish run management.

**Deliverables.**
- Concurrent run execution (configurable parallelism per
  controller)
- Cancellation propagation through VM tick loop + connector
  cooperative cancellation
- Retry policies per-workflow (`backoff: exponential, max: 3`)
- Per-connector circuit breakers
- Dead-letter queue for runs that exhausted retries

**Success criteria.**
- N concurrent runs of the same workflow finish without
  cross-contamination
- A cancelled run stops within ~1 second of the DELETE call
- A retryable workflow with a flaky ExtCall succeeds eventually
  + records every retry attempt in events

---

### C.7 — Remote controller support

**Goal.** Controllers usable across a network, not just on
localhost.

**Deliverables.**
- TLS support for HTTPS endpoint
- Auth handshake stub: bearer token from controller config
  (full auth/RBAC is Phase D)
- `host-spec` versioning negotiation on connect
- Editor "Controller URL" field accepts `https://...` with
  explicit user warning about deployment-grade hardening

**Success criteria.**
- Editor connects to a controller on a different machine over
  HTTPS with a shared bearer token
- Wire-protocol mismatch fails fast with a clear error

**Non-goals.**
- No real auth UI (Phase D)
- No multi-user controller (Phase D)

---

### C.8 — Stabilization

**Goal.** Phase C close-out. Make it shippable.

**Deliverables.**
- Performance pass (worst-case run throughput, event-stream
  fan-out)
- Reliability pass (controller restart with in-flight runs,
  network drops mid-stream)
- Docs pass (`docs/user/` updated for controller mode;
  `controller/README.md` written)
- `PHASE_C_RELEASE_NOTES.md` summarizing C.1 → C.8
- Update Phase B plan banner → "Phase C complete"

---

## What's deliberately out of Phase C

These belong to Phase D or later:

- Real authentication / authorization (currently bearer-token
  stub only)
- Multi-tenant controllers
- Distributed execution (multi-controller coordinator)
- Workflow marketplace / sharing
- Billing / usage metering
- Production SLA hosting

## Status (2026-05-27)

- **C.1 — Architecture + scaffolding** — ✅ complete (c54–c58)
  - c54 architecture doc
  - c55 this roadmap + Phase B plan flip
  - c56 host-spec crate + TS mirror
  - c57 controller crate (traits + StubController)
  - c58 editor ControllerSettingsModal stub
- **C.2 — Controller MVP (local)** — ✅ complete (c59–c66)
  - c59 `Inst` serde + host-spec wire-encoding helpers
  - c60 LocalController + SqlitePersistence + executor + axum
    server + `solflow-controller` binary
  - c61 typed `controllerClient(url)` + WASM
    `compile_for_wire_json` entry point + corrected TS types
  - c62 `useControllerStore` + live `ControllerSettingsModal`
    with connect / disconnect / retry + version-mismatch UX
  - c63 RunModal mode selector + controller-local execution
    flow + unified result rendering
  - c64 per-controller run history + Reopen
  - c65 `CONTROLLER_LOCAL.md` how-to-run + README phase status
  - c66 TS tests + polish + push
- **C.3 — Scheduling MVP** — ✅ complete (c67–c73)
  - c67 schedules table + Persistence trait extension
  - c68 TokioScheduler (cron + event triggers)
  - c69 LocalController integration + HTTP routes
  - c70 TS client schedule methods
  - c71 Editor SchedulesModal
  - c72 SCHEDULING.md + roadmap + CHANGELOG
  - c73 store tests + polish + close
- **C.4 — Connector framework** — next milestone
- C.5 / C.6 / C.7 / C.8 — not started

## How to contribute to Phase C

Pick a milestone, read its deliverables, file an issue to
discuss approach, then PR. The architecture doc is the contract
— any change to it requires explicit review.

Implementation milestones depend on the previous milestone's
deliverables. Don't skip — C.4 (connectors) requires C.2
(controller with run lifecycle); C.5 (event log) requires C.2
(persistence schema).
