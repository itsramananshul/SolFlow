# 01 — Overview

This chapter answers four questions about SOL, the language that powers
SolFlow workflows: what kind of language it is, how a SOL file gets
executed, what SOL is deliberately not, and which parts are stable today.

The canonical language lives in the `sol/` crate (the `openprem-sol-v2`
package). The editor talks to it through the `compiler-wasm` bridge.

## What this chapter answers

- What kind of language is SOL, and what shape of program is it good for?
- How does a SOL file end up being executed, what does the language hand
  off to its host, and what does the host hand back?
- What is SOL deliberately not trying to be?
- Which parts of the language are stable today and which are in motion?

## 1.1 Language identity

SOL is a small, eager, single threaded orchestration language. A program
is a flat list of top level items (`import`, `fn`, `struct`, `enum`,
`workflow`) and the unit of execution is a named `workflow`. SOL is built
for short orchestration programs that call out to external Actions, not
for general application code.

SOL is dynamically executed. There is NO type checker and NO compile time
semantic analysis pass. Type annotations are accepted in the syntax and
recorded in the AST, but mismatches are not caught before execution. They
surface at runtime instead. Every fallible step in the pipeline returns a
plain string error (`Result<_, String>`); there is no error code system.

## 1.2 Mental model

A SOL file declares the pieces a session needs:

- the helper types it speaks (`struct`, `enum`),
- the helper functions it defines (`fn`),
- the external modules it imports (`import`),
- and one or more `workflow` blocks the host can run.

A `workflow` is the entry shape. Each `workflow "name" { ... }` is an
independently runnable unit identified by its string name.

```sol
# A minimal workflow.
workflow "greet" {
    print("hello");
}
```

## 1.3 Execution shape

The pipeline is:

```
source -> Lexer -> Parser (AST) -> Compiler (bytecode Chunk) -> Vm (stack VM)
```

`WorkflowExecutor` ties parse, compile, and run together for a single
workflow. The stages live in `sol/src/lexer.rs`, `sol/src/parser.rs`,
`sol/src/compiler.rs`, and `sol/src/vm.rs`; `sol/src/workflow.rs` holds
the executor.

The VM is a stack machine that steps through bytecode. `Vm::step(budget)`
takes a statement budget (it counts statement boundary crossings, not raw
instructions) and returns one of:

- `Completed(Value)` — the workflow finished with a value,
- `Yielded(steps)` — the budget was exhausted; resume to continue,
- `RemoteCall { capability, params }` — the workflow wants to invoke an
  external Action; the host resolves it and resumes with
  `resolve_remote_call`,
- `Failed(String)` — a runtime error (a plain message).

Side effects happen through the `print` builtin (which appends to an
output buffer) and through external Actions. An Action call is expressed
as `call("module.action", params)`, as an imported `module.func(args)`
call, or as `expr::rpc(args)`; each becomes a `RemoteCall` the host must
fulfill.

There is a deprecated tree walking `interpreter` module in the crate. The
bytecode VM is the canonical execution path; the interpreter is marked
`#[deprecated]`.

## 1.4 What SOL is not

- Not a general purpose application language.
- No async, no threads, no concurrency primitives in the language.
- No static type checker and no semantic analysis phase. Type errors are
  runtime failures, not compile time diagnostics.
- No exceptions; failures are returned as string errors and surface as a
  `Failed(String)` step result.
- No first class functions, no closures.
- No module system beyond `import`; imported modules name external Action
  providers that the host resolves at runtime.

## 1.5 Diagnostics model

Because there is no type checker, there are no `E0xxx` or `T90xx` error
codes anywhere in the live pipeline. The real diagnostic surfaces are:

- The crate returns string errors from every fallible step.
- The `compiler-wasm` bridge wraps each call in a JSON envelope and emits
  exactly five diagnostic codes: `E_PARSE` (parser), `E_CODEGEN`
  (codegen), `E_NO_WORKFLOW` (no runnable workflow), `E_RUNTIME` (runtime
  warning), and `ICE0001` (internal error). See
  `compiler-wasm/src/lib.rs`.
- The editor runs its own structural checks on the graph in
  `src/graph/validate.ts`, using kebab-case codes such as `no-entry`,
  `unset-var`, and `type-mismatch`. These are editor side checks, not
  compiler output.

## 1.6 Stability surface

Stable today: the parser surface (the keywords, the `<-` return arrow,
`#` comments, prefix `[]T` arrays, the top level items, the statement and
expression forms), the primitive types, and control flow.

In motion: richer diagnostics, source spans (the lexer tracks no line or
column information today), and the library API surface the host exposes
to workflows.

## Sources

- `sol/src/lib.rs` — public crate surface
- `sol/src/lexer.rs` — tokens and trivia
- `sol/src/parser.rs` — top level items, statements, expressions
- `sol/src/ast.rs` — AST node shapes
- `sol/src/compiler.rs` — bytecode lowering
- `sol/src/vm.rs` — execution
- `sol/src/workflow.rs` — the workflow executor
- `compiler-wasm/src/lib.rs` — the editor bridge and diagnostic envelope
