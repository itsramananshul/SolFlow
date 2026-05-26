# 07 — Control Flow

> **Status:** Substantive (commit 3). Cross-checked against
> `parser.rs:361–438` (statement dispatcher, `for`, `if`, `while`),
> `parser.rs:475–486` (`return`), `analyzer.rs:172–217`
> (control-flow type checks), `vm.rs:177–179` (logical ops),
> and the fixtures `test_control.sol`, `test_arith.sol`,
> `test_edge.sol`.

SOL's control-flow surface is small and direct:

- `if` / `else` (single-arm or two-arm) for conditional branches
- `while` for condition-tested loops
- `for-in` for collection iteration
- `return` to exit a function early

There is **no** `break`, no `continue`, no `match`, no `switch`,
no C-style `for(init; cond; step)`. The compiler tracks a
`can_break` flag inside the analyzer but no statement ever sets
it; the flag is dead infrastructure.

---

## 7.1 `if` statement

### Form

```sol
if cond { body_block }
if cond { body_block } else { else_block }
if cond { body_block } else if cond { body_block } else { else_block }
```

Parsed at `parser.rs:405–423`. The condition is an expression; the
body is a block. The optional `else` arm is parsed via the same
`block()` entry, which falls through to `statement()` when the
next token isn't `{` — that's the mechanism that makes
`else if` chains work without a special-cased production.

### Parens around the condition

Parentheses around the condition are **optional**:

```sol
if (x > 5) { ... }      // OK
if x > 5 { ... }         // also OK — confirmed by test_arith.sol & test_control.sol
```

Inside the condition the parser disables struct-literal parsing
(the `can_struct` flag is set to `false`; `parser.rs:408–411`). The
mechanical reason: `if cond { ... }` would otherwise be ambiguous
with `if name { field: value }` if `name` happened to be a struct
type. Re-enable struct literals inside the condition by wrapping
in extra parentheses:

```sol
if Point { x: 0, y: 0 } { ... }       // parses as `if Point { ... }` (block body!)
if (Point { x: 0, y: 0 }) { ... }      // explicit grouping — struct literal is the condition
```

The first form is almost always a bug. Prefer to compute the
struct in a `let` first.

### Type rules

The condition must be of type `bool`. Otherwise the analyzer prints:

```
condition of if statement must be of type `bool`, got <TYPE>
```

(`analyzer.rs:174–176`). The body's type doesn't matter at the
statement level — `if` is itself a `Void` statement.

### Examples

*Valid:*

```sol
if amount > 1000 {
    print("large");
} else {
    print("small");
}
```

*Invalid — non-bool condition:*

```sol
if 5 { print("nope"); }
```

The analyzer prints the type-mismatch above. Wrap in an explicit
comparison: `if 5 != 0 { ... }`.

*else-if chain (real form):*

```sol
if amount > 1000 {
    print("big");
} else if amount > 100 {
    print("medium");
} else {
    print("small");
}
```

This works because `else` is followed by `block()`, which when the
next token is not `{` calls `statement()`, which dispatches on `if`.

### Reachability

The analyzer does **not** track unreachable code today. The pattern

```sol
if true { return 1; }
return 0;
```

compiles cleanly. The second `return` is dead but not warned about.
(Future versions of the analyzer may add reachability — the audit
notes this in `SOL_CRATE_IDE_READINESS_PLAN.md` §1.)

---

## 7.2 `while` statement

### Form

```sol
while cond { body_block }
```

Parsed at `parser.rs:425–438`. Same struct-literal-in-condition
restriction as `if` (§7.1). The body is a single block.

### Semantics

The condition is evaluated before each iteration (top-tested loop).
If the condition is `false` initially, the body never runs. There
is no `do-while`. There is no `break` to exit the loop early —
the only way out is to `return` from the surrounding function.

### Type rules

Same as `if`: condition must be `bool` (`analyzer.rs:188–193`):

```
condition of if statement must be of type `bool`, got <TYPE>
```

(Note: the analyzer reuses the `if` error message — that's a
known minor diagnostic-quality issue, not a behavior difference.)

### Examples

*Valid:*

```sol
let i: int = 0;
while i < 10 {
    print(i);
    i = i + 1;
}
```

*Loop that never executes:*

```sol
while false { print("never"); }
// continues after the loop normally
```

Demonstrated by `test_control.sol::test_while_zero`.

*Loop you cannot exit early without `return`:*

```sol
while true {
    return 42;
}
```

Demonstrated by `test_control.sol::test_return_in_while`. This is
the idiomatic way to express "loop until some inner condition fires
and exits via return".

---

## 7.3 `for-in` statement

### Form

```sol
for elem_name in array_expr { body_block }
```

Parsed at `parser.rs:383–404`. `elem_name` is the iteration
variable; `array_expr` must evaluate to an array.

### Semantics

For each element of the array, in source order, the iteration
variable is bound to that element's value and the body executes.

### Type rules

The expression after `in` must be of type `Type::Array { … }`.
Otherwise:

```
array in which for loop is iterating over must have the known type `Array`
```

(`analyzer.rs:202–209`). The iteration variable inherits the
array's element type.

### Scope quirk — iteration variable leaks

The iteration variable is added to the **enclosing scope** rather
than to the loop body's scope (`analyzer.rs:211`). After the loop
ends the binding is still in scope:

```sol
function start() -> int {
    let xs: []int = [1, 2, 3];
    for x in xs { print(x); }
    return x;           // analyzer accepts — x is still bound
}
```

This is fully covered in chapter 06 §6.5. Defend against it by
wrapping the loop in an extra block when the iteration variable
shouldn't outlive the loop.

### Examples

*Valid:*

```sol
let total: int = 0;
for item in [10, 20, 30] {
    total = total + item;
}
```

*Empty iterable:*

```sol
let xs: []int = [];
for x in xs { return 0; }
return 1;                // reached — body never executed
```

Demonstrated by `test_control.sol::test_for_empty`.

*Nested:*

```sol
for a in outer {
    for b in inner {
        sum = sum + a + b;
    }
}
```

Demonstrated by `test_control.sol::test_for_nested`.

### What `for-in` does not provide

| Feature | Status |
|---|---|
| C-style `for (init; cond; step) { ... }` | Not supported; the parser only accepts `for IDENT in expr block` |
| `enumerate` / index access | None; maintain an integer counter alongside the loop if you need the index |
| `break` / `continue` | Not in the language at all |
| Iterating a string | Not supported; strings are not arrays |
| Iterating an integer range | Not supported; build an array first or use `while` with a counter |

---

## 7.4 `return` inside control flow

`return` is legal in any block position inside a function body,
including arbitrarily nested `if` / `while` / `for-in` bodies.
Demonstrated by `test_edge.sol::test_nested_return`:

```sol
function test_nested_return() -> int {
    if (true) {
        if (true) {
            if (true) {
                return 7;
            }
        }
    }
    return 0;
}
```

Returning from inside a loop exits the *function*, not just the
loop. Use this pattern instead of `break`:

```sol
while true {
    if found(x) { return x; }
    x = x + 1;
}
```

Full chapter on `return` semantics lives in chapter 05 §5.3.

---

## 7.5 Evaluation order inside conditions

Two facts to internalize:

1. **`&&` and `||` are not short-circuiting** (`vm.rs:177–178`).
   Both operands are evaluated before the `LogAnd` / `LogOr` op
   runs. If either operand has a side effect or could trap (e.g.
   a division by zero), that effect happens regardless of whether
   the left operand would have made the result determinate.

2. **Equality and comparison return `bool` cleanly.** The result
   of `==`, `!=`, `<`, `<=`, `>`, `>=` is `0` (false) or `1`
   (true). Combining them with `&&` / `||` is safe.

If you need short-circuiting behavior, use nested `if`:

```sol
if denominator != 0 {
    if (numerator / denominator) > threshold {
        ...
    }
}
```

instead of `if denominator != 0 && (numerator / denominator) > threshold { ... }`,
which would trap on the division when `denominator == 0`.

---

## 7.6 Common diagnostics

| Diagnostic | Cause | Fixture |
|---|---|---|
| `condition of if statement must be of type `bool`, got <T>` | Non-bool condition in `if` or `while` | n/a |
| `array in which for loop is iterating over must have the known type `Array`` | Non-array expression after `in` | n/a |
| `illegal return statement` | `return` outside any function body | n/a |
| `expected `{` after for loop declaration` | Body block missing | n/a |
| `expected `{` after if statement declaration` | Body block missing | n/a |
| `expected `{` after while loop declaration` | Body block missing | n/a |

All entries are repeated with bad / fixed examples in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 7.7 Sources cited in this chapter

- `parser.rs:383–404` — `for-in`
- `parser.rs:405–423` — `if`
- `parser.rs:425–438` — `while`
- `parser.rs:475–486` — `return`
- `analyzer.rs:172–186` — `if` type check
- `analyzer.rs:188–199` — `while` type check
- `analyzer.rs:201–217` — `for-in` type check
- `analyzer.rs:468–476` — `return` analysis
- `vm.rs:177–178` — non-short-circuiting `&&` / `||`
- Fixtures: `test_control.sol`, `test_arith.sol`, `test_edge.sol`,
  `jj_comp.sol`
