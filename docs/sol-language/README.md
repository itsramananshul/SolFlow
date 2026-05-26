# The SOL Language

SOL is a small, statically-typed orchestration language. A SOL file
declares the types, functions, and external entry points that a host
runtime can load and execute. The language is deliberately compact —
the goal is to read like a short technical specification of *what
should happen*, not a general-purpose application language.

These docs are the canonical SOL language manual. They are derived
directly from the SOL compiler source — the lexer, parser, semantic
analyzer, bytecode definition, and VM — cross-checked against the
test fixture corpus. Where the compiler and the visual editor's own
emitter disagree, the **compiler is authoritative** and the editor
behavior is flagged as a tool-side mismatch.

---

## Who these docs are for

| Audience | Start here | Then read |
|---|---|---|
| **Newcomer to SOL** — wants to read & write `.sol` files | [01 — Overview](./01-overview.md) → [02 — File structure](./02-file-structure.md) | 03 → 11 in order, then [16 — Examples](./16-examples.md) |
| **SolFlow user** — wants to know how visual nodes turn into SOL | [18 — SolFlow mapping](./18-solflow-mapping.md) | 04 / 05 / 07 / 12, then [ERROR_REFERENCE](./ERROR_REFERENCE.md) |
| **Sol Man / LLM tooling author** | [19 — Sol Man generation guide](./19-solman-generation-guide.md) | 18, then [SPEC](./SPEC.md) + [ERROR_REFERENCE](./ERROR_REFERENCE.md) |
| **Compiler / IDE contributor** | [00 — Source audit](./00-source-audit.md) | [SPEC](./SPEC.md), [GRAMMAR](./GRAMMAR.md), [14 — Runtime semantics](./14-runtime-semantics.md) |
| **Looking up an error** | [ERROR_REFERENCE.md](./ERROR_REFERENCE.md) | (each entry links back to the relevant chapter) |

---

## File index

### Reference manual (read in order)

| # | File | Topic |
|---|---|---|
| 00 | [`00-source-audit.md`](./00-source-audit.md) | What was read to write these docs; confirmed-vs-uncertain conventions |
| 01 | [`01-overview.md`](./01-overview.md) | What SOL is and what it isn't; positioning; status |
| 02 | [`02-file-structure.md`](./02-file-structure.md) | Top-level file anatomy; declaration ordering; comments; whitespace |
| 03 | [`03-syntax.md`](./03-syntax.md) | Every concrete syntactic construct — declarations, statements, expressions |
| 04 | [`04-types.md`](./04-types.md) | Primitive and composite types; allowed operations per type |
| 05 | [`05-functions.md`](./05-functions.md) | Functions, parameters, returns, `ext` / `export`, forward declarations, recursion |
| 06 | [`06-variables-and-scope.md`](./06-variables-and-scope.md) | `let`, assignment, mutability, lexical scope, shadowing rules |
| 07 | [`07-control-flow.md`](./07-control-flow.md) | `if` / `else` / `while` / `for-in` / `return`; what is not supported |
| 08 | [`08-expressions.md`](./08-expressions.md) | Operators, precedence, associativity, parens; function/field/index/call forms |
| 09 | [`09-structs.md`](./09-structs.md) | Struct declarations, literals, field access, mutation, nesting |
| 10 | [`10-enums.md`](./10-enums.md) | Enum declarations, variants, auto-values, comparisons |
| 11 | [`11-arrays.md`](./11-arrays.md) | Array types and literals, indexing, `for-in`, length semantics |
| 12 | [`12-imports-and-controllers.md`](./12-imports-and-controllers.md) | `ext` / `export` functions, host-runtime wiring, session configuration |
| 13 | [`13-builtins-and-stdlib.md`](./13-builtins-and-stdlib.md) | Built-in functions (`print`, etc.); standard-library state |
| 14 | [`14-runtime-semantics.md`](./14-runtime-semantics.md) | Evaluation model, side effects, error propagation, determinism |
| 15 | [`15-errors-and-diagnostics.md`](./15-errors-and-diagnostics.md) | Parse / semantic / runtime error categories; how tools should display them |
| 16 | [`16-examples.md`](./16-examples.md) | Annotated walkthroughs of canonical sample programs |
| 17 | [`17-style-guide.md`](./17-style-guide.md) | Naming, formatting, ordering, idiomatic patterns |
| 18 | [`18-solflow-mapping.md`](./18-solflow-mapping.md) | SolFlow node ↔ SOL syntax mapping; port contracts; export rules |
| 19 | [`19-solman-generation-guide.md`](./19-solman-generation-guide.md) | Generation rules for LLM-based SOL/graph synthesis |

### Normative references

| File | Purpose |
|---|---|
| [`SPEC.md`](./SPEC.md) | Terse normative specification — the minimum a second implementation would need to honor |
| [`GRAMMAR.md`](./GRAMMAR.md) | EBNF-style grammar derived from the parser; lexical rules, declarations, statements, expressions, precedence table |
| [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) | Every diagnostic the compiler emits, with cause, fix, fixture link, plain-English explanation |
| [`EXAMPLES.md`](./EXAMPLES.md) | Long-form example catalogue (companion to chapter 16) |

### Internal

| File | Purpose |
|---|---|
| [`internal-notes.md`](./internal-notes.md) | Maintainer notes — source-tree locations, snapshot date. Not for public publication |

---

## Conventions

- Every claim about language behavior is backed by a source reference of the form
  `(lexer.rs:42)` or `(parser.rs:540–558)` pointing into the canonical compiler
  crate, or by a named fixture from the test corpus.
- Sections marked **Confirmed** are observed in source and reproduced by at
  least one fixture.
- Sections marked **Uncertain** state explicitly what evidence exists and what
  is still missing.
- Sections marked **Snapshot** describe behavior of an external runtime
  integration that may evolve independently; the snapshot date is given.
- Examples come from real fixture files where possible. When an example is
  fabricated to illustrate a point, it is labeled *(illustrative)*.

---

## Status

These docs are written incrementally. The audit (chapter 00) and the
chapter scope statements (01–19 plus the normative references) land
first; substantive content lands in subsequent commits. Each chapter
header carries a clear `Status:` line indicating what is in place
versus pending.
