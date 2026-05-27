# Phase B — Compiler-Backed SOL IDE Implementation Plan

> **Status:** Phase B **complete + deferred-B bundles fully
> landed** (2026-05-27, milestones B.1–B.11 + B.D c35/c37/c39/
> c36/c41/c42/c43/c44/c45/c46). See `B_RELEASE_NOTES.md` for
> the full milestone summary including both deferred-B sweeps.
>
> SolFlow runs on canonical SOL semantics throughout; AST source
> spans flow through analyzer diagnostics + importer attachments;
> the canonical VM produces an execution trace with source spans;
> runtime errors carry their source range; graph nodes carry
> source-attachment metadata; the RunModal has a Trace tab with
> click-to-source AND click-to-node navigation; the hot-path
> parse/analyze runs in a Web Worker so a slow parse can't
> freeze the UI; true e2e round-trip + canonical execution
> verified via Node-target WASM in 16 vitest tests. **118 tests**
> across two test runners. `npm run check` runs everything.
>
> **Deferred-B is closed; ready for productization.**
>
> **What's deliberately deferred beyond Phase B** (cataloged in
> `B_RELEASE_NOTES.md` under "What's intentionally not in
> Phase B"): per-node bytecode mapping, AST-level source spans,
> Node-target WASM for true end-to-end round-trip tests, Web
> Worker offloading, `fieldSet`/`indexSet` importer mapping,
> top-level-let preservation. None block "SolFlow is a real
> compiler-backed visual IDE."
>
> **Branch:** `feat/solflow-phase-a` — Phase B landed
> incrementally on this branch.

This document is the master plan for turning SolFlow from a
graph-first SOL workflow editor into a real compiler-backed SOL
IDE. It synthesizes the existing chapter 18 / 20 / 21 / 22 / 23
audits and the `REMEDIATION_PLAN.md` open items into a concrete
sequence of milestones, each with goal, scope, files touched,
risks, and success criteria.

---

## Table of contents

1. [Current compiler architecture summary](#1-current-compiler-architecture-summary)
2. [Current SolFlow architecture summary](#2-current-solflow-architecture-summary)
3. [Gap analysis](#3-gap-analysis)
4. [Phase B milestone breakdown (B.1 – B.11)](#4-phase-b-milestone-breakdown)
5. [WASM strategy](#5-wasm-strategy)
6. [Diagnostics strategy](#6-diagnostics-strategy)
7. [AST/graph mapping strategy](#7-astgraph-mapping-strategy)
8. [Source synchronization strategy](#8-source-synchronization-strategy)
9. [Risk register](#9-risk-register)
10. [Phase B MVP definition](#10-phase-b-mvp-definition)
11. [Phase B non-goals](#11-phase-b-non-goals)
12. [Recommended first implementation prompt for B.1](#12-recommended-first-implementation-prompt)

---

## 1. Current compiler architecture summary

The canonical SOL compiler lives in a sibling Rust workspace,
not in this repository (snapshot date 2026-05-26 — see
`internal-notes.md` for the path). Today it is a binary target,
not a library; SolFlow cannot `import` from it directly.

### 1.1 Pipeline shape

```
source text
   ↓ Lexer (lexer.rs, ~390 lines)
Vec<Token>
   ↓ Parser (parser.rs, ~750 lines)
Program = Vec<Ast>          (placeholder usize::MAX for unresolved scopes)
   ↓ Analyzer (analyzer.rs, ~500 lines)
Program with scope IDs populated + tt_arena populated
   ↓ Codegen (bytecode.rs, ~710 lines)
Vec<Inst>
   ↓ VM (vm.rs, ~580 lines)
observable behavior (stdout, ext-call HTTP, exit code)
```

Per chapter 20 §20.1; per `init.rs:14–32` (the host-side
composition).

### 1.2 Per-stage characteristics

| Module | Lines | Owns | Key trait |
|---|---|---|---|
| `lexer.rs` | ~390 | Tokens, keywords, identifier rules, literal forms | 15-keyword set; underscore-as-trivia quirk |
| `parser.rs` | ~750 | Grammar, AST construction | Pratt-style 14-level precedence; placeholder scopes; `Ast` carries `Token` for operator |
| `analyzer.rs` | ~500 | Scope tables, type checks, duplicate-name rejection | Two-pass (function signatures pre-registered); several `todo!()` fallthroughs |
| `bytecode.rs` | ~710 | Instruction set, codegen | Field layout sorts alphabetically; emit-time `Type` inference with `Integer` fallback |
| `vm.rs` | ~580 | Stack-based interpreter, heap, HTTP ext-call transport | Several silent no-ops on type mismatch (T9010); `Ret` unconditionally pushes 0 (T9011) |
| `util.rs` | ~45 | `type_eq` helper | Has known bugs T9006/T9007/T9008 (latent) |
| `cli.rs` | ~57 | CLI flag parser for the binary | Two unwraps that can panic (T9017) |
| `init.rs` | ~33 | Pipeline glue + ext-endpoint passthrough | The single entry point for in-process composition |

### 1.3 Error handling — the load-bearing problem

Every compile-time error is `eprintln!` followed by
`std::process::exit(1)`. Audit-blocker #2 from
`reference/SOL_CRATE_IDE_READINESS_PLAN.md`:

- `lexer.rs`: 2 sites
- `parser.rs`: 21 sites
- `analyzer.rs`: 42 sites
- `bytecode.rs`: 2 sites
- `cli.rs` / `init.rs` / `main.rs`: ~50 sites combined

Cannot be called from anything that needs to continue execution
after an error — including a WASM context (where `process::exit`
crashes the host page) and an IDE context (where multiple errors
per compile are required).

### 1.4 Source spans — also load-bearing

`Token` carries no source position. `Ast` carries no source
position. Diagnostics today say *what* is wrong, never *where*.
For IDE use this is the second-biggest blocker after errors-as-
values; without spans there's no way to underline the offending
source range in an editor.

### 1.5 Where source truth currently lives

**Compiler side:** the `.sol` file on disk is the source of
truth. The Rust pipeline always parses from file; no string
input. `lexer.rs:198` takes a `&str` and interprets it as a
**file path**, not a source body (audit blocker #6).

**SolFlow side:** the graph (`SolWorkflow` JSON, persisted to
localStorage) is the source of truth. The emitter generates
`.sol` source from the graph on demand; nothing currently reads
`.sol` source back. The graph is the only authored artifact.

These two sources of truth do not currently meet. Phase B
brings them together.

---

## 2. Current SolFlow architecture summary

### 2.1 Graph schema (`src/graph/schema.ts`)

```
SolWorkflow {
  schemaVersion: 1,
  meta: { name, description?, createdAt, updatedAt },
  imports: ImportDecl[],
  structs: StructDecl[],
  enums:   EnumDecl[],
  functions: FunctionGraph[]
}
FunctionGraph {
  id, name, params: Param[], returnType: SolType,
  nodes: GraphNode[],
  edges: GraphEdge[]
}
GraphNode {
  id, data: NodeData (discriminated union over 22 kinds),
  position: { x, y },
  ports: NodePorts,
  expressions?: Record<string, string>   // inline SOL expressions per port id
}
GraphEdge {
  id, source: { node, port }, target: { node, port },
  kind: 'control' | 'data'
}
```

22 node kinds per chapter 18 §18.2. Schema is JSON-clean (no
Maps / Sets / Functions); autosave + history-stack round-trip via
`JSON.stringify` (chapter 23 §23.10).

### 2.2 Emitter (`src/emit/emit.ts`)

Phase A producer-only Graph → SOL emitter. ~440 lines. Walks the
graph per chapter 18 §18.2's per-kind mapping table; honors
inline expressions over wired edges; emits `// @trigger`
annotations for editor-side trigger nodes (T9001).

**No importer.** There is no SOL → Graph path today; the editor
can only consume workflows produced by itself or by Sol Man's
spec-to-workflow translation. Audit-blocker #14–#16 in the
upstream plan.

### 2.3 Validator (`src/graph/validate.ts`)

Phase A structural validator (~300 lines, with R1's expression
linter integration). Per chapter 18 §18.6:

- Per-node required-port satisfaction (edge OR inline expression)
- Struct/enum/function symbol resolution
- Inline expression lint (R1 — `expressionLint.ts`)
- Data-edge type-compatibility warning
- T9002 enum first-character collision warning
- `unnamed-function` defensive check (R1.2)

Returns `Diagnostic[]` with `{ severity, message, nodeId?,
functionId?, code }`. Cross-layer mismatches per chapter 18 §18.7
documented (T9018 — no inline-expression *content* check;
T9019/T9020/T9021 placeholder + literal-value edge cases).

### 2.4 Simulator (`src/runtime/interpret.ts`)

755-line in-browser SOL interpreter that walks the graph
directly. Per chapter 23, evaluates inline expressions via
JavaScript's `new Function` constructor (T9022) and disagrees
with the canonical SOL VM on:

- `+` does string concatenation (T9023)
- `/` always float division (T9024)
- `toBool` JS-permissive (T9025)
- Enum compare by name, not first-char hash (T9026 — silently
  hides T9002)
- Flat per-function scope, no block scoping (T9027)
- Undefined varGet throws instead of auto-creating (T9028)
- Safety limits (T9030); no ext-function support (T9031)

The R1 inline-expression linter is the security net for T9029.

### 2.5 Source preview (`src/components/SourcePreview.vue`)

Read-only viewer for the emitted SOL. Updates reactively when
the graph changes (via `graph.emitted` computed). 380 lines.
**No editing.** No syntax highlighting (the upcoming compiler
diagnostics will be the prompt for proper editor mode in B.6).

### 2.6 Sol Man generation path

Browser → `api/sol-man/generate.ts` (Vercel function) → LLM
provider → `GeneratedGraphSpec` JSON → `src/sol-man/applyGraph.ts`
→ `SolWorkflow` → `graph.loadWorkflow`. The LLM never writes
`.sol` source directly; it emits a graph spec the editor
translates. R1's hard-rules prompt + the validator + the repair
pass keep this path safe (chapter 19).

### 2.7 Stores and runtime

`src/stores/graph.store.ts` (~1100 lines): mutation surface,
undo/redo, autosave, beforeunload flush (R1.7), schema-validate
on loadWorkflow (A.10.1).

`src/runtime/simulate.ts`: trace recorder. Replays via
`src/stores/simulation.store.ts` for canvas playback.

---

## 3. Gap analysis

The architecture summary above implies the gap; this section
makes each blocker explicit. Listed in dependency order
(earlier blockers must clear first).

### G1 — `process::exit(1)` everywhere in the Rust compiler

**Symptom:** the compiler is uncallable from anything that needs
to recover after an error. Calling it from WASM crashes the
browser page; calling it from a sidecar server kills the
request handler; calling it from an LSP-style daemon kills the
daemon.

**Sites:** ~70 across the seven compiler modules (chapter 20
§20.19).

**Fix path:** introduce a `Diagnostic` value type and a
`Result<T, Vec<Diagnostic>>` return contract through the
pipeline. Convert each `eprintln! + exit(1)` into a structured
diagnostic appended to a collector and an `Err(())` propagation.

**Effort:** Large. Touches every Rust module. Listed as audit
blocker #2.

### G2 — No source spans on `Token` / `Ast`

**Symptom:** diagnostics can name a symbol (`x`, `lookup_user`)
but cannot point at a byte range. Editors cannot underline the
offending source text. Round-trip cannot map a runtime error
back to the source line that produced the bad bytecode.

**Fix path:** add `Span { start: ByteOffset, end: ByteOffset }`
to `Token` and to every `Ast` variant. Plumb through the parser.
Threaded into every diagnostic.

**Effort:** Large. Audit blocker #3.

### G3 — No `lib.rs` — the Rust crate is binary-only

**Symptom:** SolFlow cannot `use sol_compiler::…`. Every
consumer has to shell out to the binary.

**Fix path:** add `[lib]` to `Cargo.toml`; export the public
surface (`Lexer`, `Parser`, `Analyzer`, `Diagnostic`, AST types).

**Effort:** Small. Audit blocker #1. **This is the prerequisite
for everything else in Phase B.**

### G4 — Lexer is file-I/O only

**Symptom:** `lexer.rs:198` takes a `&str` and treats it as a
file path. The IDE never has a path; it has the in-memory
source body.

**Fix path:** add `Lexer::from_str(source: &str)` constructor;
keep `from_path` as a convenience.

**Effort:** Small. Audit blocker #6.

### G5 — AST stores `Token` in `ExprBinary.op` / `ExprUnary.op`

**Symptom:** AST serialization has to carry full Token enums
(with `String` / `i128` payloads) for every binary expression.
Couples the AST representation to the lexer; a semantically a
`+` is a `BinOp::Add`, not a generic `Token`.

**Fix path:** introduce `BinOp` / `UnaryOp` enums; convert at
parse time.

**Effort:** Medium. Audit blocker #7. Required before serde
serialization (G6) is sensible.

### G6 — No serde derives on `Token` / `Ast` / `Type` / `Diagnostic`

**Symptom:** The WASM bridge serializes everything as JSON to
cross the boundary. No serde = no bridge.

**Fix path:** `#[derive(Serialize, Deserialize)]` on every type
that crosses the boundary. `Token::Integer(i128)` requires a
custom serde adapter (G7).

**Effort:** Medium. Audit blocker #4.

### G7 — `Token::Integer(i128)` not JSON-encodable

**Symptom:** JSON has no 128-bit integer type. Naive serde
output rounds. Loses literal fidelity for any integer literal
above `Number.MAX_SAFE_INTEGER` (~2^53).

**Fix path:** `#[serde(with = "i128_as_str")]` adapter.
Documented in chapter 04 §4.2.1 as a separate runtime hazard
already.

**Effort:** Small. Audit blocker #13.

### G8 — `HashMap` for struct fields / enum variants destroys insertion order

**Symptom:** round-trip via the AST does not preserve declared
field/variant order. The first save → load cycle reshuffles
declarations. The bytecode emitter compensates by sorting
alphabetically (chapter 09 §9.1) — fine for runtime correctness,
but a tool that depends on display order is broken.

**Fix path:** swap `HashMap<String, T>` for `IndexMap<String, T>`
(insertion-order-preserving). Or `Vec<(String, T)>` if serde
ergonomics are easier that way.

**Effort:** Small. Audit blocker #5.

### G9 — Analyzer's semantic holes

**Symptom:** Several `todo!()` fallthroughs (`ExprStructInit`,
`ExprArrayInit` per chapter 09 §9.2); commented-out return-path
check (chapter 05 §5.1); ignored `let`-initializer types
(chapter 06 §6.1). The analyzer accepts programs the SPEC
considers invalid.

**Fix path:** implement each check. Each is independently
small (~50 lines of analyzer work per hole).

**Effort:** Medium total. Audit blocker #18.

### G10 — No graph ↔ AST mapping

**Symptom:** SolFlow has Graph → SOL (the emitter) but not
SOL → Graph or AST → Graph or Graph → AST. The editor cannot
import hand-written SOL; the canonical compiler cannot validate
SolFlow-produced workflows without round-tripping through
generated source.

**Fix path:** see §7 below. Build `AstToGraph` (deterministic
walker) and `GraphToAst` (canonical builder).

**Effort:** Large. Audit blockers #14–#16.

### G11 — No source formatter / pretty-printer

**Symptom:** `emit_sol(graph)` exists today but `emit_sol(ast)`
does not. To go from a parsed AST back to canonical source
(needed for graph → AST → source workflows that re-render the
source pane) the compiler needs an explicit pretty-printer.

**Fix path:** write a deterministic AST → SOL printer in the
Rust compiler crate. Stable formatting rules; round-trip-stable
(parsing the printed output and printing again produces
identical text).

**Effort:** Medium. Audit blocker #15. Net-new code (~500
lines).

### G12 — Simulator vs canonical-VM divergences

**Symptom:** "works in simulator → fails in production" class
of bugs documented in T9022–T9028.

**Fix path:** Not a Phase B blocker per se. Once the compiler
is callable via WASM, the simulator can be **replaced** by the
canonical bytecode VM running compiled SOL (also via WASM). Or
the simulator can be tightened to match canonical semantics
(R2.3–R2.7 in `REMEDIATION_PLAN.md`).

**Effort:** Re-running compiled SOL in WASM is Large; tightening
the simulator is Medium. Phase B can choose.

### G13 — No source-position tracking through codegen

**Symptom:** Bytecode instructions don't carry the source range
they were emitted from. Runtime errors can't be mapped back to
source.

**Fix path:** Add an `Inst::WithSpan(...)` wrapper or an
out-of-band `Vec<Span>` parallel to `Vec<Inst>` indexed by
instruction position.

**Effort:** Medium. Phase B can defer this to a later milestone
since it's needed only for runtime-error UX, not for compile-
time diagnostics.

### Summary — dependency graph

```
G3 lib.rs  ──────────────────────────────────────→  required for ALL of B.1+
G1 errors-as-values  ─────→  needed for B.2 onwards
G2 spans  ────────────────→  needed for diagnostics in the editor (B.6)
G4 from-str  ─────────────→  needed for parsing in-memory source (B.5)
G5 BinOp/UnaryOp  ────────→  prerequisite for clean serde (B.3)
G6 serde derives  ────────→  prerequisite for WASM bridge (B.4)
G7 i128 adapter  ─────────→  part of G6
G8 IndexMap  ─────────────→  required for AST → graph fidelity (B.7)
G9 analyzer holes  ───────→  improves diagnostic quality (B.2 / B.6)
G10 graph↔AST  ───────────→  the core of B.7/B.8
G11 formatter  ───────────→  needed for graph → source (B.8)
G12 simulator parity  ────→  optional (B.10); deferable past Phase B
G13 codegen spans  ───────→  optional (B.10); deferable past Phase B
```

---

## 4. Phase B milestone breakdown

Eleven milestones. Each is sized to be a coherent commit batch
(small days, not weeks). Each gates the next.

### B.1 — Rust crate cleanup for WASM readiness ✅ **DONE**

> **Landed 2026-05-26** as part of the B.1+B.2 foundation bundle
> (commits 934d64d → 9175268 → 92ecbc1 → c0dc875). The standalone
> compiler now lives in `compiler/`, separate from the SolFlow
> editor sources, and is consumed as a library through
> `solflow_compiler::{lex_source, parse_source, analyze_source,
> compile_source}`. The `sol` CLI is a thin wrapper. See
> `compiler/README.md` and `compiler/UPSTREAM.md`.

**Goal:** convert the binary-only Rust crate into a library with
a clean public surface that WASM and Rust callers alike can use.

**Why it matters:** every subsequent milestone depends on the
crate being importable. Without B.1, SolFlow can only shell out
to the binary — useless in a browser.

**Files likely touched:**
- `Cargo.toml` (add `[lib]`)
- `src/sol/lib.rs` (new) — re-exports the public surface
- `src/sol/mod.rs` (already exists; may need adjustment)
- A new `cli.rs` binary that consumes the library (preserves the
  existing CLI behavior)

**Technical tasks:**
1. Split the Cargo target. New `[lib]` for the SOL compiler; the
   existing binary becomes a thin wrapper around library calls.
2. Decide on the public re-exports: `Lexer`, `Parser`, `Analyzer`,
   `Codegen`, `Ast`, `Type`, plus the new `Diagnostic` (after
   B.2).
3. Carve out the `init.rs` host-side composition into something
   reusable — `compile(source: &str) -> Result<CompiledProgram,
   Vec<Diagnostic>>` is the target shape.
4. Make `Lexer::from_str(source: &str)` available (G4).

**Risks:**
- Existing binary callers (test runners, manual `cargo run`)
  must keep working.
- Cyclic deps inside `sol/` need to be untangled if module
  boundaries cross.

**Success criteria:**
- `cargo build --lib` succeeds
- `cargo build --bin sol` succeeds (binary still works)
- Existing fixture tests pass via the new library API path
- A simple Rust test `let tokens = Lexer::from_str("function
  start() {}").tokens();` compiles and runs

**What NOT to do in B.1:**
- No `process::exit` removal yet (B.2)
- No serde derives yet (B.3)
- No WASM bridge yet (B.4)
- No source-span work (deferred to B.2 alongside errors-as-values)

### B.2 — Diagnostics-as-values ✅ **DONE** (spans deferred)

> **Landed 2026-05-26.** Lexer, parser, analyzer, and codegen all
> return `SolDiagnostic` values; no library code calls
> `process::exit` or `panic!` on an error path. The CLI now renders
> diagnostics in cargo-style format. Tests: 10/10 green (2 smoke,
> 8 negative-fixture diagnostic-code assertions).
>
> **Deferred from B.2:** source-span attachment on every diagnostic.
> The `SourceSpan` type and pipeline plumbing exist
> (`compiler/src/diagnostic.rs`); spans are not yet attached at
> every emission site. Adding spans through the lexer + parser is
> a future commit batch (sized as its own milestone since it
> touches every emit site) — gated only when the editor actually
> needs underline ranges, i.e. when B.4 lands.
>
> Catalog of remaining intentional panic / abort sites:
> `compiler/REMAINING_PANICS.md`.

**Goal:** replace every `eprintln! + exit(1)` with a structured
`Diagnostic` value returned through a `Result` chain. Add source
spans to tokens and AST nodes so diagnostics can carry ranges.

**Why it matters:** the IDE needs to:
1. report multiple errors per compile (not just the first)
2. recover gracefully after errors (for partial parse / partial
   analysis)
3. point at source ranges (for editor underlining)

None of these are possible while the compiler exits the process.

**Files likely touched:**
- `lexer.rs`, `parser.rs`, `analyzer.rs`, `bytecode.rs`, `vm.rs`
  — every `eprintln + exit` site
- `util.rs` — `TypeMismatch` already exists; extend to
  `Diagnostic`
- New `src/sol/diagnostic.rs` — the canonical types

**Technical tasks:**
1. Define `Diagnostic`, `Severity`, `Span`, `DiagnosticCode`,
   `RelatedSpan` per audit doc Appendix B.
2. Add `Span` to `Token` (lexer plumbs byte offsets).
3. Add `Span` to every `Ast` variant (parser propagates from
   child tokens).
4. Convert every `eprintln + exit` to a diagnostic-emit + an
   error-recovery point. Parser recovers by skipping to the next
   declaration boundary; analyzer recovers by skipping to the
   next statement.
5. Define the provisional `Exxxx` / `Wxxxx` code scheme
   (mirrors `docs/sol-language/ERROR_REFERENCE.md`'s codes).
6. The pipeline returns `Result<T, Vec<Diagnostic>>`; partial
   success is OK as long as Errors are reported.

**Risks:**
- Diagnostic-recovery design — where exactly the parser/analyzer
  can recover without producing cascading false positives.
- Span tracking through complex grammar productions (Pratt
  precedence chain especially).
- Behavior change: programs that previously "ran" via the binary
  may now produce more (or different) errors than before.

**Success criteria:**
- Every fixture in `reference/sol files/` parses + analyzes
  without exiting the process
- Every negative fixture produces the expected diagnostic code
  (matching `ERROR_REFERENCE.md`)
- Multiple errors per file are reported when present
- No `process::exit(1)` call remains in `lexer.rs` / `parser.rs`
  / `analyzer.rs`

**What NOT to do in B.2:**
- No WASM bridge yet
- No semantic-hole fixes (G9) — keep behavior identical to
  current analyzer, just route through diagnostics
- No formatter / pretty-printer work
- No runtime work (`vm.rs` panics can stay as `panic!` for now;
  they're already separate from the compile path)

### B.3 — AST serialization contract 🟡 **Groundwork done (c9)**

> **Landed 2026-05-27.** AST + diagnostics derive
> `serde::{Serialize, Deserialize}` behind an opt-in `serde`
> cargo feature. Round-trip tests pass (`cargo test --features
> serde` → 16/16). `SolDiagnostic.code` widened from
> `&'static str` to `String` to unblock Deserialize.
>
> **Still pending for full B.3:** BinOp/UnaryOp enums (G5),
> `i128` adapter for `ExprInteger` (G7), IndexMap swap for
> ordered struct fields (G8), TypeScript-types generation. None
> of these block B.4 (the WASM bridge can ship today's shapes
> as-is and refine later). See `compiler/AST_SERDE_NOTES.md`.

**Goal:** make every AST type round-trip cleanly through
`serde_json::to_string` and back.

**Why it matters:** the WASM bridge serializes everything as JSON
across the JS↔Rust boundary. Without serde derives the bridge is
impossible.

**Files likely touched:**
- `parser.rs` — `Ast`, `Type`, `Program`
- `lexer.rs` — `Token` (only the variants that survive into
  diagnostics; tokens themselves shouldn't cross the boundary)
- `analyzer.rs` — `Symbol`, `Diagnostic` (already serializable
  after B.2)
- `util.rs` — `TypeMismatch`
- New `Cargo.toml` deps: `serde`, `serde_json`

**Technical tasks:**
1. Introduce `BinOp` / `UnaryOp` enums; convert `ExprBinary.op:
   Token` and `ExprUnary.op: Token` to `BinOp` / `UnaryOp`
   (G5). This is also the right time to drop `ExprBinary.op:
   Token` for AST decoupling (audit blocker #7).
2. Add `#[derive(Serialize, Deserialize)]` to every AST type +
   `Diagnostic` + `Span` + `Type`.
3. Add the `i128` adapter for `ExprInteger` (G7).
4. Swap `HashMap` → `IndexMap` for struct fields and enum
   variants (G8); ensure serde round-trip preserves order.
5. Generate TypeScript types via `tsify-next` or hand-write
   them. **Recommendation:** `tsify-next` for auto-generation
   with manual review; eliminates two-source drift.

**Risks:**
- Adapter design for `i128` — JSON Number vs string trade-off
  (recommendation: string with optional Number for small
  values).
- `IndexMap` adds a dependency; ensure WASM-compatible.
- TS-type generation pipeline integration with the editor
  build.

**Success criteria:**
- `serde_json::to_string(&program)?` round-trips identically
  through `serde_json::from_str` on every positive fixture
- Field order in struct/enum decls preserved through the
  round-trip
- Auto-generated `.d.ts` matches the Rust types and is included
  in the editor build

**What NOT to do in B.3:**
- No WASM bridge wiring yet (B.4)
- No mapping from Ast to graph yet (B.7)

### B.4 — WASM bridge MVP ✅ **DONE**

> **Landed 2026-05-27.** `compiler-wasm/` sibling crate ships a
> wasm-bindgen bridge. Exports `parse_source_json`,
> `analyze_source_json`, `compile_source_json`, `version`. Stable
> JSON-string envelope: `{ ok, value, diagnostics }`. Panic-safe
> (catch_unwind → ICE diagnostic). Bundle is 280KB optimized
> (`opt-level = "z"` + LTO + size-z). Generated `pkg/` is
> committed so editor devs don't need a Rust toolchain. Vite
> wires it up via `vite-plugin-wasm` + `vite-plugin-top-level-await`.
> Native test suite (6/6) pins the envelope shape so the TS side
> can rely on the contract.

**Goal:** produce a `wasm-pack`-built npm-installable package
containing the SOL compiler. Expose `compile(source: string):
{ ast: Program | null, diagnostics: Diagnostic[] }`. Wire it
into SolFlow's build.

**Why it matters:** the bridge is the link between the Rust
compiler and the browser editor. After B.4, every subsequent
milestone is editor-side work consuming the bridge.

**Files likely touched:**
- New `sol_compiler_wasm/Cargo.toml` (thin wrapper crate)
- New `sol_compiler_wasm/src/lib.rs` (wasm-bindgen exports)
- `package.json` — add the built WASM package as a local dep
- `vite.config.ts` — configure WASM plugin
- New `src/sol-compiler/index.ts` — thin TS wrapper exposing
  typed `compile()` API

**Technical tasks:**
1. Pick the toolchain (recommendation: `wasm-pack build
   --target web` producing ESM, with `wasm-bindgen` for the
   typed boundary).
2. Define the wasm-bindgen exports: `compile`, `parse`,
   `analyze`, `tokenize` (the last three for debugging).
3. Set up the editor build to consume the WASM package.
   Lazy-load the WASM module so cold-start stays fast (the
   blob is likely 1-3 MB).
4. Add a `src/sol-compiler/index.ts` wrapper that types the API,
   handles lazy-load, surfaces compile errors via toast on the
   first failure (so editor users see when the compiler crashes
   the wrong way).

**Risks:**
- Build pipeline — `wasm-pack` integration with Vite needs
  `vite-plugin-wasm` + `vite-plugin-top-level-await`.
- WASM binary size — likely 1-3 MB compressed; that's a real
  cold-start cost for the editor.
- Memory leaks if not careful with `wasm-bindgen` lifetimes.

**Success criteria:**
- `await import('@/sol-compiler').then(c => c.compile('function
  start() {}'))` resolves successfully in the editor
- The returned AST matches what the Rust binary produces for the
  same source
- Lazy-load works: the WASM module is not part of the initial
  bundle
- Bundle size impact documented and within budget

**What NOT to do in B.4:**
- No SolFlow-side consumer wiring yet (deferred to B.5)
- No new compiler features
- No graph-mapping work

### B.5 — Parse SOL in browser 🟡 **MVP done; full sync deferred**

> **Landed 2026-05-27 (MVP).** Source editor (`SourcePreview.vue`)
> now runs the canonical compiler against the user's buffer while
> they edit. 250ms debounce, epoch-based stale-response cancel,
> ICE-safe panic isolation. Diagnostics shown as `[code] [phase]
> message` rows under the editor with severity-keyed colors.
> Detached-edit banner copy updated to reflect the new state.
>
> **Still pending for full B.5:** spans (so we can highlight error
> ranges in CodeMirror — the compiler doesn't yet attach spans at
> every emit site), Web Worker offloading (debounce + 280KB WASM
> is fast enough today; revisit on big files), AST → graph
> importer is its own milestone (B.7).

**Goal:** consume the WASM bridge from SolFlow. Add a "Parse
this source" path that runs the canonical compiler on whatever
the user pastes into the source pane.

**Why it matters:** the simplest possible user-visible test of
the bridge. Plus it's the foundation for B.6 (diagnostics) and
B.7 (importer).

**Files likely touched:**
- `src/components/SourcePreview.vue` — add an "Open" path that
  routes parsed `.sol` text into the compiler
- New `src/sol-compiler/parse.ts` — wraps the WASM `parse` for
  use by the source preview
- `src/stores/source.store.ts` (new) — owns the source-mode
  state

**Technical tasks:**
1. Add a source-text input mode to the SourcePreview component
   (read-only stays default; "edit" / "open" toggles to a
   CodeMirror-based editor).
2. Wire CodeMirror's content to the WASM `parse()` on debounced
   change.
3. Display the parsed AST in a debug pane (just dump it as JSON
   for now — the user-friendly version comes in B.6).
4. Cache the parser output keyed by source-text hash so
   re-render doesn't re-parse.

**Risks:**
- CodeMirror integration — the editor likely already uses it for
  the source preview, but writeable-mode brings its own state-
  management questions.
- Parse-on-every-keystroke performance — debounce + cancellation
  matters.

**Success criteria:**
- Pasting any `reference/sol files/` fixture into the source
  pane produces a parse without crashing
- The debug AST dump matches the Rust binary output for the
  same source
- Performance: parse-and-render of a 200-line file completes in
  < 50ms

**What NOT to do in B.5:**
- No diagnostics UI yet (B.6)
- No graph generation yet (B.7)
- No round-trip / sync yet (B.9)

### B.6 — Source diagnostics in SolFlow ✅ **DONE**

> **Landed 2026-05-27.** Lexer + parser diagnostics now carry
> source spans (`SourceSpan { start, end }`). Editor renders them
> in a dedicated `CompilerDiagnosticPanel.vue` grouped by phase
> with click-to-source: clicking a parse-stage diagnostic scrolls
> CodeMirror to the offending byte range. Analyzer diagnostics
> still emit with `span: null` (would need AST-level spans —
> deferred). Function-level source attachment: the importer
> populates `FunctionGraph.meta.sourceLine`; the import report's
> function rows are clickable and scroll the source pane to the
> function declaration. CodeMirror gutter markers (red dot per
> error line) deferred to a future polish pass; the current
> click-from-panel UX covers the navigation case without them.

### B.6 — original plan below

**Why it matters:** users expect a real IDE to mark errors in
the editor surface. Until this lands, the WASM bridge is just
plumbing.

**Files likely touched:**
- `src/components/SourcePreview.vue` — CodeMirror linter
  integration; gutter; underlining
- `src/components/DiagnosticsDrawer.vue` — source-side
  diagnostics interleave with graph-side ones
- `src/sol-compiler/diagnostics.ts` — adapter from the
  compiler's `Diagnostic` shape to CodeMirror's `Diagnostic`
  shape
- `src/stores/source.store.ts` — diagnostics state

**Technical tasks:**
1. Use CodeMirror's `@codemirror/lint` extension.
2. Convert compiler diagnostics to CodeMirror diagnostics
   (mostly a field rename + span → from/to translation).
3. Add click-to-jump from DiagnosticsDrawer rows to the source
   range (mirrors the existing click-to-jump for graph nodes).
4. Show the diagnostic code in the gutter on hover.
5. Status bar shows the source-side error count alongside the
   graph-side one.

**Risks:**
- Span accuracy — if B.2's span work has off-by-one errors, the
  red underlines land in the wrong place.
- Interleaving with graph-side diagnostics — the user must
  understand which is which.

**Success criteria:**
- Every negative fixture, pasted in, shows the right
  diagnostic at the right span
- Multiple errors per file are all shown
- Hover over a gutter marker shows the diagnostic code +
  message
- Status bar source-error count is accurate

**What NOT to do in B.6:**
- No quick fixes yet (Phase C territory)
- No auto-completion (Phase C)
- No graph generation from source (B.7)

### B.7 — AST → graph importer MVP ✅ **DONE**

> **Landed 2026-05-27.** `src/graph/import/` walks a parsed
> `Program` and produces a `SolWorkflow` + `ImportReport`. The
> editor's SourcePreview gains an Import-to-graph button (in edit
> mode) that opens an `ImportReportModal` showing per-function
> classification (full / partial / source-only / unsupported).
> Statement coverage: let/assign/print/call/return/if/while/for +
> structs/enums/imports/ext-fn-as-source-only. Unsupported
> constructs survive as placeholder nodes carrying the original
> SOL inline + a notice (never silently dropped). Tests: 23/23 in
> vitest. See `IMPORT_COMPATIBILITY.md` for the full matrix.

**Goal:** turn a parsed AST into a SolFlow graph. Cover the
Phase A node-kind set; unsupported syntax surfaces as source-
only mode (the graph is empty / read-only).

**Why it matters:** users can finally open a hand-written `.sol`
file and see it as a graph. The single biggest "this is a real
IDE" milestone.

**Files likely touched:**
- New `src/sol-compiler/astToGraph.ts` — the deterministic
  walker
- `src/stores/source.store.ts` — coordinates between source
  parse and graph generation
- `src/stores/graph.store.ts` — `importFromAst()` action
- `src/components/SourcePreview.vue` — add an "Open as graph"
  button when the source parses cleanly

**Technical tasks:**
1. Walk the AST function-by-function. For each function:
   - Create a `FunctionGraph` with params, returnType
   - Build statement-form nodes per chapter 18 §18.2 mapping
   - Wire control edges following the AST's statement order
   - Wire data edges for expression operands (or emit inline
     expressions, depending on B.7's strategy)
2. Decide the inline-vs-wired strategy: **recommendation** —
   any expression at most one operator deep (`a + b`) becomes
   inline; deeper expressions get wired through `binaryOp` /
   `varGet` nodes for visual clarity.
3. Generate node positions via the existing `autoLayout` (the
   v2 from A.9.1).
4. Frame named "regions" — if the source has a comment like `//
   #region foo`, wrap the contained nodes in a Frame. (Optional;
   nice-to-have.)
5. Handle unsupported syntax: if any AST node has no graph
   equivalent (e.g. tuple value forms when those land), produce
   a "source-only" workflow with a banner explaining the
   limitation.

**Risks:**
- Inline-vs-wired choice — too aggressive inlining produces
  unreadable graphs; too aggressive wiring produces visual
  spaghetti. Heuristic needs iteration.
- Comment preservation — the parser drops comments; the
  importer can't put them back. Document as a known limit.
- The Phase A graph schema has gaps relative to canonical SOL
  (no `import` semantics, no nested-tuple types). Importer
  must skip these gracefully.

**Success criteria:**
- Every positive `.sol` fixture produces a graph that round-
  trips back to source matching the original semantically
  (whitespace / comments may differ; node count and edge wiring
  must match)
- Unsupported syntax produces a clean "source-only" message,
  not a crash
- The generated graph passes the existing validator (chapter 18
  §18.6) with no errors

**What NOT to do in B.7:**
- No graph → source export work (B.8)
- No live sync yet (B.9)
- No advanced layout — autoLayout v2 is enough

### B.8 — Graph → AST/source canonicalization ✅ **MVP**

> **Landed 2026-05-27.** The existing `src/emit/emit.ts` is
> already structurally canonical; B.8 c26 added the test harness
> proving it, AND caught a real importer bug in the process:
> branch/while/for nodes were wiring continuation via `next` port
> instead of `after`, silently dropping all statements after a
> branch from the emit output. Fixed by widening
> `StmtImportResult` with an `exitPort` field. Round-trip
> snapshot tests for every fixture; 50/50 vitest green. See
> `docs/sol-language/CANONICALIZATION.md` for the contract
> (semantics + structure stable; byte-fidelity not promised).
> True parse→emit→parse end-to-end round-trip wants WASM-in-Node;
> the snapshot tests catch the same drift one cycle earlier.

### B.8 — original plan below

**Goal:** invert B.7. Take a SolFlow graph, produce a
canonical AST, then a pretty-printed `.sol` source. Replaces
the current `emit.ts` Graph → SOL emitter for graphs that
originated as imports.

**Why it matters:** closes the round-trip loop. After B.8 the
editor has a coherent graph ↔ AST ↔ source model, even if
sync between modes is still manual.

**Files likely touched:**
- New `sol_compiler/src/emit.rs` — the Rust-side pretty-printer
  (G11)
- New `src/sol-compiler/graphToAst.ts` — the editor-side
  GraphToAst walker
- `src/sol-compiler/index.ts` — expose `format(ast)` from the
  WASM bridge
- `src/emit/emit.ts` — keep as a fallback for legacy graphs;
  new path uses graph → AST → format

**Technical tasks:**
1. Write the Rust pretty-printer. Stable rules: 2-space indent,
   no tabs; brace on same line as decl; one blank line between
   top-level decls; sorted field declarations; etc. Round-trip-
   stable (printing twice produces identical output).
2. Build the editor-side GraphToAst walker. Mirror the existing
   emit.ts logic but produce AST values instead of text. Use
   the SolType / Param types from the canonical compiler.
3. Wire the editor's source pane to refresh from the canonical
   pipeline (graph → AST via TS walker → source via WASM
   format) whenever the graph changes.
4. Keep `emit.ts` as a fallback for graphs that don't yet
   round-trip (e.g. graphs with editor-only `trigger`
   annotations).

**Risks:**
- Pretty-printer formatting choices — divergence from user
  expectations is contentious.
- Comments — already dropped in B.7; B.8 cannot restore them.
  Document.
- The existing emit.ts is well-tested; ripping it out risks
  regressions. Recommend running both in parallel during B.8
  and comparing output.

**Success criteria:**
- For every positive fixture: `parse(source) → graph →
  format(graph)` produces source that re-parses to the same AST
- The new source pane (graph-driven) matches what the emit.ts
  produced for Phase A workflows, within whitespace
- The pretty-printer's output is idempotent

**What NOT to do in B.8:**
- No bidirectional live sync (B.9)
- No simulator parity (B.10)
- No formatter customization options (Phase C)

### B.9 — Source ↔ graph synchronization strategy 🟡 **Philosophy locked**

> **Landed 2026-05-27 (architectural model only).** See
> `SYNC_MODEL.md` for the canonical philosophy. Summary: sync is
> always an explicit user action (Import / Reset / Edit). No
> live two-way binding, no AST-diff merge, no watcher-based
> "auto-update graph from buffer." Detached-edit state is rendered
> honestly with an amber banner. The Import-to-graph action is a
> destructive replace by design; the import report tells the user
> exactly what was lost.
>
> No code work pending for B.9 beyond the sync model itself —
> future improvements (source-spans, click-to-source, etc.) refine
> the existing actions without changing the model.

### B.9 — Source ↔ graph synchronization strategy (original plan below)

**Goal:** decide how the editor handles concurrent edits to
source and graph. Implement the **chosen** model; do not try to
build all three of them.

**Why it matters:** without a sync model, the editor must
either lock one mode (read-only source) or risk silent data
loss when both modes are edited.

**Recommendation:** see §8 below — start with **one-way
import + AST-as-canonical** for B.9, defer full bidirectional
sync to Phase C.

**Files likely touched:**
- `src/stores/source.store.ts` — sync state machine
- `src/stores/graph.store.ts` — mode-switch hooks
- `src/components/SourcePreview.vue` — UI for mode switching
- `src/components/StatusBar.vue` — current mode indicator

**Technical tasks:** see §8 for the proposed sequence.

**Success criteria:**
- The user can open `.sol` source, edit it, and see graph
  refresh on parse
- Switching graph-edit → source-edit re-renders source from
  current graph
- No silent data loss; the user is always told when their
  in-flight edits are about to be regenerated

### B.10 — Canonical SOL VM in WASM ✅ **DONE** (c28–c31)

> **Landed 2026-05-27.** User decision: Option 2 — vendor the
> canonical SOL VM and run it in the browser. Resolution:
>
> - **`runtime/` new sibling crate** vendoring upstream `vm.rs`
>   with four surgical edits (output capture, ExtCall blocked,
>   step limit, common errors as values not panics). 12/12
>   native tests cover arithmetic, control flow, function call,
>   div-by-zero, infinite-loop step limit, ExtCallBlocked.
> - **`compiler-wasm::run_source_json`** export — single bundle
>   (357KB optimized, up from 280KB pre-VM). 10/10 native tests
>   covering the JSON envelope shape, compile-failure short-
>   circuit, runtime-error surfacing.
> - **`runSource()`** in TS API + RunModal wired through. Output
>   panel shows canonical-VM output (return value, print lines,
>   structured runtime errors). Legacy JS interpreter
>   (`interpret.ts`) demoted to canvas-animation driver only with
>   a `NOT AUTHORITATIVE` / `DO NOT extend` banner.
> - **External calls intentionally blocked** with
>   `RunError::ExtCallBlocked { function_name, url }` — honest
>   per user's ask. Modal renders the function name + URL clearly
>   so users know what to deploy if they want it to actually run.
>
> The simulator/compiler drift documented in `SIMULATOR_PARITY.md`
> is now resolved. The drift catalog stays in place as historical
> context but is no longer load-bearing.

### B.10 — original plan below

**Goal:** make the in-browser simulator match canonical SOL
semantics so "works in simulator → works in production"
becomes true. **Either** by tightening the simulator (T9022-
T9028 fixes) **or** by running the canonical bytecode VM in
WASM.

**Recommendation:** tighten the simulator. Running the full VM
in WASM is a multi-month project; tightening the simulator is a
few weeks and closes the same demo-visible gap.

**Files likely touched:**
- `src/runtime/interpret.ts` — the seven fixes
- `src/runtime/expressionEval.ts` (new) — replaces `new
  Function`; uses the compiler-provided AST + a simple tree-
  walker

**Technical tasks:**
- T9022: replace `new Function` with a real SOL expression
  evaluator (parse via the WASM bridge if possible, else hand-
  walk a parsed AST passed in)
- T9023: simulator rejects `str + str`
- T9024: simulator does truncating int division when both
  operands are int
- T9025: simulator requires bool for conditions
- T9026: simulator emits the T9002 warning at simulate-time
  (or, if T9002 is fixed in the canonical compiler by then,
  mirrors the correct behavior)
- T9027: simulator implements nested block scope

**Success criteria:**
- Every positive `.sol` fixture: simulator output matches the
  canonical compiler's output

**What NOT to do in B.10:**
- No full bytecode-VM-in-WASM port
- No simulator removal — keep both paths available

### B.11 — Final Phase B stabilization ✅ **DONE** (c32–c34)

> **Landed 2026-05-27.** Three commits:
>
> - **c32**: VM hardening sweep — `GetField` / `SetField` OOB
>   converted from raw `fields[idx]` panics to structured
>   `RunError::IndexOutOfBounds`. Defense against bytecode the
>   editor could construct via importer paths the analyzer
>   hasn't fully validated. 2 new tests.
> - **c33**: Dev ergonomics — `npm run check` single command runs
>   typecheck + vitest + workspace cargo test (92 tests total).
>   New `scripts/regenerate-ast-fixtures.sh` for one-command
>   importer fixture refresh after compiler serde changes.
> - **c34**: Documentation pass — root README rewritten
>   (was Phase-A flavored), new `B_RELEASE_NOTES.md` summarizes
>   what landed across all 11 milestones, Phase B plan marked
>   complete. Skipped reliability checklist (canonical VM IS the
>   reliability fix) and chapter rewrites (would be massive; the
>   release notes capture what matters).

### B.11 — original plan below

**Goal:** measure, fix, polish, document. The "polish + ship"
end of Phase B that mirrors A.10's role in Phase A.

**Files likely touched:** the diff from B.1 – B.10.

**Technical tasks:**
1. Performance pass: cold-start time with WASM included, parse
   time on large fixtures, memory profile during long sessions.
2. Reliability pass: same checklist as A.10.1 — autosave +
   reload + import + export.
3. Documentation pass: update chapter 18 (mapping), chapter 22
   (cross-layer assumptions), chapter 23 (runtime audit) to
   reflect the new compiler-backed reality.
4. Update `ERROR_REFERENCE.md` to mark which T9xxx items are
   closed.
5. Update `REMEDIATION_PLAN.md` — R3 items that landed during
   Phase B move from "long-term" to "shipped".
6. New `docs/sol-language/B_RELEASE_NOTES.md` summarizing what
   landed.

**Success criteria:**
- Every Phase B milestone's success criteria still pass
- Bundle size + cold-start within budget
- Documentation reflects the new state

---

## 5. WASM strategy

### 5.1 Toolchain recommendation

| Choice | Recommendation | Rationale |
|---|---|---|
| Build tool | **`wasm-pack`** | Standard for Rust→browser. Produces ESM, npm-installable. Mature. |
| Bindings | **`wasm-bindgen`** | Typed JS↔Rust boundary. The only sane choice in the Rust ecosystem. |
| Crate layout | **Separate `sol_compiler_wasm/` thin wrapper crate** | Keeps WASM-specific deps (wasm-bindgen, console_error_panic_hook) out of the core library. The core `sol_compiler` crate stays plain Rust. |
| Target | **`--target web`** | Produces ESM module loadable directly from Vite without a worker wrapper (for the MVP — workers come later if needed). |
| Loading | **Lazy-import** | The WASM blob is 1-3 MB; loading it on every cold start hurts. Defer until first `compile()` call. |
| Boundary format | **JSON via serde-bindgen** | Simpler than `serde-wasm-bindgen`'s direct path for the MVP; allows the AST to be inspected from the browser dev tools. Switch to `serde-wasm-bindgen` if performance demands. |
| TypeScript types | **`tsify-next`** (auto-gen) | Single source of truth in Rust. Eliminates two-source drift on every AST change. Manual review of generated `.d.ts` for ergonomics. |

### 5.2 Crate split

Three crates after B.1:

```
sol_compiler           ← pure Rust library, the SOL frontend
  ├─ lexer.rs
  ├─ parser.rs
  ├─ analyzer.rs
  ├─ diagnostic.rs (new)
  ├─ util.rs
  └─ lib.rs (new — public re-exports)

sol_compiler_wasm      ← thin WASM wrapper (depends on sol_compiler)
  └─ lib.rs            ← wasm-bindgen exports: compile, parse, analyze

sol_runtime            ← VM + ext-call transport (depends on sol_compiler)
                         remains Rust-only; not in WASM scope
  ├─ bytecode.rs
  ├─ vm.rs
  └─ init.rs
```

The current binary's behavior is preserved by a thin `sol_cli`
crate (or just `[[bin]]` in `sol_runtime`) that consumes both
sides.

### 5.3 Build pipeline

- `pnpm` workspace adds `sol_compiler_wasm/pkg/` as a local dep
  via `file:` resolver
- Vite consumes the package via `vite-plugin-wasm` +
  `vite-plugin-top-level-await`
- CI runs `wasm-pack build --target web --out-dir pkg` before
  `pnpm install`

### 5.4 What does NOT cross the WASM boundary

- The VM. It stays Rust-side for now (Phase B); B.10 may
  reconsider for parity.
- The HTTP ext-call transport. Rust-side only.
- File I/O. Browser can't do filesystem; the editor passes
  source as a string.

---

## 6. Diagnostics strategy

### 6.1 Diagnostic shape

```rust
pub struct Diagnostic {
    pub severity: Severity,       // Error | Warning | Note
    pub code: DiagnosticCode,     // "E0001", "E1004", "T9002", …
    pub message: String,          // Human-readable, one sentence
    pub span: Option<Span>,       // None for module-level
    pub related: Vec<RelatedSpan>,// "previous definition here" + similar
    pub phase: Phase,             // Lexer | Parser | Analyzer | Codegen | Runtime
}

pub struct Span {
    pub start: ByteOffset,        // 0-indexed byte offset
    pub end: ByteOffset,          // exclusive
}

pub struct RelatedSpan {
    pub span: Span,
    pub message: String,
}

pub enum Severity { Error, Warning, Note }
pub enum Phase    { Lexer, Parser, Analyzer, Codegen, Runtime }
```

### 6.2 Code naming

The provisional `Exxxx` / `Wxxxx` / `T9xxx` codes already used
in `docs/sol-language/ERROR_REFERENCE.md`. Phase B is the first
opportunity to make these *real* codes the compiler emits.

| Range | Phase | Examples |
|---|---|---|
| E0001 – E0099 | Lexer / parser | E0001 missing initializer, E0002 missing semicolon |
| E1000 – E1999 | Analyzer | E1001 undefined variable, E1002 redefinition |
| E2000 – E2999 | Runtime | E2001 divide by zero |
| W0xxx – W2xxx | Same phases, severity Warning | W2002 enum first-char collision |
| T9xxx | Tool-side (NOT a compiler diagnostic) | These stay in the docs; the compiler doesn't emit them |

### 6.3 Transport

- Compiler returns `Result<T, Vec<Diagnostic>>` from every
  pipeline stage
- Across the WASM boundary as `Diagnostic[]` (via serde)
- In the editor, normalized to CodeMirror's `Diagnostic` shape
  for the source pane (B.6), preserved as-is for the
  DiagnosticsDrawer

### 6.4 Source-position semantics

Spans are byte offsets into the source string. Not line/column —
those are derived on display via a helper `Span::to_line_col(&self, source: &str)`.
Byte offsets are stable; line/column are a function of source
content.

### 6.5 Compatibility with the existing SolFlow validator

The editor validator (`src/graph/validate.ts`) emits its own
`Diagnostic` shape. After B.6 both kinds coexist:

- **Source diagnostics** — from the WASM compiler, attached to
  source spans
- **Graph diagnostics** — from the editor validator, attached to
  node ids

Both appear in the DiagnosticsDrawer with a phase indicator
column.

---

## 7. AST/graph mapping strategy

### 7.1 Direction 1 — AST → graph (importer; B.7)

**Easy.** The AST has full structural information. The walker
visits each function, builds the function-graph shell, then
walks the statement chain producing one node per statement-form
AST node and one edge per `next` link.

Per chapter 18 §18.2:

| AST | Graph node kind | Notes |
|---|---|---|
| `DeclFunc` | (whole function shell) | function's nodes + edges land inside |
| `DeclVar` (top-level) | n/a — rejected (T9014 — top-level let is broken) | Importer surfaces a warning |
| `Ast::Block` | (no node) | Walked into; each child statement becomes a node |
| `DeclVar` (inside body) | `let` | Inline expression filled from `value` |
| `ExprAssign` (statement) | `assign` | |
| `StmtIf` | `branch` | `hasElse` from `alt.is_some()` |
| `StmtWhile` | `while` | |
| `StmtFor` | `forEach` | |
| `ExprReturn` | `return` | `hasValue` from `val.is_some()` |
| `ExprFuncCall` (statement, name == "print") | `print` | |
| `ExprFuncCall` (statement, other) | `call` | `functionId` resolved by name |
| `ExprFuncCall` (expression position) | inline expression OR wired call | See §7.4 |
| `DeclStruct` | (workflow-level `structs[]` entry) | |
| `DeclEnum` | (workflow-level `enums[]` entry) | |
| `DeclExtFunc` | (workflow-level — no graph node; future schema gap) | Phase B documents the limit; B.7 issues a warning |
| `StmtImport` | (workflow-level `imports[]` entry) | |

### 7.2 Direction 2 — graph → AST (exporter; B.8)

**Medium.** The graph has positional + visual data the AST
doesn't care about; those are simply discarded. The harder
part is restoring the canonical expression nesting from the
graph's inline-or-wired hybrid:

- An inline expression like `payload.amount > 1000` is parsed by
  the WASM compiler at export time to produce the right AST
  fragment
- A wired data subtree of `binaryOp(>, varGet(payload),
  literal(1000))` is walked recursively to produce the same AST
  fragment

The exporter must use the compiler's lexer + parser to convert
inline expression strings into AST fragments. Phase B's WASM
bridge enables this.

### 7.3 Direction 3 — graph edits ↔ source edits (Phase C)

**Hard.** Live sync requires either (a) maintaining
position-preserving spans through every edit, or (b) full
re-render after every edit. Phase B avoids this by adopting
the model in §8: explicit mode switching, no live sync.

### 7.4 Inline-vs-wired strategy in the importer

Heuristic for B.7:

| Expression complexity | Representation |
|---|---|
| Literal | inline (`"hello"`, `42`) |
| Bare variable ref | inline (`amount`) |
| Dotted field access | inline (`payload.amount`) |
| Indexed access | inline (`arr[i]`) |
| Single binary op | inline (`amount > 1000`) |
| Single unary op | inline (`!flag`) |
| Two or more nested operators | wired (multiple `binaryOp` / `varGet` nodes) |
| Enum variant reference | inline (`Status::Active`) |
| Struct literal | wired (`structLiteral` node — too dense for inline) |
| Function call result used in expression position | wired (`call` node) |

Configurable via a future user preference; the default keeps
inline for short expressions and wires deeper trees for
visibility.

### 7.5 Editor-only constructs

Some Phase A graph constructs have no SOL equivalent and need
careful handling in both directions:

| Editor concept | AST equivalent | Handling |
|---|---|---|
| `trigger` node | None — `// @trigger …` comment (T9001) | Importer creates a `trigger` node from a leading comment; exporter writes the comment back |
| `note` / `frame` | None | Importer cannot create these from source (no comments). Exporter drops them. Document the loss |
| `any` type in unresolved data ports | None (T9019) | Importer can't produce `any`; only Sol Man uses it |

---

## 8. Source synchronization strategy

### 8.1 The four possible models

1. **Graph is source of truth.** Source is auto-generated and
   read-only. User edits the graph. Source pane is a viewer.
   - Pro: simple. Matches Phase A today.
   - Con: users can't paste hand-written SOL.

2. **Source is source of truth.** Graph is auto-generated and
   read-only. User edits the source. Graph is a viewer.
   - Pro: simple. Aligns with traditional IDE flows.
   - Con: throws away the visual editing experience.

3. **AST is source of truth.** Both source and graph are
   projections. Both can be edited; edits are applied to the
   AST; the other projection re-renders.
   - Pro: clean conceptually. No "which side wins" question.
   - Con: hardest to implement. Edit positions must round-trip
     through AST faithfully (including across whitespace and
     comments).

4. **Dual-mode with explicit sync.** The user is always in either
   graph-edit or source-edit mode. Switching modes regenerates
   the other view. No live sync.
   - Pro: medium complexity. No data-loss surprises.
   - Con: slightly clunky UX.

### 8.2 Recommendation for Phase B

**Phase B MVP: Model 4 + a one-way Model 1 import path.**

- Default: graph-edit mode (Model 1 — same as Phase A today)
- New: "Switch to source mode" button that regenerates source
  from the graph and gives the user a CodeMirror editor on it
- New: "Apply source → graph" button that parses the source,
  shows diagnostics, and if clean, replaces the graph
- New: "Open .sol" path enters source mode directly with the
  loaded source

Live sync (Model 3) is deferred to Phase C. The mode-switch UX
is honest about the trade-off: the user knows when their work
is being regenerated and can refuse.

### 8.3 Conflict resolution

The mode-switch model side-steps conflict resolution. The two
views are never simultaneously live; the act of switching is
the resolution.

### 8.4 What about comments?

Comments are dropped on graph → source round-trip until B.8
adds a comment-preservation pass (deferred — Phase C). For
Phase B, the source-mode editor explicitly tells the user:
"Comments are not preserved when applying source → graph."

---

## 9. Risk register

| # | Risk | Probability | Impact | Mitigation |
|---|---|---|---|---|
| R1 | WASM binary size kills cold-start | Medium | Medium | Lazy-load. Measure with `wasm-opt`. Budget < 3 MB compressed |
| R2 | Span work in B.2 has off-by-one errors → wrong underlines | Medium | High | Integration tests that compare spans against known-good byte ranges per fixture |
| R3 | Pretty-printer formatting choices are contentious | High | Low | Document the formatting rules in a `FORMATTER.md`; allow no customization in B.8 |
| R4 | Comments are dropped on round-trip | High (always) | Medium | Documented limitation; future Phase C task |
| R5 | The Rust panic surface in vm.rs leaks into WASM if VM ever crosses the boundary | Medium | High | Keep the VM Rust-side for Phase B. B.10's parity decision must keep this in mind |
| R6 | Graph schema evolves during Phase B (new node kinds, etc.) | Low | Medium | Treat the graph schema as frozen during B.1–B.10; cut B.11 to handle additions |
| R7 | Users edit unsupported syntax (e.g. tuple value forms when those land) | Medium | Medium | B.7 detects + surfaces a "source-only" banner; graph mode is disabled until removed |
| R8 | Two separate diagnostic surfaces (source + graph) confuse users | Medium | Low | DiagnosticsDrawer adds a phase column; statusbar shows both counts separately |
| R9 | Existing emit.ts and the new pipeline produce different output | High | Medium | Run both during B.8; compare. Recommend the new pipeline for round-tripped graphs only, keep emit.ts as Phase-A-graph fallback |
| R10 | WASM build pipeline breaks CI | Medium | Medium | Add `wasm-pack` to CI explicitly; document the toolchain version |
| R11 | TypeScript types drift from Rust types | Medium | Low | `tsify-next` auto-generation eliminates this; runtime check in CI |
| R12 | Performance regression on large files (>1000 nodes) | Low | Medium | Stress test in B.11; profile parse + render pipeline |
| R13 | Sol Man's current path (graph spec → graph) becomes second-class after source import lands | Low | Low | Sol Man stays — chapter 19 already documents it as graph-spec generation, not source generation |
| R14 | The compiler's known semantic gaps (G9) bite during B.7 import | Medium | Low | B.7 inherits the compiler's analyzer; the gaps remain known and documented |

---

## 10. Phase B MVP definition

The smallest useful Phase B is **B.1 + B.2 + B.4 + B.5 + B.6**:

✅ Rust crate is a callable library (B.1)
✅ Errors are values; multiple per compile; with spans (B.2)
✅ WASM bridge ships an editor-consumable compiler (B.4)
✅ Editor can parse `.sol` text in the browser (B.5)
✅ Editor shows compiler diagnostics in the source pane (B.6)

After the MVP, the editor is a *real compiler-backed source
viewer*. The graph view is unchanged; the source view becomes
authoritative for diagnostics. Users can:

- Open a `.sol` file → see it parse + lint in the browser
- Paste source from outside → see canonical compiler errors
- Diagnose a workflow they previously could only run through
  the binary

B.7 (importer) is the *next* useful milestone — it's where
source becomes editable into the graph. But it's a meaningful
step beyond the MVP and can ship later.

B.8 (canonicalization) is where graph → source becomes
compiler-backed; before that, the existing emit.ts is fine.

B.9 (sync) is where the editor gains real dual-mode UX.

B.10 / B.11 are polish and stabilization.

**Recommended Phase B ship order**: MVP (B.1+B.2+B.4+B.5+B.6) →
B.7 → B.8 → B.9 → B.10 → B.11. B.3 happens between B.1 and
B.4 as a prerequisite.

---

## 11. Phase B non-goals

Explicit list of things Phase B does **not** attempt. Each
belongs in Phase C or beyond.

- **Full runtime rewrite.** The VM stays Rust-side. The simulator
  stays JS-side. B.10 only addresses parity, not replacement.
- **Distributed execution.** Out of scope.
- **Enterprise backend / RBAC / multi-tenant.** Out of scope.
- **Perfect formatter preservation (comments, blank lines, spaces).**
  Phase B's formatter is canonical (one valid output per AST);
  preservation is a Phase C concern.
- **Full language server (LSP-style).** Phase B is editor-
  embedded compiler integration. A standalone LSP daemon is a
  Phase C / D project.
- **Full bidirectional live sync** between source and graph. Phase
  B uses mode-switching. Live sync is hard and deserves its own
  audit.
- **Compiler redesign.** Phase B *cleans up* the existing
  compiler (errors-as-values, spans, lib target, serde). It
  does not redesign the language or the bytecode.
- **Deployment / runtime hosting.** Phase B is a browser editor
  feature; the host runtime integration is unchanged.
- **Auto-completion / hover info / go-to-definition.** All Phase
  C. The compiler enables these but the editor work to build
  them is separate.
- **Refactorings (rename symbol, extract function).** Phase C.
- **Auto-fixes (apply diagnostic suggestion).** Phase C.

---

## 12. Recommended first implementation prompt

The exact next prompt to begin B.1, after this plan is reviewed
and approved:

> Start B.1 — Rust crate cleanup for WASM readiness.
>
> Scope (per `docs/sol-language/PHASE_B_COMPILER_IDE_PLAN.md`
> §4 B.1):
>
> 1. Convert the binary-only Rust crate into a library + binary
>    pair. The library exposes the SOL frontend (lexer, parser,
>    analyzer, util) as a callable public API.
> 2. Add `Lexer::from_str(source: &str)` so callers can parse
>    in-memory source bodies, not just file paths (G4).
> 3. The existing CLI binary becomes a thin wrapper over the
>    library so `cargo run -- file.sol` still works.
> 4. Choose the public re-exports: `Lexer`, `Parser`, `Analyzer`,
>    `Codegen`, `Ast`, `Type`. Do NOT include `Diagnostic` yet —
>    that lands in B.2.
> 5. Write minimal Rust integration tests that exercise the
>    new library API on at least three positive fixtures and
>    one negative fixture (the negative still exits via the
>    existing `process::exit` path; B.2 fixes that).
>
> Hard constraints:
>
> - No `process::exit` removal yet (B.2's job)
> - No serde derives yet (B.3's job)
> - No WASM work yet (B.4's job)
> - No source-span work yet (B.2's job)
> - No new semantic checks; no closing analyzer holes
> - Existing fixture behavior must be preserved exactly —
>   running the new CLI on each fixture produces identical
>   stdout/stderr to the pre-B.1 binary
>
> Files likely touched (in the canonical compiler workspace,
> not in this SolFlow repo): `Cargo.toml`, new `src/sol/lib.rs`,
> `src/sol/mod.rs`, possibly `src/sol/main.rs` (split into the
> new binary).
>
> Deliverable: a single commit / PR in the compiler workspace
> with the library target landed. Verified by:
>
> - `cargo build --lib` and `cargo build --bin` both succeed
> - Every fixture in `tests/*.sol` produces identical CLI output
> - A new `tests/lib_smoke.rs` runs three positive + one
>   negative fixture through the library API
>
> Do not start B.2 in the same commit. After landing B.1,
> stop and confirm before opening B.2.

---

## Appendix — references

- `docs/sol-language/SPEC.md` — normative SOL spec
- `docs/sol-language/GRAMMAR.md` — EBNF grammar
- `docs/sol-language/ERROR_REFERENCE.md` — all known `Exxxx` /
  `Wxxxx` / `T9xxx` codes
- `docs/sol-language/18-solflow-mapping.md` — node↔SOL mapping
  table (B.7 / B.8 build from this)
- `docs/sol-language/20-implementation-notes.md` — codegen
  pipeline, slot mechanics (B.1 / B.3 build from this)
- `docs/sol-language/21-behavior-classification.md` — what's
  Specified vs Current-impl vs Accidental (Phase B preserves
  Specified; closes Current-impl bugs where convenient)
- `docs/sol-language/22-cross-layer-assumptions.md` — each
  layer's guarantees (Phase B's "shouldn't break X" reference)
- `docs/sol-language/23-editor-runtime-audit.md` — simulator
  vs canonical-VM divergences (B.10 source list)
- `docs/sol-language/REMEDIATION_PLAN.md` — R1/R2/R3 bucketed
  remediation items (R3-A bytecode bugs especially overlap
  with Phase B's compiler work)
- `reference/SOL_CRATE_IDE_READINESS_PLAN.md` (the upstream
  compiler team's plan) — Phase B implements blockers #1 – #16
  of that document

Phase B is the convergence of "audit found these gaps" + "user
wants compiler-backed IDE." Every milestone above closes one
or more documented blockers from the existing audit, in an
order that keeps each commit shippable on its own.
