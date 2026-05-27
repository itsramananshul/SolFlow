# Upstream provenance

The SOL compiler source in this crate was vendored from an internal
sibling workspace on **2026-05-26**. The original source serves a
network-server binary; this vendor extracts only the
compiler-frontend modules (lexer, parser, analyzer, codegen, util)
and adds the IDE-oriented Phase B work on top.

## Files derived from upstream

| File in this crate | Upstream origin (paths kept private) | Modifications |
|---|---|---|
| `src/lexer.rs` | upstream `src/sol/lexer.rs` | Module-path rewrites (`crate::sol::` → `crate::`); will gain `from_str` in B.2 |
| `src/parser.rs` | upstream `src/sol/parser.rs` | Module-path rewrites; B.2 converts `process::exit(1)` → diagnostics |
| `src/analyzer.rs` | upstream `src/sol/analyzer.rs` | Module-path rewrites; B.2 converts `process::exit(1)` → diagnostics |
| `src/bytecode.rs` | upstream `src/sol/bytecode.rs` | Module-path rewrites; B.2 converts the two `process::exit(1)` sites → diagnostics |
| `src/util.rs` | upstream `src/sol/util.rs` | Module-path rewrites |
| `tests/fixtures/*.sol` | upstream `tests/*.sol` (and subdirs) | Verbatim; the SolFlow editor docs (`docs/sol-language/EXAMPLES.md`) describe each |

## Files deliberately NOT vendored

| Upstream file | Why excluded |
|---|---|
| `src/sol/vm.rs` | Runtime — out of compiler-frontend scope. Future split into a separate `runtime/` crate may revisit |
| `src/sol/init.rs` | Host-side composition with HTTP transport; pulls in libp2p / tokio / server stack |
| `src/sol/main.rs` + `src/sol/cli.rs` | Upstream binary entrypoints — replaced by `src/bin/sol.rs` in this crate |
| `src/handler.rs`, `src/network/`, `src/session.rs` | Workflow-engine code; entirely separate concern from compilation |

## Synchronization strategy

This crate is **a fork**, not a live mirror. There is no automated
sync from upstream. Going forward:

- All Phase B work (B.1 → B.11) lands here. SolFlow's `compiler/`
  becomes the canonical IDE-side compiler.
- When the canonical upstream compiler ships its own IDE-readiness
  refactor (per the upstream team's own internal plan), the two
  may converge — that decision belongs to the project lead.
- Bug fixes that improve language semantics belong in both copies;
  bug fixes specific to IDE use (errors-as-values, source spans,
  WASM ergonomics) live only here.
- **Do not edit the upstream workspace from SolFlow tooling.** The
  upstream workspace is treated as read-only reference material.

## What snapshot was vendored

The vendoring captured the upstream `src/sol/` state as observed on
2026-05-26. Notable known issues at that snapshot (catalogued
elsewhere in the SolFlow docs under `docs/sol-language/`):

- `process::exit(1)` is the only error-reporting mechanism in the
  compiler. **Closed by Phase B.2 in this crate.**
- `Token` and `Ast` nodes carry no source spans. **Future Phase B
  work — span field exists in `SolDiagnostic` but is `None` for
  this bundle.**
- `Token::Integer(i128)` is not JSON-encodable. **Closed in B.3
  when serde derives land.**
- `HashMap` for struct fields / enum variants destroys order
  (T-PARSER-001 in `REMAINING_PANICS.md`). **Closed in B.3.**
- ~70 `process::exit(1)` sites across the modules. **B.2 closes
  the parse/analyze/codegen subset; VM panics tracked in
  `REMAINING_PANICS.md` for future runtime work.**

## Privacy posture

The upstream workspace name and path are intentionally not
recorded in this file. Anyone with appropriate access knows where
to look; this crate's commit history and public-facing files are
named neutrally (`solflow_compiler`).
