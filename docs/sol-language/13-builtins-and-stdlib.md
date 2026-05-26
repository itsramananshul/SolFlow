# 13 — Built-ins and Standard Library

> **Status:** Scope statement only. Substantive content lands in
> commit 4.

## What this chapter answers

- Which names are reserved by the language itself rather than by the
  host runtime?
- What does each built-in do, and what is its type signature?
- Are there builtin types vs. builtin functions vs. builtin operators
  — and which is which?
- Is there a standard library beyond the built-ins?

## Topics covered

1. **`print`.** Side-effecting output. Signature, accepted argument
   type(s), and observable behavior. To be confirmed against the
   analyzer's handling of the `print` identifier and verified
   against `s1.sol`, `s2.sol`, `retest.sol`.
2. **Reserved keywords vs. reserved identifiers.** The set is
   enumerated from `lexer.rs` and reproduced in
   [`GRAMMAR.md`](./GRAMMAR.md).
3. **Math, string, and collection helpers.** Documented if present.
   If absent, that is stated explicitly so consumers know not to
   reach for `len(arr)` / `str.length` and instead look at host
   `ext` functions.
4. **Casting / conversion.** Whether `int` ↔ `float` or `int` ↔ `str`
   conversions are spelled as builtin calls, operator syntax, or are
   not provided at all.
5. **Stability of this list.** Built-ins are *part of the language*;
   anything added later is documented here and carries an
   introduction-date note.

## Sources to be cited

- `lexer.rs` keyword table
- `analyzer.rs` for the resolution of built-in names
- `vm.rs` for the implementation of `print` and any other intrinsic
- Fixtures: `s1.sol`, `s2.sol`, `retest.sol`, `largemini.sol`
