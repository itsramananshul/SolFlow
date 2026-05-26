# 04 — Types

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

- What types does SOL admit?
- What operations are allowed on each?
- Where does a type get inferred vs. where must it be written?
- What does a type mismatch look like at compile time?
- Are there coercions, and if so where?

## Topics covered

### Primitive types

- `int` — integer width and signedness sourced from `lexer.rs` /
  `parser.rs` literal handling and the VM's arithmetic instructions
- `float`
- `bool`
- `str` (string) — the value form, escape sequences, and what is
  stored at runtime
- `char` (if present — to be verified against the lexer's literal
  forms)

### Composite types

- Array types — syntax (`[N]T`, `[]T`, or whatever the parser
  produces) and indexing semantics
- Struct types — fields, declaration order, value semantics
- Enum types — variants and their underlying values

### Special types

- `void` / unit — used as the absence of a return type
- Whether an "any" type exists in SOL itself (it appears in the
  editor's graph schema but not necessarily in the language; this
  will be confirmed)

### Type-level rules

- Where annotations are mandatory: `let x: T = …`, function
  parameters, return types
- Where inference (if any) is performed
- What counts as type equality (nominal for structs/enums, structural
  for primitives)
- Coercion: documented as exhaustively as the source warrants. If
  the compiler does not coerce, that is stated as a hard rule.
- Type-mismatch diagnostics — cross-referenced to
  [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)

## Sources to be cited

- `parser.rs` `Type` enum and parser
- `analyzer.rs` type-check entry points (`check`, related helpers)
- `bytecode.rs` for which operations exist per type
- `vm.rs` for runtime checks (e.g. division-by-zero) that materialize
  as runtime errors
- Fixtures: `test_arith.sol`, `test_array.sol`, `test_edge.sol`,
  `test_struct.sol`
