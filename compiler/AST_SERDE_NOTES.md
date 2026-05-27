# AST serialization notes (B.3 groundwork)

Status as of B.3 c9 (2026-05-27).

This crate now derives `serde::Serialize + Deserialize` on every
type that crosses the WASM bridge in B.4, gated behind the
`serde` cargo feature so the CLI build doesn't pull in serde.

## What's serialized

| Type | Location | Notes |
|---|---|---|
| `Ast` | `parser.rs` | The full AST enum. Recursion via `Box<Ast>` works out of the box. |
| `Type` | `parser.rs` | Includes recursive `Array { inner: Box<Type> }` and `Function { ret: Box<Type> }`. |
| `Token` | `lexer.rs` | Used inside `Ast::ExprBinary.op` and `Ast::ExprUnary.op`. |
| `TokenKind` | `lexer.rs` | Helper; derived for completeness. |
| `Symbol` | `analyzer.rs` | Type-table entries — needed if the WASM bridge exposes the symbol table for hover-info. |
| `SolDiagnostic` | `diagnostic.rs` | Round-trips end-to-end through JSON; see `tests/serde_roundtrip.rs`. |
| `SourceSpan`, `RelatedSpan`, `DiagnosticPhase`, `DiagnosticSeverity` | `diagnostic.rs` | All derive. |

## Breaking change rolled in alongside the derives

`SolDiagnostic::code` changed from `&'static str` to `String`.

- **Why:** Serde can't deserialize a string into `&'static str`
  without leaking memory, and we don't want to introduce a
  lifetime parameter on `SolDiagnostic`. `String` is the future-
  friendly representation for transport.
- **Cost:** one tiny allocation per diagnostic. Negligible since
  diagnostics live on error paths.
- **Callers:** all internal constructors (`SolDiagnostic::error /
  warning / internal`) still accept `&'static str` and call
  `.to_string()`. External callers that read `d.code` get
  `&String`; if they need `&str` they call `.as_str()`. (Updated
  in the same commit; tests green.)

## Known blockers / quirks

1. **`HashMap<String, _>` ordering.** `Ast::DeclStruct.fields`,
   `Ast::DeclEnum.variants`, `Symbol::Struct.fields`, and
   `Symbol::Enum.variants` all use `HashMap`. Serde serializes
   maps as JSON objects, so insertion order is lost. If a future
   WASM caller needs deterministic field/variant order it should
   either:
   - serialize alongside a stable order vector, or
   - switch the storage type to `IndexMap`/`Vec<(K, V)>` (cleaner
     but a wider AST change; out of B.3 scope).

2. **`TypeTableId = usize` arena coupling.** `Ast::DeclFunc.scope`
   and `Ast::Block.scope` carry indices into the analyzer's
   `tt_arena`. The indices serialize fine, but to interpret them
   the consumer must also receive the arena (and the arena must
   be serialized as a `Vec<TypeTable>`, which works). For the
   WASM bridge to expose useful symbol info, both `Program` and
   the analyzer's `tt_arena` must be sent together. Today
   `AnalyzedProgram` already bundles them.

3. **No `PartialEq` on `Ast`.** Round-trip tests verify
   correctness by re-serializing the deserialized value and
   comparing the JSON strings. Adding `PartialEq` is cheap but
   not needed yet; deferred.

4. **`Inst` (bytecode) is NOT yet serde-derived.** Bytecode is
   not the editor-facing surface; the AST is. If the WASM bridge
   later needs to ship compiled bytecode (e.g. for a browser-side
   VM in B.10), `Inst` will need the same treatment. Out of B.3
   scope.

## Verification

```
# default (CLI) build — no serde
cargo build
cargo test                                # 12/12 green

# with serde feature enabled — what the WASM bridge will use
cargo build --features serde
cargo test --features serde               # 16/16 green
                                          #   (12 base + 4 roundtrip)
```

`tests/serde_roundtrip.rs` covers:
- `Program` round-trip via `parse_source` + `serde_json`
- `Vec<SolDiagnostic>` round-trip via `compile_source` + `serde_json`
- `SourceSpan` round-trip
- Pin of enum representation as plain JSON strings (so a future
  serde-format change can't silently break the WASM bridge)
