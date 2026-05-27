# Phase C — Runtime Architecture

Status: **C.1 (architecture pass)** — 2026-05-27. No implementation
yet beyond scaffolding. See [`PHASE_C_ROADMAP.md`](./PHASE_C_ROADMAP.md)
for the milestone-by-milestone delivery plan.

This document is the canonical Phase C design. Implementation
commits should reference sections here. Breaking changes to the
architecture require updating this document in the same PR.

## 1. What Phase C is

Phase C transforms SolFlow from a compiler-backed visual IDE into
a real orchestration platform. Specifically:

- Workflows execute against real external systems (not just
  in-browser simulation)
- Runs persist, can be queried, replayed, cancelled
- Scheduling (cron / event triggers) actually fires
- Observability surfaces what happened, when, and why

**The canonical SOL compiler + VM remain the source of truth.**
There is no parallel execution path, no JS reimplementation, no
shortcut runtime. Phase C wraps the existing canonical runtime
with lifecycle, persistence, and integration concerns; it does
not fork or replace it.

## 2. Topology

```
┌───────────────────────────────────────────────────────────────┐
│                   Browser tab (SolFlow editor)                │
│                                                               │
│  Editor + canonical compiler (WASM) + canonical VM (WASM)     │
│  - graph editing, source editing                              │
│  - local SIMULATION via runtime/ (ExtCallBlocked)             │
│  - submits workflows to controller for REAL execution         │
└───────────────────────────┬───────────────────────────────────┘
                            │ host-spec wire protocol
                            ▼
       ┌──────────────────────────────────────────────────┐
       │             Controller (process)                  │
       │                                                   │
       │  - hosts canonical SOL VM (same runtime/ crate)   │
       │  - run lifecycle (start/queue/complete/cancel)    │
       │  - resolves Inst::ExtCall via Connector registry  │
       │  - schedules timer/event triggers                 │
       │  - persists run history + state                   │
       │  - emits structured RunEvent stream to clients    │
       └──────────────────────┬───────────────────────────┘
                              │
                              ▼
               ┌──────────────────────────────┐
               │  External systems            │
               │  (HTTP, queues, services)    │
               └──────────────────────────────┘
```

**Invariant:** the same `runtime/` Rust crate (canonical SOL VM)
runs in BOTH the browser (via WASM) AND the controller (native).
Identical bytecode-execution semantics; only ExtCall resolution
differs. The browser blocks ExtCall with a structured error; the
controller dispatches it through the Connector registry.

## 3. Execution modes

| Mode | VM location | ExtCall behavior | Persistence | Milestone |
|---|---|---|---|---|
| `browser-sim` | Browser WASM | `ExtCallBlocked` runtime error | Editor localStorage | Today (Phase B) |
| `controller-local` | Controller process (same machine) | Real via Connector registry | SQLite | **C.2** |
| `controller-remote` | Remote controller | Real | Remote DB | C.7+ |
| `distributed` | N controllers, coordinated | Real | Distributed | Beyond Phase C |

The editor doesn't care which mode it talks to — the controller
wire protocol is the same. Only implementations differ.

## 4. Controller responsibilities

A controller owns:

- **Run lifecycle.** Accept submitted workflows; queue, start,
  complete, fail, retry, or cancel runs.
- **VM hosting.** Re-uses `solflow_runtime::run_program_with` with
  a controller-supplied ExtCall hook so canonical semantics stay
  bit-for-bit identical to the browser sim path.
- **Connector registry.** Map ExtCall function names to real
  connector implementations (HTTP, Slack, etc.). Connector
  credentials NEVER leave the controller process.
- **Scheduler.** Timer triggers (cron-style) + event triggers
  (webhook). Persistent across controller restart.
- **Event stream.** Per-run structured event emission, both
  real-time (WebSocket) and persistent (event log).
- **Persistence.** Run records, event log, scheduler state.

A controller does NOT own:

- The editor (SolFlow is the editor)
- SOL semantics (the `compiler/` crate is)
- Authentication / authorization (Phase D)
- Workflow authoring (the editor handles that; the controller
  receives compiled artifacts)

## 5. IDE ↔ controller wire protocol

Two channels.

### 5.1 Control plane (HTTP REST for C.2)

| Endpoint | Purpose |
|---|---|
| `POST /workflows` | Upload compiled workflow. Returns `workflow_id` + content hash. |
| `POST /runs` | Create a run. Body: `{ workflow_id, trigger, inputs }`. Returns `run_id`. |
| `GET /runs/:id` | Current run state + last-known output. |
| `GET /runs/:id/events?after=N` | Replay event log from sequence N (for catch-up). |
| `DELETE /runs/:id` | Cancel run (best-effort interruption). |
| `GET /workflows/:id/runs` | History list. |
| `GET /workflows/:id/runs?status=Failed&limit=20` | Filtered history. |
| `POST /workflows/:id/schedules` | Configure a Timer trigger. |
| `GET /healthz` | Controller liveness. |

### 5.2 Event plane (WebSocket for C.2)

Subscribe to `/runs/:id/events` for real-time emission:

```
RunEvent ::=
  | Queued { run_id, queued_at }
  | Started { run_id, started_at }
  | Print { run_id, text, source_span?, ts }
  | ExtCallStarted { run_id, connector, fn_name, ts }
  | ExtCallCompleted { run_id, connector, fn_name, ok, ts }
  | Output { run_id, return_value, ts }
  | Diagnostic { run_id, diagnostic, ts }
  | Completed { run_id, completed_at }
  | Failed { run_id, error, source_span?, completed_at }
  | Cancelled { run_id, cancelled_at }
```

All event types include a monotonic `seq` field so clients can
detect gaps + request replay. Source spans flow through
unchanged from the existing instruction_spans + error_inst_ptr
pipeline (Deferred-B c42).

### 5.3 Wire-protocol versioning

The `host-spec/` crate is **semver-bound**. Breaking shape
changes bump the major version; controllers + editors that
disagree on major version refuse to connect. Minor + patch
versions are additive only (new optional fields, new event
variants treated as unknown by older clients).

## 6. Persistence model

### 6.1 Tables (C.2 MVP, SQLite)

```sql
CREATE TABLE workflows (
  id           TEXT PRIMARY KEY,
  content_hash TEXT NOT NULL,
  bytecode     BLOB NOT NULL,
  spans        BLOB NOT NULL,         -- instruction_spans sidecar
  source       TEXT,                  -- optional canonical source
  meta_json    TEXT NOT NULL,
  created_at   INTEGER NOT NULL
);

CREATE TABLE runs (
  id              TEXT PRIMARY KEY,
  workflow_id     TEXT NOT NULL REFERENCES workflows(id),
  status          TEXT NOT NULL,      -- Queued/Running/Succeeded/Failed/Cancelled
  trigger_json    TEXT NOT NULL,
  inputs_json     TEXT NOT NULL,
  output_json     TEXT,               -- null until completion
  diagnostics_json TEXT NOT NULL DEFAULT '[]',
  started_at      INTEGER,
  completed_at    INTEGER,
  created_at      INTEGER NOT NULL
);

CREATE TABLE run_events (
  run_id TEXT NOT NULL REFERENCES runs(id),
  seq    INTEGER NOT NULL,
  ts     INTEGER NOT NULL,
  kind   TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  PRIMARY KEY (run_id, seq)
);

CREATE TABLE schedules (
  id           TEXT PRIMARY KEY,
  workflow_id  TEXT NOT NULL REFERENCES workflows(id),
  trigger_json TEXT NOT NULL,         -- Timer { cron } / Event { path }
  enabled      INTEGER NOT NULL DEFAULT 1,
  next_fire_at INTEGER,
  created_at   INTEGER NOT NULL
);
```

### 6.2 Why SQLite for the MVP

- Zero-deployment-burden (single-file DB)
- Sufficient for local-first controller use case
- Migration path to Postgres later via SQL portability
- Avoids forcing a Postgres dependency on hobbyist users

C.7's remote-controller milestone may swap to Postgres for
multi-user / multi-node scenarios. The persistence trait abstracts
the storage choice.

## 7. Scheduling model

Three trigger kinds matching existing editor trigger nodes:

| Trigger | Fires when | Persisted | Notes |
|---|---|---|---|
| `Manual` | IDE submits a run | n/a | Default for ad-hoc runs |
| `Timer { cron }` | cron expression matches | yes | Survives controller restart |
| `Event { path }` | external POST to `/events/:path` | yes | Webhook-style |

C.3 implements all three via a single tokio task in the controller
process. C.6+ may move scheduling to a separate process if needed
(e.g. for HA controllers).

## 8. Integration model — Connectors

The compiler's `Inst::ExtCall(arg_types, ret_type)` carries a
function name + URL string at compile time. In controller mode,
the URL is reinterpreted as a connector reference:

```
connector://http?url=https://api.example.com/widgets
connector://slack?channel=alerts
connector://github?repo=myorg/myrepo
```

The controller's Connector trait (defined in `controller/` crate):

```rust
#[async_trait]
trait Connector: Send + Sync {
    fn name(&self) -> &str;

    async fn call(
        &self,
        fn_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ConnectorError>;
}
```

C.4 ships:
- `http` connector — does HTTP requests with timeout, retry,
  rate-limit; the canonical reference impl
- (no others — user-installable connectors are deferred)

**Credentials never travel through the wire protocol.** Connector
config is set in the controller's environment / config file; the
editor only sees connector NAMES, not their secrets.

## 9. Observability model

- **Real-time:** WebSocket event stream; editor renders into a
  "Run Log" panel similar to the existing Trace tab from c44.
- **Persistent:** Every event lands in the `run_events` table;
  queryable for the "Run history" panel (C.5).
- **Source mapping:** The VM's `error_inst_ptr` (c42) +
  codegen's `instruction_spans` (c36) already give
  source-span-per-instruction. The controller forwards spans
  in events; the editor uses existing `findNodeForSpan` (c43)
  to map back to graph nodes when applicable.

Event volume bounded by step limit + connector call rate; well
under SQLite's write throughput for normal workloads.

## 10. Safety boundaries

### 10.1 Submission boundary

Workflows submitted to the controller MUST be produced by
canonical `compile_source`. The controller verifies this by:

- Receiving `bytecode: Vec<u8>` + `spans: Vec<u8>` from the
  editor (both already canonical-produced)
- Re-validating bytecode shape on receipt (sanity check)
- Computing + storing the content hash for replay/audit

There is no API for "submit raw bytecode" or "submit
hand-edited spans". Editors that don't compile through canonical
`compile_source` can't talk to the controller.

### 10.2 Execution limits

- **Step limit per run** — default 10M (configurable per-workflow)
- **Wall-clock timeout per run** — default 10 minutes
- **Per-connector rate limit** — configurable; default 100/min
- **Per-connector circuit breaker** — N consecutive failures →
  open circuit, fail subsequent calls fast

### 10.3 Cancellation

`DELETE /runs/:id` triggers best-effort interruption:
- Sets a cancellation flag in the VM's tick loop
- Next bytecode instruction observes the flag, returns
  `RunError::Cancelled`
- In-flight `ExtCall` is best-effort interrupted via tokio
  cooperative cancellation (most HTTP clients honor this)
- Persistence completes regardless (cancelled state recorded)

### 10.4 Connector errors

ExtCall failures return as `RunError::ExtCallFailed { connector,
message }` — runs fail loudly. There is no path where a connector
silently succeeds; if the connector can't deliver, the VM sees an
error.

## 11. Crate boundaries (Rust workspace)

```
compiler/             # SOL compiler — unchanged from Phase B
runtime/              # SOL VM — unchanged from Phase B
compiler-wasm/        # browser bridge — unchanged

host-spec/            # NEW — wire types only (RunRecord, RunEvent, ...)
                      #       serde-derived, no transport
controller/           # NEW — Controller / Connector / Scheduler /
                      #       Persistence traits; reference impls
                      #       land in C.2+
```

`controller/` depends on `compiler/` + `runtime/` + `host-spec/`.
`host-spec/` is dependency-light (just serde) so the editor's
TS mirror has a small surface to track.

The browser editor does NOT compile `controller/` — it only knows
about the wire types from `host-spec/` (mirrored in TS).

## 12. Privacy / public-safe stance

Everything in this document is described in generic terms:
"controller", "external system", "connector", "host runtime".
SolFlow ships **one reference controller** but the architecture
supports many — the wire protocol is the contract; anyone can
implement a controller against it.

No internal-system references, branded names, or proprietary
terminology leak into Phase C deliverables.

## 13. C.1 scope discipline

This bundle ships **architecture + scaffolding only**:

- ✅ This document (`PHASE_C_ARCHITECTURE.md`)
- ✅ Phased roadmap (`PHASE_C_ROADMAP.md`)
- ✅ `host-spec/` crate (wire types + serde, no transport)
- ✅ `controller/` crate (traits + stub impls only)
- ✅ TS mirror types for `host-spec`
- ✅ Editor `ControllerSettingsModal` stub (display-only, no networking)

Explicit non-goals for C.1:
- ❌ No real controller binary (C.2)
- ❌ No persistence (C.2)
- ❌ No HTTP server (C.2)
- ❌ No connectors (C.4)
- ❌ No scheduler (C.3)
- ❌ No editor → controller submission flow (C.2)

The point of C.1 is to lock the architecture before writing
implementation code — the next milestone (C.2) builds against
this design without revisiting the foundational choices.

## 14. Open questions for future milestones

### Bytecode shape vs connector indirection (C.4)

Today, `Inst::ExtCall` includes the URL as a heap string built
at codegen time. With connectors, the URL becomes
`connector://name?...`. This works without bytecode shape changes
if we treat the URL field as a generic locator string.

If we later want to support connector ARGS structured per-call
(e.g. retry overrides), we'd need a new bytecode variant. Defer
until C.4 implementation reveals whether it's necessary.

### Multi-tenant controllers (C.7)

C.2's local controller assumes single-user. Multi-user / multi-tenant
controllers (C.7+) need workflow ownership + auth. The architecture
above is silent on this deliberately — Phase D will own
auth/RBAC. Until then, controllers are single-tenant.

### Distributed execution (beyond Phase C)

The architecture supports a single controller. Workflows that
span multiple controllers (e.g. for HA or geographic distribution)
would need a coordinator role. Out of scope for Phase C; the
single-controller architecture should compose cleanly into a
multi-controller setup later.

## 15. References

- [`PHASE_C_ROADMAP.md`](./PHASE_C_ROADMAP.md) — milestone delivery plan
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — Phase B architecture (the foundation)
- [`../sol-language/SIMULATOR_PARITY.md`](../sol-language/SIMULATOR_PARITY.md) — historical record of why we don't fork semantics
- [`../sol-language/SYNC_MODEL.md`](../sol-language/SYNC_MODEL.md) — analogous explicit-action philosophy applied to source ↔ graph
