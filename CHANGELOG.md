# Changelog

User-facing changes to SolFlow, by release.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
SolFlow uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
