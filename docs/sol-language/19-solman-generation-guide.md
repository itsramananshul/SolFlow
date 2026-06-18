# 19 — Sol Man Generation Guide

> **Status:** Substantive. Sourced from `api/sol-man/_prompt.ts`
> (the live LLM system prompt), `api/sol-man/_validate.ts`,
> `api/sol-man/_semanticRepair.ts`, `src/sol-man/applyGraph.ts`,
> `src/sol-man/types.ts`, and `src/graph/validate.ts`.

Sol Man is the LLM-driven generator that turns a plain-English
prompt into a SolFlow graph (and, transitively, a canonical SOL
program). This chapter is the contract a generator must honor and
the recipe the prompt embeds.

The crucial design fact: **Sol Man emits a graph, never SOL
source.** The LLM produces a `GeneratedGraphSpec`
(`src/sol-man/types.ts`) using SolFlow's node vocabulary; the
client validates it, runs auto-layout, and converts it into real
`GraphNode` / `GraphEdge` objects via the same `createNode`
factory the editor uses. The canonical SOL is generated FROM that
graph by the existing emitter (`src/emit/emit.ts`, chapter 18).
The graph is the source of truth; the LLM never writes SOL.

The chapter has three parts: the hard rules the validator
enforces, the soft rules that produce readable graphs, and the
repair pass that catches common LLM failure modes before they
reach the canvas.

---

## 19.1 What the LLM may emit

The spec node kinds are a strict subset of the editor's node kinds
(`GeneratedNodeKind` in `src/sol-man/types.ts`):

```
trigger  let  assign  print  return  branch  while  forEach  call
```

Each node carries its primary input as an inline expression in a
named field, mapped (in `src/sol-man/applyGraph.ts:dataFor`) onto
the node's data port:

| Node kind | Field | Lands on port | Emits |
|---|---|---|---|
| `let` | `value` | `value` | `let <varName>: <varType> = <value>;` |
| `assign` | `value` | `value` | `<varName> = <value>;` |
| `print` | `value` | `value` | `print(<value>);` |
| `return` | `value` (when `hasValue`) | `value` | `return <value>;` or `return;` |
| `branch` | `cond` | `cond` | `if (<cond>) { … } [ else { … } ]` |
| `while` | `cond` | `cond` | `while (<cond>) { … }` |
| `forEach` | `value` | `array` | `for <iteratorName> in <value> { … }` |

`varType` and `iteratorType` come from `GeneratedPrimitive`
(`int | float | bool | str`). Edges default to
`fromPort: 'next'`, `toPort: 'prev'`, `kind: 'control'`; branch
arms use `fromPort: 'then' | 'else' | 'after'`, loop bodies use
`'body'` / `'after'`.

---

## 19.2 Hard rules (validator-enforced)

Every generated workflow must satisfy these. They are enforced by
the editor's validator (`src/graph/validate.ts`) at preview time,
and any violation gates the Apply buttons in the Sol Man modal.
The codes are kebab-case (chapter 18 §18.6), not numeric.

| # | Rule | Diagnostic code when broken |
|---|---|---|
| 1 | Every `let` has a non-empty initializer (`value`) | `missing-input` |
| 2 | Every `assign` has a non-empty `value` AND `varName` | `missing-input` / `unset-var` |
| 3 | Every `print` has a non-empty `value` | `missing-input` |
| 4 | Every `return` with `hasValue: true` has a non-empty `value` | `missing-input` |
| 5 | Every `branch` / `while` has a non-empty `cond` | `missing-input` |
| 6 | Every `forEach` has a non-empty array expression (`value`) | `missing-input` |
| 7 | Every `call` resolves to a function in `workflow.functions` | `unset-call` / `unknown-call` |
| 8 | Every inline `value` / `cond` passes the expression lint | `bad-inline-expression` |
| 9 | Branch / loop edge ports use the right `fromPort` ids (`then` / `else` / `body` / `after`) | edge dropped with a warning at apply time (`src/sol-man/applyGraph.ts`) |

`missing-input` and `bad-inline-expression` are the two codes the
Sol Man store treats as **never bypassable**: the user cannot
force-apply past them. The store gate lives in
`src/stores/sol-man.store.ts` (`hasErrors`, `previewErrors`,
`previewWarnings`; `applyAsNewWorkflow` / `insertIntoCurrent`
refuse unless there are no errors OR the caller passes
`{ force: true }`), and `src/components/SolManModal.vue` swaps the
Apply buttons for an "apply anyway" pair when bypassable errors
exist.

This is the "broken generation never reaches the canvas silently"
guarantee.

---

## 19.3 Soft rules (readability)

These produce *valid* workflows; following them produces
*readable*, *editable* workflows. They are advisory, not
validator-enforced.

| # | Rule | Why |
|---|---|---|
| 1 | Prefer named intermediate `let`s over long inline expressions | Better surface for re-editing |
| 2 | One trigger per workflow; do not use both a Start and a Trigger | Avoids ambiguity at entry resolution |
| 3 | `snake_case` (or camelCase) for variable / function / field / param names; `PascalCase` for struct / enum type names | Convention across the corpus |
| 4 | Within an enum, give every variant a distinct first character | The bytecode dispatches variants by first-character hash; see §19.6 |
| 5 | Include at least one `assumption` per workflow | Makes the LLM's decisions auditable in the preview |
| 6 | Aim for 5 to 25 nodes | Larger usually hides a clarity problem |
| 7 | Group related nodes inside `frame` annotations past ~6 nodes | Visual scannability |

---

## 19.4 The repair pass

`src/sol-man/applyGraph.ts` runs a pre-translation repair pass
(`repairSpec`) on every spec before turning it into graph nodes.

### Rewrite: unresolved `call` to `print` placeholder

The most common LLM failure: a `call` node whose `callTarget` does
not match any existing function. The validator would (correctly)
reject it (`unset-call` / `unknown-call`), but the user-visible
graph looked like a clean "send for approval" node with no hint
that it was broken.

`repairSpec` rewrites every such `call` into a `print` node whose
`value` is a humanized, quoted string literal of the action:

```text
"send_for_approval"  →  print("Send for approval")
"auto_approve"       →  print("Auto approve")
```

`humanizeActionLabel` turns the snake/camel-case target into title
text; `stringLiteral` quotes and escapes it so the emitter treats
it as a SOL string literal. The result is valid SOL, documents the
missing piece, and gives the user something to click on the canvas
and replace with a real call later. A warning is recorded for the
assumptions panel.

### Drop: edges referencing ports that do not exist

After translation, `translateSpec` drops any edge whose `fromPort`
or `toPort` does not exist on the resolved node (for example a
branch arm wired to `else` on a `hasElse: false` branch, or an
`arg:<name>` port on a call that was rewritten to `print`) and
surfaces a warning rather than mis-wiring it.

### Wrapping: every node lives inside one function

`specToWorkflow` wraps all generated nodes in a single function
named `start`. The SolFlow workflow schema does not model
top-level statements at all, so there is no path for a generated
`let` to escape a function body.

---

## 19.5 The prompt contract

The LLM's system prompt (`api/sol-man/_prompt.ts`, the
`SYSTEM_PROMPT` constant) embeds the rules above. Its key
contractual sections:

- **Expression fields are expressions, not statements.** The
  prompt forbids any SOL statement keyword inside `value` / `cond`:
  `for`, `while`, `let`, `return`, `if`, `else`, `struct`, `enum`,
  `import`, `function`, `ext`, `as`. The node's kind already says
  "this is a loop / a let / a branch"; the field holds only the
  immediate datum. For example `value: "users"` on a `forEach`
  (not `value: "for user in users"`).
- **Inline-expression grammar.** The prompt enumerates exactly
  what the `value` / `cond` grammar admits: literals, bare and
  dotted variable references, indexed access, calls of existing
  functions, struct literals, enum variants, arithmetic,
  comparison, logical, parens.
- **Actions are `print` nodes.** Anything that "does X to the
  outside world" ("send for approval", "post to slack", "update
  SAP") is a `print` node with a quoted string literal, NOT a
  `call`. A `call` is reserved for a function that already exists
  in the workflow.
- **Validation contract.** The prompt lists the hard rules as a
  checklist for the LLM to verify before responding, and marks the
  missing-input and bad-expression categories as non-bypassable.

When updating the prompt, change the relevant section here in
chapter 19 too — the two should not drift.

### Validator-aware retry

`api/sol-man/_prompt.ts` also exports two retry preambles. When a
first attempt produces non-JSON or a schema-invalid spec,
`strictRetryUserPromptPreamble(reason)` re-invokes the model with
the failure reason and a "emit pure JSON only" instruction. When
the spec parses but fails the server-side semantic lint
(`api/sol-man/_validate.ts` / `_semanticRepair.ts`),
`validatorAwareRetryPreamble(issues)` hands the model the exact
failing node id + field + suggested fix so the retry targets the
specific problem instead of regenerating the same bad pattern.

---

## 19.6 Generation patterns to avoid

These pass the structural validator but produce SOL that is
non-parsing, non-runnable, or surprising. The validator checks
structure; it does not parse or type-check inline expression
content (chapter 18 §18.9). The generator must avoid them; the
prompt instructs the LLM accordingly.

### Statement keywords inside expression fields

```text
value: "for user in users"   on a forEach   ← WRONG
value: "users"               on a forEach   ← RIGHT
```

A statement keyword in `value` / `cond` emits non-parsing SOL. The
prompt forbids it and the expression lint
(`src/graph/expressionLint.ts`) rejects it as
`bad-inline-expression`.

### Non-SOL inline syntax

```text
value: "payload.amount.toFixed(2)"   ← method call; SOL has none
value: "Math.floor(price)"            ← JS global
```

SOL's `.` is field access only; there are no methods, no JS
globals, no arrow functions, no template literals, no optional
chaining. The lint rejects these. The only builtins are `print`,
`len`, `to_str`, and `type_name`.

### Multi-value `print`

```text
print("amount:", amount)   ← discouraged
```

The editor's `print` node has a single `value` port, so the graph
naturally enforces one value per `print`. To print several values,
emit several `print` nodes:

```sol
print("amount:");
print(amount);
```

### String concatenation with `+`

The VM concatenates two strings with `+` at runtime, but the
prompt steers the LLM away from building composite strings inline,
preferring multiple `print` statements. Note that `+` on two
non-string operands is arithmetic; mixing int and float coerces to
float; division by zero is a runtime error.

### Non-primitive type spellings

```text
varType: "string"   ← WRONG (use "str")
varType: "number"   ← WRONG (use "int" or "float")
varType: "any"      ← WRONG (no SOL equivalent)
```

`GeneratedPrimitive` is `int | float | bool | str`. The parser
accepts any unknown identifier in type position as a nominal
`Type::Named` reference; there is no analyzer to reject it at the
declaration site, so a misspelled type compiles and only fails
later when the VM cannot resolve the named struct.

### Enum variants with colliding first characters

```sol
enum Status { Active; Aborted; Acknowledged; }
```

The canonical bytecode dispatches each variant by
`(first_char as i128) % 10`, so `Active`, `Aborted`, and
`Acknowledged` all map to the same residue and compare equal at
runtime, even though the editor's by-name simulator runs them
correctly. The validator warns with `enum-first-char-collision`.
Pick distinct first characters:

```sol
enum Status { Active; Disabled; Pending; }
```

### Empty / partial struct literals

The emitter only writes the fields the graph supplies. A struct
literal that omits fields produces SOL that supplies fewer fields
than the struct declares; the runtime behavior of the missing
fields is not guaranteed. The generator should look up the struct
field list from `workflow.structs` and supply a value for each.

### Apply-anyway with unsatisfied ports

If the user force-applies past a bypassable error, the emitter
still runs and may insert a sentinel (`__UNRESOLVED_INPUT__`,
`/* unknown */`, `/* unset */`, `/* invalid */`) for an
unsatisfied input. None of these parse as canonical SOL (chapter
18 §18.9). The LLM should prefer the repair-pass fallback (a
`print` placeholder with a quoted label) over leaving a port
empty.

---

## 19.7 Recipes — prompt input to graph plan

A small catalogue of common prompt shapes and the graph the
generator should produce. (Diagrams below show the node chain; the
fields shown are the inline `value` / `cond` strings.)

### "When X is over $N, send it for approval; otherwise auto-approve"

```text
trigger (webhook, event="order.received")
  → let total: float = "payload.total"
  → branch (cond: "total > 1000.0", hasElse: true)
       then  → print("Send order for approval")
       else  → print("Auto-approve order")
       after → return (hasValue: true, value: "0")
```

The two branch arms are `print` placeholders for actions. Note the
quoting: a `print` action's `value` field holds a SOL string
literal, so the JSON encodes it with escaped quotes
(`value: "\"Send order for approval\""`).

### "Every N minutes, check X and alert Y if unhealthy"

```text
trigger (timer, cronExpr: "*/5 * * * *")
  → let healthy: bool = "true"
  → branch (cond: "healthy", hasElse: false)
       then  → print("System healthy")
       else  → print("Alert on-call: system unhealthy")
```

A real health probe would be a `call` to a function the user has
declared; until that exists, a `let healthy: bool = true`
placeholder keeps the flow valid and runnable.

### "When an event happens, validate, then act"

```text
trigger (event, eventName: "employee.created")
  → let email: str = "payload.email"
  → print("Provision Slack account")
  → print("Provision GitHub account")
  → print("Provision Notion account")
  → print(email)
```

Each provisioning step is a `print` placeholder; the trailing
`print(email)` logs the value for audit. Phase A SOL has no
parallel-execution primitive, so the steps run sequentially.

The emitted canonical SOL for the body of such a workflow looks
like:

```sol
# @trigger event event="employee.created"
workflow "Employee onboarding" {
    let email: str = payload.email;
    print("Provision Slack account");
    print("Provision GitHub account");
    print("Provision Notion account");
    print(email);
}
```

---

## 19.8 Where the contract lives in code

| Surface | File | Role |
|---|---|---|
| LLM system prompt | `api/sol-man/_prompt.ts` | Expression-field contract; action representation; validation checklist; retry preambles |
| Server-side spec validation | `api/sol-man/_validate.ts` | Schema + semantic lint of the LLM JSON |
| Server-side semantic repair | `api/sol-man/_semanticRepair.ts` | Targeted fixes before returning the spec |
| Spec to graph + repair | `src/sol-man/applyGraph.ts` | `repairSpec` (unresolved call to print), edge drop, function wrapping |
| Shared spec types | `src/sol-man/types.ts` | `GeneratedGraphSpec`, `GeneratedNode`, request/response envelopes |
| Editor validator | `src/graph/validate.ts` | Required-port + per-node structural checks (kebab-case codes) |
| Store gate | `src/stores/sol-man.store.ts` | Preview-time validation; apply gate; `force` override |
| Preview UI | `src/components/SolManModal.vue` | Renders preview errors / warnings; gates the Apply buttons |

A change that adds a new hard rule must: add it here in §19.2; add
it to the LLM prompt's validation-contract section; implement the
check in `src/graph/validate.ts`; and update the repair pass if the
rule has an obvious automatic fix.

---

## 19.9 Sources cited in this chapter

- `api/sol-man/_prompt.ts` — the live LLM contract and retry
  preambles
- `api/sol-man/_validate.ts`, `api/sol-man/_semanticRepair.ts` —
  server-side spec validation and repair
- `src/sol-man/applyGraph.ts` — repair pass (`repairSpec`),
  `dataFor` field-to-port mapping, function wrapping
- `src/sol-man/types.ts` — `GeneratedGraphSpec` and the node /
  edge shapes the LLM emits
- `src/graph/validate.ts` — per-port required-input check and
  per-node structural checks
- `src/graph/expressionLint.ts` — inline-expression lint
- `src/emit/emit.ts` — the Graph to SOL emitter (chapter 18)
- `src/stores/sol-man.store.ts`, `src/components/SolManModal.vue` —
  the apply gate
