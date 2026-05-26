# 06 — Variables and Scope

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

- How is a local variable introduced?
- Are SOL variables mutable, immutable, or it-depends?
- What is the visibility of a name across `if` arms, loops, and
  nested blocks?
- What happens if a name is re-declared in the same scope, or in a
  nested scope?
- How are function parameters scoped relative to the function body?

## Topics covered

1. **`let` bindings.** Required type annotation, required
   initializer (per `error_parse1.sol`), block scope, lifetime.
2. **Assignment.** `name = expr;` for already-declared variables;
   field assignment (`s.field = expr;`); element assignment
   (`a[i] = expr;`). What is *not* permitted (e.g. assigning to an
   undeclared name) and the diagnostic that surfaces.
3. **Scope discipline.** Lexical, block-local. Variables go out of
   scope when their enclosing `{ … }` ends. Verified against
   `test_scope.sol`.
4. **Shadowing.** Whether the same name can re-appear in an inner
   block, and what happens if it appears twice in the same scope.
   `error_semantic2.sol` is the canonical "duplicate `let`" case;
   `test_scope.sol` clarifies the across-block rules.
5. **Use-before-declaration.** What the analyzer does when a name is
   referenced before it appears (`error_semantic1.sol`).
6. **Parameters.** Treated as bindings in the outermost scope of the
   function body — covered here, with the function-side angle in
   chapter 05.

## Common mistakes

- `let x: int;` without an initializer
- Re-`let`ing the same name in the same block
- Reading a name from a sibling block
- Assigning to a name that was never `let`-bound

## Sources to be cited

- `parser.rs` `let` and assignment productions
- `analyzer.rs` symbol-table / scope handling
- Fixtures: `test_scope.sol`, `error_semantic1.sol`,
  `error_semantic2.sol`, `error_parse1.sol`
