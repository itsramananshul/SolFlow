# 17 — Style Guide

> **Status:** Substantive. Recommendations based on the canonical
> syntax of the `openprem-sol-v2` crate (`sol/src/*`). The rules here
> are advisory; the hard syntactic rules live in chapter 03 and in
> [`SPEC.md`](./SPEC.md).

A SOL program reads best when it follows a small set of conventions.
This chapter documents them, then lists anti-patterns to avoid.

---

## 17.1 Naming

| Kind | Convention | Example |
|---|---|---|
| Variable (`let`) | `snake_case` | `order_id`, `total_amount` |
| Function | `snake_case` | `verify_capacity`, `describe` |
| Function parameter | `snake_case` | `name`, `request_id` |
| Struct type name | `PascalCase` | `Point`, `Person`, `ProcessNode` |
| Struct field | `snake_case` | `service_name`, `is_active` |
| Enum type name | `PascalCase` | `AppHealth`, `Status` |
| Enum variant | `PascalCase` | `Stable`, `Throttled`, `Active` |
| Workflow name | `kebab-case` string | `"check-stock"`, `"emit-events"` |

Use `snake_case` for values and `PascalCase` for types. This keeps a
visual distinction between the data and the shape of the data.
Workflow names are string literals; a kebab-case string reads well as
a workflow identifier.

### Make enum variant first characters distinct

The canonical bytecode dispatches each enum variant by `(first_char
as i128) % 10`. Two variants of the same enum whose first characters
share a mod-10 residue compare equal at runtime, even though the
by-name editor simulator runs them correctly. The editor flags this
as the `enum-first-char-collision` warning (`src/graph/validate.ts`).
Pick first characters that do not collide:

```sol
enum Status { Active; Inactive; }     # A and I differ — OK
enum Status { Active; Aborted; }      # both 'A' — collide at runtime
```

If two states would naturally start with the same character, rename
one so the first characters differ: `Active`, `Disabled` instead of
`Active`, `Aborted`. As a quick check, distinct first letters whose
character codes do not share a remainder mod 10 are safe.

---

## 17.2 File layout

A `.sol` file reads cleanest in this top-to-bottom order:

1. `import` statements (they advertise the modules a workflow calls
   into; chapter 12)
2. `enum` declarations
3. `struct` declarations (leaf types first, composites later)
4. Helper functions, in dependency order or alphabetical
5. The `workflow "name" { ... }` block at the bottom

The workflow is the executable entry point, so keeping it last lets a
reader scan the supporting types and helpers first, then read the
orchestration that ties them together.

---

## 17.3 Indentation and braces

- **Four-space indentation.** This matches the canonical formatter
  (`sol/src/format.rs`).
- **Opening brace on the same line** as the construct that opens it:
  ```sol
  fn double(x: int) <- int {
      return x * 2;
  }
  ```
- **Closing brace on its own line**, at the indentation of the
  construct that opened it.

The lexer treats whitespace as trivia, so the compiler does not care
about layout; pick a style and apply it consistently. Running the
formatter normalizes layout (and drops comments, since the AST does
not carry them).

---

## 17.4 Statements

- One statement per line.
- Annotate every `let` with its type. An omitted annotation defaults
  to `bool` in the AST (chapter 06), which misleads tools that read
  the AST even though it does not change runtime behavior.
- Each `let` introduces exactly one binding.
- Prefer named intermediate `let`s over long inline expressions:
  ```sol
  # less readable
  let result: int = (compute(a, b) + compute(c, d)) * scale;

  # more readable
  let left: int   = compute(a, b);
  let right: int  = compute(c, d);
  let total: int  = left + right;
  let result: int = total * scale;
  ```

---

## 17.5 Control flow

- Parenthesize `if` and `while` conditions; this is required by the
  grammar:
  ```sol
  fn clamp(x: int) <- int {
      if (x < 0) { return 0; }
      if (x > 100) { return 100; }
      return x;
  }
  ```
- `for ... in ...` takes no parentheses:
  ```sol
  for item in items {
      print(item);
  }
  ```
- Use early `return` to flatten nesting, as in `clamp` above.
- Wrap every `if` / `while` / `for` body in `{ ... }`. The braces are
  mandatory: the parser only admits a body through a block (chapter
  07).

---

## 17.6 Structs and enums

- Struct fields and enum variants are semicolon terminated:
  ```sol
  struct Order {
      id: int;
      amount: float;
      status: OrderStatus;
  }

  enum OrderStatus {
      New;
      Approved;
      Rejected;
  }
  ```
- Struct-literal fields are comma separated and matched by name:
  ```sol
  let o: Order = Order { id: 42, amount: 0.0, status: OrderStatus::New };
  ```
- Keep field counts modest. Six fields or so is a soft limit; past
  that, split into nested types.
- Provide every field in every struct literal. There is no type
  checker to warn about a missing field, and an unset slot is almost
  never what you want.
- Use enums for tagged comparison. Keep variant first characters
  distinct (§17.1) so runtime comparisons behave as written.

---

## 17.7 Arrays

- Array types use the prefix form `[]T`:
  ```sol
  let nums: []int = [3, 7, 11];
  let grid: [][]float = [[1.0, 2.0], [3.0, 4.0]];
  ```
- There is no sized-array form. Do not write a postfix size.

---

## 17.8 External Actions

Workflows reach the outside world through capability calls. Two forms
exist:

```sol
import inventory;

workflow "reserve" {
    # capability-string form
    let level: int = call("warehouse.get_stock", { sku: "A-100" });

    # imported-module form
    let ok: bool = inventory.reserve({ sku: "A-100", qty: 2 });
}
```

- Each form carries a single params value, commonly a struct literal.
- Place `import` statements at the top of the file so a reader can
  skim the workflow's external dependencies at a glance.
- Name params clearly inside the struct (`sku`, `qty`, `user_id`)
  rather than `a`, `b`; the host sees these names when resolving the
  capability.

---

## 17.9 Comments

Comments use `#` to end of line. There are no block comments. Write a
comment only when the why is non-obvious:

- A non-obvious business rule (for example, "threshold is 1000 because
  audit policy demands manual review above $1000").
- A reference to an external system or capability contract.

Do not restate the code. `# add one to count` above `count = count +
1;` adds nothing. Remember the formatter drops comments on a round
trip, so keep load-bearing intent in code where possible.

---

## 17.10 Anti-patterns to avoid

| Anti-pattern | Why it is wrong | Fix |
|---|---|---|
| `fn f(x: int) -> int { }` | `->` is not a token; it lexes as `Minus` then `Gt` and fails to parse | Use the return arrow `<-` |
| `// a comment` | `//` is not a comment; it lexes as two `Slash` tokens | Use `#` for comments |
| `int[] nums` or `[3]int` | No postfix or sized array form exists | Use the prefix form `[]int` |
| `function f() { }` | `function` is not a keyword | Use `fn` |
| `let x = 5;` with no type | Annotation defaults to `bool` in the AST | Annotate: `let x: int = 5;` |
| Two enum variants with colliding first characters | Collide under `(first_char) % 10` at runtime | Make first characters distinct |
| `if x > 0 { }` (no parens) | `if` and `while` require parenthesized conditions | `if (x > 0) { }` |
| `for (x in xs) { }` | `for ... in` takes no parentheses | `for x in xs { }` |
| `1 / 0` | Division by zero is a runtime error | Check the denominator |
| Struct fields separated by commas | Struct fields are semicolon terminated | `id: int; amount: float;` |
| Relying on a static type checker | There is none; mismatches fail at runtime as strings | Validate values explicitly |

---

## 17.11 A short canonical example to model on

```sol
import inventory;

enum OrderStatus {
    New;
    Approved;
    Rejected;
}

struct Order {
    id: int;
    amount: float;
    status: OrderStatus;
}

fn classify(amount: float) <- OrderStatus {
    if (amount > 1000.0) {
        return OrderStatus::Rejected;
    }
    return OrderStatus::Approved;
}

workflow "process-order" {
    let raw: int = call("orders.lookup", { id: 42 });
    print("processing:");
    print(raw);

    let order: Order = Order {
        id: 42,
        amount: 250.0,
        status: OrderStatus::New,
    };

    order.status = classify(order.amount);
    print(order.status);
}
```

This file follows every recommendation: imports first, types before
functions, a helper before the workflow, the workflow last,
four-space indent, the `<-` return arrow, `#` comments where needed,
prefix array types, distinct enum variant first characters, annotated
`let`s, and a capability call for the external lookup.

---

## 17.12 Sources

- The canonical crate: `sol/src/lexer.rs`, `sol/src/parser.rs`,
  `sol/src/ast.rs`, `sol/src/vm.rs`, `sol/src/format.rs`.
- The editor structural validator: `src/graph/validate.ts`.
- The lexical and grammar rules in chapter 03 and [`SPEC.md`](./SPEC.md).
