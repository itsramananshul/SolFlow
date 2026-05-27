# SolFlow

> Visual IDE for the **SOL language**, backed by the canonical Rust
> compiler + VM compiled to WebAssembly.

SolFlow is a Vue 3 + Vue Flow editor where you can build workflows
visually OR edit them as SOL source — both views are coherent
because they share the same compiler. Diagnostics, parsing, type
checking, code generation, and execution all run through the
canonical SOL Rust crates compiled to WASM. No JavaScript
reimplementation of language semantics owns user-displayed output.

```
┌───────────────────────────────┐    ┌──────────────────────────────┐
│   Visual graph (Vue Flow)     │ ⇄  │   SOL source (CodeMirror)    │
└──────────────┬────────────────┘    └──────────────┬───────────────┘
               │ emit (TS)                          │ runSource (WASM)
               ▼                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│       solflow_compiler (Rust) — lexer · parser · analyzer       │
│       solflow_compiler::bytecode — codegen → Inst[]             │
│       solflow_runtime (Rust)    — canonical SOL bytecode VM     │
│       Compiled to WASM via solflow_compiler_wasm bridge         │
└─────────────────────────────────────────────────────────────────┘
```

## What you can do

- **Build workflows visually** — drag nodes, wire control + data
  edges, configure parameters, watch the SOL source preview
  update live.
- **Edit source with real compiler diagnostics** — open the source
  pane, click Edit, and see lexer/parser/analyzer/codegen errors
  with click-to-source navigation. The compiler is the canonical
  Rust one, running in-browser via WASM.
- **Import SOL → graph** — paste or load `.sol` source and click
  "Import to graph". An honest import report classifies every
  function as Full / Partial / Source-only / Unsupported; nothing
  is silently dropped.
- **Run workflows in-browser** — the Run modal compiles + executes
  through the canonical SOL VM. Print output, return values, and
  structured runtime errors all come from real bytecode execution.
- **External calls are honestly blocked** in browser simulation
  with a structured `ExtCallBlocked` diagnostic — no fake-success.

## Stack

| Layer | Tech |
|---|---|
| Editor | Vue 3 (Composition API + TS strict), Vue Flow, Pinia, CodeMirror 6 |
| Build | Vite 5 + `vite-plugin-wasm` + `vite-plugin-top-level-await` |
| Compiler | `compiler/` — Rust crate (lexer, parser, analyzer, codegen) |
| VM | `runtime/` — Rust crate (canonical bytecode interpreter) |
| Bridge | `compiler-wasm/` — wasm-bindgen WASM bundle |
| Tests | vitest (TS, 50 tests) + cargo workspace (Rust, 42 tests) |

## Run it

One-time:

```bash
npm install
```

Dev server:

```bash
npm run dev          # http://localhost:5173
```

Production build:

```bash
npm run build        # vue-tsc + vite build
npm run preview
```

Verify everything (typecheck + TS tests + Rust workspace tests):

```bash
npm run check
```

Regenerate the importer's pre-baked AST fixtures (after compiler
serde changes):

```bash
npm run regen:fixtures
```

Rebuild the WASM bundle (after Rust changes — requires
`cargo install wasm-pack` + `rustup target add wasm32-unknown-unknown`):

```bash
npm run build:wasm
```

## Repository layout

```
SolFlow/
├── src/                          # Vue editor
│   ├── components/               # SolNode, SourcePreview, RunModal, ImportReportModal, CompilerDiagnosticPanel, ...
│   ├── compiler/                 # TS wrapper around the WASM bridge (api.ts, ast.ts, types.ts)
│   ├── graph/                    # graph schema, factory, validator, emitter, importer
│   ├── runtime/interpret.ts      # LEGACY — canvas-animation driver only; not authoritative
│   ├── stores/                   # Pinia: graph, ui, simulation, toast, sol-man
│   ├── samples/                  # sample workflows (data, not hardcoded)
│   └── styles/
├── compiler/                     # Rust — standalone SOL compiler crate
├── compiler-wasm/                # Rust — wasm-bindgen bridge; pkg/ committed
├── runtime/                      # Rust — canonical SOL VM, browser-safe
├── docs/sol-language/            # Language docs (read 00 first)
├── scripts/                      # Dev tooling (regenerate-ast-fixtures.sh, ...)
└── vite.config.ts
```

## Documentation

| File | Read this when... |
|---|---|
| [`docs/sol-language/`](docs/sol-language/README.md) | learning SOL or its compiler |
| [`docs/sol-language/PHASE_B_COMPILER_IDE_PLAN.md`](docs/sol-language/PHASE_B_COMPILER_IDE_PLAN.md) | catching up on what shipped in Phase B |
| [`docs/sol-language/B_RELEASE_NOTES.md`](docs/sol-language/B_RELEASE_NOTES.md) | reading the Phase B summary |
| [`docs/sol-language/IMPORT_COMPATIBILITY.md`](docs/sol-language/IMPORT_COMPATIBILITY.md) | understanding what AST→graph import does + doesn't preserve |
| [`docs/sol-language/CANONICALIZATION.md`](docs/sol-language/CANONICALIZATION.md) | understanding the canonical export rules |
| [`docs/sol-language/SYNC_MODEL.md`](docs/sol-language/SYNC_MODEL.md) | understanding why source↔graph sync is explicit-action |
| [`docs/sol-language/SIMULATOR_PARITY.md`](docs/sol-language/SIMULATOR_PARITY.md) | historical record of JS-sim drift (resolved by canonical-VM-in-WASM) |
| [`compiler/REMAINING_PANICS.md`](compiler/REMAINING_PANICS.md) | catalogued intentional panic/exit sites |
| [`compiler/UPSTREAM.md`](compiler/UPSTREAM.md) + [`runtime/UPSTREAM.md`](runtime/UPSTREAM.md) | crate provenance + surgical-edit catalogs |

## Phase status

- **Phase A** — visual editor with TS-only graph + temporary emitter. ✅ Shipped, foundation for Phase B.
- **Phase B** — canonical Rust compiler + VM compiled to WASM, AST→graph importer, source spans, rich diagnostics, round-trip stability, canonical-VM execution. ✅ Shipped (B.1–B.11).
- **Phase C** — deployment, controller integration, multi-user, real external calls. Not started.

## License

TBD.
