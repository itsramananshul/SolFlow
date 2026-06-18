# Graph to Source Canonicalization

How the editor turns a graph back into canonical SOL source.

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
graph is the canonical form. Round-tripping `import` then `emit`
then `import` preserves **semantics + graph structure**, not
byte-for-byte formatting. Comments cannot survive at all: the
canonical AST (`sol/src/ast.rs`) carries no comment nodes, and the
crate formatter (`sol/src/format.rs`) drops them on any round-trip,
so the graph cannot model them either.

## Canonical surface syntax

The emitter (`src/emit/emit.ts`) writes canonical SOL exactly as the
`sol/` crate parses and the crate formatter prints it:

- Return type uses the `<-` arrow: `fn name(p: T) <- RetType { }`.
  A function with no declared return type omits the arrow entirely.
  There is no `->` in canonical SOL.
- The runnable unit is `workflow "name" { }` (the name is a string
  literal). Helper functions are `fn name(params) { }`.
- Comments are `#` to end of line. There are no `//` or block
  comments, so the emitter never produces them.
- Arrays are prefix `[]T` (`[]int`, `[][]float`).
- Struct fields and enum variants are `;` separated:
  `struct S { a: int; b: str; }`, `enum E { V1; V2; }`.
- Struct literals and call arguments are `,` separated.
- Enum variants are referenced as `Enum::Variant`.
- External Action calls are `call("m.f", params)`, imported
  `m.f(args)`, or `m::rpc(args)`.

The emitter header logic lives in `src/emit/emit.ts`: it emits
`workflow "name"` for the `isWorkflow` callable and
`fn name(params) <- ret` for helpers (the arrow is suppressed when
the return type is `void`).

## Determinism rules

The emitter iterates everything in array order. Determinism
therefore lives in the **workflow ordering**.

| Construct | Order rule | Source of truth |
|---|---|---|
| `imports` | Workflow array order | Importer preserves source order |
| `structs` | Workflow array order | Importer preserves source order |
| Struct fields | Array order | Canonical `StructDecl.fields` is an ordered `Vec` (`sol/src/ast.rs`); importer preserves it (`src/graph/import/importer.ts`) |
| `enums` | Workflow array order | Importer preserves source order |
| Enum variants | Array order | Canonical `EnumDecl.variants` is an ordered `Vec` (`sol/src/ast.rs`); importer preserves it |
| `functions` | Workflow array order | Importer preserves source order |
| Function params | Array order | Direct from AST (always ordered) |
| Function body | Control-edge walk from entry | Emitter's `emitChain` follows `next` / `then` / `else` / `body` ports |

There are no order-discarding sorts. The canonical AST stores struct
fields and enum variants as ordered `Vec`s, not hash maps, so
user-authored field and variant order survives an import unchanged
(the importer maps them with a plain `.map`, no sort). Two
consecutive imports of the same source therefore produce graphs with
identical field and variant order.

## Expression formatting

Inline expressions are produced by `src/graph/import/expressions.ts`
(import side) and `src/emit/emit.ts` (emit side). They both mirror
the crate pretty-printer `fmt_expr` in `sol/src/format.rs`:

| AST shape | Canonical surface |
|---|---|
| Binary op | `(lhs op rhs)` — **always parenthesized** |
| Unary op | `op(child)` — always parenthesized child |
| Float literal | `1.0` (never bare `1` for Float values) |
| String literal | `"escaped"` with `\\` and `\"` escapes |
| Char literal | `'x'` (a single char) |
| Array literal | `[a, b, c]` with single-space separators |
| Struct literal | `Name { f: v, f2: v2 }` |
| Enum variant | `Enum::Variant` |

The binary operators the canonical AST models are
`+ - * / == != < > <= >= && ||`; unary operators are `-` and `!`.
There are no bitwise operators in canonical SOL, so the emitter never
prints `& | ^ << >> ~`.

The always-parens choice is the cost. `1 + 2 * 3` becomes
`(1 + (2 * 3))` after a round-trip. Semantically identical; visually
busier than user-authored code.

## Round-trip stability

Tests in `src/graph/import/__tests__/round_trip.test.ts` verify:

1. **`emit(workflow) === emit(workflow)`** — pure idempotence.
2. **Independent imports of the same AST emit identically** — proves
   no `nanoid()`-generated id leaks into the emit output's ordering
   or formatting.
3. **Snapshot tests** of canonical emit per fixture — any future
   change to importer or emitter produces a visible diff that forces
   explicit review.
4. **Structural invariants** — balanced braces, every imported
   function appears in the emit, no `undefined` leakage from inline
   expressions.

A true end-to-end round-trip (`parse` then `import` then `emit` then
`parse` then `import` then `compare`) runs the canonical parser via
the `compiler-wasm` bridge (`parse_source_json` / `format_source_json`
in `compiler-wasm/src/lib.rs`). The snapshot tests catch the same
drift one cycle earlier without needing the WASM build in the Node
test runtime.

## Known asymmetries that survive round-trip

These are documented in `IMPORT_COMPATIBILITY.md` but worth
mentioning here because they shape the canonical form:

- **Expression parenthesization** as above.
- **Whitespace** — emit uses 2-space indent and a blank line between
  top-level declarations. User formatting is lost.
- **Comments** — not preserved; the canonical AST has no comments.
- **Iterator types in `for x in expr`** — degrade to `any` because
  the importer runs on parser output, with no inferred type info.
- **Embedded function calls and capability calls inside expressions**
  — stay as inline canonical SOL text on the consuming node, not
  lifted to separate nodes.

## Why these tests matter

Canonicalization tests do more than confirm determinism: they expose
structural wiring bugs that no validator catches. A representative
example is the block-exit wiring bug: the importer once wired branch,
while, and forEach exits via the `next` port instead of `after`. The
emitter walks `next` for plain statements and `after` for
block-bearing statements, so using `next` on a branch silently
dropped every subsequent statement from the emit output. The graph
looked complete, but `emit(workflow)` dropped half of it. A snapshot
test surfaced it immediately; the fix added an explicit exit-port
field that the wiring code consults.
