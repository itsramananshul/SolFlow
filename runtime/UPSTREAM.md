# Provenance of `solflow_runtime`

This crate's `vm.rs` is vendored from the upstream sibling
workspace (snapshot 2026-05-27). Same provenance rules as the
`compiler/` crate — see `../compiler/UPSTREAM.md`.

## What was vendored

- `vm.rs` — the bytecode stack-machine interpreter

## Surgical edits applied during vendoring

The upstream VM is a CLI / host-runtime binary's interpreter. The
SolFlow build is a browser-side simulator. Four concrete behavioral
changes were made; nothing else.

1. **Captured output instead of stdout.** Every `println!(...)` +
   `io::stdout().flush()` was replaced with `self.output.push(format!(...))`.
   The `output: Vec<String>` field on `VM` surfaces back to the
   caller. Browsers don't have stdout; this keeps SOL `print`
   semantics observable.

2. **`Inst::ExtCall` returns a structured error.** Upstream opens a
   real `std::net::TcpStream` and speaks HTTP/1.1 to a controller
   endpoint. Browsers can't do raw TCP. We refuse the operation
   and return `RunError::ExtCallBlocked { name, url }` so the UI
   can render an honest "external call not available in browser
   simulation" message. Cataloged in SIMULATOR_PARITY.md as the
   one remaining intentional drift.

3. **Step budget.** New `step_limit: usize` (default 1_000_000).
   Infinite loops in user code would otherwise freeze the tab;
   the limit produces a `RunError::StepLimit` instead.

4. **Common runtime errors returned as values.** Upstream
   `panic!("Runtime Error: ...")`s for div-by-zero, array OOB,
   stack underflow, and a few heap-shape mismatches. We convert
   those to `RunError::*` variants so the UI can render them
   alongside other diagnostics rather than tripping the boundary's
   panic catcher. Truly invariant-only sites stay as panics —
   they're caught by `catch_unwind` at the WASM boundary and
   surface as ICE diagnostics, which is the right shape for "the
   compiler emitted bytecode that violates a VM invariant."

## What was NOT vendored

- `network/` — libp2p / controller transport
- `session.rs` — host session lifecycle
- `init.rs` — host loader
- `handler.rs` — controller dispatch
- `cli.rs`, `main.rs` — host binary

None of those would compile to WASM cleanly even if we wanted
them; the VM is the only piece SolFlow's simulator needs.

## Why a separate crate

- The compiler crate stays compiler-only; no execution dependency.
- The VM can be unit-tested independently (no WASM toolchain
  needed to verify canonical semantics).
- `compiler-wasm` depends on both crates and exposes
  `run_source_json` — single browser bundle, two-crate
  test surface.

Privacy posture matches the compiler crate: upstream workspace
name + paths are not recorded here, only this snapshot date.
