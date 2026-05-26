# 17 — Style Guide

> **Status:** Scope statement only. Substantive content lands in
> commit 4.

## What this chapter answers

- How should SOL code be formatted to be readable?
- What naming conventions does the corpus already follow, and which
  should new code adopt?
- How should declarations be ordered inside a file?
- What patterns are idiomatic, and which are smells?

This chapter is opinionated. The rules here are derived from the
test corpus and adjusted for clarity; they are recommendations, not
language-level requirements. Anything the compiler enforces lives in
the prior chapters.

## Topics covered

1. **Naming.**
   - `snake_case` for functions, `let` bindings, struct fields,
     enum variants — pattern observed in `s1.sol`, `s2.sol`,
     `largemini.sol`.
   - `PascalCase` for struct and enum *type names*.
   - `start` as the conventional entry function — chapter 05 has
     the formal rule.
2. **File layout.** Imports / `ext` declarations first; type
   declarations (structs, enums) next; helper functions in dependency
   order or alphabetical; `start` at the end. Justification: shortest
   visual path from "what does this file depend on" to "what does it
   produce".
3. **Indentation.** Four spaces. Consistent across the corpus.
4. **Brace placement.** Opening brace on the same line as the
   declaring construct (`function f() -> int {`, `if (cond) {`).
   Closing brace on its own line.
5. **Statement separation.** Each statement on its own line; one
   semicolon per line; no `;;`.
6. **Comment use.** Use comments to explain *why*, not *what*. The
   corpus rarely comments; when it does, it explains a non-obvious
   business rule. Avoid running commentary.
7. **Function shape.** Prefer small functions with explicit return
   types. Top-level entry (`start`) is the only function whose
   return type tends to be `int` purely to satisfy the host's
   exit-code convention.
8. **Struct and enum size.** Keep struct field counts modest;
   anything over ~6 fields is a sign that the data should split into
   nested types.
9. **`ext` usage.** Treat `ext function` declarations as the file's
   *contract with the outside world*. Group them at the top so the
   contract is visible.
10. **Idioms vs. anti-idioms.** A small set of "do this / not this"
    pairs covering common cases — early return vs. nested `if`, named
    intermediate `let`s vs. long expressions, etc.

## Sources

- The full positive-fixture corpus (see chapter 16 for the index).
- Cross-checked against the visual editor's own samples in
  `src/samples/*.ts`.
