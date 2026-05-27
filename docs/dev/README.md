# SolFlow — Developer Docs

Documentation for people contributing to SolFlow's editor,
compiler, or runtime.

## Start here

- **[Architecture overview](./ARCHITECTURE.md)** — Phase B
  architecture: how the editor + compiler + VM fit together
- **[Phase C Architecture](./PHASE_C_ARCHITECTURE.md)** —
  in-progress orchestration / runtime platform design (the
  canonical Phase C contract)
- **[Phase C Roadmap](./PHASE_C_ROADMAP.md)** — C.1 → C.8
  milestone delivery plan
- **[Local Controller](./CONTROLLER_LOCAL.md)** — Phase C C.2
  how-to-run + env vars + API reference + troubleshooting
- **[`CONTRIBUTING.md`](../../CONTRIBUTING.md)** — code style + test discipline

## Deep dives

### The Rust side

- **[`compiler/README.md`](../../compiler/README.md)** — the standalone SOL compiler crate
- **[`compiler/UPSTREAM.md`](../../compiler/UPSTREAM.md)** — provenance + surgical edits
- **[`compiler/REMAINING_PANICS.md`](../../compiler/REMAINING_PANICS.md)** — intentional panic/exit catalog
- **[`compiler/AST_SERDE_NOTES.md`](../../compiler/AST_SERDE_NOTES.md)** — AST serialization contract
- **[`runtime/README.md`](../../runtime/README.md)** — the canonical SOL bytecode VM (browser-safe)
- **[`runtime/UPSTREAM.md`](../../runtime/UPSTREAM.md)** — VM provenance + surgical edits
- **[`compiler-wasm/README.md`](../../compiler-wasm/README.md)** — the wasm-bindgen bridge

### The TS side

- **[Architecture overview](./ARCHITECTURE.md)** — high-level diagram + module responsibilities
- **[`docs/sol-language/IMPORT_COMPATIBILITY.md`](../sol-language/IMPORT_COMPATIBILITY.md)** — AST→graph importer rules
- **[`docs/sol-language/CANONICALIZATION.md`](../sol-language/CANONICALIZATION.md)** — graph→source canonical-form contract
- **[`docs/sol-language/SYNC_MODEL.md`](../sol-language/SYNC_MODEL.md)** — explicit-action sync philosophy
- **[`docs/sol-language/SIMULATOR_PARITY.md`](../sol-language/SIMULATOR_PARITY.md)** — legacy JS interpreter status

### Project history

- **[Phase B plan + status](../sol-language/PHASE_B_COMPILER_IDE_PLAN.md)** — milestone tracking
- **[Phase B release notes](../sol-language/B_RELEASE_NOTES.md)** — what shipped when

## Running the test suite

```bash
npm run check
```

Runs:
- `vue-tsc --noEmit` (TypeScript typecheck)
- `vitest run` (TS unit + integration tests; 69 currently)
- `cargo test --workspace` (Rust unit + integration tests; 49 currently)

Total: ~118 tests across both runtimes.

For Rust-only iteration:

```bash
cargo test -p solflow_compiler
cargo test -p solflow_runtime
cargo test -p solflow_compiler_wasm
```

For TS-only iteration:

```bash
npm run test
npm run test:watch
```

## Rebuilding the WASM bundles

WASM bundles are committed under `compiler-wasm/pkg/` (browser
target) and `compiler-wasm/pkg-node/` (Node target, used by
e2e vitest). They only need rebuilding when Rust code changes.

```bash
# one-time setup:
cargo install wasm-pack
rustup target add wasm32-unknown-unknown

# rebuild both:
npm run build:wasm:all

# or just one:
npm run build:wasm        # browser target
npm run build:wasm:node   # Node target (for vitest)
```

## Regenerating importer test fixtures

The vitest e2e suite uses pre-baked AST JSON fixtures so it
doesn't need WASM at test time. After any change to the AST
shape or compiler serde output:

```bash
npm run regen:fixtures
git diff src/graph/import/__fixtures__/
```

Review the diff before committing.
