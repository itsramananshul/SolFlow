# 19 — Sol Man Generation Guide

> **Status:** Substantive (commit 5). Sourced from
> `api/sol-man/_prompt.ts`, `src/sol-man/applyGraph.ts`,
> `src/graph/validate.ts`, and the bug-investigation that produced
> commit `3aab8e0`.

Sol Man is the LLM-driven generator that turns a plain-English
prompt into a SolFlow graph (and, transitively, a SOL program).
This chapter is the contract a generator must honor and the
recipe an LLM prompt should embed.

The chapter has three parts: hard rules the validator enforces,
soft rules that produce readable graphs, and the repair pass that
catches common LLM failure modes before they reach the canvas.

---

## 19.1 Hard rules (validator-enforceable)

Every generated workflow must satisfy these. Each rule is enforced
by the editor's validator (`src/graph/validate.ts`) and any
violation gates the Apply buttons in the Sol Man modal.

| # | Rule | Diagnostic when broken | Underlying language reason |
|---|---|---|---|
| 1 | Every `let` has a non-empty initializer (`value`) | `let: missing input "value"` | [04 §4.2.1](./04-types.md), [06 §6.1](./06-variables-and-scope.md) |
| 2 | Every `assign` has a non-empty `value` AND `varName` | `assign: missing input "value"` / `assign: no target variable` | [06 §6.2](./06-variables-and-scope.md) |
| 3 | Every `print` has a non-empty `value` | `print: missing input "value"` | [13 §13.1](./13-builtins-and-stdlib.md) |
| 4 | Every `return` with `hasValue:true` has a non-empty `value` | `return: missing input "value"` | [05 §5.3](./05-functions.md) |
| 5 | Every `branch` / `while` has a non-empty `cond` of `bool`-shaped expression | `branch: missing input "condition"` / `while: missing input "condition"` | [07 §7.1, 07 §7.2](./07-control-flow.md) |
| 6 | Every `forEach` has a non-empty `value` (the array expression) | `forEach: missing input "array"` | [11](./11-arrays.md) |
| 7 | Every `call` resolves to a known function (`function`, `ext function`, or one declared in the workflow) | `call: no function selected` / `call: target function not found` | [05 §5.2](./05-functions.md), [12 §12.1](./12-imports-and-controllers.md) |
| 8 | No declarations duplicate names within the same scope | `error: redefinition of <name>` | [05 §5.1](./05-functions.md), [06 §6.4](./06-variables-and-scope.md) |
| 9 | Struct literals supply every declared field | (no specific diagnostic today — but field omission yields zero at runtime; see chapter 09 §9.2) | [09 §9.2](./09-structs.md) |
| 10 | Branch / loop edge ports use the correct `fromPort` ids — `then` / `else` / `body` / `after` | `Edge … referenced port "<id>" which doesn't exist` | [18 §18.2](./18-solflow-mapping.md) |

These rules are *gates*, not preferences. A graph that fails any
of them must not be silently applied to the user's canvas. The
Sol Man store enforces this:

- After the LLM responds, the store builds the prospective
  workflow and runs `validateWorkflow` on it
  (`src/stores/sol-man.store.ts`).
- The store exposes `hasErrors` / `previewErrors` /
  `previewWarnings` / `previewWarningsDiagnostics`.
- `applyAsNewWorkflow` and `insertIntoCurrent` refuse to apply
  unless either there are no errors OR the caller passes
  `{ force: true }`.
- The modal swaps Apply buttons for an "Apply draft with errors"
  / "Insert draft anyway" pair when errors exist.

This is the "broken generation never reaches the canvas silently"
guarantee.

---

## 19.2 Soft rules (readability)

These produce *valid* workflows; following them produces
*readable*, *editable* workflows. They are not validator-enforced.

| # | Rule | Why |
|---|---|---|
| 1 | Prefer named intermediate `let`s over long inline expressions | Editor surface and re-edit ergonomics improve |
| 2 | One trigger per workflow | More than one creates ambiguity at the host's entry-resolution step |
| 3 | Use `snake_case` for variable / function / field / parameter names; `PascalCase` for struct / enum type names | Convention observed across the corpus; chapter 17 |
| 4 | Choose first characters carefully for enum variants — no two within an enum should share a first character | Until T9002 is fixed, same-first-character variants collide at runtime |
| 5 | Provide at least one `assumption` per generated workflow when the prompt is under-specified | Makes the LLM's decisions auditable in the preview |
| 6 | Aim for 5 – 25 nodes; smaller is fine if the intent is genuinely small | Anything larger usually hides clarity problems |
| 7 | Group related nodes inside `frame` annotations when the workflow has more than ~6 nodes | Visual scannability |
| 8 | Place every `ext function` declaration at the top of the file (when emitting source) | Convention; chapter 17 §17.7 |

---

## 19.3 The repair pass

Sol Man's `applyGraph.ts` runs a *pre-translation repair pass* on
every LLM-generated spec before turning it into real graph nodes.
The repair pass is the safety net for the LLM's most common
failure modes.

### Rewrite: unresolved `call` → `print` placeholder

The historical failure mode that motivated the pass: the LLM
emits a `call` node whose `callTarget` doesn't match any existing
function. The validator would (correctly) fail this with `no
function selected`, but the *user-visible* graph showed a clean
"send for approval" node — there was no hint that the underlying
representation was broken.

The repair pass rewrites every such `call` into a `print` node
whose `value` is a humanized string literal of the action, and
records a warning explaining what changed:

```text
"send_for_approval"  →  print("Send for approval")
"auto_approve"       →  print("Auto approve")
```

The string-literal expression makes the resulting graph valid SOL,
explicitly documents the missing piece, and gives the user
something to click on the canvas to replace with a real call later.

### Drop: edges that reference ports that don't exist on the
resolved nodes

Common after the call-to-print rewrite: an edge that targeted the
`arg:<name>` port of a (now-replaced) call node points at a port
the `print` node doesn't have. The repair pass drops these edges
and surfaces a warning rather than silently mis-wiring them.

### Add (planned, not yet active)

Future repair-pass extensions: zero-pad missing struct fields with
explicit `0` / `""` literals; coerce inline expressions to the
right port type by inferring from context. Until those land, the
generator should follow the hard rules.

---

## 19.4 The prompt contract

The LLM's system prompt (`api/sol-man/_prompt.ts`) embeds the
hard-rules and soft-rules above. Key contractual paragraphs:

- **Inline-expression contract.** Tells the LLM where each
  expression field lives: `let → value`, `assign → value`,
  `print → value`, `return → value`, `branch → cond`,
  `while → cond`, `forEach → value`. This is the prompt-side
  mirror of the validator's required-port check.
- **Action representation.** Instructs the LLM that anything that
  looks like "send for approval" or "post to slack" or "update
  SAP" should be a `print` node with a string literal, **not** a
  `call` node — because the workflow does not yet have a function
  to call. Calls are reserved for functions that already exist in
  the workflow's `functions` array.
- **Validation contract.** Lists the hard rules as a checklist
  for the LLM to verify before responding.

When updating the prompt, change the relevant section here in
chapter 19 too — the two should not drift.

---

## 19.5 Failure modes observed in practice

| Failure | Symptom | Fix |
|---|---|---|
| Empty `call()` nodes | Validator fires `call: no function selected` after generation. The user sees a clean "Send order for approval" node in preview but four red diagnostics. | Repair pass rewrites to `print(<label>)`. Logged as the bug that produced commit `3aab8e0`. |
| Visible expression text not stored on the node | `let amount = payload.amount` shows on the node but the validator complains "missing input `value`". | Two fixes — the validator now treats inline expressions as satisfying required ports; the LLM is instructed via the prompt to put the expression in `node.value`, not just in the node label. |
| Branch condition stored as label instead of `cond` | `branch condition amount > 1000` shows on the node but the validator fires "missing input `condition`". | Same as above — prompt now explicit about `branch → cond` mapping. |
| Multi-arg `print` | LLM generates `print(label, amount)` thinking SOL supports it. | Don't. Use two `print` calls. Documented in chapter 13 §13.1 and the prompt's "actions" section. |
| Two enum variants starting with the same letter | Compiles, runs, then the comparison silently mismatches due to T9002. | Make first characters distinct (chapter 17 §17.1). The repair pass does not catch this today; flag it in the LLM prompt's quality rules. |
| Calling an undeclared `ext function` | Compile-time fail-fast in the bytecode emitter (T9004). | Declare the `ext function` at the top of the workflow, or use a `print` placeholder. |
| String concatenation via `+` | Analyzer rejects (E1006). | The LLM should NOT emit `str + str`. If multiple values need to print together, emit multiple `print` statements (or `ext function format(…) -> str;`). |

---

## 19.6 Recipes — prompt input → graph plan

A small catalogue of common prompt shapes and the graph the
generator should produce.

### "When X is over $N, send it for approval; otherwise auto-approve"

```text
trigger (manual or webhook)
  → let amount: float = payload.amount
  → branch (cond: amount > 1000.0)
       then  → print("Send order for approval")
       else  → print("Auto approve")
```

Both branch arms are `print` placeholders for actions. The
generator must:

- emit `let.value = "payload.amount"`
- emit `branch.cond = "amount > 1000.0"`
- emit two `print` nodes with `value = "\"Send order for approval\""`
  and `value = "\"Auto approve\""` respectively (note the JSON
  escaping — the SOL value is a string literal, so it must be
  quoted)

### "Every N minutes, check X and alert Y if unhealthy"

```text
trigger (timer, cronExpr: "*/N * * * *")
  → ext function check_health() -> bool  (declared at top of file)
  → branch (cond: health == false)
       then  → print("Alert on-call: system unhealthy")
       else  → print("System healthy")
```

### "When event happens, validate input, update system, notify team"

```text
trigger (event, eventName: "<name>")
  → let payload_ok: bool = validate(payload)
  → branch (cond: payload_ok)
       then  → ext call update_system(payload)
              → print("Notify finance: update succeeded")
       else  → print("Notify finance: validation failed")
```

The `ext function update_system(payload: ...) -> ...;` and
`ext function validate(payload: ...) -> bool;` must be declared at
the top of the workflow. If the LLM is unsure of the signatures,
the repair pass converts unresolved calls into `print`
placeholders so the workflow still validates.

---

## 19.7 Where the contract lives in code

| Surface | File | Role |
|---|---|---|
| LLM system prompt | `api/sol-man/_prompt.ts` | Inline-expression contract; action representation; validation checklist |
| Repair pass | `src/sol-man/applyGraph.ts` | Pre-translation rewrite of unresolved calls; edge drop; warning collection |
| Validator | `src/graph/validate.ts` | Required-port + per-node semantic checks |
| Store gate | `src/stores/sol-man.store.ts` | Preview-time validation; `applyAsNewWorkflow` / `insertIntoCurrent` gate; `force` override |
| Preview UI | `src/components/SolManModal.vue` | Renders `previewErrors` / `previewWarnings`; swaps Apply buttons for "Apply draft with errors" when errors exist |

A future change that adds a new hard rule must:

1. Add the rule to chapter 19 §19.1.
2. Add it to the LLM prompt (the validation-contract section).
3. Implement the validator check.
4. Add the diagnostic to [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).
5. Update the repair pass if the rule has an obvious automatic fix.

Skipping any of those leaves the surface inconsistent — the LLM
will keep producing the broken pattern, the validator will reject
it, and the user will hit the failure without context.

---

## 19.8 Dangerous-but-validating LLM outputs

The validator gates broken graphs from silently reaching the
canvas (§19.1), but it operates on **structure** rather than
**content** (chapter 18 §18.7). A spec that passes
`validateWorkflow` can still produce SOL whose runtime behavior
is unstable, undefined, or dangerous. This section catalogues
generation patterns the LLM should avoid even when each one
passes the validator.

### 19.8.1 Top-level `let` is a runtime panic (T9014)

The simplest dangerous shape:

```text
[var: g: int = 42]                       ← top-level let
[function start]
  [return varGet(g)]
```

This passes the validator (the `let` has its initializer, the
`varGet` resolves by name in `variableResolves`'s heuristic).
The emitted SOL is:

```sol
let g: int = 42;
function start() -> int {
    return g;
}
```

At runtime, the program **panics** with `index out of bounds` on
the first `LoadLocal(0)` inside `start`'s body. See chapter 06
§6.1 and T9014.

**Hard rule for LLM generation:** Never emit a top-level
`let`. Every variable declaration must live inside a function
body. If the user prompt asks for "a constant", produce it as a
function-local `let` in `start` or in each consumer.

### 19.8.2 Enum variants with colliding first characters (T9002)

```sol
enum Status { Active, Aborted, Acknowledged }
```

All three variants hash to `5` at runtime (`'A' % 10`).
`if status == Status::Active` matches all three. The validator
passes; the runtime silently dispatches wrong.

**Hard rule:** Within any one enum, every variant must start
with a distinct first character. If the prompt's natural names
collide, prefix one of them: `Active` / `Disabled` instead of
`Active` / `Aborted`.

### 19.8.3 Multi-arg `print` produces silent data loss (T9003)

```sol
print("order:", order_id, "amount:", amount);
```

Only the first argument (`"order:"`) is emitted at the bytecode
level. The `order_id` and `amount` values are evaluated as
discarded expressions (the surrounding statement has the
implicit `Pop`), so any side effects in them happen, but the
visible output omits them.

**Hard rule:** One value per `print` call. To print multiple
values, emit multiple `print` statements:

```sol
print("order:");
print(order_id);
print("amount:");
print(amount);
```

The SolFlow `print` node only has one `value` port, so the
editor-generated graph naturally enforces this. The Sol Man
prompt should produce one `print` node per value.

### 19.8.4 String concatenation is unreachable from source (T9005)

```sol
let label: str = "order: " + order_id_str;
```

The analyzer rejects this with `arithmetic operation Plus not
supported for type String`. The bytecode has a `ConcatStr` op
but no syntax reaches it.

**Hard rule:** Don't emit string concatenation. To build a
composite string, declare an `ext function` that the host can
implement:

```sol
ext function format_order(prefix: str, id: int) -> str;
```

Or — if all you want is print-time interleaving — use multiple
`print` statements per §19.8.3.

### 19.8.5 Forward calls returning non-int print as numbers (T9015)

```sol
function start() -> int {
    print(get_label());   // forward call
    return 0;
}

function get_label() -> str { return "hello"; }
```

`print(get_label())` dispatches via `Inst::PrintInt` because
`fn_returns` doesn't have `get_label` registered yet at the
point `start`'s body is emitted. The heap index of the string
prints as a decimal number.

**Hard rule:** Order functions such that every callee appears
*before* its first call site in the emitted SOL. The editor's
graph ordering doesn't directly control this; the emitter walks
functions in workflow.functions array order. The generator
should sort the functions array so that leaf helpers come first
and `start` comes last.

### 19.8.6 Inline expressions with non-SOL syntax (T9018)

```text
[let amount: float = "payload.amount.toFixed(2)"]
```

The validator passes (the let has a non-empty `value` string).
The emitter inserts it verbatim:

```sol
let amount: float = payload.amount.toFixed(2);
```

The parser then fails because the lexer produces a method-call-
looking sequence that the parser's postfix chain can't resolve
(no method syntax exists; `.toFixed` is a member access
returning a value that isn't callable).

**Hard rule:** Every inline expression must be parseable as a
SOL expression. The Phase A grammar admits: literals, bare
variable references, dotted field access (`payload.foo.bar`),
indexed access (`arr[i]`), function calls (`f(a)`), comparison /
arithmetic / logic / bitwise operators, and parenthesized
expressions. **No method calls. No string concatenation. No
ternary. No closures. No string interpolation.**

### 19.8.7 Misspelled type names (T9009)

```sol
let name: string = "evan";          // BAD — `string` is treated as nominal struct ref
let amount: float64 = 0.0;           // BAD — `float64` is treated as nominal struct ref
let payload: any = lookup();         // BAD — `any` is treated as nominal struct ref
```

The parser silently accepts any unknown identifier in type
position as `Type::Ident(name)`. The analyzer doesn't validate
that the named type exists at the decl site. The program runs
through the bytecode emitter (which doesn't type-check struct
field values either), and field access later fails with
`could not find struct <name> in scope`.

**Hard rule:** Use only the five primitive type spellings:
`int`, `float`, `str`, `char`, `bool`. Never `string`, `int32`,
`float64`, `boolean`, `number`, `text`, or `any` — they all
silently degrade to nominal type references.

### 19.8.8 Empty struct literals leak unset fields (chapter 09 §9.2)

```sol
struct Point { x: int, y: int }
let p: Point = Point {};            // both x and y default to 0 at runtime
```

The validator does not warn about missing fields. The bytecode
emitter walks the (sorted) struct layout and emits
`Inst::PushConst(ExprUndefined)` for each field not supplied by
the literal. `ExprUndefined` materializes as `0`. The program
runs; `p.x` and `p.y` are both 0; no diagnostic anywhere.

**Hard rule:** Every struct literal must supply every declared
field. The Sol Man generator should look up the struct's field
list from `workflow.structs` and emit a value for each one,
even if the value is a literal default.

### 19.8.9 Comparing across enum types passes the validator but fails at compile

```sol
enum A { X, Y }
enum B { X, Y }
let a: A = A::X;
let b: B = B::X;
if a == b { ... }    // analyzer: cannot compare mismatched types
```

The graph passes structural validation because both operands
exist. The analyzer rejects at compile time with E1008.

**Hard rule:** When generating a comparison, both sides must
resolve to the same type. The generator should track each
variable's declared type and refuse to compare across enums.

### 19.8.10 Reserved names for `ext function` (T9016)

```sol
ext function print(msg: str);
ext function rpc_request(payload: str) -> str;
```

Both pass validation and parse cleanly. But the bytecode emitter
dispatches `print` and `rpc_request` to the built-in handlers
*before* checking `ext_functions`. The user's host-bound
endpoints are silently shadowed; calls to these names go to the
built-in implementations and never reach the network.

**Hard rule:** Never name an `ext function` any of: `print`,
`rpc_request`, `rpc_response`, `rpc_name`, `rpc_args`,
`rpc_data`. Use a domain-specific name (`emit_log`,
`call_warehouse_api`, etc.).

### 19.8.11 Apply-anyway should still verify the SOL parses

When the user clicks "Apply draft with errors", the editor
applies the broken graph to the canvas. The emitter then runs
and produces SOL containing `/* missing */` placeholders
(chapter 18 §18.7 — T9020). These placeholders behave as
comments at parse time, reducing the surrounding code to a parse
error or silent no-op depending on context.

**Hard rule for generation:** If the LLM detects it cannot
honestly satisfy a required port, prefer to emit the auto-repair
fallback (a `print` placeholder with a string-literal label —
chapter 19 §19.3) rather than leave the port empty. Empty ports
are validator errors; the apply-anyway path produces dangerous
SOL.

### 19.8.12 Summary classification

By chapter-21 badge:

| Generation pattern | Resulting behavior badge |
|---|---|
| Top-level `let` (§19.8.1) | **Undefined** — panic or garbage read |
| Colliding enum first chars (§19.8.2) | **Accidental** — dispatch silently wrong |
| Multi-arg `print` (§19.8.3) | **Current-impl bug** — silent data loss |
| `str + str` (§19.8.4) | **Current-impl** — compile error |
| Forward calls in `print` (§19.8.5) | **Emergent** — silent wrong output |
| Non-SOL inline expressions (§19.8.6) | **Current-impl** — parse error |
| Misspelled types (§19.8.7) | **Accidental** — runs but field access fails later |
| Missing struct fields (§19.8.8) | **Current-impl** — silent zero-fill |
| Cross-enum comparison (§19.8.9) | **Specified** — compile error |
| Built-in name shadowing (§19.8.10) | **Current-impl** — silent dispatch to wrong handler |
| Apply-anyway with empty ports (§19.8.11) | **Emergent** — parse error or silent no-op |

The validator catches none of these. The generator must.

---

## 19.9 Sources cited in this chapter

- `api/sol-man/_prompt.ts` — current LLM contract
- `src/sol-man/applyGraph.ts` — repair pass (`repairSpec` + the
  rewrite functions)
- `src/graph/validate.ts` — per-port required-input check (updated
  to honor inline expressions)
- `src/stores/sol-man.store.ts` — preview-time validation gate
- `src/components/SolManModal.vue` — apply-button gating
- Commit `3aab8e0` for the underlying bug investigation
- Chapters 04 – 14 for the language rules each generation
  constraint maps onto
