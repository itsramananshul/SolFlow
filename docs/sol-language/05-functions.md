# 05 — Functions

> **Status:** Substantive (commit 2). Cross-checked against
> `parser.rs:289–325`, `parser.rs:250–287`, `analyzer.rs:66–137`,
> and `vm.rs:56–86`.

SOL has a single function kind, declared with the `function`
keyword. Functions are not first-class values; there is no closure
form, no anonymous-function literal, and no method syntax. A
program is a flat collection of declared functions plus an
optional set of `ext function` declarations that the host runtime
will supply.

This chapter covers declaration form, parameter and return
semantics, the rules for calling, the forward-declaration model, the
recursion model, and the conventional `start` entry function.

---

## 5.1 Declaration

The full surface form is:

```sol
function name(p1: T1, p2: T2, …) -> R {
    // body statements
}
```

Parsed at `parser.rs:289–325`. Components:

- **`function`** keyword (`Token::Func`).
- **`name`** — an identifier. The parser rejects anything else with:
  > name expected after function keyword
- **Parameter list** in `(` `)`. Zero or more `name: type` pairs,
  comma-separated. The parser breaks on the first token after a
  parameter that isn't a comma — a trailing comma is not supported.
- **Optional return arrow** `-> T`. Omission means `Void`
  (`parser.rs:315–320`).
- **Body** — a brace-delimited block. The braces are not optional
  for functions (`block()` insists on `{` at the top of the body
  call; see `parser.rs:347–360`).

```sol
function add(a: int, b: int) -> int {
    return a + b;
}

function announce() {
    print("ready");
}

function noop() {}
```

All three above are valid; `noop` returns `Void` and has an empty
body block.

### Parameter rules

- Every parameter requires a type annotation (`parser.rs:300–308`).
- The analyzer adds each parameter to the function's *outer body
  scope* before walking the body (`analyzer.rs:114–116`). Parameters
  therefore behave exactly like local `let`s in the body's
  outermost block.
- Duplicate parameter names within the same function trip the
  `add_entry` duplicate check (`analyzer.rs:50–53`):
  ```
  error: redefinition of `<name>`
  ```

### Return-type rules

- `-> R` is parsed verbatim into `Ast::DeclFunc.ret`.
- **The analyzer does not currently verify that the function's
  body returns a value of type `R`.** The relevant check is
  commented out at `analyzer.rs:120–132`. A function declared
  `-> int` whose body never returns, or returns a `str`, will
  compile and run; reading off the top of the stack at the end can
  produce arbitrary values.
- Plan accordingly: treat return-type validation as the author's
  responsibility today. The audit doc records this as a high-impact
  IDE-UX gap (`SOL_CRATE_IDE_READINESS_PLAN.md` §1, blocker #18).

### What you cannot put in a function declaration

| Construct | Reason |
|---|---|
| `fn` (instead of `function`) | The keyword is `function` only |
| Generic parameters (`function f<T>(…)`) | The lexer has no `<` token in declarator position; `<` is the binary less-than operator |
| Default parameter values | The grammar requires `name: type`, with no `=` permitted |
| `export function` | No `export` keyword exists; treat any source that uses it as broken |
| `pub` / visibility modifiers | None exist; all top-level functions are visible to the analyzer's global table |

---

## 5.2 Calling

A call is parsed in primary expression position
(`parser.rs:668–681`):

```sol
add(1, 2)
announce()
do_thing(some_var, helper(x))
```

The arguments are comma-separated expressions. Empty argument
lists are fine. A trailing comma is not supported.

### Argument evaluation order

The bytecode emits each argument expression in source order, then
emits the call instruction. The VM `Call` op pops the arguments off
the stack into the new call frame (`vm.rs:56–67`). In practice this
means **left-to-right evaluation** — but the language spec does
**not** today guarantee evaluation order; treat it as
implementation-defined unless a fixture asserts otherwise.

### Argument count and type checking

`analyzer.rs:391–404`:

- The number of arguments must equal the number of declared
  parameters. Otherwise:
  > function `<name>` expects N arguments but received M
- Each argument's type must match the corresponding parameter's
  type per `type_eq` (chapter 04 §4.6). Otherwise:
  > function `<name>` expected `<T>` in position `<i>` but was passed `<S>`

There is **no overloading**. There is **no implicit conversion**.

### Calling unknown functions

If the name does not resolve to a function symbol the analyzer
prints:

```
attempting to make a function call on an undefined name `<name>`
```

If the name resolves but isn't a function symbol:

```
attempting to make a function call on a non-function type: `<name>`
```

(`analyzer.rs:376–388`).

---

## 5.3 Returning

Two forms (`parser.rs:475–486`):

```sol
return;
return expr;
```

A bare `return;` produces `Type::Void` at the analyzer level. A
`return expr;` walks `expr` to determine its type, but — per §5.1 —
the analyzer does not compare that against the declared return type
today.

Returns are only legal inside function bodies. The analyzer tracks a
`can_return` flag, set to `true` on entry into `DeclFunc.body` and
restored on exit (`analyzer.rs:118–133`). A `return` outside any
function body (e.g. at the top level) trips:

```
illegal return statement
```

Returns are legal inside `if` / `while` / `for-in` bodies; they
exit the surrounding function immediately. Code after a `return` in
the same block is unreachable; the analyzer does not warn about
unreachable code today.

---

## 5.4 External functions (`ext function`)

```sol
ext function fetch_orders(query: str) -> str;
```

Parsed at `parser.rs:250–287`. Differences from a normal `function`:

- The keyword `ext` precedes `function`.
- **No body**; the declaration ends with `;`.
- Otherwise the parameter and return-type syntax is identical.

At the analyzer level (`analyzer.rs:84–89`), `ext function`
declarations are registered exactly like regular ones: a function
symbol is added to the global scope with the declared signature.
The call-site type rules (§5.2) are identical for both kinds. A
caller cannot — and does not need to — distinguish whether the
target is local or external.

The host-runtime wiring that maps an `ext function` name to a real
implementation is documented in
[chapter 12](./12-imports-and-controllers.md).

---

## 5.5 Forward declarations and recursion

The analyzer runs in two passes (`analyzer.rs:80–98`):

1. **Pass 1.** Walks every top-level `DeclFunc` and `DeclExtFunc`
   and registers its signature in the global type table.
2. **Pass 2.** Walks each declaration's body and type-checks it.

Because every signature is registered before any body is checked,
**every order of declarations works**:

- Forward declaration: function `a` defined before function `b` can
  call `b`. Demonstrated by `fwdecl.sol`.
- Self-recursion: a function may call itself directly. Demonstrated
  by `test_func.sol`.
- Mutual recursion: two functions may call each other in either
  order. Verified by reading the analyzer's two-pass design; no
  fixture exercises this pattern, so it is *Confirmed* by source
  but lacks a dedicated test case.

### Duplicate function names

`analyzer.rs:50–53` rejects a duplicate insert into the current
scope. Two functions sharing a name at the top level produce:

```
error: redefinition of `<name>`
```

Demonstrated by `error_semantic3.sol`.

---

## 5.6 The conventional entry function: `start`

A SOL session is loaded by the host runtime, compiled to bytecode,
and then a single function is invoked to begin execution. By
convention that function is named `start` and is the only function
whose return value the host typically inspects.

There is no parser-level rule that mandates `start` (`parser.rs`
does not special-case the name), but the host runtime that loads
the session selects an entry function by name; the conventional
name observed in every positive fixture is `start`. Treat `start`
as a strong convention.

A typical entry function shape:

```sol
function start() -> int {
    // body
    return 0;
}
```

A few fixtures (`retest.sol`, `s1.sol`) declare `start` without a
trailing `return`; this compiles today because the analyzer doesn't
enforce a return path, but the resulting top-of-stack value is
undefined. Idiomatic SOL ends `start` with an explicit `return 0;`.

---

## 5.7 Common mistakes

| Pattern | What happens |
|---|---|
| `function start { … }` | Parse error: parser expects `(` after the function name |
| `function start() -> { … }` | Parse error: `parse_type` cannot parse `{`; the message is "`LCurly` is not valid in a type specifier" |
| Calling `print` with the wrong type | None; `print` accepts any arg types (chapter 13) |
| Recursive function with no base case | Type-checks fine; stack-overflows at runtime (uncaught) |
| `ext function f();` without `-> T` | Returns `Void`; calls of `f()` in an expression position will compile but their value is unusable |
| Calling a struct as a function (`Point(1, 2)`) | The name resolves to a struct symbol, not a function. The analyzer prints:<br>`attempting to make a function call on a non-function type: 'Point'` |

---

## 5.8 Sources cited in this chapter

- `parser.rs:289–325` — function declaration
- `parser.rs:250–287` — `ext function` declaration
- `parser.rs:475–486` — return statement
- `parser.rs:668–681` — function call expression
- `parser.rs:347–360` — function body block
- `analyzer.rs:50–53` — duplicate-name diagnostic
- `analyzer.rs:80–98` — two-pass forward-declaration design
- `analyzer.rs:113–137` — function body analysis
- `analyzer.rs:120–132` — *commented-out* return-type check
- `analyzer.rs:340–408` — call type-checking
- `analyzer.rs:468–476` — return analysis
- `vm.rs:56–67` — call entry and frame setup
- Fixtures: `fwdecl.sol`, `test_func.sol`, `error_semantic3.sol`,
  `retest.sol`, `s1.sol`, `jjsi.sol`
