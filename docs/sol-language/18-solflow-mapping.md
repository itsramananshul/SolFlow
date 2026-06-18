# 18 — SolFlow Mapping

> **Status:** Substantive. Sourced from `src/graph/schema.ts`,
> `src/graph/kinds.ts`, `src/graph/factory.ts`,
> `src/graph/validate.ts`, and `src/emit/emit.ts` — the visual
> editor's own definitions of its node kinds, port shapes,
> validator, and Graph to SOL emitter. The canonical AST node set it
> targets lives in `sol/src/ast.rs`.

SolFlow is a visual editor for SOL programs. A workflow on the
canvas is a graph of typed nodes connected by control flow and
data edges; the editor's emitter walks that graph and produces the
matching `.sol` source. This chapter is the contract between the
two representations: every node kind, every port, every emission
rule, and how each maps onto a node of the canonical AST.

The chapter is also the **conformance ledger** between the editor
and canonical SOL. Wherever the editor produces something the
canonical parser does not accept, the mismatch is logged here.

---

## 18.1 The canonical AST the editor targets

The editor emits text that the canonical crate (`openprem-sol-v2`,
the `sol/` crate) parses. The relevant AST node set is in
`sol/src/ast.rs`:

- top level: `TopLevel::{Function, Struct, Enum, Workflow, Import}`
- statements (`Stmt`): `Let`, `Assign`, `If`, `While`, `For`,
  `Return`, `Expr`, `Emit`
- expressions (`Expr`): `Int`, `Float`, `Bool`, `Char`, `Str`,
  `Array`, `StructInstance`, `EnumVariant`, `Ident`,
  `MemberAccess`, `Index`, `BinOp`, `UnaryOp`, `Call`,
  `WorkflowCall`, `NamespaceCall`

The runnable unit is a `workflow "name" { ... }`; helper functions
are `fn name(params) <- RetType { ... }`. Return types use the
canonical arrow `<-` (not `->`), comments use `#`, arrays are
prefix `[]T`, and struct fields and enum variants are
`;`-separated. The editor's emitter (`src/emit/emit.ts`) follows
all of these.

The bridge between editor and language is `compiler-wasm/src/lib.rs`,
which wraps the crate and returns a stable JSON envelope. It does
NOT run a type checker; there is no semantic-analysis pass and no
numeric error-code system. See §18.6 for the real diagnostic
vocabulary.

---

## 18.2 Reading conventions

Each node kind is documented in this shape:

```
Kind name (NodeKind)
  data shape           : the per-kind data variant (src/graph/schema.ts NodeData)
  input ports          : { id, name, kind, type?, required }, …
  output ports         : { id, name, kind, type?, required }, …
  emitted SOL          : what src/emit/emit.ts produces
  emit position        : statement / expression
  AST node             : the sol/src/ast.rs construct it parses into
  notes                : caveats, round-trip status, validator rules
```

Port `kind` is `control` or `data`. Data ports carry a `SolType`
(`src/graph/schema.ts`). Required input ports must be satisfied by
either a wired data edge **or** a non-empty inline expression on
the same port id; the validator honors inline expressions,
mirroring the emitter's precedence (`emitDataInput` in
`src/emit/emit.ts`).

---

## 18.3 Node to SOL mapping table

### Entry-point nodes

#### `start`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'start' }` |
| Input ports | (none) |
| Output ports | `next` (control, required) |
| Emitted SOL | opens the body of a `workflow` or `fn`; not itself rendered as text |
| Emit position | body entry |
| AST node | none directly; marks the head of a `WorkflowDecl.body` / `FunctionDecl.body` |
| Notes | "Start" is the implicit head of a function body. There is no SOL token for "start"; `emitFunction` walks forward from the `start` (or a `trigger`) node's `next` port into the body |

#### `trigger`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'trigger', triggerKind, eventName, payloadSchema, samplePayload, webhookPath?, cronExpr?, httpMethod?, httpPath? }` |
| Input ports | (none) |
| Output ports | `next` (control, required); `payload` (data, `any`, optional) |
| Emitted SOL | a `#` comment line ahead of the header, e.g. `# @trigger webhook event="order.received" path="/webhooks/orders"` |
| Emit position | function preamble |
| AST node | none; canonical comments (`#`) are stripped by the lexer and carry no semantics |
| Notes | **Editor extension; NOT canonical SOL.** `emitTriggerComment` builds the `#` line; the lexer treats it as a comment, so it round-trips into the source but the AST has no record of it (the formatter `sol/src/format.rs` would drop it). A `trigger` may serve as the entry node in place of `start` |

### Statement-form nodes

#### `let`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'let', varName: string, varType: SolType }` |
| Input ports | `prev` (control, required); `value` (data, `varType`, required) |
| Output ports | `next` (control, required); `var` (data, `varType`, optional — stable id `var` so renaming `varName` does not break wired edges) |
| Emitted SOL | `let <varName>: <typeLabel(varType)> = <emitDataInput(value)>;` |
| Emit position | statement |
| AST node | `Stmt::Let { name, type_, value }` |
| Notes | Inline expression takes precedence over the wired edge. A missing `value` is satisfied by either a wired edge OR a non-empty `node.expressions['value']`; the validator mirrors that so a `let amount = payload.amount` typed inline does not show a false `missing-input` |

#### `assign`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'assign', varName: string }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<varName> = <emitDataInput(value)>;` |
| Emit position | statement |
| AST node | `Stmt::Assign { target: Target::Ident, value }` |

#### `print`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'print' }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `print(<emitDataInput(value)>);` |
| Emit position | statement |
| AST node | `Stmt::Expr(Expr::Call(Expr::Ident("print"), [value]))` |
| Notes | `print` is one of the four VM builtins (`print`, `len`, `to_str`, `type_name`). The emitter only ever produces a single-argument `print`. The VM's `print` is variadic, but the editor cannot express `print(a, b)`; this aligns with one value per node |

#### `return`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'return', hasValue: boolean }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) only when `hasValue` |
| Output ports | (none) |
| Emitted SOL | `return;` or `return <emitDataInput(value)>;` |
| Emit position | statement (terminator) |
| AST node | `Stmt::Return(Option<Expr>)` |
| Notes | `emitChain` stops walking the control chain at this node |

#### `branch` (`if` / `if-else`)

| Field | Value |
|---|---|
| Data shape | `{ kind: 'branch', hasElse: boolean }` |
| Input ports | `prev` (control, required); `cond` (data, `bool`, required) |
| Output ports | `then` (control, required); `else` (control, required) only when `hasElse`; `after` (control, optional) |
| Emitted SOL | `if (<cond>) { <then-chain> } [ else { <else-chain> } ]` |
| Emit position | statement |
| AST node | `Stmt::If { condition, then, else_ }` |
| Notes | Parentheses around the condition are required by the parser, and the emitter always emits them. `emitChain` walks the `then` chain, then (if present) the `else` chain, then continues from `after`; `after`-attached chains land after the entire `if`/`else` closes |

#### `while`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'while' }` |
| Input ports | `prev` (control, required); `cond` (data, `bool`, required) |
| Output ports | `body` (control, required); `after` (control, optional) |
| Emitted SOL | `while (<cond>) { <body-chain> }` |
| Emit position | statement |
| AST node | `Stmt::While { condition, body }` |
| Notes | Parentheses around the condition are required by the parser and always emitted |

#### `forEach` (`for-in`)

| Field | Value |
|---|---|
| Data shape | `{ kind: 'forEach', iteratorName: string, iteratorType: SolType }` |
| Input ports | `prev` (control, required); `array` (data, `[]iteratorType`, required) |
| Output ports | `body` (control, required); `after` (control, optional); `item` (data, `iteratorType`, optional) |
| Emitted SOL | `for <iteratorName> in <array> { <body-chain> }` |
| Emit position | statement |
| AST node | `Stmt::For { item, iter, body }` |
| Notes | The `for-in` form takes NO parentheses (unlike `if` / `while`), matching the parser |

### Expression-form nodes

#### `binaryOp`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'binaryOp', op: BinaryOpSymbol, valueType: SolType }` |
| Input ports | `lhs` (data, `valueType`, required); `rhs` (data, `valueType`, required) |
| Output ports | `result` (data, `binaryOpResultType(op, valueType)`, optional) |
| Emitted SOL | `(<lhs> <op> <rhs>)` |
| Emit position | expression |
| AST node | `Expr::BinOp(lhs, op, rhs)` |
| Notes | `op` is one of `+ - * / == != < > <= >= && \|\|`. Comparison and logical ops resolve to `bool` via `binaryOpResultType` |

#### `unaryOp`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'unaryOp', op: UnaryOpSymbol, valueType: SolType }` |
| Input ports | `operand` (data, `valueType`, required) |
| Output ports | `result` (data, `unaryOpResultType(op, valueType)`, optional) |
| Emitted SOL | `<op><operand>` |
| Emit position | expression |
| AST node | `Expr::UnaryOp(operand, op)` |
| Notes | `op` is `-` (negation) or `!` (logical not). The editor emits `<op><operand>` without parentheses; the canonical formatter would re-emit it parenthesized as `(<op><operand>)`. Both parse |

#### `varGet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'varGet', varName: string, resolvedType: SolType }` |
| Input ports | (none) |
| Output ports | `value` (data, `resolvedType`, optional) |
| Emitted SOL | the bare `<varName>` (or `/* unset */` if `varName` is empty) |
| Emit position | expression |
| AST node | `Expr::Ident(varName)` |

#### `literal`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'literal', litType: SolPrimitive, value: string }` |
| Input ports | (none) |
| Output ports | `value` (data, `{ kind: litType }`, optional) |
| Emitted SOL | the value, formatted per type by `formatLiteral` — `0` for empty int; `0.0` for empty float and `N.0` for a float lacking a dot; `true`/`false` for bool; `"..."` with `\\` and `"` escaped for str; `'X'` (first char) for char |
| Emit position | expression |
| AST node | `Expr::Int` / `Expr::Float` / `Expr::Bool` / `Expr::Char` / `Expr::Str` |

#### `arrayLiteral`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'arrayLiteral', itemType: SolType, length: number }` |
| Input ports | `item:0`, `item:1`, … `item:N-1` (data, `itemType`, required, where N = `length`) |
| Output ports | `array` (data, `[]itemType`, optional) |
| Emitted SOL | `[<item:0>, <item:1>, …, <item:N-1>]` |
| Emit position | expression |
| AST node | `Expr::Array(Vec<Expr>)` |

#### `structLiteral`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'structLiteral', structName: string }` |
| Input ports | one `field:<fieldName>` data port per declared field of the named struct |
| Output ports | `value` (data, `{ kind: 'named', name: structName }`, optional) |
| Emitted SOL | `<structName> { <fieldName>: <value>, … }` (or `<structName> {}` if no fields) |
| Emit position | expression |
| AST node | `Expr::StructInstance { name, fields }` |

#### `fieldAccess`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'fieldAccess', structName: string, fieldName: string }` |
| Input ports | `target` (data, `named(structName)`, required) |
| Output ports | `value` (data, field's declared type, optional) |
| Emitted SOL | `<target>.<fieldName>` |
| Emit position | expression |
| AST node | `Expr::MemberAccess(target, fieldName)` |

#### `fieldSet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'fieldSet', structName: string, fieldName: string }` |
| Input ports | `prev` (control, required); `target` (data, `named(structName)`, required); `value` (data, field's type, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<target>.<fieldName> = <value>;` |
| Emit position | statement |
| AST node | `Stmt::Assign { target: Target::MemberAccess, value }` |

#### `indexRead`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'indexRead', elementType: SolType }` |
| Input ports | `array` (data, `[]elementType`, required); `index` (data, `int`, required) |
| Output ports | `value` (data, `elementType`, optional) |
| Emitted SOL | `<array>[<index>]` |
| Emit position | expression |
| AST node | `Expr::Index(array, index)` |

#### `indexSet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'indexSet', elementType: SolType }` |
| Input ports | `prev` (control, required); `array` (data, `[]elementType`, required); `index` (data, `int`, required); `value` (data, `elementType`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<array>[<index>] = <value>;` |
| Emit position | statement |
| AST node | `Stmt::Assign { target: Target::Index, value }` |

#### `enumVariant`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'enumVariant', enumName: string, variantName: string }` |
| Input ports | (none) |
| Output ports | `value` (data, `named(enumName)`, optional) |
| Emitted SOL | `<enumName>::<variantName>` |
| Emit position | expression |
| AST node | `Expr::EnumVariant { enum_name, variant }` |

#### `call`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'call', functionId: string }` |
| Input ports | `prev` (control, required); one `arg:<paramName>` per param of the resolved function |
| Output ports | `next` (control, required); `return` (data, function's return type, optional) — only when the resolved function returns non-void |
| Emitted SOL | `<funcName>(<arg>, …);` (statement) or `<funcName>(<arg>, …)` (expression, only via the `return` port) |
| Emit position | statement; or expression via `return` |
| AST node | `Expr::Call(Expr::Ident(funcName), args)` |
| Notes | If `functionId` does not resolve in `workflow.functions`, the emitter falls back to the sentinel name `/* unknown */`, which does not parse. The Sol Man repair pass rewrites unresolved calls into `print` placeholders before emission (chapter 19 §19.3). Calls to external capabilities (`call("m.f", params)`, imported `m.f(args)`, namespace `m::rpc(args)`) are not produced by the Phase A graph emitter |

### Annotation nodes (editor-only)

#### `note`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'note', text: string }` |
| Input / output ports | (none) |
| Emitted SOL | nothing — `emitStatement` returns `''` for notes |
| Notes | Visual aid only |

#### `frame`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'frame', title: string, width: number, height: number }` |
| Input / output ports | (none) |
| Emitted SOL | nothing |
| Notes | Visual grouping. Sol Man also uses frames to group LLM-generated regions |

---

## 18.4 Inline expressions vs. wired data edges

Every input port can be satisfied two ways:

1. **Wired data edge** — connect the source node's data output to
   this port via an edge of `kind: 'data'`.
2. **Inline expression** — set `node.expressions[portId]` to a
   non-empty string of SOL expression text.

The **emitter prefers the inline form** (`emitDataInput` in
`src/emit/emit.ts`):

```ts
const inline = node?.expressions?.[portId];
if (inline !== undefined && inline.trim() !== '') {
  return inline.trim();
}
// fall back to the wired edge
```

The **validator** (`src/graph/validate.ts`) mirrors that
precedence: a required port is satisfied by either a wired edge or
a non-empty inline expression. Without that, every
Sol-Man-generated `let amount = payload.amount` node would show a
false `missing-input` diagnostic because the editor stores the
expression as inline text rather than as an edge.

The inline form is the Phase A escape hatch for fast authoring;
the wired form is intended for more complex compositions that
benefit from visualization. Tools should use whichever produces a
more readable graph; the emitted SOL is identical either way.

---

## 18.5 Editor extensions documented as non-SOL

| Editor concept | Canonical SOL equivalent | Status |
|---|---|---|
| `trigger` node + `# @trigger …` annotation | (none) | Editor extension; emitted as a `#` comment, stripped by the lexer |
| `note` / `frame` annotations | (none) | Editor-only; never appear in emitted SOL |
| `any` type marker on unresolved data ports | (none — SOL has no `any` type) | Editor-only; `typeLabel` emits the literal string `any`, which the parser reads as `Type::Named("any")` |
| Per-node `position`, `expressions`, `meta` metadata | (none) | Stored in the workflow JSON for round-trip into the editor; not part of SOL itself |

A SOL file produced by the editor and edited by hand keeps these
concepts only if the tool also preserves the workflow JSON. The
canonical formatter (`sol/src/format.rs`) drops comments, so a
format round-trip loses the `# @trigger` annotation.

---

## 18.6 Diagnostics: the bridge codes vs. the editor's structural checks

There are TWO distinct diagnostic systems, and neither uses a
numeric `E0xxx`/`T90xx` scheme.

**The bridge** (`compiler-wasm/src/lib.rs`) emits exactly five
codes, each with a severity / phase:

| Severity | Phase | Code | When |
|---|---|---|---|
| Error | Parser | `E_PARSE` | parse failed (the parser's plain string message) |
| Error | Codegen | `E_CODEGEN` | bytecode compilation failed |
| Error | Analyzer | `E_NO_WORKFLOW` | `run_source_json` found no `workflow` declaration |
| Warning | Runtime | `E_RUNTIME` | the VM returned `Failed(string)` or yielded an error during a run |
| Error | Internal | `ICE0001` | a panic was caught inside the wasm bridge |

The diagnostic JSON shape (severity, phase, code, message, span,
related, help) is the stable editor contract in
`src/compiler/types.ts`. Its `DiagnosticPhase` enumerates
`Lexer | Parser | Analyzer | Codegen | Runtime | Internal`; `Lexer`
is reserved and never emitted today. Runtime errors the browser
sim surfaces are only `ExtCallBlocked { function_name, url }` (a
`RemoteCall` the sim cannot fulfil) and `StepLimit { limit }`.

**The editor's validator** (`src/graph/validate.ts`) runs
structural checks on the graph BEFORE any SOL is emitted, with
kebab-case codes (not bridge codes):

`no-entry`, `unnamed-function`, `enum-first-char-collision`,
`missing-input`, `bad-inline-expression`, `unset-struct`,
`unknown-struct`, `unset-field`, `unset-enum`, `unknown-enum`,
`unset-variant`, `unset-call`, `unknown-call`, `unset-var`,
`unresolved-var`, `type-mismatch`.

`enum-first-char-collision` is the editor's hazard warning: the
canonical bytecode dispatches each enum variant by
`(first_char as i128) % 10`, so two variants whose first
characters share a mod-10 residue compare equal at runtime even
though the by-name simulator runs them correctly. The validator
surfaces this as a warning so the user is not surprised at deploy
time.

---

## 18.7 Round-trip status

| Direction | Status |
|---|---|
| Graph to SOL (emit) | **Implemented** in `src/emit/emit.ts`. Walks the graph and produces canonical text per the table above. |
| SOL to Graph (import) | Importer scaffolding exists (the `src/graph/import/` directory and the `FunctionGraph.meta.sourceLine` / `GraphNode.meta.sourceSpan` fields it populates). Hand-written arbitrary SOL is not reliably round-trippable today; treat the editor as producer-first. |

---

## 18.8 Validator rules

The editor's validator (`src/graph/validate.ts`) is the source of
truth for "is this graph emittable?". Per-node check summary
(each in addition to required-port satisfaction via edge or inline
expression):

| Node kind | Editor-side check | Code on failure |
|---|---|---|
| any function | non-empty name | `unnamed-function` |
| any required input port | satisfied by edge OR non-empty inline expr | `missing-input` |
| any inline expression | passes `lintInlineExpression` (`src/graph/expressionLint.ts`) | `bad-inline-expression` |
| `structLiteral`, `fieldAccess`, `fieldSet` | struct name set and resolves in `workflow.structs` | `unset-struct` / `unknown-struct` |
| `fieldAccess`, `fieldSet` | field name set | `unset-field` |
| `enumVariant` | enum set and resolves; variant set | `unset-enum` / `unknown-enum` / `unset-variant` |
| `call` | `functionId` set and resolves in `workflow.functions` | `unset-call` / `unknown-call` |
| `assign` | `varName` set | `unset-var` |
| `varGet` | `varName` set (warning); resolves in `let` / param / `forEach` scope (warning) | `unset-var` / `unresolved-var` |
| workflow | at least one `start` function or trigger node | `no-entry` (warning) |
| each enum | no two variants share a first character | `enum-first-char-collision` (warning) |

Type-mismatch warnings (`type-mismatch`) fire on wired data edges
whose source and target port types disagree, via the `typeEqual`
check in `src/graph/schema.ts`.

---

## 18.9 Where the editor can emit non-parsing SOL

The validator is structural — it checks port shapes, symbol
references, and basic type compatibility on wired data edges. It
does NOT parse the content of inline expression strings or verify
that the emitted program parses. Several paths exist where a graph
that passes SolFlow validation produces SOL the canonical parser
rejects.

### Inline expression strings are passed through verbatim

`emitDataInput` inserts a non-empty `node.expressions[portId]`
into the output SOL as-is. The validator's `bad-inline-expression`
lint (`src/graph/expressionLint.ts`) blocks obviously-wrong
strings (JS globals, method calls, statement keywords), but it is
not a full SOL parser. A surviving string that is not a valid SOL
expression still produces non-parsing SOL.

SOL has no `if`-expression form — `if` is a statement. An inline
`if true { 1 } else { 2 }` placed in a `let` value would emit:

```sol
let x: int = if true { 1 } else { 2 };
```

and fail at parse time, because `if` is not an expressionable
token. The lint catches the leading `if` keyword; an expression
that slips past the lint and is still malformed surfaces as a
`E_PARSE` diagnostic from the bridge.

### Sentinel strings for unsatisfied inputs

When a required data input has neither an inline expression nor a
wired edge, the emitter does not insert a SOL comment (SOL has no
block comments). It inserts a sentinel string and pushes a
warning:

| Situation | Sentinel emitted (`src/emit/emit.ts`) |
|---|---|
| missing required input | `__UNRESOLVED_INPUT__` |
| `call` whose `functionId` does not resolve | `/* unknown */` |
| `varGet` with empty `varName` | `/* unset */` |
| expression node read via an invalid output port | `/* invalid */` |

None of these parse as canonical SOL: `__UNRESOLVED_INPUT__` lexes
as a bare identifier (usually an unbound variable), and `/* … */`
lexes as `/`, `*`, identifiers, `*`, `/` — there is no block
comment in SOL, so the surrounding statement fails to parse. The
validator's `missing-input` and the kind-specific `unset-*` /
`unknown-*` codes are designed to stop a graph from reaching
emission in any of these states; the sentinels are a last-resort
fallback, not normal output.

### The editor's `any` type leaks into SOL

`typeLabel` emits the editor-only `{ kind: 'any' }` as the literal
string `any`. Used in a `let` annotation (`let x: any = …;`), the
canonical parser reads `any` as `Type::Named("any")` — a nominal
type reference. There is no analyzer to reject an unknown named
type at the declaration site, so the program parses and compiles;
a later field access on `x` would fail at runtime when the VM
cannot find a struct named `any`. This only fires when a node's
data type genuinely cannot be resolved, typically in a
work-in-progress workflow.

The general pattern: the editor validates **structure** (graph
shape, symbol references); canonical SOL is checked only by
**parsing and running** (there is no type checker). Any path that
bridges the two — most prominently the inline expression
mechanism — is where non-parsing SOL can come from a structurally
valid graph.

---

## 18.10 Sources cited in this chapter

- `sol/src/ast.rs` — the canonical AST node set the emitter targets
- `sol/src/format.rs` — the canonical pretty-printer (reference for
  exact canonical formatting; drops comments)
- `compiler-wasm/src/lib.rs` — the wasm bridge and its five
  diagnostic codes
- `src/compiler/types.ts` — the stable diagnostic JSON shape
- `src/graph/schema.ts` — `NodeKind`, `NodeData`, `Port`,
  `GraphNode`, `GraphEdge`, `typeLabel`, `typeEqual`
- `src/graph/kinds.ts` — palette catalog of node kinds
- `src/graph/factory.ts` — port construction per kind
- `src/graph/validate.ts` — port satisfaction rules; per-kind
  checks; kebab-case diagnostic codes
- `src/graph/expressionLint.ts` — inline-expression lint
- `src/emit/emit.ts` — Graph to SOL emission walk
- `src/sol-man/applyGraph.ts` — Sol Man repair pass (call to print)
