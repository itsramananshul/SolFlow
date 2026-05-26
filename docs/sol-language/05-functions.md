# 05 — Functions

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

- How is a function declared?
- What are the calling conventions — argument evaluation order,
  return semantics?
- What distinguishes `function`, `ext function`, and `export function`?
- How does forward declaration work?
- Are recursion and mutual recursion supported?
- Which function does the host execute when it loads a session?

## Topics covered

1. **Declaration syntax.** Parameter list, optional return type,
   body. Whether `-> T` is required vs. omitted for void.
2. **Parameters.** Type annotations, multiple parameters, parameter
   ordering rules, what happens when an argument's type doesn't match.
3. **Returns.** Plain `return;` vs. `return <expr>;`. Whether every
   path in a non-void function must return (verified against
   `analyzer.rs` and the `retest.sol` note about an omitted return
   in `start`).
4. **Forward declarations.** Behavior demonstrated by `fwdecl.sol` —
   how the analyzer's two-pass design admits call-before-definition.
5. **Recursion.** Direct and mutual; verified against `test_func.sol`.
6. **External entry points.** `ext function name() -> T;` (no body)
   declares that a function is provided by the host runtime. The
   syntactic form is part of the language; the binding mechanism is
   covered in chapter 12.
7. **Exported entry points.** `export function name(…) -> T { … }`
   advertises a function as callable from outside the SOL program.
8. **Entry function naming.** The conventional name (`start`) and
   what the compiler / runtime require formally.

## Common mistakes

- Returning a value from a function declared without `-> T`
- Calling a function not declared in the file (and not declared `ext`)
- Re-declaring a function name (`error_semantic3.sol`)
- Leaving an `ext function` with a body, or an `export function`
  without one

## Sources to be cited

- `parser.rs` function-declaration block
- `analyzer.rs` symbol table + forward-declaration pass; return-path
  analysis
- `vm.rs` call frame mechanics
- Fixtures: `test_func.sol`, `fwdecl.sol`, `retest.sol`,
  `error_semantic3.sol`
- `syntax_test.sol` (shows `ext function` and `export function` side
  by side)
