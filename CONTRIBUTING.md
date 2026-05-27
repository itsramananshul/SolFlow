# Contributing to SolFlow

Thanks for your interest. SolFlow is a Vue 3 visual IDE backed by
a Rust compiler + VM compiled to WebAssembly. Contributions are
welcome — read this first.

## Quickstart for contributors

```bash
git clone <repo>
cd SolFlow
npm install
npm run dev       # http://localhost:5173
```

Verify everything before pushing:

```bash
npm run check     # typecheck + vitest + cargo workspace tests (~118 tests)
```

Rebuilding the WASM bundles (only needed when Rust code changes):

```bash
# One-time toolchain setup:
cargo install wasm-pack
rustup target add wasm32-unknown-unknown

# Rebuild both browser + Node WASM targets:
npm run build:wasm:all
```

## Repo layout

```
SolFlow/
├── src/                  # Vue editor
│   ├── components/       # UI: SolNode, SourcePreview, RunModal, ...
│   ├── compiler/         # TS wrapper around WASM bridge
│   ├── graph/            # graph schema, factory, validator, emit, import
│   ├── runtime/          # LEGACY JS interpreter (canvas-animation only)
│   ├── stores/           # Pinia stores
│   └── samples/          # Sample workflows shipped with the editor
├── compiler/             # Rust — standalone SOL compiler crate
├── compiler-wasm/        # Rust — wasm-bindgen bridge (pkg/ + pkg-node/ committed)
├── runtime/              # Rust — canonical SOL VM, browser-safe
├── docs/                 # User + reference + dev docs
└── scripts/              # Dev tooling
```

## What kind of contribution

| Type | How |
|---|---|
| **Bug fix** | Open issue with reproduction; PR welcome. |
| **Editor UX improvement** | PR with a clear before/after. |
| **New sample workflow** | Add to `src/samples/`; see existing files for the builder pattern. |
| **Docs improvement** | All docs are markdown; PR away. |
| **New SOL language feature** | Discuss in an issue first — the SOL language is a separate cross-cutting concern. |
| **Compiler / VM changes** | High bar; please open an issue first. The compiler is canonical; UX changes belong in the editor. |

## Code style

- TypeScript: strict mode is on; no `any` without a comment justifying it.
- Vue: Composition API + `<script setup>`. SFCs only.
- Rust: idiomatic `cargo fmt` style. Diagnostics return values, never `panic!` on user-reachable paths.
- Comments: explain WHY (non-obvious decisions, constraints), not WHAT (the code already says that).

## Test discipline

- Every behavior change should land with a test.
- Existing tests must stay green: `npm run check`.
- Snapshot tests (`*.snap` files) are explicit gates; if a snapshot diff is expected, regenerate with `npm test -- -u` and commit the diff.

## What stays out of public contribution

- Branding / logo work — needs design coordination
- Phase C topics: deployment infrastructure, multi-user auth, real external-call execution

## Release cycle

Currently lazy: tagged releases when major productization steps land. See `CHANGELOG.md` for what shipped when.
