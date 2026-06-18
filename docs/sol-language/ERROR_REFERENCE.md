# Error Reference

> **Status:** Rebuilt against the canonical `openprem-sol-v2` crate (the
> `sol/` crate). The previous version of this document catalogued a large
> set of numbered codes (E0xxx parse, E1xxx semantic, E2xxx runtime, T90xx
> tool) emitted by a type-checking compiler that no longer exists. That
> compiler and its analyzer phase were deleted. The canonical crate has NO
> error codes, NO source spans, and NO compile-time type checker. This
> reference documents the error model that actually ships.

The narrative companion is [chapter 15](./15-errors-and-diagnostics.md).

---

## The error model in one paragraph

The canonical pipeline is source to `Lexer` to `Parser` to `Compiler` to
`Vm`,
tied together for a single workflow by `WorkflowExecutor`. Every fallible
step returns `Result<_, String>`: a plain human readable message, with no
code and no span. Type mismatches are not caught ahead of time; they
surface at run time as a `Failed(String)` step result. The lexer never
hard errors (unterminated strings and bad numbers fall back silently).
The editor does not see these raw strings directly. It talks to the crate
through the `compiler-wasm` bridge, which wraps each message in a stable
JSON envelope and tags it with one of exactly five diagnostic codes. A
separate, editor only layer (`src/graph/validate.ts`) runs structural
checks on the visual graph and emits its own kebab-case codes; those are
not compiler diagnostics.

So there are three distinct error surfaces, documented in order below:

1. **Language layer** (`sol/src/*`): raw `String` messages.
2. **Bridge diagnostics** (`compiler-wasm/src/lib.rs`): five codes in a
   JSON envelope, plus the runtime error views.
3. **Editor structural validation** (`src/graph/validate.ts`): kebab-case
   codes on the graph.

---

## 1. Language layer: raw string errors (`sol/src/*`)

These are the messages the crate itself produces. They are returned as the
`Err` arm of a `Result<_, String>` and have no code or span. The bridge
forwards their text into the `message` field of a diagnostic (section 2).
The lists below are representative real messages taken from the crate, not
an exhaustive enumeration.

### Lexer (`sol/src/lexer.rs`)

The lexer never hard errors. Unterminated strings and chars and malformed
numbers fall back silently rather than producing a diagnostic, so there is
no message to catalogue here. Anything wrong lexically shows up later as a
parser message.

### Parser (`sol/src/parser.rs`)

The parser returns a `String` on the first construct it cannot accept.
Representative messages:

| Message | Triggered by |
| --- | --- |
| `expected {Token}, got {Token}` | a required token is missing (the generic `expect`) |
| `unexpected top-level token {Token}` | a statement keyword where a top level item was expected |
| `unexpected token {Token}` | a token that cannot start a primary expression |
| `unexpected EOF` / `expected identifier, got EOF` | input ends mid construct |
| `expected identifier, got {Token}` | a name was required (param, field, type) |
| `expected string literal, got {Token}` | `workflow` / `emit` / `import "..."` without a string |
| `expected type, got {Token}` | a type position holds a non type token |
| `expected enum name before \`::\`` | `::` used without a preceding enum identifier |
| `expected field name, got {Token}` | malformed struct literal / member access |
| `invalid assignment target: {Expr}` | assigning to something that is not an identifier, `a.field`, or `a[i]` |
| `unexpected block as statement` | a bare `{ ... }` block in statement position |
| `could not extract expression` | internal parser recovery dead end |

Example that triggers a parse error (the return arrow is `<-`, never `->`):

```sol
# `->` lexes as two tokens (Minus, Gt) and fails to parse.
fn double(x: int) -> int {
    return x * 2;
}
```

The fix is to write the canonical arrow:

```sol
fn double(x: int) <- int {
    return x * 2;
}
```

### Compiler / codegen (`sol/src/compiler.rs`)

The compiler walks the AST into a bytecode `Chunk`. It rejects a handful of
constructs it cannot lower. Representative messages:

| Message | Triggered by |
| --- | --- |
| `no workflow found in program` | compiling a program that declares no `workflow "..."` |
| `index assignment not supported` | `a[i] = v;` (indexed assignment is not lowered) |
| `complex target in assignment not supported` | an assignment target that is neither a plain name nor a simple field set |
| `variable '{name}' not found for assignment` | assigning to a name that was never declared with `let` |

Example that triggers `index assignment not supported`:

```sol
workflow "demo" {
    let xs: []int = [1, 2, 3];
    xs[0] = 9;   # indexed assignment is not supported by codegen
}
```

### VM / runtime (`sol/src/vm.rs`, `value.rs`, `workflow.rs`)

The VM evaluates bytecode. A failing step returns
`StepResult::Failed(String)` (or an internal `Err(String)`); both carry one
of these messages. Because there is no type checker, every type error lands
here. Representative messages:

| Message | Triggered by |
| --- | --- |
| `division by zero` | integer or float divide / modulo by `0` |
| `variable '{name}' not found` | reading a name with no binding in scope |
| `field '{f}' not found` | reading a struct field that does not exist |
| `cannot access field '{f}' on {value}` | member access on a non struct value |
| `cannot assign to field of non-struct` | `x.f = v` where `x` is not a struct |
| `index {N} out of bounds` | array index past the end |
| `cannot index {value} with {value}` | indexing a non array, or with a non int |
| `cannot use {value} as condition` | an `if` / `while` condition that is not `bool` or `int` |
| `function '{name}' not found` | calling a builtin or function name that does not exist |
| `cannot {op} {value} and {value}` | arithmetic / comparison on incompatible operand types |
| `cannot negate {value}` / `cannot apply 'not' to {value}` | unary `-` or `!` on an unsupported type |
| `len() takes exactly 1 argument` etc. | builtin arity violations (`len`, `to_str`, `type_name`) |
| `workflow '{name}' not found` | `WorkflowExecutor::new` with a name no workflow declares |

Example that fails at run time with `division by zero`:

```sol
workflow "demo" {
    let a: int = 10;
    let b: int = 0;
    print(a / b);   # runtime Failed("division by zero")
}
```

Example that fails at run time with `variable 'x' not found` (no compile
time check catches this):

```sol
workflow "demo" {
    print(x);   # x was never declared
}
```

The full builtin set is `print`, `len`, `to_str`, and `type_name`. Calling
anything else by bare name yields `function '{name}' not found`. External
Actions are not builtins; they are reached via `call("m.f", p)`, an
imported `m.f(args)`, or `m::rpc(args)`, each of which becomes a remote
call rather than an error (see section 2.4).

---

## 2. Bridge diagnostics (`compiler-wasm/src/lib.rs`)

The editor calls the wasm bridge (`parse_source_json`, `compile_source_json`,
`run_source_json`, and friends). Each returns a JSON `Envelope` and, for
runs, a `run` object. Inside that envelope, every error the bridge raises is
one of exactly **five** codes. There are no E0xxx, E1xxx, E2xxx, or T90xx
codes anywhere in the live pipeline.

### 2.1 The complete code vocabulary

| Severity | Phase | Code | When it fires | Tiny trigger |
| --- | --- | --- | --- | --- |
| Error | Parser | `E_PARSE` | the parser returned an `Err` | `fn f( {` (malformed) |
| Error | Codegen | `E_CODEGEN` | the compiler returned an `Err` | `workflow "w" { let xs: []int = [1]; xs[0] = 2; }` (`index assignment not supported`) |
| Error | Analyzer | `E_NO_WORKFLOW` | a run was requested but the program declares no workflow | a source file with only `fn` / `struct` and no `workflow "..."` |
| Warning | Runtime | `E_RUNTIME` | a `run` produced `StepResult::Failed` (or an internal step error) | `workflow "w" { print(1 / 0); }` |
| Error | Internal | `ICE0001` | the bridge caught a panic while invoking the crate | an internal compiler panic (should not happen in practice) |

Notes on each:

- **`E_PARSE`** carries the parser's raw message (section 1, Parser).
  Emitted by `parse_source_json`, `compile_source_json`,
  `format_source_json`, `compile_for_wire_json`, and the parse step of
  `run_source_json`.
- **`E_CODEGEN`** carries the compiler's raw message (section 1, Compiler).
  Emitted by `compile_source_json`, `compile_for_wire_json`, and the
  compile step of `run_source_json`. Note `format_source_json` only parses,
  so it reports `E_PARSE` and never `E_CODEGEN`.
- **`E_NO_WORKFLOW`** is unique to `run_source_json`: parse and compile
  succeeded but there is no workflow to execute. Its message is
  `no workflow declaration found`. The phase string is `Analyzer` even
  though there is no analyzer pass; it is just the bridge's label for this
  pre run check.
- **`E_RUNTIME`** is a **Warning**, not an Error, so the run envelope's
  `ok` stays `true` (compilation succeeded; the failure was at run time).
  Its message is the VM's raw string (section 1, VM).
- **`ICE0001`** is an Internal Compiler Error. The bridge wraps every entry
  point in `catch_unwind`; if the crate panics, the bridge returns an
  `ICE0001` envelope with the panic text rather than crashing the wasm
  module.

### 2.2 The diagnostic JSON shape

This is the stable editor contract, mirrored in
[`src/compiler/types.ts`](../../src/compiler/types.ts) as `SolDiagnostic`:

```jsonc
{
  "severity": "Error",       // "Error" | "Warning" | "Note"
  "phase": "Parser",         // "Lexer" | "Parser" | "Analyzer" | "Codegen" | "Runtime" | "Internal"
  "code": "E_PARSE",         // one of the five codes above
  "message": "expected RBrace, got EOF",
  "span": null,              // SourceSpan or null; null in practice today
  "related": [],             // RelatedSpan[]; empty today
  "help": null               // string or null; null today
}
```

The `phase` enum reserves `Lexer`, but the lexer never hard errors, so
`Lexer` is **never emitted**. The `span`, `related`, and `help` fields exist
in the contract for forward compatibility but are null / empty today,
because the crate carries no span information.

The envelope that wraps the diagnostics:

```jsonc
{
  "ok": true,                // false iff a fatal Error short-circuited
  "value": { /* per entry point */ },
  "diagnostics": [ /* SolDiagnostic[] */ ]
}
```

`run_source_json` adds a `run` object alongside `ok`, `value`, and
`diagnostics`:

```jsonc
{
  "ok": true,
  "value": { "instruction_count": 12 },
  "diagnostics": [ /* may contain an E_RUNTIME warning */ ],
  "run": {
    "return_value": 42,        // number | null
    "output": ["line one"],    // captured print() lines
    "steps": 7,
    "runtime_error": null,     // RuntimeError | null (section 2.4)
    "runtime_error_source_span": null,
    "trace": [],
    "trace_truncated": false
  }
}
```

### 2.3 Where each code can appear, by entry point

| Entry point | Possible error codes |
| --- | --- |
| `parse_source_json` | `E_PARSE`, `ICE0001` |
| `analyze_source_json` | `E_PARSE`, `ICE0001` |
| `compile_source_json` | `E_PARSE`, `E_CODEGEN`, `ICE0001` |
| `compile_for_wire_json` | `E_PARSE`, `E_CODEGEN`, `ICE0001` |
| `format_source_json` | `E_PARSE`, `ICE0001` |
| `run_source_json` | `E_PARSE`, `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001` |

### 2.4 Runtime error views

When a run does not complete cleanly, `run.runtime_error` holds a structured
value (a discriminated union tagged by `kind`), separate from the
`E_RUNTIME` text diagnostic. There are two relevant unions:

- The **browser sim** (`RtErr` in `compiler-wasm/src/lib.rs`) emits only:
  - `ExtCallBlocked { function_name, url }`: the workflow tried to make an
    external Action call; the sim blocks it and reports the capability name.
  - `StepLimit { limit }`: the run exceeded the bridge's step guard.
- The wider **`RuntimeErrorView`** union (shared with the controller in
  [`src/runtime-host/types.ts`](../../src/runtime-host/types.ts), mirrored as
  `RuntimeError` in `src/compiler/types.ts`) is the wire shape that exists so
  editor code can `switch` on `kind` exhaustively. Most of its variants are
  **wire only**: they are defined for the controller's real runtime but are
  not produced by the in browser sim.

| `kind` | Emitted by sim? | Meaning |
| --- | --- | --- |
| `ExtCallBlocked` | yes | external Action call blocked in the sim |
| `StepLimit` | yes | step guard exceeded in the sim |
| `DivByZero` | wire only | divide by zero (sim surfaces this as `E_RUNTIME` text instead) |
| `IndexOutOfBounds` | wire only | array index past the end |
| `StackUnderflow` | wire only | VM stack underflow |
| `ExtCallFailed` | wire only | a connector call failed on the controller |
| `HeapShapeMismatch` | wire only | heap value shape did not match expectations |
| `Cancelled` | wire only | run was cancelled |
| `Timeout` | wire only | wall clock budget exhausted |
| `ResourceLimit` | wire only | a per run resource cap was exceeded |

In other words: in the browser, expect only `ExtCallBlocked` and
`StepLimit` in `runtime_error`; arithmetic and indexing failures show up as
`E_RUNTIME` warning text. The rest of the union is reserved for the
controller side runtime.

---

## 3. Editor structural validation (`src/graph/validate.ts`)

This layer is **not the compiler**. It runs structural checks on the visual
graph before SOL is even emitted, and it uses kebab-case codes (distinct
from the bridge's five codes). It catches mistakes that would otherwise
produce broken SOL or surprising runtime behavior.

| Code | Severity | Meaning |
| --- | --- | --- |
| `no-entry` | warning | no `start` function and no trigger node, so the workflow has no entry point |
| `unnamed-function` | error | a function has an empty name; emission would produce SOL the parser rejects |
| `enum-first-char-collision` | warning | two enum variants share a first character (see note below) |
| `missing-input` | error | a required input port has neither a wired edge nor a non empty inline expression |
| `bad-inline-expression` | error | an inline expression failed the lint (`lintInlineExpression`); unsafe to emit or to evaluate in the sim |
| `unset-struct` | error | a struct literal / field access / field set node has no struct selected |
| `unknown-struct` | error | the selected struct name is not defined in the workflow |
| `unset-field` | error | a field access / field set node has no field selected |
| `unset-enum` | error | an enum variant node has no enum selected |
| `unknown-enum` | error | the selected enum name is not defined in the workflow |
| `unset-variant` | error | an enum variant node has no variant selected |
| `unset-call` | error | a call node has no target function selected |
| `unknown-call` | error | the call node's target function is not found |
| `unset-var` | error / warning | an assign node has no target variable (error), or a varGet node has none selected (warning) |
| `unresolved-var` | warning | a varGet references a variable not declared in its function |
| `type-mismatch` | warning | a data edge connects ports whose types are not equal |

`missing-input` and `bad-inline-expression` are the two codes the Sol Man
store treats as never bypassable via `force=true`.

### Note: `enum-first-char-collision` (the historical `T9002` hazard)

The canonical bytecode dispatches each enum variant by
`(first_char as i128) % 10`. Two variants whose first characters share a
mod-10 residue therefore compare **equal at run time**, even though the by
name simulator runs them correctly, so the bug is invisible during in
browser testing. The editor surfaces it as a warning so the user is not
ambushed at deploy time. For example, in:

```sol
enum Status { Active; Aborted; }
```

both `Active` and `Aborted` start with `A` (`'A' % 10 == 5`) and collide at
run time. The fix is to rename one so every variant has a distinct first
character. This is the only place the old `T9002` concept survives, and it
now lives in the editor, not the compiler.

---

## Quick map of the surfaces

| Surface | Source | Codes | Has spans? |
| --- | --- | --- | --- |
| Language layer | `sol/src/parser.rs`, `compiler.rs`, `vm.rs` | none (raw `String`) | no |
| Bridge diagnostics | `compiler-wasm/src/lib.rs` | `E_PARSE`, `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001` | no (`span: null`) |
| Runtime error views | `src/runtime-host/types.ts`, `src/compiler/types.ts` | `kind`-tagged union (2 sim, rest wire only) | no |
| Editor structural validation | `src/graph/validate.ts` | kebab-case (table above) | n/a (node ids) |
