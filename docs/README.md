# SolFlow Documentation

Welcome. SolFlow is a visual IDE for the SOL language, backed by
a canonical Rust compiler + VM compiled to WebAssembly.

> If you're looking for the **product overview**, the
> [repo README](../README.md) is the landing page. This directory
> is the **deeper documentation**.

## Three tracks

| Track | For | Start here |
|---|---|---|
| **[`user/`](./user/README.md)** | People using SolFlow to build workflows | [Quickstart →](./user/QUICKSTART.md) |
| **[`sol-language/`](./sol-language/README.md)** | People learning SOL itself or its compiler | [Overview →](./sol-language/01-overview.md) |
| **[`dev/`](./dev/README.md)** | People contributing to SolFlow's editor or compiler | [Architecture →](./dev/ARCHITECTURE.md) |

## Quick links

- **[Quickstart](./user/QUICKSTART.md)** — go from zero to a running workflow in 5 minutes
- **[Install](./user/INSTALL.md)** — toolchain prerequisites + setup
- **[FAQ](./user/FAQ.md)** — common questions answered concisely
- **[Editor Guide](./user/EDITOR_GUIDE.md)** — how to use the visual editor
- **[Demo walkthrough](./DEMO.md)** — show SolFlow in 5 minutes
- **[CHANGELOG](../CHANGELOG.md)** — what shipped when
- **[Contributing](../CONTRIBUTING.md)** — repo layout + how to contribute

## Why three tracks?

SolFlow has three audiences with different needs:

- **Users** want to know how to build a workflow, what each node
  does, how to import/export, and how to debug.
- **SOL learners** want to know the language: types, syntax,
  semantics, error codes.
- **Contributors** want to know the architecture: how the WASM
  bridge works, how the importer maps AST to graph, how to add
  a new node kind.

Mixing these into one giant manual makes everyone's life worse.
Each track has its own README index.
