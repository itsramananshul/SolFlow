# Phase B — Release Notes

> Scope: SolFlow Phase B (canonical compiler + IDE). Shipped over
> commits c1 → c34 on branch `feat/solflow-phase-a` (the branch
> was named before Phase B was carved out; Phase B work landed
> incrementally without re-branching).
>
> Status as of 2026-05-27: **complete**.

## The headline

SolFlow used to be a visual editor with a TypeScript-only graph
emitter and a JavaScript approximation interpreter. Phase B
replaced both with the canonical Rust SOL compiler + VM, compiled
to WebAssembly and embedded in the browser editor.

What this means in practice:

- Every diagnostic the user sees comes from the canonical
  lexer / parser / analyzer / codegen.
- Every `print` line + return value + runtime error in the Run
  modal comes from the canonical SOL VM.
- The graph → source emitter (TS) is now backed by a round-trip
  stability contract verified by snapshot tests.
- AST → graph import exists, with an honest report classifying
  every construct as Full / Partial / Source-only / Unsupported.

No JavaScript reimplementation of SOL semantics owns user-displayed
output anywhere in the editor.

## What shipped, by milestone

### B.1 — Library API skeleton
- Standalone `compiler/` Rust crate with `lexer`, `parser`,
  `analyzer`, `bytecode` modules.
- `[lib]` + `[[bin]]` split; thin `sol` CLI consuming the library.

### B.2 — Diagnostics as values (c3–c10)
- Replaced every `eprintln + process::exit(1)` with structured
  `SolDiagnostic` values across lexer, parser, analyzer, codegen.
- Parser recovery (panic-mode sync to next top-level keyword).
- Analyzer recovery (per-statement; surfaces multiple errors per
  file).
- 19+ stable error codes (E0xxx parse, E1xxx semantic, etc.).
- ICE diagnostic boundary (`DiagnosticPhase::Internal`,
  `ICE_*` codes) for compiler-bug-vs-user-error distinction.
- 8 negative-fixture diagnostic-code-assertion tests.

### B.3 — AST serde groundwork (c9)
- `serde::{Serialize, Deserialize}` derives on `Ast`, `Type`,
  `Token`, `Symbol`, `SolDiagnostic`, `SourceSpan`, `RelatedSpan`,
  `DiagnosticPhase`, `DiagnosticSeverity` — feature-gated.
- Round-trip tests verify JSON ⇄ Rust types stay lossless.

### B.4 — WASM bridge MVP (c11–c13)
- New `compiler-wasm/` sibling crate (wasm-bindgen).
- Stable JSON envelope contract: `{ ok, value, diagnostics }`.
- Exports: `parse_source_json`, `analyze_source_json`,
  `compile_source_json`, `version`, and later `run_source_json`.
- Panic isolation via `console_error_panic_hook` +
  `std::panic::catch_unwind` → synthetic ICE diagnostic.
- 280KB initial bundle (357KB with B.10 VM).

### B.5 — Browser parsing + live diagnostics (c15)
- `src/compiler/api.ts` lazy-loads the WASM module.
- SourcePreview shows live compiler diagnostics while editing
  (250ms debounced, epoch-based stale-response guard).
- `vite-plugin-wasm` + `vite-plugin-top-level-await` wire the
  bundler-target WASM through normal dynamic `import()`.

### B.6 — Rich diagnostics UX (c23–c25)
- Lexer + parser diagnostics carry `SourceSpan`.
- Extracted `CompilerDiagnosticPanel.vue` — diagnostics grouped
  by phase, sorted by severity + position, clickable rows scroll
  the source pane to the offending byte range.
- Function-level source attachment via importer's
  `scanFunctionLines()`; clicking a function summary in the
  import report scrolls the source pane to its declaration.

### B.7 — AST → graph importer (c17–c22)
- New `src/graph/import/` module: walks parsed `Program` →
  produces `SolWorkflow` + `ImportReport`.
- Per-statement classification (Full / Partial / Source-only /
  Unsupported).
- Unsupported constructs preserved as placeholder `print` nodes
  with original SOL text inline + warning notice — nothing
  silently dropped.
- Multi-function support; cross-function call resolution via
  pre-allocated function ids.
- `ImportReportModal.vue` renders per-function breakdown with
  click-to-source navigation.

### B.8 — Graph → source canonicalization (c26)
- Existing `emit.ts` audited as already deterministic.
- New round-trip vitest suite caught a real importer bug
  (`exitPort` wiring on branch/while/for silently dropping
  subsequent statements) — fixed by widening
  `StmtImportResult` with explicit `exitPort` field.
- Snapshot tests + structural invariants + idempotence tests
  (every fixture).
- `docs/sol-language/CANONICALIZATION.md` documents the
  semantics-stable / structure-stable / not-byte-stable contract.

### B.9 — Sync model (c22)
- Explicit-action philosophy: Import / Edit / Reset are
  user-named moments; no live two-way binding; no AST-diff merge.
- Detached-edit state rendered honestly.
- `docs/sol-language/SYNC_MODEL.md` documents the four sanctioned
  transfers + the anti-features we deliberately don't build.

### B.10 — Canonical SOL VM in WASM (c28–c31)
- New `runtime/` sibling crate. Vendored upstream VM with four
  surgical edits: output buffer (no `println!`), `ExtCall` blocked
  (browser can't do raw TCP), step limit (1M default), common
  runtime errors as `RunError` values instead of panics.
- `compiler-wasm::run_source_json` export ships compile + run in
  one bundle (357KB optimized).
- TS `runSource()` + RunModal display canonical-VM output, return
  values, structured runtime errors (DivByZero / IndexOutOfBounds /
  StackUnderflow / StepLimit / ExtCallBlocked / HeapShapeMismatch).
- Legacy `interpret.ts` demoted to canvas-animation driver only;
  explicit `NOT AUTHORITATIVE` / `DO NOT extend` banner.

### B.11 — Stabilization (c32–c34, this commit)
- VM hardening sweep: `GetField` / `SetField` OOB → structured
  `IndexOutOfBounds` (previously panicked → ICE).
- `npm run check` single command runs typecheck + vitest + Rust
  workspace tests (92 tests total).
- `scripts/regenerate-ast-fixtures.sh` for one-command importer
  fixture refresh.
- Documentation pass: root README rewritten, this file added,
  Phase B plan marked complete.

## Test scoreboard at Phase B close

```
$ npm run check
  typecheck    ✓ clean
  vitest       ✓ 50 / 50
  cargo workspace
    compiler smoke      ✓  2 /  2
    compiler diagnostics ✓ 11 / 11
    compiler serde       ✓  5 /  5
    compiler-wasm        ✓ 10 / 10
    runtime              ✓ 14 / 14
                         ─────────
                         ✓ 42 / 42
  ─────────────────────────────────
  total tests: 92 ✓
```

## What's intentionally not in Phase B

These are open questions Phase B chose not to answer:

- **Per-node bytecode mapping** — would let canvas highlighting
  track canonical execution instead of approximate JS trace.
  Requires threading node IDs from graph → AST → bytecode and
  publishing per-step events from the VM. Out of scope; left
  for a future bundle.
- **AST-level source spans** — would unlock click-to-source on
  analyzer diagnostics (currently only lexer + parser spans).
  Requires every AST variant to carry a span; mechanical change
  across the parser. Out of scope.
- **End-to-end source → emit → source via WASM in Node** — the
  importer tests use pre-generated AST JSON. Adding a Node-target
  WASM build would enable true round-trip vitest assertions.
  Snapshot tests catch the same drift one cycle earlier in
  practice.
- **Web Worker offloading** — canonical compile + run runs on the
  main thread today. Big-file users may want a worker. The 250ms
  debounce + 357KB WASM is fast enough at current scale.
- **`fieldSet` / `indexSet` import** — complex assignments
  (`a.b = x`, `a[i] = x`) become unsupported placeholders. Mapping
  needs LHS type resolution; B.8 territory if revisited.
- **Top-level `let`** — SolFlow's graph schema doesn't model
  module-level lets; they're lost on round-trip. Either extend
  the schema or auto-wrap in an implicit `init()` function.

## What's intentionally not in SolFlow at all (Phase C territory)

These would change SolFlow from "compiler-backed IDE" into a
full execution platform — different product:

- Real external-call execution (HTTP / RPC to live controllers).
- Multi-user authentication / authorization.
- Persistent server-side workflow storage.
- Deployment / scheduling infrastructure.
- Collaboration features.

## Architectural commitments that survived Phase B

The bigger-picture choices that the work didn't compromise on:

1. **The compiler is the source of truth.** Every place SolFlow
   could have implemented SOL semantics in JS, it didn't. The
   editor calls into Rust via WASM.
2. **Honesty over magic.** Unsupported syntax surfaces as a
   notice + a preserved-source placeholder, never silently
   dropped. External calls in browser sim are blocked with a
   structured diagnostic, never faked. Sync is explicit-action,
   never a watcher pretending to handle conflicts.
3. **The graph is the canonical form for the workflow.**
   Source editing is detached + explicit. Round-trip preserves
   semantics + structure, not byte-formatting.
4. **Every commit ships tests.** The 92-test corpus across two
   test runners is what gives confidence that the multi-crate +
   TS + WASM stack stays coherent.
