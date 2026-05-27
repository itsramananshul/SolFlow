# solflow_compiler_wasm

wasm-bindgen bridge that exposes [`solflow_compiler`](../compiler/)
to the SolFlow browser editor.

This crate is intentionally tiny — it owns nothing except the
JSON-shaped boundary contract. The real lexer/parser/analyzer/codegen
live in the `compiler/` crate.

## Why JSON?

The bridge returns strings, not `JsValue`s. Three reasons:

1. **Boundary stability.** wasm-bindgen's `serde-serialize` story
   has shifted across versions; JSON over `&str` doesn't.
2. **Debuggability.** A JSON string is easy to `console.log`,
   diff, or paste into a bug report.
3. **TypeScript clarity.** The TS side parses with a single
   `JSON.parse(...) as Envelope<T>` and gets full type inference.

The envelope is identical for every entry point:

```ts
type Envelope<T> = {
  ok: boolean;            // true iff diagnostics has no Error severity
  value: T | null;
  diagnostics: SolDiagnostic[];
};
```

## Exports

| Function | Returns |
|---|---|
| `parse_source_json(source)` | `Envelope<Program>` |
| `analyze_source_json(source)` | `Envelope<{ program: Program }>` |
| `compile_source_json(source)` | `Envelope<{ program: Program; instruction_count: number }>` |
| `version()` | crate-version string |

`tt_arena` (the analyzer's type-table arena) and the full `Inst`
list (bytecode) are deliberately **not** in the envelope yet — the
editor doesn't need them for diagnostics, and skipping them keeps
the payload small. Re-add when the editor needs hover/symbol info
(B.6+) or a browser-side VM (B.10).

## Build

One-time setup:

```bash
cargo install wasm-pack
rustup target add wasm32-unknown-unknown
```

Build (writes `pkg/`, which is committed):

```bash
./build.sh         # macOS / Linux / Git Bash
build.cmd          # Windows cmd
npm run build:wasm # from repo root (alias)
```

## Native tests

The same JSON wrappers compile as a native `rlib`, so they're
testable without a browser:

```bash
cargo test -p solflow_compiler_wasm
```

The integration tests in `src/lib.rs` lock the envelope shape
(human-string enums, `ok` boolean semantics, ICE on panic) so the
TypeScript side never has to guess.

## Panic isolation

Every entry point installs `console_error_panic_hook` and wraps the
body in `std::panic::catch_unwind`. If the compiler panics, the
bridge synthesizes an `ICE0001` diagnostic and returns a normal
envelope rather than letting the WASM instance abort. The editor
renders that diagnostic exactly like any other compiler error
(red, with the help-string "this is a bug in solflow_compiler;
please report it").

## Why no `tokio` / async?

The compiler is synchronous. If the editor wants to keep the UI
responsive on huge files, the cleanest move is to run the WASM in
a Web Worker — not to make the Rust API async. That's a B.5+ choice.

## Provenance

This crate is a thin shim; the compiler logic is upstream of any
host. See `../compiler/UPSTREAM.md` for the compiler's provenance.
