# 11 — Arrays

> **Status:** Scope statement only. Substantive content lands in
> commit 3.

## What this chapter answers

- What is the syntax of an array type?
- What is the syntax of an array literal?
- Are arrays fixed-length, dynamic, or both?
- What does indexing look like, and what happens on out-of-bounds?
- How does `for-in` iteration work over arrays?

## Topics covered

1. **Array type syntax.** Exact surface — `[N]T`, `[]T`, or both —
   to be confirmed against `parser.rs` `Type` parsing and the
   fixtures.
2. **Array literal syntax.** Form to be confirmed against
   `test_array.sol`.
3. **Length semantics.** Fixed-at-declaration vs. dynamic / growable.
   Whether `length`-like access is a field, a method, or a builtin
   call.
4. **Indexing.** `a[i]` for read; `a[i] = v;` for write. Whether
   indices are `int` or some narrower type.
5. **Bounds.** Whether out-of-bounds is a runtime error (chapter 14)
   or silently undefined.
6. **`for-in`.** `for x in array { … }` — iteration variable binding
   per chapter 06. Whether the iteration variable is mutable.
7. **Arrays of structs.** Practical pattern shown in
   `test_array.sol` and `gemini_long.sol`.

## Common mistakes

- Indexing with a non-integer
- Iterating with `for-in` and trying to mutate the source array
  through the iteration variable
- Confusing array literal with struct literal

## Sources to be cited

- `parser.rs` array type and literal productions
- `analyzer.rs` index type-check; iteration variable typing
- `bytecode.rs` `IndexLoad` / `IndexStore` (or equivalent) ops
- `vm.rs` bounds handling
- Fixtures: `test_array.sol`, `gemini_long.sol`
