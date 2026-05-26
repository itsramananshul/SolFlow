# 18 — SolFlow Mapping

> **Status:** Scope statement only. Substantive content lands in
> commit 5.

## What this chapter answers

- For every node kind SolFlow exposes, what SOL syntax does it
  represent?
- Which node inputs / outputs correspond to which SOL expression
  positions?
- What does Graph → SOL emission look like, and what does it *not*
  yet round-trip?
- Where does SolFlow's editor model exceed the language, and where
  does it under-cover it?

This chapter is the contract between the visual editor and the
language. Anywhere SolFlow's behavior diverges from canonical SOL,
the divergence is logged here and in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

## Topics covered

### Node ↔ syntax table

A complete table covering every node kind defined in
`src/graph/schema.ts`:

- `start` — opening of the entry function.
- `trigger` — editor-side annotation; emitted as a comment by
  `src/emit/emit.ts`. **Not part of canonical SOL.** Documented as
  an editor extension.
- `let` — `let name: T = <value-expression>;`
- `assign` — `name = <value-expression>;`
- `print` — `print(<value-expression>);`
- `return` — `return;` or `return <value-expression>;`
- `branch` — `if (<cond>) { … } [ else { … } ]`
- `while` — `while (<cond>) { … }`
- `forEach` — `for name in <array-expression> { … }`
- `binaryOp`, `unaryOp` — emitted inline inside the consuming expression.
- `varGet`, `literal` — emitted as the bare expression.
- `arrayLiteral` — `[a, b, c]`
- `structLiteral` — `Name { field: v, … }`
- `fieldAccess`, `fieldSet` — `s.f` / `s.f = v;`
- `indexRead`, `indexSet` — `a[i]` / `a[i] = v;`
- `enumVariant` — `Name::Variant`
- `call` — `name(arg, …);` or `name(arg, …)` as an expression
- `note`, `frame` — editor-only; never appear in emitted SOL.

For each row the table documents:

- the SOL syntax produced
- the input port → expression-slot mapping (port id, expected type)
- the output port → SOL "value" or "control" mapping
- whether the construct round-trips back from SOL → graph (today:
  the round-trip path doesn't exist; the chapter marks this state
  explicitly)

### Port-contract requirements

For each statement-form node (`let`, `assign`, `print`, `return`,
`branch`, `while`, `forEach`, `fieldSet`, `indexSet`, `call`):

- which input ports are *required*
- which can be satisfied by an *inline expression on the node* vs.
  only by a wired data edge
- which output ports are control vs. data

This section is the source of truth for the validator and for the
auto-repair pass in Sol Man's apply pipeline.

### Editor extensions documented as non-SOL

- `trigger` annotations (emitted as `// @trigger …` comments).
- Notes / frames (visual aids only).
- The "any" type used for unresolved data edges in the editor graph.

These are *editor* concepts; a SOL file written by hand will not
have them, and that is fine. They are documented here only so a
reader of SolFlow source isn't surprised.

### Emitter mismatches with canonical SOL

A running list, populated as discovered. Examples that may appear:

- The emitter inserts `// @trigger` comments; the parser tolerates
  them as comments — confirmed.
- The emitter may produce a function header with `-> void` while the
  parser admits only omitted return; verified or refuted in the
  substantive pass.

Every mismatch carries: where it lives in the emitter, what canonical
SOL says, and what should change to bring them into agreement.

## Sources to be cited

- `src/graph/schema.ts` (node kinds, port shapes)
- `src/graph/factory.ts` (port construction)
- `src/graph/validate.ts` (port validation)
- `src/emit/emit.ts` (Graph → SOL)
- All chapters 02 – 14 for the language side of each mapping
