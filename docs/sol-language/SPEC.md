# SOL Language Specification (Normative)

> **Status:** Scope statement only. Substantive content lands across
> commits 2 – 6 and is consolidated in commit 6.

This file is the terse, normative spec — the minimum a second
implementation would need to honor in order to be called
"SOL-compatible". The reference manual (chapters 01 – 19) carries the
prose, examples, and rationale; this file states the rules.

## Sections

1. **Lexical structure** — character set, whitespace, comments,
   identifiers, keywords, literal forms, operators, punctuation.
   Source: `lexer.rs`.
2. **Types** — primitive types, composite type constructors, type
   equality. Source: `parser.rs` `Type`, `analyzer.rs`.
3. **Declarations** — `let`, `function`, `ext function`,
   `export function`, `struct`, `enum`. Source: `parser.rs`.
4. **Statements** — assignment, return, blocks, `if`, `while`,
   `for-in`. Source: `parser.rs`, `analyzer.rs`.
5. **Expressions** — literal forms, identifier reference, field
   access, index access, call, struct/enum/array literal,
   parenthesized expression. Source: `parser.rs`.
6. **Operators** — full table, precedence, associativity, operand
   type rules. Source: `parser.rs:540–558`, `analyzer.rs`,
   `bytecode.rs`.
7. **Scoping rules** — lexical block scope, shadowing rule,
   forward-declaration rule. Source: `analyzer.rs`.
8. **Type checking** — every check the analyzer performs, in
   per-construct order. Source: `analyzer.rs`.
9. **Runtime behavior** — evaluation order, argument evaluation,
   short-circuiting, side-effect ordering, runtime traps. Source:
   `bytecode.rs`, `vm.rs`.
10. **Host-runtime interface** — what the language promises about
    `ext` declarations and `export` declarations, independent of any
    specific host's transport.
11. **Open questions** — anything not yet resolved by the audit
    (`00-source-audit.md`, §6).

## Relationship to other documents

- The **reference manual** (chapters 01 – 19) is normative for
  *explanations*; this spec is normative for *rules*.
- **`GRAMMAR.md`** is the EBNF companion to this spec.
- **`ERROR_REFERENCE.md`** enumerates every diagnostic; this spec
  cites diagnostics by code, never re-states them.

## Versioning

This spec carries a snapshot date. When the compiler changes a rule,
the spec line changes and the previous behavior is footnoted, never
silently removed.

Snapshot date: **(to be set in commit 6 once the body is filled in.)**
