# 21 — Behavior Classification

> **Status:** Substantive (commit 9). A normative reference that
> tags every notable SOL behavior with a stability badge. The
> taxonomy here exists so future implementers, future SolFlow
> contributors, and future LLM tooling have a single place to look
> when asking "can I rely on this?"

The earlier chapters describe **what** the language does. This
chapter classifies **how dependable** each behavior is.

---

## 21.1 The taxonomy

Six badges. Apply exactly one per behavior.

| Badge | Meaning | Guarantee |
|---|---|---|
| **Specified** | Documented in `SPEC.md` or in the relevant chapter as a normative rule. A conformant implementation must honor it. | Strong. Will not change between minor versions. |
| **Current-impl** | Documented behavior of the current compiler, not formally specified. A second implementation might choose differently. | Stable across patches; may change between compiler revisions. Document the choice when it changes. |
| **Accidental** | Behavior that emerges from the implementation but is not designed; it works because two unrelated mechanisms happen to align. Programs can rely on it today but probably shouldn't. | Likely to break the moment either underlying mechanism is refactored. |
| **Emergent** | Composite behavior that arises from the interaction of multiple specified or current-impl behaviors. Predictable from the components but not separately documented. | As stable as its weakest component. |
| **Undefined** | The implementation makes no promise. The result is whatever the underlying machinery happens to produce — may panic, may corrupt, may silently produce garbage. | None. Do not rely on. |
| **Unstable** | Behavior the maintainers explicitly mark as subject to change in the next compiler revision. | Will change; build defensively. |

A behavior that has different badges in different conditions
(e.g. specified for ints, undefined for arrays) is listed
separately for each case.

---

## 21.2 Lexical behavior

| Behavior | Badge | Notes |
|---|---|---|
| The fifteen keywords (`ext`, `for`, `in`, `as`, `function`, `if`, `else`, `import`, `while`, `struct`, `enum`, `let`, `return`, `true`, `false`) | **Specified** | `lexer.rs:341–356` |
| `//` line comments | **Specified** | `lexer.rs:311–317` |
| `/* … */` block comments, non-nestable | **Specified** | `lexer.rs:319–328` |
| Identifier rule: `IDENT_START IDENT_CONT*` with `IDENT_START` = Unicode `is_alphabetic()`, `IDENT_CONT` = alphanumeric or `_` | **Specified** | `lexer.rs:215, 336` |
| `_` treated as trivia outside identifiers | **Current-impl** | Drives the `1_000` and `_x` footguns (§3.1). Unstable: a future lexer revision may stop treating `_` as trivia |
| Integer literals parsed as `i128` then truncated to `i64` at runtime | **Current-impl** | `lexer.rs:383` parses `i128`; `vm.rs:143–146` operates on `i64`. The truncation point is implementation-specific |
| Integer literal overflow at runtime: wraps in release, panics in debug | **Undefined** at the language level; **Current-impl** behavior is "whatever Rust `i64` arithmetic does" | The SPEC explicitly leaves this unspecified (§7.4 of SPEC) |
| String literals carry no escape sequences | **Specified** | `lexer.rs:224–233`. A future revision could add escapes; current behavior is normative |
| Char literal reads exactly one source character then skips two | **Current-impl** | `lexer.rs:218–223`. Doesn't validate the closing quote; behavior on `'AB'` is undefined |
| Maximal-munch for two-character operators | **Specified** | `lexer.rs:238–296` |
| Lexer panics on unrecognized character via `process::exit(1)` | **Current-impl** | Compile-time error; will become a structured diagnostic when the audit's blocker #2 lands |

---

## 21.3 Parsing behavior

| Behavior | Badge | Notes |
|---|---|---|
| Top-level dispatcher accepts `ext`, `function`, `let`, `struct`, `enum`, `import`; everything else panics | **Specified** | `parser.rs:177–194` |
| Tuple types are parser-accepted with no value form | **Current-impl** | `parser.rs:227–242`. A future revision may add a tuple value literal |
| Struct literals are disabled inside `if`/`while`/`for-in` conditions, re-enabled inside `()` | **Specified** | `parser.rs:131, 394–397, 408–411, 428–431, 714–716`. This is necessary to disambiguate; cannot change without a grammar shift |
| Trailing comma in struct/enum/parameter lists is *not* accepted, but a single trailing item without a comma *is* | **Current-impl** | The parser breaks the loop on any non-comma; doesn't actively reject a trailing comma either. Behavior is "what the recursive descent does" |
| Forward references work between top-level functions (declaration order doesn't matter) | **Specified** | `analyzer.rs:80–98`, two-pass design. Necessary for any non-trivial program |
| `import` is a top-level declaration; alias becomes a global Void-typed name | **Current-impl** | `parser.rs:440–474`, `analyzer.rs:166–171`. Essentially inert today |
| `import` is also accepted in statement position (inside a function body) | **Current-impl** | `parser.rs:365`. Adds a local Void-typed slot that is never initialized at runtime. Don't use |
| `let x: int;` (no initializer) is parser-accepted | **Specified** | `parser.rs:326–345`. Semantically uninitialized; reads as `0` if it ever runs |
| `let x: int = ;` (empty initializer) is a parse error | **Specified** | `parser.rs:339, 749`. Diagnostic `E0001` |
| Missing semicolon on a statement is a parse error | **Specified** | Multiple sites; diagnostic `E0002` |
| Parser exits on first error via `process::exit(1)` | **Current-impl** | Will become "continue past recoverable errors" when blocker #2 lands |
| Type position recognizes only `int, float, str, char, bool` as primitives; anything else becomes a nominal `Ident` type | **Current-impl** | `parser.rs:198–209`. The lack of a "did you mean `str`?" check for `string` is the substance of T9009 |

---

## 21.4 Analyzer behavior

| Behavior | Badge | Notes |
|---|---|---|
| Two-pass analysis: pass 1 registers function signatures; pass 2 checks bodies | **Specified** | `analyzer.rs:80–98`. Necessary for forward references |
| Duplicate names in the same scope are rejected | **Specified** | `analyzer.rs:50–53`, diagnostic `E1002`. Applies to functions, structs, enums, top-level lets, parameters, and same-scope `let`s |
| Shadowing across nested scopes is permitted | **Specified** | `analyzer.rs:57–60`. Inner scope's binding takes precedence |
| `if` / `while` conditions must be `bool` | **Specified** | `analyzer.rs:174, 190`. Diagnostic `E1003` (using "if statement" wording for both — known quality issue) |
| `for-in`'s iterable must be an array type | **Specified** | `analyzer.rs:202–209`. Diagnostic `E1004` |
| `for-in` iteration variable lives in the *enclosing* scope, not the body block | **Current-impl** | `analyzer.rs:201–217`. Leaks the binding after the loop ends. May be tightened in a future revision (already documented as a quirk in chapter 06 §6.5) |
| Function return-type is *not* checked against the body | **Current-impl** | `analyzer.rs:120–132` (commented out). Marked as blocker #18 in the audit. Specifically: a function declared `-> int` whose body returns `str` (or nothing) compiles |
| `let` initializer is *not* type-checked against the declared type | **Current-impl** | `analyzer.rs:138–141` (the `..` in the match pattern skips the value). Marked as blocker #18 |
| Struct literal field validity is *not* checked | **Current-impl** | `analyzer.rs:499` (`todo!`). Missing fields, wrong-typed values, extra fields — all parser-accepted, all silently passed to bytecode |
| Array literal type uniformity is *not* checked | **Current-impl** | Same `todo!` fallthrough at `analyzer.rs:500` |
| Analyzer panics scope-leak on early-return error paths | **Current-impl** | Practically irrelevant because `process::exit(1)` happens immediately; would matter if errors became recoverable values |
| `print` accepts any number of arguments of any types and returns `Void` | **Current-impl** | `analyzer.rs:340–345`. The bytecode then drops everything after the first arg — see T9003 |
| `rpc_request`'s second arg must be an array; `rpc_response` takes one arg of any type | **Specified** | `analyzer.rs:346–372` |
| `&&`, `\|\|` require both operands to be `bool` | **Specified** | `analyzer.rs:273–280` |
| Bitwise ops require both operands to be `int` | **Specified** | `analyzer.rs:283–289` |
| Unary `!` is accepted on `int`/`float`/`bool` (not just `bool`) | **Current-impl** | `analyzer.rs:317–323`. Probably an oversight; idiomatic SOL uses `!` on `bool` only |
| Array index type may be `int` or `float` | **Current-impl** | `analyzer.rs:459`. The float case is almost certainly a typo |
| `type_eq` array comparison checks sizes | **Specified** | `util.rs:10–16`; `[3]int` ≠ `[5]int` |
| `type_eq` tuple comparison zip-truncates (T9007) | **Current-impl**, bug | Latent because tuples have no value form |
| `type_eq` function comparison ignores actual return types (T9008) | **Current-impl**, bug | Latent because function types aren't first-class values |
| `error: redefinition of <name>` text for every duplicate symbol | **Current-impl** | Stable diagnostic wording; programmatic consumers can match on it |

---

## 21.5 Bytecode emitter behavior

| Behavior | Badge | Notes |
|---|---|---|
| Struct fields are recorded in alphabetical order in the layout, used positionally for `NewStruct`/`GetField`/`SetField` | **Current-impl** | `bytecode.rs:126–131, 494–520`. Stable within one compilation; not stable across implementations |
| Missing fields in a struct literal emit `PushConst(ExprUndefined)` and materialize as `0` at runtime | **Current-impl** | `bytecode.rs:500`. Not warned about |
| Enum variant values are `(first_char as i128) % 10` (T9002) | **Current-impl**, bug | `bytecode.rs:538–541`. Same-first-character variants collide; explicit `= N` annotations are silently ignored |
| `print` only emits the first argument (T9003) | **Current-impl**, bug | `bytecode.rs:425`. Subsequent args are dropped |
| `print` dispatch picks the `Print*` op by the argument's inferred type; falls back to `PrintInt` for `Void` and unknowns | **Current-impl** | `bytecode.rs:423–432, 634–654` |
| Comparisons inside `print` are rendered as `Integer` (so `print(5==5)` prints `1`) | **Current-impl** | `display_type` at `bytecode.rs:634–645` |
| `for-in` is desugared into a `while` over `Inst::ArrayLen`-driven synthetic locals | **Current-impl** | `bytecode.rs:272–328`. Only path that emits `ArrayLen` |
| Function declarations are emitted inline with a `Jump`-over | **Current-impl** | `bytecode.rs:393–422`. A relocatable-function-table approach could replace this |
| Each function decl resets `self.locals.clear()` and `self.next_slot = 0` | **Current-impl** | `bytecode.rs:401–402`. Drives the top-level-let bug (T9014) |
| `find_local_offset` auto-creates a fresh local for unknown names, defaulting to `Type::Integer` | **Current-impl** | `bytecode.rs:559–578`. Defensive but masks programmer errors that the analyzer should have caught |
| Forward calls patched via `pending_calls` after full program emission | **Current-impl** | `bytecode.rs:151–157, 478–481` |
| Built-in name dispatch happens *before* `ext_functions` and local-function checks (T9016) | **Current-impl** | `bytecode.rs:423–481`. User-declared `ext function rpc_request(...)` is shadowed |
| `infer_type` falls back to `Integer` for unknown nodes (T9015) | **Current-impl** | `bytecode.rs:627–629`. Affects `print` of forward-call results and ext-call arg type inference |
| Bare expression statements emit `<expr>; Pop` (T9013) | **Current-impl** | `bytecode.rs:218–223`. Useful pattern for `f();`-style discards |
| `active_scopes: Vec<Scope>` is maintained but never read | **Current-impl** | Dead infrastructure; future cleanup |
| Missing `ext function` endpoint exits at compile time (T9004) | **Current-impl** | `bytecode.rs:457–460`. Fail-fast intentional |

---

## 21.6 Runtime / VM behavior

| Behavior | Badge | Notes |
|---|---|---|
| Stack-based interpreter | **Current-impl** | `vm.rs`. SPEC §7.1 permits a different model with the same observable behavior |
| Heap is monotonic — no garbage collection | **Current-impl** | Sustainable only because SOL programs are short-lived per session |
| Structs and arrays have reference semantics | **Current-impl** | SPEC §7.2 permits copy-on-pass with documented choice |
| Strings are heap-resident; `==` compares content via `Inst::EqStr`; `+` is unreachable from source (T9005) | **Specified** for ==/!=; **Current-impl** for the unreachable `+` | `vm.rs:255–261`, `bytecode.rs:683–687` |
| Integer arithmetic operates on `i64` | **Current-impl** | `vm.rs:143–146`. Underlying width is a compiler choice |
| Integer division by zero panics | **Specified** for "terminates"; **Current-impl** for "panic via Rust `i64`" | SPEC §7.4 |
| Float division by zero produces IEEE `inf`/`NaN` | **Specified** | SPEC §7.4 |
| `&&`/`\|\|` are NOT short-circuiting at the bytecode level | **Current-impl** | SPEC §6.2 permits either choice but requires it to be documented |
| `Ret` always pushes `0` onto the caller's stack (T9011) | **Current-impl** | `vm.rs:283–293`. Makes "missing return" cases look successful — see chapter 5 §5.1 |
| `RetVal` pops the return value, then truncates the stack to fp, then pushes the value | **Specified** | `vm.rs:295–306`. Necessary for correct returns |
| `LoadLocal` does an unchecked stack index (panics on out-of-bounds) | **Current-impl** | `vm.rs:118–122`. The panic is the only safety net |
| `StoreLocal` `0`-fills the stack up to the target offset | **Current-impl** | `vm.rs:124–131`. Silently grows the stack; can hide programmer errors |
| Struct/array/string heap-op silent no-ops on type mismatch (T9010) | **Current-impl**, bug | Future refactor should panic |
| `print` of a heap-string-index treats the index as an integer (when emitted with `PrintInt` due to fallback) | **Emergent** | Composition of the `infer_type` fallback + `PrintInt`'s integer-formatting. Produces garbage output. See T9015 |
| `ExtCall` is HTTP/1.1 only with no HTTPS, no timeout, no status check (T9012) | **Current-impl** | `vm.rs:469–579`. Will likely change; treat as unstable for production use |
| `ExtCall` silent defaults on missing/wrong-shape response fields | **Current-impl** | Programs should defensively validate ext-call results |
| Top-level `let` reads from inside a function panic on out-of-bounds or read garbage (T9014) | **Undefined** at the language level; **Current-impl** behavior is "either panic or read garbage" | The most severe latent bug documented |
| Program "ends" when `inst_ptr` runs off the end OR a top-level `Ret` with no frame is hit | **Specified** | `vm.rs:88–96, 290–292` |

---

## 21.7 SolFlow editor behavior

| Behavior | Badge | Notes |
|---|---|---|
| Graph → SOL emission walks the graph and produces text matching the per-node table in chapter 18 | **Specified** | `src/emit/emit.ts` |
| SOL → Graph (parse-and-import) is **not** implemented | **Current-impl** | A future addition; today the editor is producer-only |
| `// @trigger …` annotations emitted by the editor are parser-tolerated as comments (T9001) | **Current-impl** | Not part of canonical SOL |
| The editor's "any" type marker has no SOL equivalent | **Current-impl** | Editor-only |
| Inline expression takes precedence over wired data edge at emission time | **Specified** | `src/emit/emit.ts:emitDataInput`. Both the emitter and the validator (since commit `3aab8e0`) honor this |
| The validator gates broken graphs from silently reaching the canvas | **Specified** | `src/stores/sol-man.store.ts`. Documented as the hard rules in chapter 19 |
| The Sol Man repair pass rewrites unresolved `call` nodes into `print` placeholders | **Specified** | `src/sol-man/applyGraph.ts`. Logged in chapter 19 §19.3 |

---

## 21.8 What is guaranteed vs. NOT guaranteed

### What is guaranteed

A short list of behaviors callers can rely on through any future
revision short of an explicit major-version language change:

- The fifteen keywords and their semantics.
- The operator-precedence chain (chapter 08 §8.1).
- The operator type rules (chapter 08 §8.13).
- The two-pass forward-declaration model (chapter 05 §5.5).
- The block-scoping rule and shadowing-across-scopes rule.
- The `for-in` form being the only loop with collection
  iteration (no C-style `for`).
- The `print` semantic of "side effect that outputs followed by
  newline" — the single-argument restriction is current-impl,
  but the *output happens in source order* property is
  guaranteed.
- The contract that an `ext function` declared in source resolves
  to a host-provided endpoint, with compile-time failure when
  unresolved.
- Integer division by zero terminates the session (the *form* of
  termination is current-impl).

### What is NOT guaranteed

A short list of behaviors that callers **should not** rely on,
even when the current compiler appears to support them:

- Top-level `let` propagating its value into function bodies
  (T9014).
- Multiple `print` arguments printing more than the first
  (T9003).
- Enum variants with the same first character being distinguishable
  at runtime (T9002).
- `= N` annotations on enum variants influencing runtime values
  (T9002).
- Forward-called functions' return types being inferred at the
  call site (T9015).
- Calls to `print`, `rpc_*` being routable to user-declared `ext
  function` of the same name (T9016).
- `str + str` working (T9005 — bytecode op exists, analyzer
  rejects).
- `string` (or any other misspelling) being treated as `str`
  (T9009).
- VM ops failing loudly on type mismatch — most silently no-op
  (T9010).
- ExtCall over HTTPS, with timeouts, or with HTTP error
  reporting (T9012).
- Tuple value forms existing (parser accepts the type form;
  no value form).
- `break` / `continue` ever working (no keywords).
- Pattern matching (`match`) ever working in current compiler.
- Source spans appearing in diagnostics (planned, not yet
  implemented).
- Multiple errors per compile (planned, not yet implemented).

### What is implementation-defined

A short list of behaviors that the SPEC explicitly permits a
conformant implementation to choose differently:

- Whether `&&`/`\|\|` short-circuit (SPEC §6.2).
- Whether structs/arrays are passed by reference or by copy
  (SPEC §7.2 requires a consistent choice, not a particular
  one).
- Integer-overflow behavior (wrap, panic, saturate — SPEC §7.4).
- The exact runtime container for `int` (current is `i64`; spec
  says "at least 64-bit signed").
- The exact runtime container for strings, structs, arrays
  (current is heap-resident with reference semantics).
- Whether tail-call optimization happens.
- Whether the compiler emits structured `Diagnostic` values or
  unstructured `eprintln!` text.

### What may change

A short list of behaviors maintainers have flagged as planned
changes (per `SOL_CRATE_IDE_READINESS_PLAN.md` §1):

- Errors will become `Result`-style return values instead of
  `process::exit(1)` (blocker #2).
- `Token` and `Ast` nodes will carry source spans (blocker #3).
- `Token` / `Ast` / `Type` / `Symbol` / `Inst` will gain serde
  derives for WASM bridging (blocker #4).
- Struct field and enum variant storage will move from `HashMap`
  to an order-preserving container (blocker #5).
- The lexer will accept in-memory source bytes, not just file
  paths (blocker #6).
- Several analyzer holes (let-initializer type check,
  return-type check, struct-init check) will be filled (blocker
  #18).

### What currently works "by accident"

A short list of behaviors that produce correct-looking output
through a coincidence of two unrelated mechanisms — and would
break under refactoring:

- A function declared `-> int` whose body never returns appears
  to return `0`. This is the composition of: (a) the analyzer
  not checking return paths, and (b) `Ret` unconditionally
  pushing `0`. If either changes, the apparent success goes
  away.
- `Person { name: "evan" }` typing as a struct field of type
  `Type::Ident("string")` and being printable. This is the
  composition of: (a) the analyzer not checking field-value
  types, (b) the bytecode emitter not checking either, and (c)
  the `print` fallback to `PrintInt` for unknown types. Output
  is garbage but the program runs.
- Enum variants whose first characters happen to be unique
  producing the apparent integer-tag behavior of an iota
  algorithm. This is the composition of: (a) the bytecode hash,
  (b) the specific variant names a programmer happened to
  choose. The moment a variant is renamed, the runtime value
  changes.

---

## 21.9 Sources cited in this chapter

This chapter is a synthesis of the eight prior commit's findings.
Specific source citations live in chapters 02 – 20 and
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md); refer there when a
badge needs verification.
