# 07 — Control Flow

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate.
> Cross-checked against `sol/src/parser.rs` (`parse_stmt` for `if`,
> `while`, `for`, `return`, `emit`), `sol/src/compiler.rs` (`compile_stmt`
> jump emission and the `for` desugaring), and `sol/src/vm.rs`
> (`JumpIfFalse` truthiness, `Return`/`Halt`).

SOL's control flow surface is small and direct:

- `if` / `else` for conditional branches
- `while` for condition tested loops
- `for ... in ...` for array iteration
- `return` to end the running workflow
- `emit` to mark an event (see §7.5)

There is no `break`, no `continue`, no `match`, no `switch`, and no
C style `for(init; cond; step)`.

---

## 7.1 `if` statement

### Form

```sol
if (cond) { then_block }
if (cond) { then_block } else { else_block }
```

Parsed by `parse_stmt` in `sol/src/parser.rs`. The condition is an
expression and **must be wrapped in parentheses**: the parser expects
`if`, then `(`, the condition, `)`, then a `{ }` block. The `else` arm
is optional and is itself a `{ }` block.

There is no built in `else if` production. To chain, nest an `if` inside
the `else` block:

```sol
if (amount > 1000) {
    print("big");
} else {
    if (amount > 100) {
        print("medium");
    } else {
        print("small");
    }
}
```

### Truthiness

The condition value is checked at runtime by `JumpIfFalse` in
`sol/src/vm.rs`. A condition is truthy when it is:

- a `Bool` that is `true`, or
- an `Int` that is nonzero.

Any other value type (float, string, struct, array, and so on) raises a
runtime string error:

```
cannot use <value> as condition
```

So `if (5) { ... }` runs the body (5 is a nonzero int), while
`if (3.5) { ... }` fails at runtime. There is no compile time check on
the condition type; prefer an explicit comparison such as
`if (x != 0) { ... }`.

### Examples

```sol
if (amount > 1000) {
    print("large");
} else {
    print("small");
}
```

---

## 7.2 `while` statement

### Form

```sol
while (cond) { body_block }
```

Parsed by `parse_stmt`. Parentheses around the condition are
**required**, the same as `if`.

### Semantics

The condition is evaluated before each iteration (a top tested loop).
The same truthiness rule applies: the condition must be a `Bool` or an
`Int`, otherwise the loop traps at runtime. If the condition is false on
entry, the body never runs. There is no `do/while`, and there is no
`break`; the only early exit is `return`, which ends the whole workflow.

### Examples

```sol
let i: int = 0;
while (i < 10) {
    print(i);
    i = i + 1;
}
```

```sol
# Loop until an inner condition fires, then exit via return.
while (true) {
    if (found) {
        return 42;
    }
    i = i + 1;
}
```

---

## 7.3 `for ... in` statement

### Form

```sol
for item in iterable { body_block }
```

Parsed by `parse_stmt`. Note there are **no parentheses** around the
`item in iterable` header; that distinguishes `for` from `if` and
`while`. The `item` is an identifier; `iterable` is an expression that
must evaluate to an array at runtime.

### Semantics

The compiler desugars the loop into an index counter (see
`compile_stmt` in `sol/src/compiler.rs`). It evaluates the iterable once
into a hidden slot, sets a hidden index to 0, and on each pass compares
the index against the array length with `Len` and `Lt`. While the index
is in range it indexes the array, binds the element to `item`, runs the
body, then increments the index.

Because the iterable is indexed and `Len` is applied to it, only an
array iterates cleanly. A non array iterable fails at runtime when the
loop tries to index it (for example `cannot index <value> with ...`).

The `item` binding is an ordinary local slot, so it persists after the
loop ends (locals are not popped). See chapter 06 for the flat locals
model.

### Examples

```sol
let total: int = 0;
for item in [10, 20, 30] {
    total = total + item;
}
```

```sol
# Empty array: the body never runs.
let xs: []int = [];
for x in xs {
    print(x);
}
```

### What `for ... in` does not provide

| Feature | Status |
|---|---|
| C style `for (init; cond; step)` | Not supported |
| `enumerate` / direct index access | None; keep your own counter if you need the index |
| `break` / `continue` | Not in the language |
| Iterating a string or an integer range | Not supported; build an array first |

---

## 7.4 `return`

```sol
return;
return value;
```

A `return` ends execution of the running workflow and reports the
returned value as the result. `return;` returns `Unit`. `return value;`
returns the value. There is no `break`, so a `return` inside a loop ends
the whole workflow, not just the loop:

```sol
while (true) {
    if (found) {
        return x;
    }
    x = x + 1;
}
```

Full `return` semantics, including falling off the end of a workflow,
are covered in chapter 05 §5.3.

---

## 7.5 `emit`

```sol
emit "event_name";
```

Parsed by `parse_stmt`: `emit` followed by a string literal and an
optional `;`. In the canonical bytecode VM, `emit` currently compiles to
a no op that pushes `Unit` (`compile_stmt`, `Stmt::Emit`). It does not,
by itself, dispatch anything to the host today. Treat it as a documented
marker statement whose host wiring is environment specific.

---

## 7.6 Evaluation of `&&` and `||`

`&&` and `||` are plain boolean operators evaluated by popping both
operands (`And` / `Or` in `sol/src/vm.rs`). They require `Bool`
operands; a non bool operand raises a runtime error such as
`cannot 'and' <value> and <value>`. They are not lazy guards: both sides
are evaluated before the operator runs. If you need to guard a value
that could trap (for example a divide by zero), use a nested `if`
instead of relying on a left operand to short circuit:

```sol
if (denominator != 0) {
    if ((numerator / denominator) > threshold) {
        print("ok");
    }
}
```

---

## 7.7 Common errors

These are plain string messages; there are no `E00xx` or `T90xx` codes.

| Message | Cause |
|---|---|
| `cannot use <value> as condition` | An `if`/`while` condition that is not a `Bool` or `Int` |
| `cannot index <value> with <value>` | `for` over a non array, or a bad index |
| `index <n> out of bounds` | Indexing past the end of an array |
| `cannot 'and' <value> and <value>` | `&&` / `||` on non bool operands |

---

## 7.8 Sources cited in this chapter

- `sol/src/parser.rs` — `parse_stmt` (`if`, `while`, `for`, `return`,
  `emit`)
- `sol/src/compiler.rs` — `compile_stmt` jump emission, `for`
  desugaring (`Len`/`Lt`/`Index`), `Emit` to `PushUnit`
- `sol/src/vm.rs` — `JumpIfFalse` truthiness (`Bool`/`Int`), `And`/`Or`,
  `Return`/`Halt`
