# SOL Grammar

> **Status:** Scope statement only. Substantive content lands across
> commits 2 – 3.

This file is the EBNF-style grammar derived from the SOL parser. It
covers lexical rules, declarations, statements, and expressions
including the operator precedence table.

## Conventions

- `UPPER_CASE` names are lexical tokens defined in §1.
- `lower_case` names are syntactic productions defined in §2 – §4.
- `'x'` is a literal terminal.
- `{ x }` is zero-or-more `x`.
- `[ x ]` is optional `x`.
- `( a | b )` is one of `a` or `b`.
- A trailing `;` in the EBNF corresponds to a literal source-level
  `;`; trailing whitespace in productions is insignificant.

## §1 — Lexical structure

To be filled in commit 2. Sourced from `lexer.rs`.

- Whitespace and comments
- Identifiers and keywords
- Integer / float / string / char literals
- Operator tokens
- Punctuation

## §2 — Top-level declarations

To be filled in commit 2. Sourced from `parser.rs` top-level loop.

- `file`
- `decl`
- `struct_decl`
- `enum_decl`
- `function_decl`
- `ext_function_decl`
- `export_function_decl`

## §3 — Statements

To be filled in commit 2. Sourced from `parser.rs` statement
productions.

- `block`
- `stmt`
- `let_stmt`
- `assign_stmt`
- `return_stmt`
- `if_stmt`
- `while_stmt`
- `for_stmt`
- `expr_stmt`

## §4 — Expressions and operators

To be filled in commit 3.

- `expr`
- Operator precedence table sourced verbatim from
  `parser.rs:540–558`. Highest-precedence first; associativity
  noted per level.

## §5 — Type syntax

To be filled in commit 2.

- `type`
- `primitive_type`
- `array_type`
- `named_type`

## Cross-references

- Explanatory prose: chapters 03, 04, 07, 08 of the manual.
- Normative rules: [`SPEC.md`](./SPEC.md).
- Conformance fixtures: every positive `.sol` test file (chapter 16).
