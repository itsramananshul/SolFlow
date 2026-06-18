# SOL → Graph Import — compatibility matrix

Status as of B.7 c22 (2026-05-27).

This document records exactly what `importProgram()` (in
`src/graph/import/importer.ts`) can and cannot represent visually
when it converts a parsed SOL AST into a SolFlow `SolWorkflow`.

The matrix is **honest**: every entry reflects what the importer
actually does today, not what we'd like it to do eventually.

## Classification

| Tier | Meaning | UI signal |
|---|---|---|
| **full** | Clean visual representation; the graph is the canonical form. Round-trips cleanly. | Green "Full" pill |
| **partial** | A graph node exists, but at least one subexpression is preserved as **inline SOL text** on the node rather than as a sub-graph. Round-trips canonical-but-textually-different SOL (semantics preserved). | Blue "Partial" pill |
| **source-only** | The construct survives in source mode but produces **no graph node**. Adding a notice ensures the user knows it was set aside. | Amber "Source-only" pill |
| **unsupported** | The importer doesn't know what to do with this AST shape. Yields a placeholder `print` node carrying the original SOL text inline + a warning. Function marked degraded. | Amber "Unsupported" pill |

The headline counts on the import report are unions of these tiers
across every statement in every function.

## Top-level declarations

| SOL construct | Tier | Notes |
|---|---|---|
| `fn name(p: T) -> R { ... }` | **full** | Becomes a `FunctionGraph`. Parameters + return type carry. |
| `ext fn name(p: T) -> R;` | **source-only** | Preserved in import report counts; no `extFunction` node kind exists yet in the graph schema. |
| `struct Name { f: T, ... }` | **full** | Becomes a `StructDecl`. Field order is **alphabetical** in the imported graph (the AST's HashMap loses insertion order; sorting makes imports deterministic). |
| `enum Name { V1, V2 }` | **full** | Becomes an `EnumDecl`. Variants ordered by parser-assigned ordinal. |
| `import "path" as alias;` | **full** | Becomes an `ImportDecl`. Alias defaults to last path segment when omitted. |
| Top-level `let` | **partial** | Auto-wrapped into a synthetic `__init()` function (B.D c37). Preserves the binding through round-trip, but changes the let's scope from module-level to function-level — documented via an import notice. (Original was already broken: the graph emit pipeline can't produce module-level lets either, so any cross-function reference would fail analyzer's `SEMA_UNDEFINED_NAME`.) |

## Statements (inside function bodies)

| SOL construct | Tier | Notes |
|---|---|---|
| `let name: T = expr;` | **partial** | Becomes a `let` node. `expr` lives as inline text on the `value` port. |
| `name = expr;` (assignment) | **partial** | Becomes an `assign` node. Parser encodes this as `ExprBinary { op: 'Eq' }`; the importer handles both that and `ExprAssign`. |
| `a.b = expr;` (field set) | **partial** | Becomes a `fieldSet` node. Struct name inferred from a function-wide scan of `let varName: TypeName` declarations + params (B.D c37). Falls back to empty `structName` with a warning when inference can't pin a type (typically: assigning through a function-call-returned struct, or deeply-nested member access). |
| `a[i] = expr;` (index set) | **partial** | Becomes an `indexSet` node. `array`, `index`, `value` all live as inline expressions; `elementType` defaults to `any` (no analyzer type info at import time). |
| `print(expr);` | **partial** | Becomes a `print` node; `expr` lives as inline text on `value`. Multi-arg `print(a, b)` is rendered as `[a, b]` (graph schema has one value port). |
| `name(args);` (call) | **partial** | Becomes a `call` node if the function exists in the workflow; otherwise a placeholder `print` carrying `name(args)` as text plus a notice. |
| `return [expr];` | **partial** | Becomes a `return` node. Optional value lives inline on `value`. |
| `if (cond) { ... } [else { ... }]` | **partial** | Becomes a `branch` node + two sub-bodies wired via `then` / `else` ports. `cond` lives as inline text. |
| `while (cond) { ... }` | **partial** | Becomes a `while` node + body wired via `body`. `cond` inline. |
| `for x in expr { ... }` | **partial** | Becomes a `forEach` node + body. `expr` inline on `array`; iterator type defaults to `any` (the importer can't infer it without re-analyzing). |
| Bare `Block { ... }` at statement level | **partial** | Flattened — block statements are inlined into the enclosing flow. |

## Expressions

Expressions are **never** lifted into the graph as sub-nodes by the
importer. They're stringified via `stringifyExpr()` and embedded as
inline text on the parent statement's relevant port. This is the
deliberate design choice that lets the importer round-trip
*semantically* without trying to render arbitrary expression trees
as visual node graphs.

The graph editor lets the user manually break an inline expression
apart into `binaryOp` / `varGet` / `literal` / etc. nodes if they
want a visual view — that's a Phase A power-user feature, not an
import responsibility.

Operator handling:

| AST op | Surface | Notes |
|---|---|---|
| `Plus / Dash / Star / Slash` | `+ - * /` | |
| `EqEq / BangEq / MoreThan / LessThan / MoreEq / LessEq` | `== != > < >= <=` | |
| `AmpAmp / PipePipe` | `&& \|\|` | |
| `Ampersand / Pipe / Caret / LShift / RShift` | `& \| ^ << >>` | |
| `Eq` (binary) | `=` | Assignment — importer treats this as a statement, not an expression. |
| Unary `Dash / Bang / Tilde` | `- ! ~` | |

Float literals always carry a decimal point in the printed form
(`1.0` not `1`) so the re-parsed SOL keeps its Float type.

## Limitations that matter for round-trip

1. **HashMap order** (struct fields, enum variants) — the AST serializes from `HashMap`, losing insertion order. The importer sorts to be deterministic; the graph then re-emits in sorted order. **User-authored** order won't survive an import.
2. **Expression formatting** — the printer always parenthesizes binary ops. `a + b * c` round-trips as `(a + (b * c))`. Equivalent semantically; not byte-identical.
3. **Iterator types** — the importer can't infer the element type of an arbitrary array expression at import time. Defaults to `any`; user can retype.
4. **Top-level lets** — see top-level table; lost on round-trip.
5. **`tt_arena` / scope ids** — the AST carries scope ids (`TypeTableId`) that the analyzer fills in. The importer ignores them; the resulting workflow doesn't depend on them.
6. **Compiler-level type information** — the importer runs on the parser's output, not the analyzer's. It doesn't know inferred types. Editing the graph and re-emitting may produce code that needs explicit type annotations the original didn't.

## Future work

| Item | Plan |
|---|---|
| `fieldSet` / `indexSet` node mapping | ✅ Shipped (B.D c37) with scope-based struct-name inference. |
| Top-level lets | ✅ Shipped (B.D c37) — auto-wrapped into synthetic `__init()`. |
| `ext function` graph representation | Could be a special trigger sub-kind; needs UI + semantics. |
| Source-span preservation | ✅ Shipped for the 10 high-value AST variants (B.D c35). Analyzer diagnostics + importer `meta.sourceLine` both consume them. Per-statement / per-leaf-expression spans remain coarser. |
| Per-statement node mapping for fine-grained click-to-source | Build on the c36 `instruction_spans` sidecar — would need finer-grained AST span attachment + a VM step trace. |
| `IndexMap` for ordered struct fields | Optional polish to keep user-authored field order round-trip safe. |
