# 18 — SolFlow Mapping

> **Status:** Substantive (commit 5). Sourced from
> `src/graph/schema.ts`, `src/graph/factory.ts`,
> `src/graph/validate.ts`, and `src/emit/emit.ts` — the visual
> editor's own definitions of its node kinds, port shapes,
> validator, and Graph → SOL emitter.

SolFlow is a visual editor for SOL programs. A workflow on the
canvas is a graph of typed nodes connected by control-flow and
data edges; the editor's emitter walks that graph and produces the
matching `.sol` source. This chapter is the contract between the
two representations: every node kind, every port, every emission
rule.

The chapter is also the **conformance ledger** between the editor
and canonical SOL. Wherever the editor produces something the
canonical compiler doesn't accept, the mismatch is logged here and
in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 18.1 Reading conventions

Each node kind is documented in this shape:

```
Kind name (NodeKind)
  data shape           : the per-kind data variant
  input ports          : { id, name, kind, type?, required }, …
  output ports         : { id, name, kind, type?, required }, …
  emitted SOL          : what `src/emit/emit.ts` produces
  emit position        : statement / expression
  notes                : caveats, round-trip status, validator rules
```

Port `kind` is `control` or `data`. Data ports carry a `SolType`
(chapter 04). Required input ports must be satisfied by either a
wired data edge **or** a non-empty inline expression on the same
port id (the validator was updated in commit `3aab8e0` to honor
inline expressions, mirroring the emitter's precedence).

---

## 18.2 Node ↔ SOL mapping table

### Entry-point nodes

#### `start`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'start' }` |
| Input ports | (none) |
| Output ports | `next` (control, required) |
| Emitted SOL | opens the body of a `function`; not itself rendered as text |
| Emit position | statement chain entry |
| Notes | The editor's "Start" represents the implicit head of a function body. There is no SOL token for "start"; the emitter walks forward from this node into the function body |

#### `trigger`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'trigger', triggerKind, eventName, payloadSchema, samplePayload, webhookPath?, cronExpr?, httpMethod?, httpPath? }` |
| Input ports | (none) |
| Output ports | `next` (control, required), `payload` (data, `any`, optional) |
| Emitted SOL | a comment line, e.g. `# @trigger webhook event="order.received" path="/wh/orders"`, ahead of the function header |
| Emit position | function preamble |
| Notes | **Editor extension; NOT canonical SOL.** The comment is parser-tolerated as a comment but carries no language semantics. Logged as `T9001` |

### Statement-form nodes

#### `let`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'let', varName: string, varType: SolType }` |
| Input ports | `prev` (control, required); `value` (data, `varType`, required) |
| Output ports | `next` (control, required); `var` (data, `varType`, optional — stable id `var` so renaming `varName` doesn't break wired edges) |
| Emitted SOL | `let <varName>: <typeLabel(varType)> = <emitDataInput(value)>;` |
| Emit position | statement |
| Notes | Inline expression takes precedence over the wired edge (`src/emit/emit.ts:emitDataInput`). Missing `value` is satisfied by either a wired edge OR a non-empty `node.expressions['value']` — that match was added in commit `3aab8e0` after Sol Man was generating "valid-looking" let nodes whose visible text wasn't reaching emission |

#### `assign`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'assign', varName: string }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<varName> = <emitDataInput(value)>;` |
| Emit position | statement |

#### `print`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'print' }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `print(<emitDataInput(value)>);` |
| Emit position | statement |
| Notes | The emitter never produces a multi-arg `print`. This aligns with the bytecode's single-arg behavior (chapter 13 §13.1) but means the editor cannot express a `print(a, b)` form even though the parser would accept it. Generally the right behavior given T9003 |

#### `return`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'return', hasValue: boolean }` |
| Input ports | `prev` (control, required); `value` (data, `any`, required) only when `hasValue` |
| Output ports | (none) |
| Emitted SOL | `return;` or `return <emitDataInput(value)>;` |
| Emit position | statement (terminator) |
| Notes | Emitter stops walking the control chain at this node |

#### `branch` (`if` / `if-else`)

| Field | Value |
|---|---|
| Data shape | `{ kind: 'branch', hasElse: boolean }` |
| Input ports | `prev` (control, required); `cond` (data, `bool`, required) |
| Output ports | `then` (control, required), `else` (control, required) only when `hasElse`, `after` (control, optional) |
| Emitted SOL | `if (<cond>) { <then-chain> } [ else { <else-chain> } ]` |
| Emit position | statement |
| Notes | The emitter walks the `then` chain, then (if present) the `else` chain, then continues from `after`. `after`-attached chains land *after* the entire if/else closes |

#### `while`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'while' }` |
| Input ports | `prev` (control, required); `cond` (data, `bool`, required) |
| Output ports | `body` (control, required), `after` (control, optional) |
| Emitted SOL | `while (<cond>) { <body-chain> }` |
| Emit position | statement |

#### `forEach` (`for-in`)

| Field | Value |
|---|---|
| Data shape | `{ kind: 'forEach', iteratorName: string, iteratorType: SolType }` |
| Input ports | `prev` (control, required); `array` (data, `[]iteratorType`, required) |
| Output ports | `body` (control, required), `after` (control, optional), `item` (data, `iteratorType`, optional) |
| Emitted SOL | `for <iteratorName> in <array> { <body-chain> }` |
| Emit position | statement |

### Expression-form nodes

#### `binaryOp`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'binaryOp', op: BinaryOpSymbol, valueType: SolType }` |
| Input ports | `lhs` (data, `valueType`, required); `rhs` (data, `valueType`, required) |
| Output ports | `result` (data, `binaryOpResultType(op, valueType)`, optional) |
| Emitted SOL | `(<lhs> <op> <rhs>)` |
| Emit position | expression |

#### `unaryOp`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'unaryOp', op: UnaryOpSymbol, valueType: SolType }` |
| Input ports | `operand` (data, `valueType`, required) |
| Output ports | `result` (data, `unaryOpResultType(op, valueType)`, optional) |
| Emitted SOL | `<op><operand>` |
| Emit position | expression |

#### `varGet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'varGet', varName: string, resolvedType: SolType }` |
| Input ports | (none) |
| Output ports | `value` (data, `resolvedType`, optional) |
| Emitted SOL | the bare `<varName>` |
| Emit position | expression |

#### `literal`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'literal', litType: SolPrimitive, value: string }` |
| Input ports | (none) |
| Output ports | `value` (data, `{kind: litType}`, optional) |
| Emitted SOL | the value, formatted per type — `0` for empty int; `0.0` for empty float; `true`/`false` for bool; `"..."` with escapes for str; `'X'` for char |
| Emit position | expression |

#### `arrayLiteral`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'arrayLiteral', itemType: SolType, length: number }` |
| Input ports | `item:0`, `item:1`, … `item:N-1` (data, `itemType`, required, where N = `length`) |
| Output ports | `array` (data, `[]itemType`, optional) |
| Emitted SOL | `[<item:0>, <item:1>, …, <item:N-1>]` |
| Emit position | expression |

#### `structLiteral`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'structLiteral', structName: string }` |
| Input ports | one `field:<fieldName>` data port per declared field of the named struct |
| Output ports | `value` (data, `{kind: 'named', name: structName}`, optional) |
| Emitted SOL | `<structName> { <fieldName>: <value>, … }` (or `<structName> {}` if no fields) |
| Emit position | expression |

#### `fieldAccess`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'fieldAccess', structName: string, fieldName: string }` |
| Input ports | `target` (data, `named(structName)`, required) |
| Output ports | `value` (data, field's declared type, optional) |
| Emitted SOL | `<target>.<fieldName>` |
| Emit position | expression |

#### `fieldSet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'fieldSet', structName: string, fieldName: string }` |
| Input ports | `prev` (control, required); `target` (data, `named(structName)`, required); `value` (data, field's type, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<target>.<fieldName> = <value>;` |
| Emit position | statement |

#### `indexRead`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'indexRead', elementType: SolType }` |
| Input ports | `array` (data, `[]elementType`, required); `index` (data, `int`, required) |
| Output ports | `value` (data, `elementType`, optional) |
| Emitted SOL | `<array>[<index>]` |
| Emit position | expression |

#### `indexSet`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'indexSet', elementType: SolType }` |
| Input ports | `prev` (control, required); `array` (data, `[]elementType`, required); `index` (data, `int`, required); `value` (data, `elementType`, required) |
| Output ports | `next` (control, required) |
| Emitted SOL | `<array>[<index>] = <value>;` |
| Emit position | statement |

#### `enumVariant`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'enumVariant', enumName: string, variantName: string }` |
| Input ports | (none) |
| Output ports | `value` (data, `named(enumName)`, optional) |
| Emitted SOL | `<enumName>::<variantName>` |
| Emit position | expression |

#### `call`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'call', functionId: string }` |
| Input ports | `prev` (control, required); one `arg:<paramName>` per param of the resolved function |
| Output ports | `next` (control, required); `return` (data, function's return type, optional) — only when the resolved function returns non-void |
| Emitted SOL | `<funcName>(<arg>, …);` (statement) or `<funcName>(<arg>, …)` (expression, only via `return` port) |
| Emit position | statement; or expression via `return` |
| Notes | The auto-repair pass added in commit `3aab8e0` rewrites any `call` whose `functionId` doesn't resolve into a `print` placeholder so the resulting graph passes validation. See chapter 19 §19.3 |

### Annotation nodes (editor-only)

#### `note`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'note', text: string }` |
| Input / output ports | (none) |
| Emitted SOL | nothing — `emitStatement()` returns `''` for notes |
| Notes | Visual aid only |

#### `frame`

| Field | Value |
|---|---|
| Data shape | `{ kind: 'frame', title: string, width: number, height: number }` |
| Input / output ports | (none) |
| Emitted SOL | nothing |
| Notes | Visual grouping. Sol Man also uses frames to group LLM-generated regions |

---

## 18.3 Inline expressions vs. wired data edges

Every input port can be satisfied two ways:

1. **Wired data edge** — connect the source node's data output to
   this port via an edge of `kind: 'data'`.
2. **Inline expression** — set `node.expressions[portId]` to a
   non-empty string of SOL expression text.

The **emitter prefers the inline form** (`src/emit/emit.ts:emitDataInput`):

```ts
const inline = node?.expressions?.[portId];
if (inline !== undefined && inline.trim() !== '') {
  return inline.trim();
}
// fall back to the wired edge
```

The **validator** (`src/graph/validate.ts`) was updated in commit
`3aab8e0` to mirror that precedence: a required port is satisfied
by either a wired edge or a non-empty inline expression. Before
that change, every Sol-Man-generated `let amount = payload.amount`
node showed a false "missing input" diagnostic because the editor
stored the expression as inline text but the validator only looked
at edges.

The inline form is the Phase A escape hatch for fast authoring;
the wired form is intended for more complex compositions that
benefit from visualization. Tools should use whichever produces a
more readable graph; the SOL output is identical either way.

---

## 18.4 Editor extensions documented as non-SOL

| Editor concept | Canonical SOL equivalent | Status |
|---|---|---|
| `trigger` node + `# @trigger …` annotation | (none) | Editor extension; tolerated by the parser as a comment; T9001 |
| `note` / `frame` annotations | (none) | Editor-only; never appear in emitted SOL |
| `any` type marker on unresolved data edges | (none — SOL has no `any` type) | Editor-only; chapter 04 §4.1 |
| Per-node `position`, `expressions` metadata | (none) | Stored in the workflow JSON for round-trip into the editor; not part of SOL itself |

A SOL file produced by the editor and edited by hand will lose
these concepts on its next visit unless the tool also preserves
the workflow JSON.

---

## 18.5 Round-trip status

| Direction | Status |
|---|---|
| Graph → SOL (emit) | **Implemented.** Walks the graph and produces text per the table above. |
| SOL → Graph (parse → import) | **Not implemented today.** There is no parser-side hook in the editor that turns a hand-written `.sol` into a graph. Sources opened in the editor must come *from* the editor; arbitrary SOL is not roundtrippable. |

This is recorded in the audit (`SOL_CRATE_IDE_READINESS_PLAN.md`
§1, blockers #14 – #16) as a future-work item. Until SOL → Graph
exists, the editor is **producer-only** for canonical SOL.

---

## 18.6 Validator rules

The editor's validator (`src/graph/validate.ts`) is the source of
truth for "is this graph emittable?". It mirrors the analyzer's
rules where the language requires them, and adds a few editor-
specific checks where the editor's structure goes beyond the
language. The full diagnostic catalogue lives in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

Per-node check summary:

| Node kind | Editor-side check (in addition to required-port satisfaction) |
|---|---|
| `structLiteral`, `fieldAccess`, `fieldSet` | struct name must resolve in `workflow.structs` |
| `fieldAccess`, `fieldSet` | field name must be set |
| `enumVariant` | enum name must resolve; variant name must be set |
| `call` | `functionId` must be set and must resolve in `workflow.functions` |
| `assign` | `varName` must be set |
| `varGet` | `varName` must be set; should resolve in the function's `let` / param scope |

Type-mismatch warnings fire on wired data edges whose source and
target port types disagree (`typeEqual` check).

---

## 18.7 Where the editor can emit invalid SOL

The editor's validator is structural — it checks port shapes,
symbol references, and basic type compatibility on wired data
edges. It does **not** validate the *content* of inline
expression strings or check that the program's emitted SOL
actually parses. Several paths exist where a graph that passes
SolFlow validation produces SOL that the canonical compiler
rejects.

### Inline expression strings are passed through verbatim

The validator (`src/graph/validate.ts:82–96`) checks only that a
required port has *either* a wired edge *or* a non-empty
`node.expressions[portId]` string. It never lints the string's
syntax, type, or content. The emitter
(`src/emit/emit.ts:emitDataInput`) inserts the string as-is into
the output SOL.

A node with `expressions['value'] = "if true { 1 } else { 2 }"`
passes validation and emits:

```sol
let x: int = if true { 1 } else { 2 };
```

SOL has no `if`-expression form (chapter 07 §7.1 — `if` is a
statement). The compiler parses this as `let x: int = if …` and
calls `expression()`, which calls `primary()`, which doesn't
recognize the `if` keyword as a primary form → parser exit with
`not an expressionable token: If`.

**Recommendation:** The validator should at minimum reject inline
expressions containing SOL keywords that aren't valid in
expression position (`if`, `else`, `while`, `for`, `let`,
`return`, `struct`, `enum`, `import`, `fn`, `ext`, `as`).
A stronger fix would route inline expressions through a real SOL
expression parser. Logged as **T9018**.

### Literal node value text is unchecked

`literal` nodes carry a free-form `value: string` plus a
`litType: SolPrimitive`. The formatter
(`src/emit/emit.ts:formatLiteral`) does light type-aware
formatting (`0` for empty int, `0.0` for empty float, escape
double-quotes for strings) but never validates the value text
against the type:

| `litType` | User typed | Emitted SOL | Compiler verdict |
|---|---|---|---|
| `int` | `"42"` | `42` | OK |
| `int` | `"0xFF"` | `0xFF` | Parse error: lexer accepts `0`, then sees `x` as identifier start — produces `0` and `xFF` as separate tokens |
| `int` | `"hello"` | `hello` | Parse error or semantic error depending on context |
| `int` | `"3.14"` | `3.14` | Parses as a float literal; subsequent expression context determines whether it compiles |
| `str` | `"foo"` | `"foo"` | OK |
| `bool` | `"yes"` | `false` | Silently coerced to `false` (any value ≠ `"true"` → `false`) |
| `char` | `""` | `' '` | Single space char — unintended but compiles |

Each of these is a path where the editor emits parser-valid or
parser-invalid SOL with no warning. Logged as **T9021**.

### Apply-anyway produces `/* missing */` placeholders

When the user clicks "Apply draft with errors" in the Sol Man
modal (chapter 19), the validator's errors are bypassed but the
emitter still runs. For each missing required input, the emitter
inserts the literal string `/* missing */` (`src/emit/emit.ts:324`)
and pushes a warning.

`/* missing */` is a SOL block comment that the lexer consumes as
trivia (`lexer.rs:319–328`). The parser then sees the surrounding
text as if the missing port had no content — e.g.:

- `let x: int = /* missing */;` reduces to `let x: int = ;`,
  which is parse error E0001.
- `if /* missing */ { … }` reduces to `if { … }`, which is also
  a parse error (the parser tries to call `expression()` and
  sees `{`).
- `print(/* missing */);` reduces to `print();`, which the
  parser accepts (empty arg list), but then bytecode emission
  reaches `print` with `args.is_empty()` → no Print op is
  emitted at all (the `&& !args.is_empty()` guard at
  `bytecode.rs:424`). The call has no observable effect.

The first two cases fail loudly at compile time — acceptable.
The third case silently no-ops, which is a worse outcome than
either passing or failing. Logged as **T9020**.

### The editor's `any` type leaks into SOL

The editor uses `{ kind: 'any' }` as a SolType for unresolved
data ports. `typeLabel` emits this as the literal string `any`
(`src/graph/schema.ts:typeLabel`). When used as a type in a
`let` annotation (`let x: any = …;`), the canonical parser
treats `any` as `Type::Ident("any")` — a nominal struct
reference. The analyzer doesn't check struct existence at the
decl site (T9009), so the program compiles. Any later field
access on `x` would fail with "could not find struct `any` in
scope".

In practice this only fires when a node's data type genuinely
cannot be resolved by the editor — typically inside a
work-in-progress workflow. Logged as **T9019**.

### Cross-layer audit table

| What the editor's validator checks | What canonical SOL requires (analyzer) | Mismatch |
|---|---|---|
| Required input has edge OR non-empty inline expression | Required argument is well-typed and resolves | Validator passes nodes whose inline expression is gibberish |
| Struct exists | Same; plus literal supplies every field (analyzer's `todo!` — chapter 09 §9.2) | Both validator and analyzer omit field-coverage check |
| Enum exists; variant name is set | Same | OK |
| Call's functionId resolves | Call's name is in scope as a `function`/`ext function` | OK; validator and analyzer agree |
| `assign` has varName | Same | OK |
| `varGet` resolves in function-local scope (heuristic) | Same plus scope-aware analyzer walk | Validator's heuristic doesn't honor control-flow reachability |
| Data-edge type compatibility | Per-operator type rules (chapter 08) | Validator only checks edge endpoints; misses inline-expression type mismatches |
| (none) | `if`/`while` condition must be `bool` | Validator does not type-check conditions |
| (none) | Arithmetic operand types must match and be numeric | Validator does not type-check inline arithmetic |
| (none) | Function call argument count matches signature | Validator's port system creates exact ports, so a structural mismatch surfaces as missing-input; but doesn't validate that the call's `functionId` selects a function with the right param shape after the user changed param names |
| `varGet` resolves (warning) | Same | OK |
| "No `start` function or trigger node" (warning) | None — SOL has no formal entry rule | Warning is editor-only |

The general pattern: the editor validates **structure** (graph
shape), canonical SOL validates **content** (types, names,
syntactic well-formedness of expressions). Any path that bridges
the two — most prominently the inline expression mechanism — is
where invalid SOL can be emitted from a "valid" graph.

---

## 18.8 Sources cited in this chapter

- `src/graph/schema.ts` — `NodeKind`, `NodeData`, `Port`,
  `GraphNode`, `GraphEdge`
- `src/graph/factory.ts` — port construction per kind
- `src/graph/validate.ts` — port satisfaction rules; per-kind checks
- `src/emit/emit.ts` — Graph → SOL emission walk
- `src/sol-man/applyGraph.ts` — auto-repair pass (call → print)
- [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) — T9001 (trigger
  annotation), T9003 (print single arg)
