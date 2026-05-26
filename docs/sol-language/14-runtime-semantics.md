# 14 — Runtime Semantics

> **Status:** Scope statement only. Substantive content lands in
> commit 4.

## What this chapter answers

- How does a SOL program actually execute?
- What is the evaluation order at each step?
- What can go wrong at runtime (as distinct from compile time)?
- What guarantees does the language make about determinism, side
  effects, and isolation between sessions?

## Topics covered

1. **Execution model.** The compiler lowers the parsed/checked AST
   to a stack-based bytecode (`bytecode.rs`); a VM (`vm.rs`)
   executes it on a per-session interpreter loop. Each session has
   its own stack and environment.
2. **Argument evaluation.** Left-to-right vs. unspecified — verified
   against the `Call` instruction emission in `bytecode.rs`.
3. **Short-circuiting.** Behavior of `&&` and `||`, with explicit
   reference to the relevant bytecode handlers.
4. **`print` and side effects.** When output is flushed; whether
   `print` is guaranteed to happen in source order.
5. **External calls.** Synchronous from the program's point of view;
   blocking from the perspective of the executing function.
6. **Runtime errors.**
   - Division by zero (`error_runtime.sol`)
   - Array out-of-bounds (if confirmed at runtime)
   - Type errors that escape compile time (should be none for
     correctly-typed programs; documented here in case the VM has any
     residual checks)
7. **Termination.** When does a SOL program "finish" — return from
   the entry function, exhaustion of the bytecode, or a runtime
   error.
8. **Isolation.** What two concurrent sessions share (the
   compiler/runtime image) and what they don't (per-session
   environment, stacks, allocations).

## Cross-references

- The full diagnostic catalogue lives in
  [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md). This chapter cites
  diagnostic codes inline but does not duplicate the catalogue.
- Concurrency / scheduling at the level of the host runtime is
  outside the SOL language and outside this manual.

## Sources to be cited

- `bytecode.rs` — full instruction set and emission patterns
- `vm.rs` — execution loop, frame layout, arithmetic / branch /
  call / return handlers, runtime-error sites
- Fixtures: `error_runtime.sol`, plus any runtime-trapping behavior
  found in `test_arith.sol`, `test_array.sol`, `test_edge.sol`
