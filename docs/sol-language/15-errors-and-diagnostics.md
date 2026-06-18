# 15 — Errors and Diagnostics

> **Status:** Canonical. Sourced from the `sol/` crate
> (`sol/src/parser.rs`, `sol/src/vm.rs`), the editor bridge
> `compiler-wasm/src/lib.rs`, the diagnostic contract
> `src/compiler/types.ts`, and the editor validator
> `src/graph/validate.ts`.

There are two distinct error worlds in SolFlow, and keeping them
straight is the whole point of this chapter:

1. **The language returns plain string errors.** The canonical `sol/`
   crate has no type checker, no semantic-analysis pass, no error codes,
   and no source spans. Every fallible step returns
   `Result<_, String>` — a bare message.
2. **The editor sees structured diagnostics.** It gets them from the
   `compiler-wasm` bridge (a small, fixed code vocabulary) and from its
   own graph validator (kebab-case structural codes). These are tooling
   constructs layered on top of the language, not produced by it.

There are **no** `E0xxx` / `T90xx` codes anywhere in the live pipeline.

---

## 15.1 Language-level errors are plain strings

Every fallible stage in the crate returns `Result<_, String>`:

- The lexer never hard-errors (unterminated strings/chars and bad
  numbers fall back silently; no spans are tracked).
- The parser returns `Err(String)` on a malformed program
  (`sol/src/parser.rs`).
- `Compiler::compile` returns `Err(String)` on a lowering failure.
- The VM surfaces a runtime fault as `StepResult::Failed(String)` (or the
  `Err(String)` arm of `step`) — see chapter 14.

Type mismatches are **not** caught before runtime. There is no analyzer
that rejects `bool + bool` at compile time; it runs and fails at the
arithmetic instruction with a string like `cannot add true and false`.

Representative runtime messages (`sol/src/vm.rs`): `division by zero`,
`index <i> out of bounds`, `field '<name>' not found`,
`variable '<name>' not found`, `function '<name>' not found`,
`cannot use <value> as condition`.

None of these carry a code or a span. They are human-readable strings.

---

## 15.2 The editor bridge diagnostic vocabulary

The editor talks to the language through `compiler-wasm`
(`compiler-wasm/src/lib.rs`). Every bridge entry point returns a JSON
`Envelope { ok, value, diagnostics }`; `run_source_json` additionally
carries a `run` object. The bridge translates the crate's string errors
into structured `Diag` records.

The **complete** vocabulary the bridge emits is exactly five entries
(severity / phase / code):

| Severity | Phase | Code | Emitted when |
|---|---|---|---|
| Error | Parser | `E_PARSE` | `Parser::parse` returns `Err` |
| Error | Codegen | `E_CODEGEN` | `Compiler::compile` (or executor construction) returns `Err` |
| Error | Analyzer | `E_NO_WORKFLOW` | a run was requested but no `workflow` declaration exists |
| Warning | Runtime | `E_RUNTIME` | a run produced `StepResult::Failed` or the `Err` arm of `step` |
| Error | Internal | `ICE0001` | the bridge caught an internal panic / serialization failure |

That is the entire set. There are no `E0003` / `E0009` / `T90xx` codes
in the live pipeline; any document or tooling that references them is
describing a deleted compiler.

---

## 15.3 The diagnostic JSON shape (the stable contract)

The shape of each diagnostic — independent of the small code set above —
is the stable editor contract in `src/compiler/types.ts`:

```ts
interface SolDiagnostic {
  severity: 'Error' | 'Warning' | 'Note';
  phase: 'Lexer' | 'Parser' | 'Analyzer' | 'Codegen' | 'Runtime' | 'Internal';
  code: string;
  message: string;
  span: SourceSpan | null;     // { start, end } | null
  related: RelatedSpan[];
  help: string | null;
}
```

Notes on the shape:

- `DiagnosticPhase` enumerates six phases, but `Lexer` is **reserved and
  never emitted today**; the live bridge uses only Parser, Codegen,
  Analyzer, Runtime, and Internal (§15.2).
- `span` is always `null` from the current bridge — the crate tracks no
  source positions. The field exists so the contract is stable if spans
  are added later.
- The envelope wrapper (`CompileEnvelope<T> { ok, value, diagnostics }`)
  is uniform across every bridge entry point.

---

## 15.4 Runtime errors the browser sim emits

When the editor runs a workflow via `run_source_json`, the browser sim
reports a runtime error using a small tagged union (`RtErr` in
`compiler-wasm/src/lib.rs`). The sim emits only **two** kinds:

| Kind | Meaning |
|---|---|
| `ExtCallBlocked { function_name, url }` | the workflow hit an external Action; the sim cannot resolve it, so it reports it as blocked |
| `StepLimit { limit }` | the run exceeded the sim's step guard |

A `StepResult::Failed` from the VM is surfaced separately, as a
`Warning / Runtime / E_RUNTIME` diagnostic (§15.2), not as an `RtErr`.

The TypeScript side (`src/compiler/types.ts`) declares a wider
`RuntimeError` union — `DivByZero`, `IndexOutOfBounds`, `StackUnderflow`,
`ExtCallFailed`, `HeapShapeMismatch`, `Cancelled`, `ResourceLimit`, plus
the two above. That wider union is the **shared wire shape** with the
controller so editor code can `switch` on `kind` exhaustively; the
browser sim does **not** emit most of those variants. Do not read the
union as a list of what the sim produces — only `ExtCallBlocked` and
`StepLimit` come from the sim today.

---

## 15.5 Editor-side structural validation

Separately from the bridge, the editor runs structural checks over the
visual graph in `src/graph/validate.ts`. These never touch the
language; they catch graph-shape problems before emission. They use
kebab-case codes (severity is per-check, error or warning):

| Code | What it flags |
|---|---|
| `no-entry` | no `start` function or trigger node |
| `unnamed-function` | a function with an empty name |
| `enum-first-char-collision` | two enum variants share a first character (the runtime dispatch hazard, chapter 14 §14.7) |
| `missing-input` | a required input port with no edge and no inline expression |
| `bad-inline-expression` | an inline expression that fails the lint |
| `unset-struct` / `unknown-struct` | a struct node with no struct selected, or a struct that is not defined |
| `unset-field` | a field node with no field selected |
| `unset-enum` / `unknown-enum` | an enum node with no enum selected, or an enum that is not defined |
| `unset-variant` | an enum node with no variant selected |
| `unset-call` / `unknown-call` | a call node with no target, or a target that is not found |
| `unset-var` | an assign/get node with no variable name |
| `unresolved-var` | a `varGet` that references a name not declared in the function |
| `type-mismatch` | a data edge whose source and target port types differ |

These are the editor's own checks. They are distinct from the five
bridge codes (§15.2) and from the language's plain-string errors (§15.1).

---

## 15.6 Summary: three layers, no codes from the language

- **Language** (`sol/`): plain `String` errors. No codes, no spans, no
  type checker. Runtime faults are `StepResult::Failed(String)`.
- **Bridge** (`compiler-wasm`): five structured codes — `E_PARSE`,
  `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001` — in the stable
  envelope shape from `src/compiler/types.ts`.
- **Editor validator** (`src/graph/validate.ts`): kebab-case structural
  codes over the graph.

If you see `E0xxx`, `T90xx`, an "analyzer phase" rejecting type errors,
or a citation to `compiler/src` or `runtime/src`, it is describing
software that no longer exists.

---

## 15.7 Sources cited in this chapter

- `sol/src/parser.rs`, `sol/src/vm.rs` — language-level `Result<_,
  String>` errors and `StepResult::Failed`
- `compiler-wasm/src/lib.rs` — the five-code diagnostic vocabulary and
  the `RtErr` sim runtime-error union
- `src/compiler/types.ts` — the stable `SolDiagnostic` shape and the
  wider `RuntimeError` wire union
- `src/graph/validate.ts` — the editor's kebab-case structural codes
