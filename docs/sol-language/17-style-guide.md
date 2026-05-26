# 17 — Style Guide

> **Status:** Substantive (commit 4). Recommendations derived from
> the positive-fixture corpus and adjusted for clarity. The rules
> here are advisory; anything the compiler enforces lives in
> chapters 02 – 14.

A SOL program reads best when it follows a small set of
conventions. This chapter documents them as recommendations, then
adds a short list of anti-patterns to avoid.

---

## 17.1 Naming

| Kind | Convention | Example |
|---|---|---|
| Variable (`let`) | `snake_case` | `order_id`, `total_amount` |
| Function | `snake_case` | `verify_capacity`, `print_person` |
| Function parameter | `snake_case` | `name`, `request_id` |
| Struct *type* name | `PascalCase` | `Point`, `Person`, `ProcessNode` |
| Struct *field* | `snake_case` | `service_name`, `is_active` |
| Enum *type* name | `PascalCase` | `AppHealth`, `Status` |
| Enum *variant* | `PascalCase` | `Stable`, `Overloaded`, `Active` |
| Entry function | `start` | (fixed convention) |

Justification: the corpus uses `snake_case` for values and
`PascalCase` for types. Following the same convention everywhere
keeps the visual distinction between *the data* and *the shape of
the data*.

### Avoid leading underscore in identifiers

The lexer consumes `_` as trivia outside of identifier-continuation
positions (chapter 03 §3.1). A leading underscore (`_x`) is
*silently eaten* — the identifier becomes `x`. Don't write
identifiers like `_internal`. Use `internal` instead.

### Avoid first-character collisions in enum variants

Until T9002 is fixed in the compiler (chapter 10 §10.5), two
variants of the same enum that share a first character will
collide at runtime. Pick first characters carefully:

```sol
enum Status { Active, Inactive }      // A and I — OK
enum Status { Active, Aborted }       // both A — BAD (collides at runtime)
```

If you must use multiple variants with the same first character,
prefix them to disambiguate: `Active`, `Disabled` instead of
`Active`, `Aborted`.

---

## 17.2 File layout

A `.sol` file reads cleanest with this top-to-bottom order:

1. `import` statements (today they're inert — chapter 12 §12.3 —
   but they advertise dependencies)
2. `ext function` declarations — these are the file's contract
   with the outside world
3. `enum` declarations
4. `struct` declarations (leaf types first, composites later)
5. Helper functions — in dependency order or alphabetical
6. `start` function at the bottom

Why this order: a reader's first questions are usually "what does
this file *need*?" (imports + ext) and "what does it *produce*?"
(start). Putting imports first and `start` last keeps the answer
to both questions visible without scrolling.

---

## 17.3 Indentation and braces

- **Four-space indentation.** Used uniformly in the corpus.
- **Opening brace on the same line** as the construct that opens
  it:
  ```sol
  function start() -> int {
      ...
  }
  ```
- **Closing brace on its own line**, at the indentation level of
  the construct that opened it.

The compiler doesn't care about whitespace; pick a style and
apply it consistently.

---

## 17.4 Statements

- One statement per line.
- One `;` per line; no `;;`.
- Each `let` introduces exactly one binding.
- Prefer named intermediate `let`s over long inline expressions:
  ```sol
  // less readable
  let result: int = (compute(a, b) + compute(c, d)) * scale;

  // more readable
  let left:   int = compute(a, b);
  let right:  int = compute(c, d);
  let total:  int = left + right;
  let result: int = total * scale;
  ```
- Chained assignment (`a = b = c = 42;`) is parser-accepted
  (chapter 08 §8.2) but rarely the clearest way to write the
  intent. Prefer three statements.

---

## 17.5 Control flow

- Use early `return` to flatten nesting:
  ```sol
  function clamp(x: int) -> int {
      if x < 0 { return 0; }
      if x > 100 { return 100; }
      return x;
  }
  ```
- Wrap `if`/`while`/`for-in` body blocks in `{ … }` even when the
  body is a single statement. The parser admits the braces only
  via `block()`, so the braces are mandatory anyway (chapter 07).
- When you must short-circuit (the language doesn't, chapter 08
  §8.3), express it as nested `if`:
  ```sol
  if denominator != 0 {
      if numerator / denominator > threshold { ... }
  }
  ```
- Don't rely on `for-in`'s iteration variable surviving the loop
  (it does today — chapter 06 §6.5 — but readers won't expect it).
  Wrap the loop in an extra block if you need the binding
  tightly scoped.

---

## 17.6 Structs and enums

- Keep field counts modest. Six fields or so is a soft limit; past
  that, split into nested types.
- Provide every field in every struct literal. The compiler does
  not currently warn about missing fields (chapter 09 §9.2), but
  the omitted slots become zero at runtime, which is almost never
  what you want.
- Use enums for *tagged comparison*, not for arithmetic encoding.
  Until T9002 is fixed, enum values at runtime are a hash of the
  variant name, not the iota; programs that depend on enum
  values being small consecutive integers will break.

---

## 17.7 External functions

Group every `ext function` at the top of the file, immediately
after `import`s. Treat the block as the file's contract with the
host runtime. A reader should be able to skim the top of the file
and know exactly what external dependencies a session has.

Use a clear convention for naming `ext function` parameters —
they appear in the host's API documentation and in error messages
for argument-count / argument-type mismatches. Vague names like
`a`, `b` lose half their value here; prefer `query`, `user_id`,
`payload`.

---

## 17.8 Comments

The corpus uses comments sparingly. Write a comment only when the
*why* is non-obvious:

- A non-obvious business rule (e.g. "threshold is 1000 because
  audit policy demands manual review above $1000").
- A workaround for a known compiler quirk (e.g. "we use two
  separate `print` calls because the bytecode only emits the
  first arg — see T9003").
- A reference to an external system or contract.

Don't write comments that restate the code. `// add one to count`
above `count = count + 1;` adds nothing.

---

## 17.9 Anti-patterns to avoid

| Anti-pattern | Why it's bad | Fix |
|---|---|---|
| `export function foo() { … }` | `export` isn't a keyword — fails at parse (E0003) | Drop `export`; every top-level `function` is host-visible |
| Identifier starting with `_` | Lexer eats the leading `_` as trivia | Don't lead with `_` |
| Numeric digit separator (`1_000`) | Lexer eats the `_` as trivia; you get two integers | Write integers without separators |
| `if cond { let x: int = …; } print(x);` | `x` is out of scope here | Move the `let` up |
| `print("count is", count)` | Only the first argument is emitted (T9003) | Use two `print` statements |
| `1 / 0` | Integer division by zero panics at runtime (E2001) | Check the denominator |
| Two enum variants starting with the same letter | Collide at runtime per T9002 | Make first characters distinct |
| String concatenation with `+` | Rejected by the analyzer (E1006) | Use an `ext function` for now |
| Using `let x: int;` with no initializer | Uninitialized; reads as `0` | Always initialize |
| `=` inside a condition without parens (`if x = 5 { … }`) | Parses as assignment (right-recursive `=`), not comparison | Use `==` for comparison |
| `for i = 0; i < 10; i = i + 1 { … }` | C-style `for` doesn't exist | Use `while i < 10 { … i = i + 1; }` |

---

## 17.10 A short canonical example to model on

```sol
ext function lookup_order(id: int) -> str;

enum OrderStatus {
    New,
    Approved,
    Rejected,
}

struct Order {
    id: int,
    amount: float,
    status: OrderStatus,
}

function process_order(id: int) -> int {
    let raw: str = lookup_order(id);
    print("processing:");
    print(raw);

    let order: Order = Order {
        id: id,
        amount: 0.0,
        status: OrderStatus::New,
    };

    if order.amount > 1000.0 {
        order.status = OrderStatus::Rejected;
        return 0;
    }
    order.status = OrderStatus::Approved;
    return 1;
}

function start() -> int {
    return process_order(42);
}
```

This file follows every recommendation above: imports first, `ext`
second, types before functions, helper before `start`, four-space
indent, one statement per line, one `print` argument at a time,
named intermediate `let`s, early return.

---

## 17.11 Sources

- The full positive-fixture corpus, especially `s1.sol`, `s2.sol`,
  `jjsi.sol`, `test_*.sol`.
- The known-bug entries T9002, T9003, T9005 in
  [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).
- The lexical and grammar rules in chapters 03 and `GRAMMAR.md`.
