# Phase B — Release Notes

> **Historical document.** These notes record the Phase B release built on the
> previous standalone SOL compiler (the removed `compiler/` and `runtime/`
> crates). The canonical language now lives in the `sol/` crate
> (`openprem-sol-v2`); it has no type checker and no `E0xxx` or `T90xx` error
> codes. For the current, accurate reference see the rewritten chapters 01
> through 23, plus `SPEC.md`, `GRAMMAR.md`, and `ERROR_REFERENCE.md`. The text
> below is kept as a historical record and does not describe the current
> system.

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

## Deferred-Phase-B bundle (B.D, post-stabilization, 2026-05-27)

After B.11 closed Phase B, a follow-up bundle landed targeting the
items listed in the original B_RELEASE_NOTES "What's intentionally
not in Phase B" section. Four of the six items shipped; two stay
deferred with documented reasoning.

### Shipped

**B.D c35 — AST source spans foundation.** Ten high-value struct
variants (`Block`, `DeclFunc`, `DeclExtFunc`, `DeclVar`,
`DeclStruct`, `DeclEnum`, `StmtImport`, `StmtIf`, `StmtWhile`,
`StmtFor`) gain `span: Option<SourceSpan>` with
`#[serde(default)]` for backward compat. Parser threads spans
through; analyzer's `check()` reads them via a `node_span()`
helper and attaches an approximate-but-real source location to
every diagnostic. TS-side `ast.ts` mirrors the change with
optional `span` fields. Tuple variants (`ExprInteger`, `ExprVar`,
…) stay as-is — converting them for marginal benefit was
rejected on discipline grounds.

**B.D c37 — Importer expansion.** Three importer upgrades close
the biggest "partial / source-only" gaps in
`IMPORT_COMPATIBILITY.md`:
- `varName.field = expr` → `fieldSet` node (struct name inferred
  from a function-wide scope scan)
- `array[i] = expr` → `indexSet` node (`elementType` defaults to
  `any`; user can retype)
- Top-level `let` → auto-wrapped into a synthetic `__init()`
  function (with a notice documenting the scope change)
- Importer source attachment now prefers AST span over textual
  function-line scan (`scanFunctionLines` stays as fallback)

**B.D c39 — Node-target WASM + true e2e round-trip tests.** New
`compiler-wasm/pkg-node/` target (committed). Vitest now runs
real `parse → import → emit → parse → import → compare` cycles
via the canonical compiler. Catches integration drift that
snapshot tests can't (e.g. emit producing syntax the parser
rejects, HashMap ordering surviving past determinism guards).
5 fixtures covered; top-level-let intentionally skipped (the
`__init` auto-wrap is a known semantic-change, not round-trip
stable).

**B.D c36 — Per-instruction source-span sidecar.** Codegen now
produces a `Vec<Option<SourceSpan>>` parallel to its `Vec<Inst>`
output. Approach: save+restore `current_span` around each
`compile()` call; backfill the sidecar with that span when
the inner body returns. Granularity is block-level today;
finer granularity requires per-leaf-expression spans (deferred).
The VM doesn't yet emit per-step trace events — the data
foundation is in place; the UI surface (live execution trace
panel) is a small future commit.

### Deferred (with reasoning)

**B.D c41 — Web Worker offloading.** ✅ **Landed 2026-05-27** after
the initial bundle. `src/compiler/worker.ts` runs the hot-path
`parseSource` + `analyzeSource` in a dedicated worker; the main
thread keeps `compileSource` + `runSource` (explicit user actions,
worker overhead not justified). Vite's `worker.plugins` config
replays the `wasm` + `topLevelAwait` plugins so the worker can
load the same `compiler-wasm/pkg/` bundle the main thread uses.
Build produces a 5.67KB worker chunk + shared 370KB WASM.
SourcePreview's epoch-based stale-response guard absorbs in-flight
obsolescence; no separate cancellation logic needed in the
worker. One worker instance per page, lazy-spawned on first call.

**Per-statement / per-leaf-expression spans.** Spans on tuple
variants (`ExprInteger`, `ExprVar`, `ExprString`, etc.) would
unlock per-token diagnostic precision and per-statement
canvas highlighting. Doing it without churning every match arm
in analyzer/importer requires converting tuple variants to
struct variants — a bigger shape change than this bundle's
budget. Block-level span attribution covers the most common
diagnostic UX cases.

## Deferred-B execution-mapping bundle (c42–c46, 2026-05-27)

Second deferred-B sweep, closing the per-node bytecode/execution
mapping that the original bundle stubbed but didn't finish.
Headline: the canonical SOL VM's execution now maps back to
source ranges AND graph nodes, with click-to-jump UX in the
RunModal.

**B.D c42 — VM execution trace + runtime-error spans.**
- Runtime: opt-in `with_trace()` records inst_ptrs per step;
  default off (zero overhead). Bounded by `trace_limit` (10k
  default); `trace_truncated` flips when cap hit. `error_inst_ptr`
  captured on the `RunError` path.
- Bridge: `run_source_json` enables tracing on every run, then
  maps trace inst_ptrs → source spans via the c36
  `instruction_spans` sidecar with adjacent-equal collapse
  (a 1000-step inner loop produces ONE trace entry, not 1000).
- Envelope extended (additive): `runtime_error_source_span`,
  `trace`, `trace_truncated`.
- `CompiledProgram` surfaces `instruction_spans` so consumers
  don't need a fresh Codegen instance.
- 4 new runtime tests, 2 new bridge tests.

**B.D c43 — Per-node source attachment.**
- `GraphNode.meta.sourceSpan?` added to the schema.
- Importer's `importStatement` shell attaches the AST span to
  the entry node after delegation; `astStatementSpan()` mirrors
  the Rust `Analyzer::node_span` helper.
- New `src/graph/nodeLookup.ts::findNodeForSpan(workflow, span)`
  scans every function's nodes for ones whose `meta.sourceSpan`
  contains the query and returns the smallest-containing match
  (most specific). Returns null for "no enclosing node found" —
  honest non-match, no synthesis.
- 3 new vitest tests.

**B.D c44 — RunModal Trace tab + click-to-source/node nav.**
- New "Trace" tab between Output and Generated SOL; step count
  badge.
- Each row: step index + line:col + snippet + optional canvas link
- "line N:C" clicks → switch to Generated SOL tab + scroll to
  that line (data-sol-line attributes on the SOL preview rows).
- "→ canvas" link (when graph mapping exists) → setActiveFunction
  + ui.requestFocus + close modal.
- Runtime errors get the same source + canvas links when
  `runtime_error_source_span` is populated.
- "(no graph mapping)" inline label for honest non-match cases.
- "truncated at cap" tag when `trace_truncated` is true.

**B.D c45 — Node-WASM e2e tests for execution paths.**
- Test file extended with 7 new cases covering: compile+run,
  control flow, invalid source short-circuit, div-by-zero +
  source span, trace de-duplication, ExtCall blocking,
  round-trip-then-execute semantic stability.

**B.D c46 — This commit.** Docs sync.

### Test scoreboard at second-bundle close

```
$ npm run check
  typecheck    ✓
  vitest       ✓ 69 / 69    (was 59; +3 c43 + +7 c45)
    expressions    15
    importer       14   (+3 source-attachment)
    round-trip     24
    e2e-round-trip 16   (+7 execution-path)
  cargo workspace
    compiler smoke      2 /  2
    compiler diagnostics 12 / 12
    compiler serde       5 /  5
    compiler-wasm       12 / 12   (+2 trace + error-span tests)
    runtime             18 / 18   (+4 trace tests)
                       ─────────
                       49 / 49
  ──────────────────────────────────
  total: 118 tests ✓
```

### Bundle close

The deferred-B agenda is now complete enough to move into
productization. The remaining items are explicit non-goals
already cataloged elsewhere:
- Per-leaf-expression spans (would require tuple→struct variant
  conversion; deferred indefinitely)
- Real external-call execution (Phase C product surface)
- Multi-user / deployment / scheduling (Phase C)

## Architectural commitments that survived Phase B

### Test scoreboard at B.D close

```
npm run check → all green
  typecheck    ✓
  vitest       ✓ 59 / 59
    expressions   15
    importer      11  (+3 fieldSet/indexSet/top-level-let)
    round-trip    24
    e2e-round-trip 9  (NEW — true parse→import→emit→parse cycles)
  cargo workspace
    compiler smoke      2 /  2
    compiler diagnostics 12 / 12  (+1 instruction_spans test)
    compiler serde       5 /  5
    compiler-wasm       10 / 10
    runtime             14 / 14
                       ─────────
                       43 / 43
  ─────────────────────────────────
  total: 102 tests ✓
```

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
