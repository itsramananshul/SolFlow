# SOL to Graph Import — compatibility matrix

This document records exactly what `importProgram()` (in
`src/graph/import/importer.ts`) can and cannot represent visually when
it converts a parsed SOL AST into a SolFlow `SolWorkflow`. The AST is
the canonical `Program` produced by the `compiler-wasm` bridge
(`parse_source_json` in `compiler-wasm/src/lib.rs`), which wraps the
`sol/` crate parser.

The matrix is **honest**: every entry reflects what the importer
actually does today, not what we would like it to do eventually.

## Classification

| Tier | Meaning | UI signal |
|---|---|---|
| **full** | Clean visual representation; the graph is the canonical form. Round-trips cleanly. | Green "Full" pill |
| **partial** | A graph node exists, but at least one subexpression is preserved as **inline SOL text** on the node rather than as a sub-graph. Round-trips canonical-but-textually-different SOL (semantics preserved). | Blue "Partial" pill |
| **source-only** | The construct survives in source mode but produces **no graph node**. Adding a notice ensures the user knows it was set aside. | Amber "Source-only" pill |
| **unsupported** | The importer does not know what to do with this AST shape. Yields a placeholder `print` node carrying the original SOL text inline plus a warning. Function marked degraded. | Amber "Unsupported" pill |

The headline counts on the import report are unions of these tiers
across every statement in every function.

## Top-level declarations

| SOL construct | Tier | Notes |
|---|---|---|
| `fn name(p: T) <- R { ... }` | **full** | Becomes a `FunctionGraph`. Parameters plus return type carry. The `<- R` arrow is optional; a function with no declared return type imports the same way. |
| `workflow "name" { ... }` | **full** | Becomes a `FunctionGraph` tagged `isWorkflow: true` so the emitter round-trips the `workflow "name"` form. |
| `struct Name { f: T; ... }` | **full** | Becomes a `StructDecl`. Field order is **preserved** — the canonical `StructDecl.fields` is an ordered `Vec` (`sol/src/ast.rs`), so the importer keeps insertion order with a plain map (no sort). |
| `enum Name { V1; V2; }` | **full** | Becomes an `EnumDecl`. Variant order is preserved; the canonical `EnumDecl.variants` is an ordered `Vec`. The enum has variant names only, no explicit values. |
| `import module;` / `import "name" from module;` | **full** | Becomes an `ImportDecl`. The canonical `ImportSpec` is either `Module(name)` or `Named { name, module }` (`sol/src/ast.rs`). |
| Top-level `let` | **source-only** | The canonical grammar has no module-level `let` (`parse_stmt` only runs inside function or workflow bodies), and the graph schema does not model one. A top-level `let` is therefore not produced by the parser and has no graph node. |

## Statements (inside function or workflow bodies)

| SOL construct | Tier | Notes |
|---|---|---|
| `let name: T = expr;` | **partial** | Becomes a `let` node. `expr` lives as inline canonical text on the `value` port. Annotate the type — when omitted, the AST records `bool` by default (`sol/src/parser.rs`). |
| `print(expr);` | **partial** | Becomes a `print` node; `expr` lives as inline text on `value`. The canonical builtin `print` is variadic; multi-arg `print(a, b)` is rendered as `[a, b]` because the graph schema has one value port. |
| `name(args);` (local call) | **partial** | Becomes a `call` node if the function exists in the workflow; otherwise a placeholder `print` carrying `name(args)` as text plus a notice. |
| `call("m.f", params)` / `m.f(args)` / `m::rpc(args)` (external Action) | **partial** | No dedicated capability node kind exists yet. Preserved as inline canonical SOL text on a placeholder node, never silently dropped. |
| `emit "event";` | **partial** | The event is a string literal. No dedicated node kind; preserved as inline text on a placeholder. |
| `return;` / `return expr;` | **partial** | Becomes a `return` node. Optional value lives inline on `value`. |
| `if (cond) { ... } [else { ... }]` | **partial** | Becomes a `branch` node plus two sub-bodies wired via `then` / `else` ports. `cond` lives as inline text. |
| `while (cond) { ... }` | **partial** | Becomes a `while` node plus body wired via `body`. `cond` inline. |
| `for x in expr { ... }` | **partial** | Becomes a `forEach` node plus body. `expr` inline on `array`; iterator type defaults to `any` (the importer runs on parser output and has no inferred type info). |
| `target = expr;` (assignment) | **unreachable** | The canonical parser does NOT currently parse assignment statements: `parse_stmt` in `sol/src/parser.rs` produces only `Let / If / While / For / Return / Emit / Expr`. The importer's `assign` / `fieldSet` / `indexSet` handlers exist for forward-compatibility but never fire on real parser output. |

## Expressions

Expressions are **never** lifted into the graph as sub-nodes by the
importer. They are stringified via `stringifyExpr()` in
`src/graph/import/expressions.ts` and embedded as inline text on the
parent statement's relevant port. This is the deliberate design choice
that lets the importer round-trip *semantically* without trying to
render arbitrary expression trees as visual node graphs.

`stringifyExpr` mirrors the crate pretty-printer `fmt_expr`
(`sol/src/format.rs`) exactly, so the text it embeds re-parses to an
equal expression.

The graph editor lets the user manually break an inline expression
apart into smaller nodes if they want a visual view — that is a
power-user feature, not an import responsibility.

Operator handling (the full canonical operator set —
`sol/src/ast.rs::BinOp` / `UnaryOp`):

| AST op | Surface |
|---|---|
| `Add / Sub / Mul / Div` | `+ - * /` |
| `Eq / Ne / Lt / Gt / Le / Ge` | `== != < > <= >=` |
| `And / Or` | `&& \|\|` |
| Unary `Neg / Not` | `- !` |

There are no bitwise operators in canonical SOL (no `& | ^ << >> ~`),
so the importer never emits them. Enum variants are printed
`Enum::Variant`. Float literals always carry a decimal point in the
printed form (`1.0`, not `1`) so the re-parsed SOL keeps its Float
type.

## Limitations that matter for round-trip

1. **Expression formatting** — the printer always parenthesizes binary
   ops. `a + b * c` round-trips as `(a + (b * c))`. Equivalent
   semantically; not byte-identical.
2. **Iterator types** — the importer cannot infer the element type of
   an arbitrary array expression at import time. Defaults to `any`;
   the user can retype.
3. **Comments** — dropped. The canonical AST has no comment nodes, so
   they cannot survive any import.
4. **No inferred type information** — the importer runs on the
   parser's output, not on any later analysis. There is no
   type-checker in the pipeline (`sol/` has no analyzer phase), so the
   importer never has inferred types to attach. Editing the graph and
   re-emitting may produce code that needs explicit type annotations
   the original did not.
5. **Assignment statements** — not parsed today, so they never reach
   the importer (see the statements table).

## Future work

| Item | Plan |
|---|---|
| Assignment-statement parsing | Once `parse_stmt` parses `target = expr;`, the importer's existing `assign` / `fieldSet` / `indexSet` handlers light up. |
| Dedicated capability / Action node | A first-class node kind for `call("m.f", p)` / `m.f(args)` / `m::rpc(args)` so external Actions render visually instead of as inline placeholders. |
| Dedicated `emit` node | A first-class trigger/event node for `emit "x";`. |
| Source-span preservation | The crate lexer (`sol/src/lexer.rs`) tracks no spans yet; finer node-level source mapping waits on span support. The wire protocol already carries an `instruction_spans` sidecar for runtime-error mapping. |
