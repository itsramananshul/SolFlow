# 03 — Syntax Reference

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

For every concrete syntactic construct the parser accepts:

- What does the construct look like?
- What is it for?
- What is a minimal *valid* example?
- What is a minimal *invalid* example, and what diagnostic does the
  compiler emit?

This chapter is the broad surface-syntax reference; semantic depth
lives in chapters 04 – 14, and the normative grammar is in
[`GRAMMAR.md`](./GRAMMAR.md).

## Topics covered

### Declarations

- `let` bindings
- `struct` declarations
- `enum` declarations
- `function` declarations (with and without return type)
- `ext function` declarations (no body; terminated with `;`)
- `export function` declarations (with body)

### Statements

- Expression statements
- Assignment (`=`) — to variables and to fields / indices
- `return` (with and without value)
- `if` / `if … else`
- `while`
- `for-in`
- Blocks (`{ … }`)
- Semicolon termination rules

### Expressions

- Integer literals (with base prefixes if accepted — to be verified)
- Float literals
- Boolean literals (`true`, `false`)
- String literals (escape sequences to be enumerated from `lexer.rs`)
- Character literals (if supported — to be verified)
- Identifier references
- Function call: `f(a, b, c)`
- Field access: `e.field`
- Index access: `e[i]`
- Struct literal: `Name { field: value, … }`
- Enum variant reference: `Name::Variant`
- Array literal: `[…]` (form to be verified)
- Parenthesized expression

### Operators

- Arithmetic, comparison, logical, bitwise, unary — full table sourced
  from the Pratt block at `parser.rs:540–558` and reproduced in
  [`GRAMMAR.md`](./GRAMMAR.md).

### Trivia

- Comments
- Whitespace
- Separator behavior

## Cross-references

- Operator precedence and associativity: [chapter 08](./08-expressions.md)
- Type rules per construct: [chapter 04](./04-types.md)
- Statement-level semantics: chapters [06](./06-variables-and-scope.md), [07](./07-control-flow.md)

## Sources to be cited

- `lexer.rs` (full file — token set, literal forms)
- `parser.rs` (full file — declaration and statement productions)
- `analyzer.rs` (for the semantic-error examples paired with each
  construct)
- Every positive fixture for the *valid* examples
- `error_parse1.sol`, `error_parse2.sol` for the *invalid* examples
