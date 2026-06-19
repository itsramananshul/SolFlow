# Changelog

User-facing changes to SolFlow, by release.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
SolFlow uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### OpenPrem-native provider system (production commit a48ba48)

SolFlow is now OpenPrem-native. The OpenPrem SDK protocol is the canonical
provider system; the SolFlow-specific `SOLFLOW_CONNECTORS` registry is demoted
to an internal/dev/test fallback only.

- **`POST /register` is canonical.** SolFlow's Local Controller plays the role
  of one OpenPrem controller: real upstream OpenPrem SDK agents register via
  `POST /register` and SolFlow invokes them directly with the upstream
  controller-to-agent wire contract (object params flattened with `capability`
  merged in, scalar params wrapped). `/providers` lists registered OpenPrem
  agents and their actions; Browser Simulation still blocks external calls
  clearly; the trace shows EXTCALL then EXTRESULT; missing providers fail with
  a source-mapped error.
- **Local/dev mode is unauthenticated by design.** SolFlow omits
  `controller_public_key` from the `/register` response, so upstream Python and
  Rust agents register and run without enforcing Ed25519 signatures. Signing is
  future work.
- **Canonical sol grammar accepts the upstream example dialect** so the
  controller runs raw `.sol` verbatim: zero-arg and multi-arg namespace calls,
  bare-identifier workflow names, unparenthesized `if` / `while`, `x.len()`
  method desugaring, and native `__system.sleep`.
- **Upstream examples compatibility suite: 19 of 19 covered.** 18 run end to
  end with their upstream-shipped agents unchanged (Python and TypeScript/JS
  SDKs). 1 (`supply-chain/check-inventory`) runs with a clearly labeled SolFlow
  compatibility fixture (`tools/openprem-compat/central_warehouse_fixture.py`)
  because the upstream repo ships no provider implementation for it; the
  upstream `.sol` is unchanged. See `docs/dev/OPENPREM_COMPAT_MATRIX.md` and
  `docs/dev/OPENPREM_PROVIDERS.md`.
- **Known caveats.** `diagnostic` is 2 of 4 on Windows because the upstream
  agent calls Unix-only `os.getloadavg()` (not a SolFlow issue). Four
  `while(true)` worker examples do not terminate by design; the compatibility
  harness cancels them after confirming repeated provider invocation. Provider
  auth runs in unauthenticated local/dev mode.

### Added — Phase C C.7 (remote controller support)

- **TLS / HTTPS support.** Setting
  `SOLFLOW_CONTROLLER_TLS_CERT` + `SOLFLOW_CONTROLLER_TLS_KEY`
  switches the controller binary from plain HTTP to TLS via
  `axum-server` + rustls (ring-backed). HTTP remains the
  default. Half-configured TLS (only one of the two vars set)
  is refused at boot with a clear stderr message so operators
  never get a silent HTTP fallback.
- **Bearer-token auth.** Setting `SOLFLOW_CONTROLLER_AUTH_TOKEN`
  requires `Authorization: Bearer <token>` on every protected
  endpoint. `/healthz` stays open so editors can fingerprint +
  capability-probe before sending credentials. Comparison is
  constant-time. Distinct 401 codes (`auth_missing` /
  `auth_malformed` / `auth_mismatch`) ride through to the editor
  for per-case guidance.
- **Health capability probe.** `/healthz` response now carries
  `name` ("solflow-controller") + `auth_required` so editors
  can decide what to render BEFORE the user gets a 401.
  Additive on the wire — pre-C.7 controllers still parse fine.
- **TS client hardening.** New `authToken` option on
  `controllerClient(...)`; new error kinds `auth` (carrying
  `code: auth_missing | auth_malformed | auth_mismatch |
  unauthorized`) and `invalid_url` (carrying `reason: no_scheme
  | bad_scheme | unparseable | no_host`). `normalizeBaseUrl`
  now parses via WHATWG URL; never silently rewrites HTTP→HTTPS.
- **`classifyControllerUrl(url)`** — pure helper returning one
  of `local` / `loopback_https` / `https_remote` /
  `unsafe_remote` / `invalid`. Drives the editor's transport
  badge + unsafe-HTTP warning.
- **ControllerSettingsModal upgrade.** Transport badge next to
  the URL field; unsafe-HTTP banner when typing plain http to a
  remote host; new Authentication section (bearer token, masked
  input, localStorage-persisted); auth-error banner with
  per-code guidance; expanded connection-detail card (controller
  name, auth-required, transport).
- **Execution-mode list** distinguishes `controller-local` vs
  `controller-remote` based on whether the connected URL
  resolves to a loopback or remote host.
- **`docs/dev/REMOTE_CONTROLLER.md`** — TLS setup recipe with
  working `openssl req` command, URL classification table,
  auth-failure-mode guidance, smoke recipes, deployment
  guardrails.

### Added — Phase C C.8 (stabilization + release packaging)

- **`docs/dev/CONTROLLER_OPERATIONS.md`** — full operator
  reference: env-var table (every var the binary reads),
  startup log format, lifecycle-of-a-request walkthrough,
  health-probe + dashboard endpoint guidance, failure-mode
  catalog, resource-sizing rules of thumb, backup recipes.
- **`npm run release:check`** — single-command release gate.
  Runs typecheck + vitest + cargo test --workspace + controller
  release build + editor build. Per-stage timing summary; aborts
  on first failure.
- **`npm run package:local`** — assembles a versioned release
  bundle under `dist-release/solflow-<v>-<platform>-<arch>/`
  containing the release controller binary, the editor dist,
  controller SQLite migrations, curated operator-facing docs,
  LICENSE, CHANGELOG, and a top-level RELEASE.txt with smoke
  + prod boot recipes.
- **`npm run build:controller`** — convenience alias for
  `cargo build --release --bin solflow-controller`.

### Internal — C.7 + C.8

- `host-spec::Health` gains `name` + `auth_required` (serde-
  defaulted; pre-C.7 controllers still parse). New
  `CONTROLLER_NAME` constant exported from both sides.
- `controller::AuthConfig::{Disabled, Bearer}` +
  `AuthFailure::{Missing, Malformed, Mismatch}` + constant-
  time-byte comparison. `LocalController::with_auth(...)`
  builder.
- `controller::server::require_bearer_token` axum middleware:
  protects every endpoint except `/healthz` + `OPTIONS`;
  rejects with structured 401 + `code` discriminator.
- `controller::tls` module: `TransportConfig::{Http, Https}` +
  `tls::from_env(...)`. Binary refuses half-configured TLS.
- `controller::Cargo.toml` adds `axum-server` (TLS) +
  `rustls = { features = ["ring"] }` (process-level
  CryptoProvider) + `rcgen` (dev-dep for self-signed test
  certs).
- 4 server auth tests + 5 tls config unit tests + 2 HTTPS
  integration tests (rcgen-minted ephemeral cert,
  `axum_server::bind_rustls` round trip via reqwest).
- TS `controller.store` gains `authToken` ref +
  `setAuthToken(...)` with localStorage persistence; cached
  client keys on (url, token); `error{auth}` reason variant.
- +21 Rust tests / +22 vitest across C.7. Totals: **181 rust +
  158 vitest at C.8 close.**

### Added — Phase C C.6 (multi-run management + orchestration maturity)

- **Concurrent execution.** The controller's `RunManager` runs
  up to N workflows in parallel (default 8, configurable via
  `SOLFLOW_CONTROLLER_MAX_CONCURRENT_RUNS`). Queue capacity
  defaults to 64; configurable via
  `SOLFLOW_CONTROLLER_MAX_QUEUED_RUNS`.
- **Real cancellation.** `DELETE /runs/:id` now interrupts the
  VM between instructions, aborts in-flight HTTP connector
  calls via a 50ms cancel poll, and lands the run on
  `RunStatus::Cancelled`. Cancellation latency = one VM step
  (~µs) under normal load.
- **Expanded lifecycle:** `RunStatus` gains `Starting`,
  `Cancelling`, `TimedOut`, `Rejected` (additive — old wire
  formats still parse). State machine enforced via
  `RunStatus::can_transition_to`.
- **Wall-clock timeout** lands as `RunStatus::TimedOut` (not
  generic Failed) with a dedicated `RunEvent::TimedOut
  { wall_clock_secs }`.
- **Saturation policies.** Default `Queue` returns HTTP 503
  with `code: "queue_full"` when at capacity; alternative
  `Reject` persists a `Rejected` terminal record. Editor
  surfaces a friendly "controller busy" message on
  queue-full.
- **At-least-once boot recovery.** Runs that were Running,
  Starting, or Cancelling when the controller crashed get
  re-enqueued on restart. Sticky `cancel_requested` bit
  survives the crash so mid-cancel runs finalize Cancelled
  on the first post-reboot dispatch.
- **Per-run resource caps.** `max_output_lines` (default 100k)
  and `max_events_per_run` (default 1M) protect against
  runaway workflows. Cap violation surfaces as
  `RuntimeErrorView::ResourceLimit { resource, limit }`.
- **Active runs UI.** New `ActiveRunsModal` (Toolbar) polls
  `/runs/active` + `/controller/concurrency` every 2 s; shows
  live runs with per-row Cancel + a saturation-tinted
  concurrency banner ("Running 8/8 · Queued 12/64").
- **Run modal Cancel button** appears in controller-local
  mode while a run is in flight; one click → real
  cancellation through the orchestration loop.
- **Scheduler routes through orchestration.** Timer + Event
  triggered runs hit the same queue + concurrency caps as
  Manual runs; scheduler ticks gracefully retry on next
  cadence if the queue is full.
- **Authoritative `docs/dev/RUN_LIFECYCLE.md`** documenting
  the state machine, cancellation paths, recovery semantics,
  concurrency policy, observability events, HTTP API, and
  failure-mode troubleshooting.

### Internal

- New `controller::run_manager` module: `RunManager` (mpsc
  queue + `tokio::sync::Semaphore` worker gating + in-memory
  active registry), `ConcurrencyPolicy`, `SaturationPolicy`,
  `EnqueueOutcome`, `ConcurrencyMetrics`.
- `host-spec::RunStatus` + `RunEvent` extended additively; new
  `transition_to(InvalidTransition)` helper enforces the
  state machine in one place; `RunStatus::is_terminal`,
  `can_transition_to`, `RunEvent::is_terminal` updated.
- Runtime VM gains `cancel_callback: CancelCallback`
  (`Arc<dyn Fn() -> bool>`) polled between every instruction
  + `max_output_lines` cap on the print buffer. Browser-sim
  installs neither → zero overhead in the WASM path.
- `RunError::Cancelled` + `RunError::ResourceLimit`; both
  bridged through `RuntimeErrorView` so the SSE / event-log
  surfaces stay uniform.
- `ConnectorInvocation::cancel_flag` + `HttpConnector`
  `tokio::select!`-races each in-flight request against the
  flag.
- Migration `0004_lifecycle_expansion.sql` relaxes the runs
  CHECK constraint + adds `cancel_requested INTEGER` column.
- New HTTP routes: `GET /runs/active`,
  `GET /controller/concurrency`.
- TS `runtime-host`: `ActiveRunSummary`, `ConcurrencyMetrics`,
  `SaturationPolicy` types; `listActiveRuns` +
  `getConcurrencyMetrics` client methods; `ActiveRunsModal`
  + RunModal Cancel button.
- +25 Rust tests, +2 vitest across C.6. Totals: 160 rust + 136 vitest.

### Added — Phase C C.5 (event log + observability)

- **Real-time run event stream.** Every run on the controller
  emits structured events (Queued / Started / Print /
  ExtCallStarted / ExtCallCompleted / Completed / Failed)
  persisted in SQLite + broadcast over an SSE endpoint
  (`GET /runs/:id/events`). Long-running workflows show live
  output in the editor without polling.
- **`Live` tab in the Run modal** — for controller-local runs,
  every event the controller emits renders as it arrives.
  Print rows carry source spans (looked up via the workflow's
  instruction_spans sidecar) so users can click straight to
  the source line or canvas node.
- **Run History modal** (Toolbar list-with-arrow icon) — past
  runs queryable by workflow / status / limit. Clicking any
  row opens an inline event replay panel that streams the
  full persisted event log via SSE.
- **`docs/dev/EVENTS.md`** — event-type reference, architecture
  diagram, HTTP API + lifecycle, TS client examples, "add a
  new event kind" recipe, failure-mode troubleshooting.

### Internal

- `migrations/0003_run_events.sql` with `(run_id, seq)`
  composite PK; `Persistence::append_event` + `list_events`
  go from no-ops (C.2 stubs) to real implementations plus
  a non-trait `list_all_events` for SSE replay-from-start.
- VM gains optional `print_callback: PrintCallback` —
  browser-safe (no callback installed by compiler-wasm) but
  lets the controller fire `RunEvent::Print` per print
  instruction with the line + inst_ptr.
- New `controller::event_sink` module with the `EventSink`
  trait, `PersistentEventSink` (SQLite + 1024-event tokio
  broadcast), and `RunEventCtx` per-run helper sharing an
  `Arc<AtomicU64>` seq counter across sources.
- SSE handler in axum recovers from broadcast `Lagged` by
  re-querying the persistent log + emits 15s keep-alive
  heartbeats for reverse-proxy compatibility.
- New TS `openRunEventStream(...)` wrapping the browser
  `EventSource` API with `onDone('terminal' | 'closed')`
  discrimination and a `eventSourceCtor` test seam.
- +7 Rust tests, +7 vitest. Totals: 137 rust + 134 vitest.

### Added — Phase C C.4 (connector framework)

- **`ext function` works for real now.** When a workflow runs
  through a controller, ExtCall instructions dispatch through a
  typed connector registry instead of returning the
  browser-sim "ExtCallBlocked" error. Browser-sim's blocked
  behavior is unchanged — it's the same VM, just with no
  handler installed.
- **HTTP reference connector** — `connector://http?url=...&method=POST`
  speaks HTTP/1.1 + HTTP/2 with conservative defaults (10s wall
  clock, 1 MiB response cap, retries off). Configurable per-call
  via URL params (`timeout_ms`, `header.<name>`, `body_format`).
  Optional host allowlist for production deployments.
- **Connector URL grammar:**
  `connector://<name>?<key>=<value>(&...)`. Parsed by
  `parse_connector_url(...)`; rejects non-connector schemes and
  path segments.
- **Structured runtime error model** — `RunError::ExtCallFailed
  { connector, function_name, message }` carries every connector
  failure mode (timeouts, retries exhausted, 4xx/5xx, DNS,
  auth, payload/response-too-large, URL-not-allowed) so the
  editor renders distinct UX per failure kind.
- **`GET /connectors`** HTTP endpoint + editor surface:
  `ControllerSettingsModal` shows a Connectors section listing
  each connector's name, description, version, and default
  policy when connected.
- **`docs/dev/CONNECTORS.md`** — full URL grammar, HTTP connector
  reference, type-bridging rules, security boundaries (allowlist,
  size caps), end-to-end example, troubleshooting, "add a new
  connector" recipe.

### Internal

- New `controller::connector` module with `Connector` trait +
  `ConnectorRegistry` + `ConnectorError` (14 variants) +
  `InvocationPolicy` + `ConnectorMeta`.
- New `runtime::extcall` module: `ExtCallHandler` callback
  trait, `ExtCallType` (primitives only in C.4), `ExtCallValue`
  bridging the synchronous VM to async connectors via
  `tokio::runtime::Handle::block_on` inside `spawn_blocking`.
- `compiler-wasm` mirrors the new `ExtCallFailed` variant in its
  `RuntimeErrorView` (no behavior change for browser-sim).
- TS `ControllerClient::listConnectors()` + `useControllerStore`
  populates connectors on connect (degrades to empty if the
  controller's `/connectors` 404s).
- +32 Rust tests, +3 vitest. Totals: 130 rust + 127 vitest.

### Added — Phase C C.3 (scheduling MVP)

- **Timer + Event triggers** — workflows can now run on a cron
  cadence or in response to webhook POSTs. The controller's
  tokio scheduler ticks every second; due Timer schedules fire
  automatically and Event schedules fire on
  `POST /events/:path`.
- **`Schedules` modal** (Toolbar clock icon) — workflow-scoped
  list with enable/disable/delete, create form for Timer
  (cron expression) and Event (path) triggers, and a test-fire
  webhook pane so you can validate Event schedules without an
  external sender.
- **Schedule persistence** — schedules live in SQLite and survive
  controller restarts. The scheduler resumes from the persisted
  `next_fire_at`.
- **HTTP API additions:**
  `POST` / `GET` `/workflows/:id/schedules`,
  `GET` / `DELETE` / `PATCH` `/schedules/:id`,
  `POST /events/*path` (wildcard for multi-segment paths).
- **`docs/dev/SCHEDULING.md`** — cron syntax cheatsheet,
  HTTP API examples, failure-mode table.

### Internal

- 8 new schedule methods on the Persistence trait;
  `TokioScheduler` in `controller/src/scheduler.rs` owns the
  tick loop + cron normalization (`*/5 * * * *` style → 7-field
  internal form).
- `cron = "0.16"` added as a controller dep.
- ControllerClient gains 6 schedule methods + structured-error
  handling for all of them.
- +21 Rust tests, +6 vitest. Totals: 98 rust + 124 vitest.

### Added — Phase C C.2 (controller MVP, local)

- **`solflow_controller` binary** — `cargo run -p solflow_controller`
  boots a local HTTP controller (default `127.0.0.1:3939`) with
  SQLite persistence. Config via `SOLFLOW_CONTROLLER_BIND`,
  `_DB`, `_STEP_LIMIT`, `_TIMEOUT_SECS` env vars.
- **`Controller Settings` modal** is now live (was a C.1 stub):
  real /healthz check, connect/disconnect, retry on error,
  prominent UX for each failure mode (network / timeout /
  HTTP / decode / version-mismatch / invalid URL). URL +
  auto-reconnect persisted to localStorage.
- **Run modal mode selector** — Browser-sim / Controller-local
  toggle. Controller-local mode compiles for wire, submits to
  the controller, polls until terminal, shows workflow_id +
  run_id + duration. Same canonical SOL VM either way.
- **Run history per controller** — collapsible "Recent runs"
  section in the Run modal with Reopen, proving the controller's
  persistence survives restarts.
- **Developer docs** — `docs/dev/CONTROLLER_LOCAL.md` with
  how-to-run, env vars, HTTP API quick reference, troubleshooting.

### Internal

- `host-spec` ships JSON wire-encoding helpers (`encode_bytecode`,
  `encode_instruction_spans`); `Inst` gains a feature-gated serde
  derive.
- New `src/runtime-host/client.ts` — typed `controllerClient(url)`
  with structured `ControllerClientError`, AbortSignal timeouts,
  host-spec major check, `pollRun` with overall-timeout.
- Pinia stores: `useControllerStore` (connection state machine)
  and `useControllerRunHistoryStore` (per-URL history index).
- 20 new controller tests (Rust) covering persistence, executor,
  LocalController end-to-end, and axum routes; 18 new client
  tests (TS). Workspace totals: 77 Rust + 97 vitest.

## [0.2.0] — 2026-05-27 — Productization release

Phase B + deferred-B + productization. SolFlow becomes a
public-presentable product:

### Added

- **User documentation** under `docs/user/` — Quickstart,
  Install, FAQ, Editor Guide. Three-track docs split:
  user / sol-language / dev.
- **`CONTRIBUTING.md`** + **`LICENSE`** (MIT).
- **Docs links in HelpModal** — Quickstart / Editor Guide / FAQ
  / SOL Language reachable from the `?` key inside the editor.
- **CI gate on bundled samples** — every sample on the welcome
  screen now has automated assertions that its emitted SOL
  parses + analyzes cleanly via the canonical compiler.

### Changed

- Escape-key now closes RunModal and ImportReportModal (was
  backdrop-click + ✕ button only). Brings them in line with the
  other modals.
- Privacy scrub: removed all internally-branded references from
  public-facing files. Sample names + descriptions now use
  generic language.
- README + repo organization restructured for public landing
  consumption.

### Not changed

The Phase B compiler-backed IDE architecture is unchanged: this
release is productization polish, not engineering. See the
**Phase B** entry below for what shipped engineering-wise.

## [0.1.0+B] — 2026-05-27 — Phase B + Deferred-B

Engineering completeness milestone. SolFlow runs on canonical
SOL semantics throughout the compile + execute pipeline.

### Phase B (B.1 – B.11)

- **Standalone SOL Rust compiler** vendored into `compiler/`
  with diagnostics-as-values, parser recovery, analyzer recovery
- **wasm-bindgen bridge** in `compiler-wasm/` — stable JSON envelopes
- **Live in-browser compiler diagnostics** with click-to-source
- **AST → graph importer** with honest classification (full /
  partial / source-only / unsupported)
- **Graph → source canonicalization** with round-trip stability
  tests
- **Sync model** explicit-action only (no live two-way binding;
  see `SYNC_MODEL.md`)
- **Canonical SOL VM in WASM** — `runtime/` crate; external
  calls blocked with structured `ExtCallBlocked` error rather
  than faked
- **VM hardening** — GetField/SetField OOB returns structured
  error instead of panic

### Deferred-B (c35 – c46)

- **AST source spans** flow through analyzer diagnostics + importer
  attachments + codegen sidecar
- **Importer expansion** — fieldSet, indexSet, top-level let
  auto-wrap into `__init()`
- **Web Worker** for hot-path parse/analyze (UI no longer freezes
  on long files)
- **Node-target WASM** for true e2e round-trip tests
- **Per-instruction span sidecar** in codegen
- **VM execution trace** + runtime-error spans
- **Per-node source attachment** on imported graph nodes
- **RunModal Trace tab** with click-to-source + click-to-canvas
  navigation

Test scoreboard at end of Deferred-B:
```
vitest    79 / 79
cargo     49 / 49
total    128 / 128
```

## [0.1.0] — Earlier — Phase A vertical slice

The original Vue 3 + Vue Flow editor with a TypeScript-only
graph emitter and a JS approximation interpreter. Foundation for
the canonical compiler work that followed in Phase B.

Phase A features (all retained in v0.2.0):
- Visual graph editor with 22 node kinds
- Live source preview (graph → SOL)
- Sample workflows (Hello, Monitor, Orchestration, Payments,
  Enterprise)
- Sol Man — LLM-assisted workflow generation (BYO key)
- Pinia-based state + autosave to localStorage
- 5 sample workflows on the welcome screen

[Unreleased]: https://github.com/itsramananshul/SolFlow/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/itsramananshul/SolFlow/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/itsramananshul/SolFlow/releases/tag/v0.1.0
