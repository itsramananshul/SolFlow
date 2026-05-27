# Changelog

User-facing changes to SolFlow, by release.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
SolFlow uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing pending — the most recent productization sweep landed as
v0.2.0.

## [0.2.0] — 2026-05-27 — Productization release

Phase B + deferred-B + productization. SolFlow becomes a
public-presentable product:

### Added

- **User documentation** under `docs/user/` — Quickstart,
  Install, FAQ, Editor Guide. Three-track docs split:
  user / sol-language / dev.
- **`CONTRIBUTING.md`** + **`LICENSE`** (MIT).
- **Docs links in HelpModal** — Quickstart / Editor Guide / FAQ
  / SOL Language reachable from the `?` key inside the editor.
- **CI gate on bundled samples** — every sample on the welcome
  screen now has automated assertions that its emitted SOL
  parses + analyzes cleanly via the canonical compiler.

### Changed

- Escape-key now closes RunModal and ImportReportModal (was
  backdrop-click + ✕ button only). Brings them in line with the
  other modals.
- Privacy scrub: removed all internally-branded references from
  public-facing files. Sample names + descriptions now use
  generic language.
- README + repo organization restructured for public landing
  consumption.

### Not changed

The Phase B compiler-backed IDE architecture is unchanged: this
release is productization polish, not engineering. See the
**Phase B** entry below for what shipped engineering-wise.

## [0.1.0+B] — 2026-05-27 — Phase B + Deferred-B

Engineering completeness milestone. SolFlow runs on canonical
SOL semantics throughout the compile + execute pipeline.

### Phase B (B.1 – B.11)

- **Standalone SOL Rust compiler** vendored into `compiler/`
  with diagnostics-as-values, parser recovery, analyzer recovery
- **wasm-bindgen bridge** in `compiler-wasm/` — stable JSON envelopes
- **Live in-browser compiler diagnostics** with click-to-source
- **AST → graph importer** with honest classification (full /
  partial / source-only / unsupported)
- **Graph → source canonicalization** with round-trip stability
  tests
- **Sync model** explicit-action only (no live two-way binding;
  see `SYNC_MODEL.md`)
- **Canonical SOL VM in WASM** — `runtime/` crate; external
  calls blocked with structured `ExtCallBlocked` error rather
  than faked
- **VM hardening** — GetField/SetField OOB returns structured
  error instead of panic

### Deferred-B (c35 – c46)

- **AST source spans** flow through analyzer diagnostics + importer
  attachments + codegen sidecar
- **Importer expansion** — fieldSet, indexSet, top-level let
  auto-wrap into `__init()`
- **Web Worker** for hot-path parse/analyze (UI no longer freezes
  on long files)
- **Node-target WASM** for true e2e round-trip tests
- **Per-instruction span sidecar** in codegen
- **VM execution trace** + runtime-error spans
- **Per-node source attachment** on imported graph nodes
- **RunModal Trace tab** with click-to-source + click-to-canvas
  navigation

Test scoreboard at end of Deferred-B:
```
vitest    79 / 79
cargo     49 / 49
total    128 / 128
```

## [0.1.0] — Earlier — Phase A vertical slice

The original Vue 3 + Vue Flow editor with a TypeScript-only
graph emitter and a JS approximation interpreter. Foundation for
the canonical compiler work that followed in Phase B.

Phase A features (all retained in v0.2.0):
- Visual graph editor with 22 node kinds
- Live source preview (graph → SOL)
- Sample workflows (Hello, Monitor, Orchestration, Payments,
  Enterprise)
- Sol Man — LLM-assisted workflow generation (BYO key)
- Pinia-based state + autosave to localStorage
- 5 sample workflows on the welcome screen

[Unreleased]: https://github.com/itsramananshul/SolFlow/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/itsramananshul/SolFlow/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/itsramananshul/SolFlow/releases/tag/v0.1.0
