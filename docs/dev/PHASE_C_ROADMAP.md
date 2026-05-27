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

### C.1 — Architecture + scaffolding 🟡 in progress (c54–c58)

**Goal.** Lock the architecture. Land scaffolding so subsequent
milestones build against a stable contract.

**Deliverables.**
- `PHASE_C_ARCHITECTURE.md` (c54) ✅
- `PHASE_C_ROADMAP.md` (this doc; c55) ✅
- `host-spec/` Rust crate — wire types + serde, no transport
  (c56)
- `controller/` Rust crate — Controller / Connector / Scheduler /
  Persistence traits + stub impls (c57)
- TS mirror of `host-spec` types in `src/runtime-host/types.ts`
  (c56)
- Editor `ControllerSettingsModal.vue` stub — display-only,
  no networking (c58)

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

### C.2 — Controller MVP (local)

**Goal.** A running controller binary on `localhost` that accepts
workflow submissions + creates + executes runs via the canonical
SOL VM.

**Deliverables.**
- `controller/` binary target (`cargo run -p solflow_controller`)
  serving HTTP on a configurable port
- SQLite persistence (schema from architecture §6) with
  `sqlx` migrations
- HTTP API: `POST /workflows`, `POST /runs`, `GET /runs/:id`,
  `DELETE /runs/:id`, `GET /healthz`
- Editor connects via `ControllerSettingsModal`; on
  connect-success, "Run" button gains a Mode selector
  (browser-sim / controller-local)
- Editor + controller exchange the new `host-spec` types
- Run history persists across controller restart

**Success criteria.**
- Editor can submit the Hello sample to a controller and see
  the same output it gets in browser-sim
- Editor can see run history across restarts of both editor and
  controller
- Step-limit + wall-clock-timeout enforced

**Non-goals.**
- No real ExtCall — connectors land in C.4; until then,
  controller's ExtCall returns the same `ExtCallBlocked`
  structured error the browser-sim does
- No scheduling — manual runs only
- No real-time event stream — clients poll `GET /runs/:id` for
  status until C.5

---

### C.3 — Scheduling MVP

**Goal.** Timer + event triggers actually fire and create runs.

**Deliverables.**
- `schedules` table populated
- Tokio scheduler task running cron expressions
- `POST /workflows/:id/schedules` endpoint
- Webhook endpoint `POST /events/:path` creates an Event-trigger
  run
- Editor "Schedules" panel in the workflow inspector

**Success criteria.**
- A Timer-triggered workflow with `*/5 * * * *` fires every 5
  minutes
- A schedule survives controller restart
- A webhook POST creates a new run with the request body as
  inputs

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

- **C.1 — Architecture + scaffolding** — 🟡 in progress
  - c54 architecture doc ✅
  - c55 this roadmap ✅
  - c56–c58 scaffolding (next)
- All later milestones not started

## How to contribute to Phase C

Pick a milestone, read its deliverables, file an issue to
discuss approach, then PR. The architecture doc is the contract
— any change to it requires explicit review.

Implementation milestones depend on the previous milestone's
deliverables. Don't skip — C.4 (connectors) requires C.2
(controller with run lifecycle); C.5 (event log) requires C.2
(persistence schema).
