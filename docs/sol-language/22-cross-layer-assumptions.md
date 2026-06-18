# 22. Cross-Layer Assumptions

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate
> (the `sol/` crate). This chapter describes the real cross-layer
> contract of the current system: the editor graph, the SOL source
> it emits, the `compiler-wasm` bridge, the canonical crate, the
> controller that runs that same crate natively, and the `host-spec`
> wire types. The old standalone compiler and runtime crates were
> deleted; this chapter no longer references them, the analyzer
> phase, or any `E0xxx` / `T90xx` code system.

SolFlow has one canonical language implementation (`openprem-sol-v2`)
and several layers that wrap, feed, or invoke it. Each layer assumes
specific things of its neighbors. This chapter catalogues those
assumptions so anyone writing a tool that sits between two layers
knows the contract they must honor.

The layers, end to end:

```
Editor graph (src/graph/*)
   |  emit (src/emit/emit.ts)
SOL source text (canonical: `<-`, `#`, `[]T`, `fn`, `workflow "name" {}`)
   |  compiler-wasm bridge (parse / compile / run JSON envelope)
Canonical crate openprem-sol-v2 (sol/src/*): Lexer, Parser, Compiler, Vm
   |
   |== in-browser: compiled to WASM, run via run_source_json
   |== controller: same crate, native (controller/src/canonical_exec.rs)
                     |  host-spec wire types (host-spec/src/lib.rs)
                     v
                Editor runtime-host client (src/runtime-host/*)
```

There is exactly one execution engine. The browser sim and the
controller run the *same* crate; the only difference is whether it
is compiled to WASM or native, and whether external Actions are
resolved or blocked.

---

## 22.1 Editor graph to emitted SOL source

### What the editor emitter guarantees

The emitter (`src/emit/emit.ts`) is producer-only: it turns a
validated `SolWorkflow` graph into canonical SOL text.

| Guarantee | Source |
|---|---|
| The runnable unit emits as `workflow "name" { ... }` (name is a string literal) | `emit.ts:134-138` |
| Helper functions emit as `fn name(params) <- RetType { ... }`; void return omits the `<- RetType` | `emit.ts:135-142` |
| Return types use the canonical arrow `<-`, never `->` | `emit.ts:141` |
| Trigger annotations emit as `#` line comments preceding the header (canonical comments are `#`) | `emit.ts:145-168` |
| Struct fields and enum variants are `;`-terminated; struct/call arguments are comma-separated | `emit.ts:110-129` |
| Array types use the canonical prefix form `[]T` via `typeLabel` | `src/graph/schema.ts` (`typeLabel`) |

### What the bridge / canonical crate assumes of the emitted source

| Assumption | What breaks if violated |
|---|---|
| The source is canonical SOL the `Lexer`/`Parser` accept (`sol/src/lexer.rs`, `sol/src/parser.rs`) | The bridge returns an `E_PARSE` diagnostic; nothing runs |
| Comments are `#` to end of line; there are no `//` or block comments | `//` lexes as `Minus`/`Minus`-adjacent tokens and fails to parse |
| Return arrows are `<-`; `->` is not a token | `->` lexes as two tokens and fails to parse |
| Every workflow declaration carries a string-literal name | A bare-identifier name fails to parse |

**Cross-layer concern:** the editor is the *only* SOL producer in
the system, and it has no SOL importer. The emitter must stay inside
the canonical grammar because the bridge has no repair pass: a single
non-canonical construct turns the whole run into an `E_PARSE`
failure. Emitter correctness is the entire contract here.

---

## 22.2 Emitted source to compiler-wasm bridge envelope

The bridge (`compiler-wasm/src/lib.rs`) is a thin wasm-bindgen
wrapper over the canonical crate. Every entry point returns a
stable JSON envelope.

### What the bridge guarantees

| Guarantee | Source |
|---|---|
| Every entry point returns `Envelope { ok, value, diagnostics }`; `run_source_json` adds a `run` object | `lib.rs:36-48, 170-180` |
| `ok` is true only when no `Error`-severity diagnostic is present | `lib.rs:43-47` |
| On any internal panic the bridge returns a hand-built `ICE0001` envelope rather than crashing the host | `lib.rs:39-55` (`ice`, `guarded`) |
| Exported functions: `version`, `parse_source_json`, `analyze_source_json`, `compile_source_json`, `compile_for_wire_json`, `format_source_json`, `run_source_json` | `lib.rs:57-180` |

### The complete diagnostic vocabulary

The bridge emits exactly five codes. There is no analyzer pass and
no `E0xxx` / `T90xx` code family in the live pipeline.

| Severity | Phase | Code | Emitted when |
|---|---|---|---|
| Error | Parser | `E_PARSE` | `Parser::parse()` returns `Err` (`lib.rs:63-66`) |
| Error | Codegen | `E_CODEGEN` | `Compiler::compile()` or `WorkflowExecutor::new` returns `Err` (`lib.rs:99, 124, 142-144`) |
| Error | Analyzer | `E_NO_WORKFLOW` | `run_source_json` finds no `workflow` declaration (`lib.rs:143`) |
| Warning | Runtime | `E_RUNTIME` | a run ends in `Failed(reason)` or a stepping `Err` (`lib.rs:162-163`) |
| Error | Internal | `ICE0001` | the bridge caught a panic or a serialization failure (`lib.rs:39-55`) |

The `Analyzer` *phase* string survives only as the carrier for
`E_NO_WORKFLOW`; it does not imply a semantic-analysis pass. Type
mismatches are not caught at compile time anywhere; they surface at
run time as `E_RUNTIME` (the canonical crate returns plain string
errors, per `sol/src/vm.rs`).

### What the editor assumes of the envelope

| Assumption | Source |
|---|---|
| The TypeScript `CompileEnvelope<T>` / `RunEnvelope` shapes match the bridge's JSON byte for byte | `src/compiler/types.ts:45-150` |
| `DiagnosticPhase` is one of `Lexer \| Parser \| Analyzer \| Codegen \| Runtime \| Internal`; `Lexer` is reserved and never emitted today | `src/compiler/types.ts:12-18` |
| `run.runtime_error` is the `RuntimeError` union tagged on `kind` | `src/compiler/types.ts:88-101` |
| `run_source_json` in-browser only ever emits the `ExtCallBlocked` and `StepLimit` runtime-error variants | `lib.rs:129-134, 154, 161` |

**Cross-layer concern:** the envelope shape is the editor's hard
contract, not the internal AST. The editor mirrors the canonical
`ast::Program` in `src/compiler/ast.ts`, but the load-bearing
guarantee is the envelope wrapper plus the five-code vocabulary.

---

## 22.3 Bridge run loop to canonical VM

`run_source_json` drives the canonical `WorkflowExecutor` in-browser.

### What the canonical crate guarantees

| Guarantee | Source |
|---|---|
| `WorkflowExecutor::new(source, name)` ties parse + compile for one workflow; `step(budget)` runs up to `budget` statements | `sol/src/workflow.rs`, `sol/src/vm.rs` |
| `step` returns `Completed(Value) \| Yielded(steps) \| RemoteCall { capability, params } \| Failed(String)` | `sol/src/vm.rs` (`StepResult`) |
| `print` output accumulates in a thread-local buffer drained by `take_output()` | `sol/src/vm.rs` (`take_output`) |
| External Actions (`call("m.f", p)`, imported `m.f(args)`, `m::rpc(args)`) each surface as a `RemoteCall` carrying a capability string and one params value | `sol/src/vm.rs`, `sol/src/analysis.rs` |
| Errors are plain strings; there are no codes or spans inside the crate | `sol/src/vm.rs` |

### What the bridge run loop assumes

| Assumption | Source |
|---|---|
| A `RemoteCall` in-browser cannot be resolved, so it is mapped to `ExtCallBlocked { function_name: capability }` and the run stops | `lib.rs:161` |
| A statement budget of 64 per `step` plus an outer guard of 200000 iterations bounds infinite loops, surfacing as `StepLimit { limit: 1_000_000 }` | `lib.rs:150-154` |
| Only `Int` / `Float` returns map to a JSON `return_value`; everything else is `null` | `lib.rs:156-158` |

**Cross-layer concern:** the budget numbers are a browser-safety
choice, not language semantics. The canonical VM itself imposes no
step or depth limit; the editor wraps it in a guard so a runaway
loop cannot freeze the tab.

---

## 22.4 Canonical crate, browser and controller in lockstep

The controller runs the identical crate natively
(`controller/src/canonical_exec.rs`). This is the contract that
makes "what I saw in the browser" match "what ran on the server".

### What both sides share

| Shared invariant | Source |
|---|---|
| Same `WorkflowExecutor` pull-stepper, same `StepResult` variants | `canonical_exec.rs:1-21`, `sol/src/workflow.rs` |
| Same `print` thread-local buffer drained per run; a pooled thread is cleared before each run | `canonical_exec.rs:80-82, 184-191` |
| Same return-value narrowing (`Int`/`Bool` to `i64`, else `None`) as the browser sim | `canonical_exec.rs:193-202` |
| The controller reads SOL *source* bytes and compiles + runs them; there is no shared bytecode format between editor and controller | `canonical_exec.rs:1-12`, `host-spec/src/lib.rs:56-69` |

### What differs by design

| Aspect | Browser (`run_source_json`) | Controller (`run_canonical`) |
|---|---|---|
| External Action (`RemoteCall`) | always `ExtCallBlocked` | resolved against the `SOLFLOW_CONNECTORS` registry; unregistered modules stay honestly blocked | (`canonical_exec.rs:123-163`) |
| Step budget per `step` | 64 | 10000, re-checking cancel/timeout flags between batches (`canonical_exec.rs:33, 104-109`) |
| Cancellation / timeout | none | `Arc<AtomicBool>` flags polled between batches, surfacing as `Cancelled` (`canonical_exec.rs:104-108`) |
| Error classification | `E_RUNTIME` warning | best-effort map to `RuntimeErrorView` (`DivByZero`, `StackUnderflow`, else `ExtCallFailed`) (`canonical_exec.rs:206-219`) |

**Cross-layer concern:** the controller resolves a capability into a
real HTTP connector call. `split_capability` handles both the
`module::func` and `module.func` forms; `invoke_connector` POSTs
`{module, function, params}` to the registered base URL and feeds
the JSON response back via `resolve_remote_call`
(`canonical_exec.rs:235-279`). A module with no registered endpoint
and no `*` wildcard returns `ExtCallBlocked`, matching the browser's
honest "blocked" behavior.

---

## 22.5 Controller and editor over the wire (host-spec)

`host-spec/src/lib.rs` is a pure-data crate (serde derives, no
transport). The editor mirrors it in `src/runtime-host/types.ts`,
pinned by the round-trip tests at the bottom of the Rust file.

### What host-spec guarantees

| Guarantee | Source |
|---|---|
| `WorkflowSubmission.bytecode` is a `Vec<u8>` that in practice carries SOL *source* bytes; host-spec neither encodes nor inspects it | `host-spec/src/lib.rs:46-69, 590-599` |
| `SourceSpan { start, end }` is owned locally by host-spec (no compiler dependency) so the wire shape is stable | `host-spec/src/lib.rs:506-510` |
| `SolDiagnostic` mirrors the bridge's diagnostic shape (severity, phase, code, message, span, related, help) | `host-spec/src/lib.rs:548-559` |
| `DiagnosticPhase` is the same six-variant set as the editor's; `RuntimeErrorView` is the controller-side runtime-error union | `host-spec/src/lib.rs:531-539, 476-501` |
| `RunStatus` is a 9-state lifecycle; new states are additive (older editors render unknown strings as "Unknown") | `host-spec/src/lib.rs:136-163` |
| `RunEvent` is the streamed execution-event union, tagged on `kind`, with monotonic `seq` | `host-spec/src/lib.rs:285-377` |
| `host_spec_major` is the compatibility gate; editor and controller refuse to connect on mismatch | `host-spec/src/lib.rs:23, 621-639` |

### What the editor client assumes

| Assumption | Source |
|---|---|
| The submission carries source bytes; the editor encodes `graph.emitted.source` via `TextEncoder` and an empty `[]` spans sidecar | `src/components/RunModal.vue:189-199` |
| Discriminated unions are tagged on a `kind` field exactly as the Rust serde derives produce | `src/runtime-host/types.ts:69-240` |
| `host_spec_major` must equal `HOST_SPEC_MAJOR`; the client throws a `version` error otherwise | `src/runtime-host/client.ts:399-410` |
| Terminal run statuses are `Succeeded` / `Failed` / `Cancelled`; `pollRun` resolves on those | `src/runtime-host/client.ts:280-284, 446` |

**Cross-layer concern:** the wire types own their own `SourceSpan`
and diagnostic shapes so that the editor and controller agree byte
for byte without either depending on the compiler crate. The field
named `bytecode` is a historical name; the blob is SOL source.

---

## 22.6 Editor runtime-host client to controller run

The editor's controller-local run path (`src/components/RunModal.vue`,
`src/runtime-host/client.ts`) submits a workflow, starts a run, and
polls or streams events.

| Step | Editor call | Controller endpoint |
|---|---|---|
| Submit source | `submitWorkflow` | `POST /workflows` |
| Start run | `createRun` with `trigger: { kind: 'Manual' }` | `POST /runs` |
| Wait | `pollRun` (200ms cadence, 60s cap) | `GET /runs/:id` |
| Cancel | `cancelRun` | `DELETE /runs/:id` |
| Stream | SSE event stream | `GET /runs/:id/events` |

| Assumption | What breaks if violated |
|---|---|
| The controller compiles + runs the submitted source on the canonical VM | A controller that expected bytecode would reject the source bytes |
| Capabilities the workflow calls resolve to connectors the controller has registered | Unregistered Actions return `ExtCallBlocked`; the run is reported `Failed` with the blocked reason in `output` |
| `host_spec_major` matches before any run call | The client refuses with a `version` error |

**Cross-layer concern:** the editor never resolves an external
Action itself. In browser-sim mode every Action is blocked; in
controller-local mode the controller is the only place Actions
become real HTTP calls. This is the single most important behavioral
seam between the two run modes.

---

## 22.7 Editor-side structural validation to emitted source

`src/graph/validate.ts` runs cheap structural checks on the graph
*before* emission. These are NOT compiler diagnostics; they use
kebab-case codes and exist to catch graph problems the emitter would
otherwise turn into broken SOL.

The full editor check vocabulary: `no-entry`, `unnamed-function`,
`enum-first-char-collision`, `missing-input`, `bad-inline-expression`,
`unset-struct`, `unknown-struct`, `unset-field`, `unset-enum`,
`unknown-enum`, `unset-variant`, `unset-call`, `unknown-call`,
`unset-var`, `unresolved-var`, `type-mismatch`
(`validate.ts:39-319`).

| Editor check | Hazard it guards |
|---|---|
| `missing-input`, `bad-inline-expression` | A node with an unsatisfied required port or a non-canonical inline expression would emit SOL the bridge rejects with `E_PARSE`. These two codes are the ones the Sol Man store treats as never-bypassable (`validate.ts:130-176`) |
| `unknown-struct` / `unknown-enum` / `unknown-call` | A reference to a declaration that does not exist would emit dangling SOL |
| `enum-first-char-collision` | The canonical bytecode dispatches each enum variant by `(first_char as i128) % 10`, so two variants whose first characters share a mod-10 residue compare equal at run time even though the by-name browser sim runs them correctly. The editor surfaces a warning (`validate.ts:66-102`) |

**Cross-layer concern:** the editor validator is a pre-flight gate,
not a type checker. A graph that passes validation and emits cleanly
can still fail at the bridge's parse or codegen stage, or surface an
`E_RUNTIME` warning when run. Structural validity does not imply
semantic correctness.

---

## 22.8 The bypass paths

A few ways to reach the canonical VM while skipping a layer's
guarantees:

1. **Hand-written workflow JSON loaded into the store.** Skips the
   Sol Man repair pass; the graph goes straight to the validator and
   emitter. Structural shape is checked; canonical semantics are not.
2. **`force` apply in Sol Man.** Bypasses the editor validator gate.
   The emitter still runs and the emitted SOL still goes through the
   bridge, so the failure surfaces as an `E_PARSE` / `E_CODEGEN`
   diagnostic at run time rather than at apply time.
3. **Calling a bridge entry point directly with arbitrary text.**
   Skips the editor entirely. The bridge's `guarded` wrapper still
   contains any panic as `ICE0001`, and the five-code vocabulary
   still applies.
4. **Submitting source to the controller without the editor.** Any
   client that speaks `host-spec` can `POST /workflows` with source
   bytes; the controller compiles + runs it on the same canonical VM.
   The connector registry still gates external Actions.

Each bypass widens what reaches the VM, but none of them changes the
execution semantics: there is one crate, and it produces string
errors, the five bridge codes, and the `RuntimeErrorView` wire union.

---

## 22.9 Sources cited in this chapter

- `sol/src/lexer.rs`, `sol/src/parser.rs`: canonical grammar the
  emitted source must satisfy
- `sol/src/workflow.rs`, `sol/src/vm.rs`: `WorkflowExecutor`,
  `StepResult`, `take_output`, string errors
- `sol/src/analysis.rs`: capability extraction
- `compiler-wasm/src/lib.rs`: the JSON envelope, the five diagnostic
  codes, the in-browser run loop
- `host-spec/src/lib.rs`: wire types `WorkflowSubmission`,
  `SourceSpan`, `SolDiagnostic`, `RunStatus`, `RunEvent`,
  `RuntimeErrorView`, `host_spec_major`
- `controller/src/canonical_exec.rs`: native canonical execution,
  connector resolution, error classification
- `src/emit/emit.ts`: graph to canonical SOL
- `src/graph/validate.ts`: editor structural checks
- `src/compiler/types.ts`, `src/runtime-host/types.ts`: the editor
  mirrors of the envelope and wire shapes
- `src/runtime-host/client.ts`, `src/components/RunModal.vue`: the
  controller-local run path
- Cross-references: chapters 18, 19, 23
