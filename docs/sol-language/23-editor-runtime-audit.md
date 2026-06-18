# 23. Editor Runtime Audit

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate
> (the `sol/` crate). This chapter audits how the SolFlow editor
> actually runs SOL today: in-browser through the `compiler-wasm`
> bridge (the same VM compiled to WASM) and remotely through the
> controller via the runtime-host client. The old "in-browser JS
> interpreter versus canonical SOL" divergence audit and every
> `E0xxx` / `T90xx` code have been removed; the editor runs the
> canonical VM in both modes.

The editor has two run modes, both surfaced by the Run modal
(`src/components/RunModal.vue`). Both execute the *same* canonical
`openprem-sol-v2` VM:

- **Browser sim**: compiles + runs the emitted SOL source through
  the `compiler-wasm` bridge's `run_source_json`, which is the
  canonical crate compiled to WASM. External Actions are blocked.
- **Controller-local**: submits the emitted SOL source to the
  connected controller, which compiles + runs it on the canonical
  crate natively and streams run events back. External Actions are
  resolved by the controller's connector registry.

There is no separate JavaScript SOL interpreter in the run path. A
JavaScript trace recorder still exists, but only to animate the
canvas; it is explicitly not the source of truth for output or
semantics (see §23.7).

---

## 23.1 The two run modes in one paragraph

The Run modal auto-runs each time it opens (`RunModal.vue:668-692`).
In browser-sim mode `execute()` calls `runSource(graph.emitted.source)`
(`RunModal.vue:159-164`), which forwards to the WASM bridge's
`run_source_json` (`src/compiler/api.ts:164-167`). In controller-local
mode `executeControllerLocal()` encodes the same emitted source as
UTF-8 bytes, submits it via `POST /workflows`, starts a run via
`POST /runs`, and polls `GET /runs/:id` to completion while an SSE
stream delivers live events (`RunModal.vue:173-257`). Both paths
converge on one `RunResult`-shaped object so the rest of the template
renders identically (`RunModal.vue:461-486`).

---

## 23.2 In-browser execution via `run_source_json`

`runSource` is the single entry point for in-browser canonical
execution (`src/compiler/api.ts:144-167`). It runs on the main
thread (explicit user action), lazy-loads the WASM module, and parses
the bridge's JSON envelope into a `RunEnvelope`
(`src/compiler/types.ts:145-150`).

The bridge's `run_source_json` (`compiler-wasm/src/lib.rs:136-180`):

1. Parses the source. A parse failure short-circuits to an
   `E_PARSE` diagnostic with `run: null` (`lib.rs:141, 183-187`).
2. Compiles for an `instruction_count` and compile-stage
   diagnostics. A codegen failure short-circuits to `E_CODEGEN`
   (`lib.rs:142`).
3. Requires a `workflow` declaration; none yields `E_NO_WORKFLOW`
   (`lib.rs:143`).
4. Drives the canonical `WorkflowExecutor` with a 64-statement
   budget per `step`, an outer guard of 200000 iterations, and
   drains the thread-local print buffer with `take_output()`
   (`lib.rs:146-167`).

The `run` object carries `return_value`, `output`, `steps`,
`runtime_error`, `runtime_error_source_span`, `trace`, and
`trace_truncated` (`lib.rs:170-177`, mirrored in
`src/compiler/types.ts:108-138`).

**Key property:** browser-sim output is canonical. The VM that runs
in the tab is the exact crate the controller runs natively; integer
versus float arithmetic, string concatenation rules, division
behavior, and enum dispatch are whatever the crate does, not a JS
approximation.

---

## 23.3 What the editor shows after a run

The Run modal renders a unified result for both modes
(`RunModal.vue:936-1106`):

| View | Source | Notes |
|---|---|---|
| Compile errors | `compileDiagnostics` | Lists the diagnostic `code` / `phase` / `message` and skips execution when compile failed (`RunModal.vue:1008-1023`) |
| stdout / output | `runResult.output` | The captured `print` lines, numbered, with a Copy button (`RunModal.vue:1056-1076`) |
| Return value | `runResult.return_value` | Shown only on clean completion; `Int`/`Float` in browser-sim, `Int`/`Bool` narrowed to `i64` on the controller (`RunModal.vue:1078-1085`) |
| Steps | `runResult.steps` | VM steps executed, shown in the status bar (`RunModal.vue:917-932`) |
| Runtime error | `runErrorMsg` + `formatRuntimeError` | Rendered as a distinct error block with an optional jump-to-source / jump-to-canvas link (`RunModal.vue:1025-1046`) |
| Generated SOL | `graph.emitted.source` | The canonical SOL the graph emitted, line-numbered (`RunModal.vue:1231-1245`) |

The status dot summarizes the run as `compile failed`, `runtime
error`, or `completed` (`RunModal.vue:921-933`).

---

## 23.4 Runtime-error views

`formatRuntimeError` maps the `RuntimeError` union onto user-facing
text (`RunModal.vue:616-637`). The union is defined in
`src/compiler/types.ts:88-101` and mirrors the controller's
`RuntimeErrorView` (`host-spec/src/lib.rs:476-501`) so editor code can
switch on `kind` exhaustively across both modes.

| `kind` | Message intent | Where it comes from |
|---|---|---|
| `DivByZero` | "Division by zero." | controller classification (`canonical_exec.rs:208`) |
| `IndexOutOfBounds` | index + length | wire shape (browser-sim does not emit it) |
| `StackUnderflow` | "compiler bug; please report" | controller classification (`canonical_exec.rs:210-211`) |
| `StepLimit` | "step limit reached, possible infinite loop" | browser-sim guard (`lib.rs:154`) and controller (`canonical_exec.rs:116-119`) |
| `ExtCallBlocked` | "external calls not available in browser simulation; switch to controller-local" | the in-browser `RemoteCall` mapping (`lib.rs:161`) and the controller's unregistered-module path (`canonical_exec.rs:153-162`) |
| `ExtCallFailed` | connector + message | controller connector failure (`canonical_exec.rs:143-149, 212-217`) |
| `HeapShapeMismatch` | "likely a compiler bug" | wire shape |
| `Cancelled` | "Run cancelled." | controller cancel/timeout flag (`canonical_exec.rs:106`) |
| `ResourceLimit` | resource + limit | controller resource cap |

In browser-sim mode only `ExtCallBlocked` and `StepLimit` actually
occur (`compiler-wasm/src/lib.rs:129-134`); the remaining variants
exist so the same exhaustive `switch` covers controller runs.

When the bridge captured a `runtime_error_source_span`, the modal
shows the failing line and offers "show source" and "show on canvas"
links that map the span back to a graph node via `findNodeForSpan`
(`RunModal.vue:760-773, 1030-1045`).

---

## 23.5 External Actions: blocked in-browser, resolved by the controller

This is the defining difference between the two run modes.

In browser-sim, the canonical VM yields a `RemoteCall` whenever the
workflow reaches an external Action (`call("m.f", p)`, imported
`m.f(args)`, or `m::rpc(args)`). The bridge cannot resolve it, so it
reports `ExtCallBlocked { function_name: capability }` and stops the
run (`compiler-wasm/src/lib.rs:161`). The modal's footer states this
plainly: external calls are blocked in browser simulation
(`RunModal.vue:1250-1255`).

In controller-local mode the controller handles the same `RemoteCall`
by resolving the capability's module against its `SOLFLOW_CONNECTORS`
registry and invoking the connector over HTTP, feeding the JSON
response back into the VM with `resolve_remote_call`
(`controller/src/canonical_exec.rs:123-163, 249-279`). A module with
no registered connector (and no `*` wildcard) stays honestly blocked
and the run is reported `Failed` with the reason in `output`
(`RunModal.vue:494-505`).

The editor never resolves an Action itself. Switching to
controller-local is the only way to exercise real external calls
(`formatRuntimeError` for `ExtCallBlocked` tells the user exactly
this, `RunModal.vue:626-627`).

---

## 23.6 Controller-local execution path

`executeControllerLocal` (`RunModal.vue:173-257`) is the remote run
driver. It carries the same canonical SOL source the browser-sim path
runs.

| Step | Call | Notes |
|---|---|---|
| Encode source | `TextEncoder().encode(source)` into `WorkflowSubmission.bytecode` | The `bytecode` field is a historical name; it carries SOL source bytes. An empty `[]` spans sidecar is sent (`RunModal.vue:191-198`) |
| Submit | `client.submitWorkflow(...)` calls `POST /workflows` | Returns a `workflow_id` |
| Run | `client.createRun({ trigger: { kind: 'Manual' } })` calls `POST /runs` | Returns a `run_id` + initial status |
| Poll | `client.pollRun(runId, { intervalMs: 200, overallTimeoutMs: 60_000 })` | Resolves on a terminal status (`src/runtime-host/client.ts:412-449`) |
| Cancel | `client.cancelRun(runId)` calls `DELETE /runs/:id` | Wired to the Cancel button while a run is in flight (`RunModal.vue:290-336`) |

The controller runs the submitted source on the canonical VM
(`controller/src/canonical_exec.rs:67-182`). Its `RunRecord.output`
(return value, print lines, steps) maps straight into the unified
`runResult` the modal renders (`RunModal.vue:462-486`).

The client enforces the wire contract: it validates
`host_spec_major` (`src/runtime-host/client.ts:399-410`), classifies
transport failures into discriminated `ControllerClientError` kinds
(`network` / `timeout` / `http` / `decode` / `version` / `auth` /
`invalid_url` / `aborted`), and the modal renders a tailored message
per kind (`RunModal.vue:577-614`). These transport errors are
distinct from a workflow runtime error: a run that reaches the
controller and fails inside the VM shows up as a `Failed` `RunRecord`,
not a `ControllerClientError`.

---

## 23.7 Live event stream

In controller-local mode the modal opens an SSE stream and collects
`RunEvent`s in `liveEvents` (`RunModal.vue:81-91`,
`src/runtime-host/event-stream.ts`). The Live tab renders each event
by `kind` (`RunModal.vue:1108-1175`):

- `Print` lines, each with an optional `source_span` "show source"
  link.
- `ExtCallStarted` / `ExtCallCompleted` with the connector + function
  name and a success/failure tag.
- `Completed` with the return value + step count.
- `Failed` with the runtime-error `kind`.

The event shapes are the `host-spec` `RunEvent` union
(`host-spec/src/lib.rs:285-377`), mirrored in
`src/runtime-host/types.ts:158-240`. The stream is closed on terminal
events, mode switch, or modal close (`RunModal.vue:93-98, 689`).

The Trace tab is browser-sim only: it lists the
source-range trace the bridge recorded, mapping each span to a graph
node where possible (`RunModal.vue:741-758, 1177-1229`). The controller
does not stream a source-range trace today; the Trace tab says so
explicitly (`RunModal.vue:1182-1188`).

---

## 23.8 The canvas animation is decorative, not authoritative

The editor still records a JavaScript interpreter trace
(`recordTrace`, `src/runtime/simulate.ts`) purely so the canvas can
animate node-by-node playback (`RunModal.vue:160-164, 639-641`). This
animation is approximate and is not the source of truth for anything:

- The text output panel, return value, and runtime errors all come
  from the canonical VM envelope (browser-sim) or the controller's
  `RunRecord` (controller-local), never from the JS trace.
- The modal footer and the canvas labeling call the animation
  "approximate animation for per-node highlighting only" and tell
  users to trust the text output for semantics
  (`RunModal.vue:1250-1262`).

Playback does not re-execute anything; it replays the recorded trace
visually (`RunModal.vue:639-641`).

---

## 23.9 Mode fallback + lifecycle

The modal degrades gracefully when the controller is unavailable:

- Controller-local mode is gated on a controller URL being set
  (`RunModal.vue:119-122`).
- If the user previously chose controller-local but the controller is
  no longer connected, opening the modal silently falls back to
  browser-sim (`RunModal.vue:676-680`).
- If the controller disconnects mid-session while the modal is open,
  the mode drops back to browser-sim (`RunModal.vue:696-703`).
- Closing the modal aborts any in-flight controller poll and closes
  the SSE stream (`RunModal.vue:681-690`).

Recent controller runs for the connected URL are listed and can be
reopened (re-fetched by id) without re-running (`RunModal.vue:347-398`).

---

## 23.10 Summary

| Editor behavior | Where |
|---|---|
| In-browser run goes through the canonical VM compiled to WASM (`run_source_json`) | `src/compiler/api.ts:144-167`, `compiler-wasm/src/lib.rs:136-180` |
| Compile diagnostics use the five bridge codes (`E_PARSE`, `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001`) | `compiler-wasm/src/lib.rs:31-48, 141-163` |
| Runtime errors share one `kind`-tagged union across both modes | `src/compiler/types.ts:88-101`, `host-spec/src/lib.rs:476-501` |
| External Actions are blocked in-browser, resolved by the controller's connectors | `compiler-wasm/src/lib.rs:161`, `controller/src/canonical_exec.rs:123-163` |
| Controller-local submits SOL source, polls to completion, streams `RunEvent`s | `src/components/RunModal.vue:173-257`, `src/runtime-host/client.ts:412-449` |
| Canvas animation is approximate JS playback, not authoritative | `src/components/RunModal.vue:639-641, 1250-1262` |

---

## 23.11 Sources cited in this chapter

- `compiler-wasm/src/lib.rs`: `run_source_json`, the five diagnostic
  codes, the in-browser run loop + blocked Actions
- `sol/src/vm.rs`, `sol/src/workflow.rs`: the canonical VM,
  `StepResult`, `take_output`
- `controller/src/canonical_exec.rs`: native canonical run,
  connector resolution, error classification
- `host-spec/src/lib.rs`: `RuntimeErrorView`, `RunEvent`,
  `RunStatus`, `SourceSpan`, `host_spec_major`
- `src/compiler/api.ts`, `src/compiler/types.ts`: `runSource` and the
  envelope shapes
- `src/runtime-host/client.ts`, `src/runtime-host/types.ts`,
  `src/runtime-host/event-stream.ts`: the controller client + wire
  mirrors + SSE stream
- `src/components/RunModal.vue`: the Run modal, both modes, the
  result views, the runtime-error views, the live stream
- Cross-references: chapters 18, 19, 22
