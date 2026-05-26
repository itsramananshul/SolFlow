# 08 — Expressions and Operators

> **Status:** Scope statement only. Substantive content lands in
> commit 3.

## What this chapter answers

- What forms can an expression take?
- What is the precedence and associativity of each operator?
- Which operator combinations are accepted by the parser but rejected
  by the type checker?
- How does the parser handle ambiguity? (Pratt-style precedence per
  `parser.rs:540–558`.)

## Topics covered

1. **Literal expressions.** All literal forms enumerated from the
   lexer — see chapter 03.
2. **Identifier expression.** Bare-name reference resolved against
   the current scope.
3. **Arithmetic.** `+`, `-`, `*`, `/`, and `%` if present —
   verified against `test_arith.sol` and `bytecode.rs`. Type rules
   (only numeric types) come from chapter 04.
4. **Comparison.** `==`, `!=`, `<`, `<=`, `>`, `>=` — and the type of
   the result (`bool`).
5. **Logical.** `&&`, `||`, `!` — short-circuit behavior verified
   against `vm.rs` bytecode handlers.
6. **Bitwise.** Set is sourced verbatim from the Pratt block at
   `parser.rs:540–558` and reproduced in [`GRAMMAR.md`](./GRAMMAR.md).
7. **Unary.** `-`, `!`, and any others discovered.
8. **Function call.** `f(a, b, c)`. Argument evaluation order is
   verified against `bytecode.rs` / `vm.rs`.
9. **Field access.** `e.field`. Type depends on the struct definition
   (chapter 09).
10. **Index access.** `e[i]`. Bounds checking and runtime errors are
    covered here and in chapter 14.
11. **Struct literal.** `Name { field: value, … }`. Field order in
    the literal vs. order in the struct declaration — interaction
    with the field-ordering hazard documented in chapter 09.
12. **Enum variant reference.** `Name::Variant` — see chapter 10.
13. **Array literal.** Form to be verified against `test_array.sol`.
14. **Parenthesized expression.** `( expr )`.

### Precedence table

A full table — pulled verbatim from `parser.rs:540–558` — will appear
both here and in [`GRAMMAR.md`](./GRAMMAR.md). The plan is to keep one
copy authoritative and link.

## Common mistakes

- Mixing types across an arithmetic operator (e.g. `int + str`)
- Mis-using `=` where `==` was meant (parser distinguishes, but the
  resulting diagnostic is worth showing)
- Calling a method-style `e.f()` when SOL only admits free-function
  calls and `e.f` is a field access

## Sources to be cited

- `parser.rs` expression productions; precedence table at lines
  ~540–558
- `lexer.rs` for the full operator token set
- `bytecode.rs` arithmetic / comparison / logic instructions
- `vm.rs` operator evaluation
- Fixtures: `test_arith.sol`, `test_edge.sol`
