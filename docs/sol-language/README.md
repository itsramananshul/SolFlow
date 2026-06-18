# The SOL Language

SOL is a small orchestration language. A SOL file declares the types,
functions, and workflows that a host runtime can load and execute. The
language is deliberately compact: the goal is to read like a short
technical specification of what should happen, not a general-purpose
application language. Type annotations are recorded but not statically
enforced; mismatches surface at runtime as string errors.

These docs are the canonical SOL language manual. They are derived
directly from the canonical `openprem-sol-v2` crate (`sol/src/*`): the
lexer, parser, bytecode compiler, and stack VM. There is no
type-checking or semantic-analysis phase. The editor reaches the
language through the `compiler-wasm` bridge
(`compiler-wasm/src/lib.rs`). Where the in-browser simulator and the
canonical VM disagree, the **canonical crate is authoritative** and the
simulator behavior is flagged as a tool-side mismatch.

---

## Who these docs are for

| Audience | Start here | Then read |
|---|---|---|
| **Newcomer to SOL** — wants to read & write `.sol` files | [01 — Overview](./01-overview.md) → [02 — File structure](./02-file-structure.md) | 03 → 11 in order, then [16 — Examples](./16-examples.md) |
| **SolFlow user** — wants to know how visual nodes turn into SOL | [18 — SolFlow mapping](./18-solflow-mapping.md) | 04 / 05 / 07 / 12, then [ERROR_REFERENCE](./ERROR_REFERENCE.md) |
| **Sol Man / LLM tooling author** | [19 — Sol Man generation guide](./19-solman-generation-guide.md) | 18, then [SPEC](./SPEC.md) + [ERROR_REFERENCE](./ERROR_REFERENCE.md) |
| **Crate / IDE contributor** | [00 — Source audit](./00-source-audit.md) | [SPEC](./SPEC.md), [GRAMMAR](./GRAMMAR.md), [14 — Runtime semantics](./14-runtime-semantics.md), [20 — Implementation notes](./20-implementation-notes.md) |
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
| 12 | [`12-imports-and-controllers.md`](./12-imports-and-controllers.md) | `import` declarations, capability calls, host-runtime wiring, session configuration |
| 13 | [`13-builtins-and-stdlib.md`](./13-builtins-and-stdlib.md) | The four VM built-ins (`print`, `len`, `to_str`, `type_name`); host natives |
| 14 | [`14-runtime-semantics.md`](./14-runtime-semantics.md) | Stepping stack VM, `StepResult`, side effects, error propagation, determinism |
| 15 | [`15-errors-and-diagnostics.md`](./15-errors-and-diagnostics.md) | String runtime errors, the five bridge diagnostics, editor structural checks; how tools display them |
| 16 | [`16-examples.md`](./16-examples.md) | Annotated walkthroughs of canonical sample programs |
| 17 | [`17-style-guide.md`](./17-style-guide.md) | Naming, formatting, ordering, idiomatic patterns |
| 18 | [`18-solflow-mapping.md`](./18-solflow-mapping.md) | SolFlow node ↔ SOL syntax mapping; port contracts; export rules |
| 19 | [`19-solman-generation-guide.md`](./19-solman-generation-guide.md) | Generation rules for LLM-based SOL/graph synthesis |
| 20 | [`20-implementation-notes.md`](./20-implementation-notes.md) | Deep implementation details — codegen pipeline, slot mechanics, struct/array runtime layout, RPC wire shapes, `ExtCall` transport, the implementation's current gaps |
| 21 | [`21-behavior-classification.md`](./21-behavior-classification.md) | Behavior stability badges — Specified / Current-impl / Accidental / Emergent / Undefined / Unstable. The single place to look when asking "can I rely on this?" |
| 22 | [`22-cross-layer-assumptions.md`](./22-cross-layer-assumptions.md) | What each layer (lexer, parser, bytecode compiler, VM, editor validator, editor simulator, Sol Man store) guarantees and what each consumer assumes. Bypass paths. Future-verifier guidance |
| 23 | [`23-editor-runtime-audit.md`](./23-editor-runtime-audit.md) | The editor's in-browser simulator audited against canonical SOL — every divergence, the `new Function` security hazard, scope-model mismatches, graph mutation hazards, serialization invariants, determinism, ordering. Where simulator/canonical diverge enough that "works in simulator" is not "works in production" |

### Normative references

| File | Purpose |
|---|---|
| [`SPEC.md`](./SPEC.md) | Terse normative specification — the minimum a second implementation would need to honor |
| [`GRAMMAR.md`](./GRAMMAR.md) | EBNF-style grammar derived from the parser; lexical rules, declarations, statements, expressions, precedence table |
| [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) | The five bridge diagnostics and the editor structural checks, with cause, fix, and plain-English explanation |
| [`EXAMPLES.md`](./EXAMPLES.md) | Long-form example catalogue (companion to chapter 16) |
| [`IMPORT_COMPATIBILITY.md`](./IMPORT_COMPATIBILITY.md) | What the AST→graph importer can / cannot represent visually; per-construct classification + round-trip caveats |
| [`SYNC_MODEL.md`](./SYNC_MODEL.md) | The explicit-action sync philosophy between source pane and visual graph (B.9 architectural model) |
| [`CANONICALIZATION.md`](./CANONICALIZATION.md) | Graph → source canonicalization rules; round-trip stability contract; known formatting asymmetries (B.8) |
| [`SIMULATOR_PARITY.md`](./SIMULATOR_PARITY.md) | Drift audit between the in-browser simulator and the canonical SOL crate VM (B.10 groundwork; resolved by canonical-VM-in-WASM) |
| [`B_RELEASE_NOTES.md`](./B_RELEASE_NOTES.md) | Summary of everything that shipped in Phase B (B.1–B.11) |

### Internal

| File | Purpose |
|---|---|
| [`internal-notes.md`](./internal-notes.md) | Maintainer notes — source-tree locations, snapshot date. Not for public publication |

---

## Conventions

- Every claim about language behavior is backed by a source reference of the form
  `sol/src/lexer.rs` or `sol/src/parser.rs` pointing into the canonical
  `openprem-sol-v2` crate, the bridge `compiler-wasm/src/lib.rs`, or the editor
  `src/*`.
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

| Chapter / file | State |
|---|---|
| `00 – 00-source-audit.md` | Substantive |
| `01 – 01-overview.md` | Scope statement (substantive treatment lives across chapters 02 – 14) |
| `02 – 02-file-structure.md` | Scope statement; the substantive material lives in chapter 03 + GRAMMAR §2 |
| `03 – 03-syntax.md` | Substantive |
| `04 – 04-types.md` | Substantive |
| `05 – 05-functions.md` | Substantive |
| `06 – 06-variables-and-scope.md` | Substantive |
| `07 – 07-control-flow.md` | Substantive |
| `08 – 08-expressions.md` | Substantive |
| `09 – 09-structs.md` | Substantive |
| `10 – 10-enums.md` | Substantive |
| `11 – 11-arrays.md` | Substantive |
| `12 – 12-imports-and-controllers.md` | Substantive (host-runtime section dated as a 2026-05-26 snapshot) |
| `13 – 13-builtins-and-stdlib.md` | Substantive |
| `14 – 14-runtime-semantics.md` | Substantive |
| `15 – 15-errors-and-diagnostics.md` | Substantive |
| `16 – 16-examples.md` | Substantive |
| `17 – 17-style-guide.md` | Substantive |
| `18 – 18-solflow-mapping.md` | Substantive |
| `19 – 19-solman-generation-guide.md` | Substantive |
| `20 – 20-implementation-notes.md` | Substantive |
| `21 – 21-behavior-classification.md` | Substantive |
| `22 – 22-cross-layer-assumptions.md` | Substantive |
| `23 – 23-editor-runtime-audit.md` | Substantive |
| `SPEC.md` | Substantive |
| `GRAMMAR.md` | Substantive |
| `ERROR_REFERENCE.md` | Substantive |
| `EXAMPLES.md` | Substantive (lookup index; the guided tour lives in chapter 16) |
| `internal-notes.md` | Maintainer-only |

Each chapter header carries its own `Status:` line and snapshot
date where relevant. When the canonical `openprem-sol-v2` crate
changes a rule that this manual documents, the matching chapter and
cross-references should be updated in the same documentation pass.
