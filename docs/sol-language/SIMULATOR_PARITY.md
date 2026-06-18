# Simulator and Canonical Execution Parity

## The headline: it is the same VM

There is no separate hand-written simulator with its own semantics.
The in-browser "simulator" is the canonical SOL VM
(`openprem-sol-v2`, the `sol/` crate) compiled to WASM and invoked
through `run_source_json` in `compiler-wasm/src/lib.rs`. The same
bytecode `Compiler` and stack `Vm` that the controller runs natively
are what the browser runs in WASM.

> **Parity is high by construction: one VM, two host environments.**

The pipeline is identical on both sides — source goes through
`Parser` then `Compiler` (bytecode `Chunk`) then `Vm`, tied together
by `WorkflowExecutor` (`sol/src/workflow.rs`). The browser and the
controller differ only in the **host** that surrounds the VM: how
output is captured, whether external Actions are resolved, and what
step or resource limits apply. They do not differ in language
semantics: integer division, float coercion, enum dispatch, equality,
truthiness, and arithmetic all come from the one VM.

This means the long catalog of "simulator quirks" that older drafts
of this document described — bool-coerces-to-int addition, JS float
division, `toBool` on strings, JS operator precedence — **does not
exist**. Those were artifacts of a legacy JS interpreter that is no
longer the execution path. The browser run and the controller run
evaluate the same compiled bytecode.

## The real remaining gaps

Because the VM is shared, the only parity gaps are host-environment
differences plus one genuine language-level hazard.

### Gap 1 — external Actions do not execute in the browser

The browser host cannot make real network calls, so it does not
execute external Actions. In the VM, every `call("m.f", p)`, imported
`m.f(args)`, or `m::rpc(args)` compiles to a `RemoteCall` step
(`StepResult::RemoteCall { capability, params }` in `sol/src/vm.rs`).
The browser host (`run_source_json`) does not resolve these; it
surfaces a structured `ExtCallBlocked { function_name, url }` runtime
error and stops, rather than faking success.

The controller host resolves the same `RemoteCall` through its
connectors and resumes the VM via `resolve_remote_call`. So a workflow
that calls external Actions runs to completion on the controller but
halts at the first Action in the browser, by design.

### Gap 2 — the browser surfaces a narrow runtime-error set

The browser host emits only two structured runtime errors:

- `ExtCallBlocked { function_name, url }` — an Action was reached.
- `StepLimit { limit }` — the run exceeded the step budget.

These are the `RtErr` variants in `compiler-wasm/src/lib.rs`. Anything
the VM itself reports as `StepResult::Failed(String)` (for example
division by zero, index out of bounds, a type mismatch surfacing at
runtime) comes back as a plain `E_RUNTIME` warning diagnostic with the
VM's message string — there is no error-code taxonomy.

The controller resolves real Actions, so it can additionally surface
the wider runtime-error union that the wire protocol models in
`src/runtime-host/types.ts` (`RuntimeErrorView`): `DivByZero`,
`IndexOutOfBounds`, `StackUnderflow`, `StepLimit`, `ExtCallBlocked`,
`ExtCallFailed`, `HeapShapeMismatch`, `Cancelled`, `Timeout`,
`ResourceLimit`. Of these, only `ExtCallBlocked` and `StepLimit` ever
originate in the browser; the rest (notably `ExtCallFailed`,
`Timeout`, `ResourceLimit`) only arise once a controller is actually
resolving connectors and enforcing per-run caps. The union exists so
both sides can exhaustively match the same shape.

### Gap 3 — step and resource limits differ by host

The browser `run_source_json` drives the executor with `step(64)` in a
loop and aborts with `StepLimit` after a fixed number of iterations
(the stated limit is 1,000,000). The controller enforces its own
limits and can additionally surface `Timeout` and `ResourceLimit`.
A workflow that loops longer than the browser's budget aborts in the
browser but may complete (or hit a different cap) on the controller.

### Gap 4 — the enum first-character dispatch hazard (the real one)

This is the one genuine language-level parity gap, and it is worth
documenting because it is invisible until deploy time.

The canonical bytecode dispatches each enum variant by
`(first_char as i128) % 10` (`sol/src/compiler.rs` / `sol/src/vm.rs`).
Two variants whose first characters share a mod-10 residue therefore
compare **equal** at runtime in compiled bytecode — for example
`Status::Active` and `Status::Aborted` both reduce to `'A' % 10`.

Where does the gap come from if it is one VM? The VM's own
`Value::Enum(name, variant)` compares enum values **by name**, but the
compiled equality path for enum comparisons in bytecode goes through
the first-char hash. The result is a hazard that depends on how a
given comparison was compiled, and it is exactly the kind of mismatch
a naive by-name expectation would miss.

The editor protects against this with a structural check rather than
relying on the run to expose it. `src/graph/validate.ts` emits the
`enum-first-char-collision` warning whenever two variants in the same
enum collide on `charCodeAt(0) % 10`, telling the user to rename one
so every variant has a distinct first character. This is a warning,
not an error — the workflow still applies — but it makes the deploy
time surprise visible at edit time.

## The error model on both sides

There is no type-checker and no `E0xxx` / `T90xx` code taxonomy
anywhere in the live pipeline. The `compiler-wasm` bridge emits a
fixed five-code vocabulary:

| Severity | Phase | Code |
|---|---|---|
| Error | Parser | `E_PARSE` |
| Error | Codegen | `E_CODEGEN` |
| Error | Analyzer | `E_NO_WORKFLOW` |
| Warning | Runtime | `E_RUNTIME` |
| Error | Internal | `ICE0001` |

Type mismatches are never caught at compile time; the VM surfaces them
at runtime as `Failed(string)`, which the bridge reports as an
`E_RUNTIME` warning. The editor's own structural checks
(`src/graph/validate.ts`) use kebab-case codes (`no-entry`,
`missing-input`, `bad-inline-expression`, `enum-first-char-collision`,
and the rest); those are graph validations, distinct from the bridge's
diagnostics.

## What this means in practice

- If a workflow uses no external Actions, the browser run and the
  controller run produce the **same** return value and output, because
  they execute the same bytecode on the same VM.
- If a workflow calls external Actions, the browser run halts at the
  first one with `ExtCallBlocked`; only the controller can run it to
  completion.
- The one semantic gotcha to watch is colliding enum variant first
  characters; the editor warns about it via
  `enum-first-char-collision`.
- The wider runtime-error union (`ExtCallFailed`, `Timeout`,
  `ResourceLimit`, and the panic-class errors) is a controller-side
  surface; the browser only ever produces `ExtCallBlocked`,
  `StepLimit`, and plain `E_RUNTIME` messages.
