# Remaining panic / abort sites in solflow_compiler

After Phase B.2 (c3 lexer + parser, c4 analyzer, c5 codegen) every
**error** in the compile pipeline is reported as a `SolDiagnostic`
value. No surface-level error path calls `eprintln + process::exit`
or `panic!` anymore.

This document catalogs the panic / abort sites that intentionally
remain. They fall into three categories:

## 1. CLI exit codes (intentional, not library code)

`compiler/src/bin/sol.rs` calls `std::process::exit(N)`:

| Line | Why |
|---|---|
| `usage_and_exit()` (top) | Bad argv → exit 2 |
| read_to_string failure | I/O failure on the input file → exit 2 |
| each `if has_errors() { exit(1) }` | Surface diagnostic exit code |

These belong in the CLI, not the library. Library callers (the
SolFlow editor through WASM in B.4) never reach this binary.

## 2. Invariant-only `unwrap` / `unreachable` (TODO: tighten in B.3+)

| File:line | Pattern | Reason |
|---|---|---|
| `lexer.rs:204` | `read_to_string(path).unwrap()` inside `Lexer::from(path)` | File-path constructor used only by the CLI; CLI now reads the file itself and calls `Lexer::from_str` via `lex_source`. The constructor remains for backward compatibility; the unwrap should be removed when the CLI fully migrates. |
| `analyzer.rs:70` | `self.tts.last().unwrap()` in `add_entry` | Invariant: `add_entry` always runs after `new_table()` has been called (either explicitly in `run()` or by the `if self.tt_arena.is_empty()` guard immediately above). Cannot fail in practice. |
| `parser.rs:702` | `_ => unreachable!()` in a match over a fixed-size token kind | Token kind range is exhaustive at call site. |
| `parser.rs:742` | `if let Token::Ident(n) = self.advance() { n } else { unreachable!() }` | Caller already checked `peek().kind() == Ident`. |

These can be replaced with `debug_assert!` + safe fallbacks in a
later hardening pass; they don't affect Phase-B correctness because
no real input can reach them.

## 3. ~~`todo!()` for unhandled AST variants~~ ✅ resolved in c7

The analyzer's catch-all `todo!("{x:?}")` was converted to an
internal compiler error (ICE) diagnostic in B.2 c7:

```
internal compiler error[ICE0001] internal compiler error:
  analyzer has no rule for AST node: ExprStructInit { ... }
  help: this is a bug in solflow_compiler; please report it
```

Editor-generated AST shapes that hit unfinished checker arms now
produce a structured `DiagnosticPhase::Internal` diagnostic
instead of aborting the test runner / WASM worker. See
`compiler/src/diagnostic.rs` (`SolDiagnostic::internal`) and the
`ICE_*` code constants.

## 4. Deferred work the compiler should still do

Not panics — but tracked here so we don't forget.

### AST-level source spans

B.6 c23 attached spans to lexer + parser diagnostics. The
analyzer's diagnostics still emit with `span: null` because the
spans the analyzer would need live on AST nodes, and AST nodes
don't carry spans today.

Plumbing requires:
- Add `span: SourceSpan` to every `Ast` variant (or wrap with
  `Spanned<T>`)
- Parser propagates token-start → node-end as it builds each AST
  node
- Analyzer emit sites read the AST node's span and attach to the
  diagnostic

When this lands, the editor's `CompilerDiagnosticPanel` analyzer
rows become clickable too (currently non-affordant), and the
importer can lift its current textual function-line scan to a
real per-node attachment.

## 5. Excluded by design (not in this crate)

The following upstream-host concerns were deliberately not vendored
in B.1 and therefore have no panics here at all:

- VM execution (`vm.rs`) — not in this crate
- Host-runtime / loader (`init.rs`) — not in this crate
- Network / libp2p / session — not in this crate
- HTTP / `ExtCall` execution — codegen emits `Inst::ExtCall`
  but nothing here runs it

Future B.10 work that adds a VM crate will catalog its own panics
separately.
