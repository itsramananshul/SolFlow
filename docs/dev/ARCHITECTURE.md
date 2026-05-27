# SolFlow Architecture

High-level guide for contributors. For the user-facing version
see the [main README](../../README.md).

## The stack

```
┌────────────────────────────────────────────────────────────────┐
│                     Vue 3 editor (browser)                     │
│                                                                │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │   Vue Flow      │  │  CodeMirror 6   │  │  Pinia         │  │
│  │   canvas        │  │  source pane    │  │  stores        │  │
│  └────────┬────────┘  └────────┬────────┘  └────────────────┘  │
│           │                    │                               │
│           ▼                    ▼                               │
│  ┌───────────────────────────────────────────────────────┐     │
│  │            src/graph/   (TS)                          │     │
│  │   schema · factory · validate · emit · import         │     │
│  └─────────────────┬─────────────────────────────────────┘     │
│                    │                                           │
│  ┌─────────────────▼─────────────────────────────────────┐     │
│  │            src/compiler/   (TS)                       │     │
│  │   typed-AST + worker-channel wrappers + types         │     │
│  └─────────────────┬─────────────────────────────────────┘     │
│                    │ postMessage / direct call                 │
│  ┌─────────────────▼─────────────────────────────────────┐     │
│  │       compiler-wasm/pkg/   (WASM)                     │     │
│  │   wasm-bindgen bridge — stable JSON envelopes         │     │
│  └─────────────────┬─────────────────────────────────────┘     │
└────────────────────┼───────────────────────────────────────────┘
                     │
                     ▼
   ┌────────────────────────────────────────────────────────┐
   │  Rust workspace (compiled to WASM)                     │
   │                                                        │
   │  compiler/                                             │
   │   ├─ lexer.rs    → tokens + per-token spans            │
   │   ├─ parser.rs   → AST + per-node spans (10 variants)  │
   │   ├─ analyzer.rs → semantic checks + diagnostics       │
   │   ├─ bytecode.rs → codegen + instruction-span sidecar  │
   │   ├─ diagnostic.rs → SolDiagnostic + SourceSpan        │
   │   └─ api.rs      → public lex/parse/analyze/compile    │
   │                                                        │
   │  runtime/                                              │
   │   ├─ vm.rs       → canonical stack-machine interpreter │
   │   ├─ error.rs    → RunError variants                   │
   │   └─ lib.rs      → run_program + RunOutcome            │
   │                                                        │
   │  compiler-wasm/                                        │
   │   └─ lib.rs      → wasm-bindgen exports + JSON         │
   │                    envelope contracts                  │
   └────────────────────────────────────────────────────────┘
```

## Module responsibilities

### `src/components/` — Vue UI

- `Canvas.vue` — Vue Flow wrapper; nodes + edges + selection
- `SolNode.vue` — renders ALL node kinds (single component;
  switches by `data.kind`)
- `Inspector.vue` — per-kind property editor (right side panel)
- `SourcePreview.vue` — CodeMirror + edit mode + Import button
- `CompilerDiagnosticPanel.vue` — phase-grouped diagnostics list
- `RunModal.vue` — Output + Trace + SOL tabs
- `ImportReportModal.vue` — per-function import classification
- `SolManModal.vue` — LLM-assisted workflow generation
- `Sidebar.vue` — palette / types / imports tabs
- `DiagnosticsDrawer.vue` — graph-level validation drawer

### `src/graph/` — graph model (TS)

- `schema.ts` — `SolWorkflow`, `FunctionGraph`, `GraphNode`, etc. — authoritative TS shape
- `factory.ts` — `createNode()` builds nodes with correct ports
- `validate.ts` — graph-level validation (missing inputs,
  type-mismatches, branch termination, T9002 enum collisions)
- `expressionLint.ts` — sanity-lint inline expression strings
- `emit/emit.ts` — graph → SOL source (canonical, deterministic)
- `import/` — SOL AST → graph
  - `importer.ts` — main walker (handles every supported AST kind)
  - `expressions.ts` — AST expression → SOL source printer
  - `report.ts` — `ImportReport` types
  - `types.ts` — compiler `SolType` ↔ graph `SolType` mapping
- `nodeLookup.ts` — `findNodeForSpan(workflow, span)` for
  execution-trace → graph-node correlation

### `src/compiler/` — WASM bridge wrapper (TS)

- `worker.ts` — Web Worker entry; handles parse/analyze on every keystroke
- `api.ts` — public TS API (parseSource, analyzeSource,
  compileSource, runSource, compilerVersion, preloadCompiler)
- `ast.ts` — typed mirror of the Rust AST (tagged-union)
- `types.ts` — diagnostic + run envelope types

### `src/stores/` — Pinia state

- `graph.store.ts` — workflow state + undo + autosave + import-from-source
- `ui.store.ts` — panel state, focus requests
- `sol-man.store.ts` — LLM generation + provider config
- `simulation.store.ts` — canvas playback animation state
- `toast.store.ts` — toast notification queue

### `src/runtime/interpret.ts` — LEGACY

A JS graph-walking interpreter. Kept ONLY as a canvas-animation
driver — its output is NOT authoritative. The canonical SOL VM
in `runtime/` is the source of truth for execution. Banner at
top of file says `NOT AUTHORITATIVE — DO NOT EXTEND`. See
[`SIMULATOR_PARITY.md`](../sol-language/SIMULATOR_PARITY.md) for
the drift history (now historical; resolved by canonical-VM-in-WASM).

## Two WASM bundles

- `compiler-wasm/pkg/` — `--target bundler`; loaded by Vite
  via `vite-plugin-wasm`. Browser-side. Two instances per page:
  one on the main thread (compile/run), one in the worker
  (parse/analyze).
- `compiler-wasm/pkg-node/` — `--target nodejs`; loaded by
  vitest via `createRequire`. Used by the e2e round-trip suite
  for real `parse → import → emit → parse` cycles in pure Node.

Both committed; rebuild with `npm run build:wasm:all`.

## Sync model

Source pane and visual graph are NEVER live-bound. Four sanctioned
transfers, all user-triggered. Full philosophy:
[`docs/sol-language/SYNC_MODEL.md`](../sol-language/SYNC_MODEL.md).

## Adding a new node kind

1. Add the variant to `NodeData` in `src/graph/schema.ts`
2. Add port shape in `src/graph/factory.ts::rebuildPorts`
3. Add palette entry in `src/components/NodePalette.vue` (or
   wherever the palette items are defined)
4. Add Inspector property editor (if the kind has properties)
5. Add render case in `src/components/SolNode.vue`
6. Add emit case in `src/emit/emit.ts`
7. Add validate case in `src/graph/validate.ts`
8. Add importer case in `src/graph/import/importer.ts` (if the
   kind can be reconstructed from SOL source)
9. Update sample workflows that should use the new kind
10. Test: snapshot + e2e + manual

## Where to start reading

- For UI work: `src/components/Canvas.vue` and `src/components/SolNode.vue`
- For state: `src/stores/graph.store.ts`
- For schema: `src/graph/schema.ts`
- For compiler integration: `src/compiler/api.ts`
- For Rust: `compiler/src/lib.rs` → `parser.rs` → `analyzer.rs` → `bytecode.rs`
- For execution: `runtime/src/lib.rs` → `vm.rs`
