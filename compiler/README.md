# solflow_compiler

The standalone SOL language compiler vendored into SolFlow. Provides a
reusable library API (and a thin CLI) for lexing, parsing, analyzing,
and code-generating SOL programs.

This crate is the engine SolFlow's editor will call through WASM (Phase
B.4) to provide real compiler-backed diagnostics, source import,
canonical formatting, and source ↔ graph synchronization.

## What's inside

```
compiler/
├── Cargo.toml              package: solflow_compiler
├── src/
│   ├── lib.rs              public API surface
│   ├── lexer.rs            token + lexer
│   ├── parser.rs           grammar + AST
│   ├── analyzer.rs         scope + type checks
│   ├── bytecode.rs         codegen
│   ├── util.rs             type_eq helper
│   ├── diagnostic.rs       structured diagnostics (Phase B.2)
│   ├── api.rs              result-returning compile pipeline (Phase B.2)
│   └── bin/sol.rs          minimal CLI consuming the library
└── tests/
    ├── fixtures/           every .sol fixture the test corpus needs
    ├── api_smoke.rs        positive-fixture coverage
    └── diagnostics.rs      negative-fixture diagnostic-code coverage
```

## What's deliberately NOT here

- **No VM / runtime.** The compiler emits bytecode; running it is a
  future crate's job (B.10 territory). The editor's in-browser
  simulator (`src/runtime/interpret.ts`) covers Phase A execution
  needs.
- **No network / host-runtime / controller code.** This is a compiler,
  not a workflow engine.
- **No HTTP / external-call infrastructure.** `Inst::ExtCall` is
  emitted by the codegen as a bytecode value, but nothing in this
  crate executes it.

## Build / test

```
cargo build
cargo test
cargo run --bin sol -- tests/fixtures/retest.sol
```

## Phase B status

| Milestone | Status |
|---|---|
| B.1 — library API skeleton | ✅ |
| B.2 — diagnostics as values | ✅ (lexer + parser + analyzer + codegen) |
| B.3 — AST serde derives | pending |
| B.4 — WASM bridge | pending |

See [REMAINING_PANICS.md](REMAINING_PANICS.md) for the catalog of
intentional `unwrap` / `unreachable` / `todo!` sites that remain
(none of them on a user-reachable error path).

See `docs/sol-language/PHASE_B_COMPILER_IDE_PLAN.md` at the SolFlow
repo root for the full plan.

## License

Inherits SolFlow's repository license.
