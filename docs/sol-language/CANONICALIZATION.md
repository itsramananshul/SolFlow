# Graph → Source Canonicalization

Status as of B.8 c26 (2026-05-27).

## What "canonical" means here

This is **compiler-style canonicalization**, not Prettier-style
formatting fidelity. The goal is:

> Given the same graph state, the emitter always produces
> byte-identical SOL output.

It is explicitly **not**:

> The emit output preserves the user's original whitespace,
> comments, and expression parenthesization choices.

The trade-off is intentional. Source-fidelity preservation requires
the emitter to consume original source text and patch it; the
graph is the canonical form. Round-tripping `import → emit →
import` preserves **semantics + graph structure**, not byte-for-byte
formatting.

## Determinism rules

The emitter (`src/emit/emit.ts`) iterates everything in array
order. Determinism therefore lives in the **workflow ordering**.

| Construct | Order rule | Source of truth |
|---|---|---|
| `imports` | Workflow array order | Importer preserves source order |
| `structs` | Workflow array order | Importer preserves source order |
| Struct fields | Array order | Importer sorts alphabetically (HashMap order loss in serde) |
| `enums` | Workflow array order | Importer preserves source order |
| Enum variants | Array order | Importer sorts by parser-assigned ordinal (HashMap order loss) |
| `functions` | Workflow array order | Importer preserves source order |
| Function params | Array order | Direct from AST (always ordered) |
| Function body | Control-edge walk from entry | Emitter's `emitChain` follows `next` / `then` / `else` / `body` / `after` ports |

The two HashMap-driven sorts (struct fields + enum variants) are
the only places where user-authored order is intentionally
discarded. Without them, two consecutive imports of the same source
could produce graphs with different field orders depending on the
HashMap iteration order serde happened to choose.

## Expression formatting

Inline expressions are produced by `src/graph/import/expressions.ts`
(import side) and `src/emit/emit.ts` (emit side). They both
canonicalize:

| AST shape | Canonical surface |
|---|---|
| Binary op | `(lhs op rhs)` — **always parenthesized** |
| Unary op | `op(child)` — always parenthesized child |
| Float literal | `1.0` (never bare `1` for Float values) |
| String literal | `"escaped"` with `\\` and `\"` escapes |
| Array literal | `[a, b, c]` with single-space separators |
| Struct init | `Name { f: v, f2: v2 }` |
| Enum variant | `Enum::Variant` |

The always-parens choice is the cost. `1 + 2 * 3` becomes
`(1 + (2 * 3))` after a round-trip. Semantically identical; visually
busier than user-authored code.

## Round-trip stability

Tests in `src/graph/import/__tests__/round_trip.test.ts` verify:

1. **`emit(workflow) === emit(workflow)`** — pure idempotence.
   Every fixture passes.
2. **Independent imports of the same AST emit identically** — proves
   no `nanoid()`-generated id leaks into the emit output's
   ordering or formatting.
3. **Snapshot tests** of canonical emit per fixture — any future
   change to importer or emitter produces a visible diff that
   forces explicit review.
4. **Structural invariants** — balanced braces, every imported
   function appears in the emit, no `undefined` leakage from
   inline expressions.

True end-to-end round-trip (`parse → import → emit → parse →
import → compare`) needs the WASM compiler in the Node test
runtime. Currently the WASM build targets the browser bundler;
adding a Node target is a future commit. The snapshot tests
catch the same drift one cycle earlier.

## Known asymmetries that survive round-trip

These are documented in `IMPORT_COMPATIBILITY.md` but worth
mentioning here because they shape the canonical form:

- **Expression parenthesization** as above.
- **Whitespace** — emit uses 2-space indent, blank line between
  top-level decls. User formatting is lost.
- **Comments** — not preserved through import → graph.
- **HashMap-ordered fields/variants** — re-ordered alphabetically
  / by ordinal.
- **Iterator types in `for x in expr`** — degrade to `any`.
- **Top-level lets** — lost (graph schema doesn't model them).
- **Embedded function calls inside expressions** — stay as inline
  text on the consuming node, not lifted to separate `call` nodes.

## What B.8 fixed (and why round-trip tests matter)

The c26 work caught a real bug: the importer was wiring
branch / while / forEach exits via the `next` port instead of
`after`. The emitter walks `next` for plain statements and `after`
for block-bearing statements; using `next` on a branch silently
dropped every subsequent statement from the emit output.

This was invisible without tests — the importer produced a graph
that looked complete, but `emit(workflow)` dropped half of it. The
snapshot test surfaced it immediately. Bug fixed in the same
commit (`StmtImportResult` gained an `exitPort: 'next' | 'after'`
field; the wiring code consults it).

This is the value of canonicalization tests beyond "is it
deterministic": **they expose structural bugs the validator and
typecheck never see**.
