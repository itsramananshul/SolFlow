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

### C.4 — Connector framework ✅ SHIPPED

**Goal.** Real external calls work via a typed Connector trait.

**Delivered (c74 – c80).**
- `Connector` trait (`controller/src/connector/mod.rs`):
  async `invoke(ConnectorInvocation) -> ConnectorOutcome`,
  `meta()` for self-description.
- `ConnectorRegistry` (build-time, lock-free reads, lookup by
  name) with `ConnectorMeta` exposure.
- `ConnectorError` — 14 structured variants for every failure
  mode (connector_not_found / invalid_connector_url /
  missing_param / timeout / auth_failed / dns_failure /
  http_status / retry_exhausted / payload_too_large /
  response_too_large / url_not_allowed / cancelled /
  invalid_response / network).
- `parse_connector_url("connector://name?k=v")` rejects
  non-connector schemes + path segments; query params only.
- `HttpConnector` reference implementation
  (`controller/src/connector/http.rs`) — reqwest + rustls,
  conservative defaults (10s wall-clock, 1 MiB response cap,
  0 retries), exponential backoff on transport/5xx, optional
  host allowlist with exact + subdomain matching, content-
  length and read-time response-size enforcement, 4xx
  not retried.
- VM `ExtCallHandler` hook (`runtime/src/extcall.rs`) — VM
  stays browser-safe; controller installs a handler that
  bridges synchronous `Inst::ExtCall` to async connector
  invocations via `Handle::block_on` inside `spawn_blocking`.
- `RunError::ExtCallFailed { connector, function_name,
  message }` — distinct from the pre-existing `ExtCallBlocked`.
- `LocalController::new()` registers HttpConnector by default;
  `with_connector(...)` builder adds more. `TokioScheduler`
  inherits the same registry so Timer + Event triggered runs
  also dispatch through it.
- `GET /connectors` HTTP endpoint returns `Vec<ConnectorMeta>`.
- TS client `listConnectors()` + `useControllerStore` populates
  on connect; `ControllerSettingsModal` renders a Connectors
  section with each entry's name / description / default
  policy / version.
- `RuntimeError` TS union + `RunModal::formatRuntimeError`
  handle `ExtCallFailed` distinctly from `ExtCallBlocked`.
- `docs/dev/CONNECTORS.md` — full URL grammar, HTTP connector
  reference, type bridging rules, error model table, end-to-end
  example, failure-mode troubleshooting, "add a new connector"
  recipe.

**Success criteria — met.**
- ✅ Workflows that previously failed in browser-sim with
  `ExtCallBlocked` now succeed when run via the controller:
  the end-to-end test
  `ext_call_runs_through_http_connector_end_to_end` submits
  hand-crafted ExtCall bytecode to a controller backed by a
  live wiremock server, the VM hits ExtCall, the controller
  HTTP connector POSTs to the mock, and the response value
  reaches the run record.
- ✅ Connector errors surface as `RunError::ExtCallFailed`
  with structured `{ connector, function_name, message }` —
  verified by both the runtime test
  `ext_call_handler_error_surfaces_as_extcall_failed` and the
  controller-level test
  `ext_call_unknown_connector_fails_with_extcall_failed`.

**Test coverage.**
- Rust: 71 controller (+30 from C.3) including
  - 12 connector framework (URL parser, registry, default policy)
  - 15 HTTP connector (wiremock-driven: GET 200, POST roundtrip,
    GET args→query, header.* headers, missing/invalid params,
    404/401, 500 retry success + exhausted, allowlist
    deny/allow/subdomain, response-too-large)
  - 3 controller integration (GET /connectors, e2e ExtCall via
    HTTP, unknown connector → ExtCallFailed)
- Runtime: 20 (+2 for the new handler dispatch + error mapping
  paths). Total Rust workspace: 130.
- TS: 127 vitest (+3 for connector list + store population
  happy/sad paths).

**Non-goals (deferred as planned, on roadmap or future).**
- No marketplace, no OAuth UI, no cloud secret manager.
- No user-uploaded code execution — connectors are
  compile-time-registered on the controller binary.
- No SOL-source `at "url"` syntax integration — the language
  doesn't yet have endpoint declarations bound to functions;
  endpoint mappings live outside the language (architecture
  §8). When the language gains them, the editor's
  compile_for_wire path will populate them.
- No request body / response body streaming — single-shot
  buffered transfers with size caps.
- Compound type marshalling (struct, array) at the ExtCall
  boundary — only primitives in C.4; compound support can
  land without changing the trait shape.

---

### C.5 — Event log + observability UI ✅ SHIPPED

**Goal.** Real-time + persistent execution visibility.

**Delivered (c81 – c88).**
- `migrations/0003_run_events.sql` — `(run_id, seq)` composite
  PK + denormalized `payload_json`; per-arch §6.1.
- `Persistence::append_event` + `list_events` are no longer
  no-ops (C.2 left them stubbed); plus a non-trait
  `list_all_events` for the SSE "from start" replay path.
- `host-spec::RunEvent` gains `run_id()` / `seq()` / `ts()` /
  `kind()` / `is_terminal()` helpers so persistence + SSE
  don't pattern-match every variant.
- New `runtime::extcall::PrintCallback` hook on the VM —
  browser-safe (no callback installed by compiler-wasm) but
  lets the controller emit real-time `RunEvent::Print` on
  every print instruction with the line + inst_ptr.
- New `controller::event_sink::EventSink` trait with
  `PersistentEventSink` (SQLite + 1024-event tokio
  broadcast) and `RunEventCtx` per-run helper carrying the
  shared `Arc<AtomicU64>` seq counter.
- `execute_run` emits Queued / Started / Print* / ExtCallStarted /
  ExtCallCompleted / Completed | Failed in monotonic seq
  order. Print events carry source spans decoded from the
  workflow's `instruction_spans` sidecar so the editor can
  click-to-source on each print line.
- SSE endpoint `GET /runs/:id/events?after=N` combining
  persistent replay (strict `seq > N`, or all when omitted)
  with the in-process broadcast subscription. Handles
  broadcast `Lagged` by re-querying the persistent log so no
  event is silently missed. 15s keep-alive heartbeat for
  reverse-proxy-friendliness. Terminal-event auto-close.
- TS `openRunEventStream(...)` client wrapping browser
  EventSource — per-kind `addEventListener` registration,
  `onDone` discriminator (`'terminal'` vs `'closed'`),
  testable seam via `eventSourceCtor` injection.
- RunModal gains a "Live" tab streaming events as they
  arrive in controller-local mode; Print rows with source
  spans get a show-source affordance using the existing
  `findNodeForSpan` / `jumpToNode` machinery.
- New `RunHistoryModal` (Toolbar list-with-arrow icon)
  filters past runs by workflow + status + limit; clicking
  any row opens an inline event-replay panel.
- `docs/dev/EVENTS.md` — event-type table, architecture
  diagram, HTTP API + lifecycle, TS client examples, editor
  UX overview, "add a new event kind" recipe, failure-mode
  troubleshooting.

**Success criteria — met.**
- ✅ Long-running workflows show live output streaming
  without polling — live smoke (binary on :13943): submit
  `print("hello"); print("world")` workflow, curl SSE → 5
  events arrive in seq order (Queued / Started / Print "hello"
  / Print "world" / Completed) with correct timestamps.
  Editor's Live tab in RunModal renders the same stream
  client-side.
- ✅ Past runs queryable by status / time / workflow —
  RunHistoryModal filters via `GET /workflows/:id/runs?status=&limit=`;
  clicking any row replays the full event log via SSE.

**Test coverage.**
- Rust: 78 controller (+7 from C.4) — 2 persistence
  (append_event round-trip, every-variant round-trip), 2
  event_sink (PersistentEventSink + CapturingEventSink),
  1 executor (end-to-end emit), 2 server (SSE replay, after=N).
  Total Rust workspace: 137.
- TS: 134 vitest (+7) — full event-stream client coverage
  with FakeEventSource (URL, ?after=N, terminal close,
  explicit close, bad-JSON onError, no-EventSource error).

**Non-goals (deferred as planned).**
- WebSocket not used — SSE is the better fit (one-way,
  built-in browser auto-reconnect via Last-Event-ID, no
  framing overhead). WebSocket can return in C.7 if needed
  for bidirectional streams.
- No backpressure to the VM. If the broadcast lags, the
  SSE handler recovers via the persistent log; the VM keeps
  emitting at full speed.
- No metric aggregation (events-per-second, p99 latency,
  etc.). That's a C.8 stabilization concern.

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
- **C.4 — Connector framework** — ✅ complete (c74–c80)
  - c74 Connector trait + registry + URL parser + structured errors
  - c75 HTTP reference connector (reqwest + rustls)
  - c76 VM ExtCall hook + executor wiring
  - c77 LocalController integration + /connectors route + end-to-end smoke
  - c78 TS client connector surface + editor error mapping
  - c79 CONNECTORS.md + roadmap + CHANGELOG
  - c80 final polish + close
- **C.5 — Event log + observability UI** — ✅ complete (c81–c88)
  - c81 run_events table + Persistence trait real impl
  - c82 EventSink + VM print hook + executor emit wiring
  - c83 SSE /runs/:id/events endpoint (replay + live + Lagged recovery)
  - c84 TS event-stream client using EventSource
  - c85 RunModal Live tab + click-to-source/node
  - c86 RunHistoryModal — past runs queryable by status/workflow
  - c87 EVENTS.md + roadmap + CHANGELOG
  - c88 final polish + close
- **C.6 — Multi-run management** — next milestone
- C.7 / C.8 — not started

## How to contribute to Phase C

Pick a milestone, read its deliverables, file an issue to
discuss approach, then PR. The architecture doc is the contract
— any change to it requires explicit review.

Implementation milestones depend on the previous milestone's
deliverables. Don't skip — C.4 (connectors) requires C.2
(controller with run lifecycle); C.5 (event log) requires C.2
(persistence schema).
