# 08 — Expressions and Operators

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate.
> Cross-checked against `sol/src/parser.rs` (the precedence chain
> `parse_or` through `parse_primary`), `sol/src/ast.rs`
> (`Expr`, `BinOp`, `UnaryOp`), `sol/src/compiler.rs` (`compile_expr`),
> and `sol/src/vm.rs` (`bin_op`, `cmp_op`, `Div`, `And`/`Or`, postfix
> instructions).

This chapter is the complete reference for SOL expressions: the
operators, their precedence, their associativity, and their runtime
behavior. There is no compile time type checking of expressions; every
operand rule below is enforced at runtime and reported as a plain string
error.

---

## 8.1 The precedence chain

The parser is a recursive descent cascade. From lowest precedence to
highest, the levels are exactly:

| Level | Operators | Associativity | Parser function |
|---|---|---|---|
| 1 (lowest) | `\|\|` | left | `parse_or` |
| 2 | `&&` | left | `parse_and` |
| 3 | `==` `!=` `<` `>` `<=` `>=` | non associative | `parse_comparison` |
| 4 | `+` `-` | left | `parse_term` |
| 5 | `*` `/` | left | `parse_factor` |
| 6 | `-` `!` (unary prefix) | prefix | `parse_unary` |
| 7 | postfix: `.field`, `[i]`, `Enum::Variant`, `module::rpc(args)`, `call(args)` | left | `parse_postfix` |
| 8 (highest) | primary | — | `parse_primary` |

Higher level number means tighter binding. Comparison is
**non associative**: `parse_comparison` applies at most one comparison
operator. A chained form such as `a < b < c` does not parse as two
comparisons; only the first is consumed and the rest fails to parse.
Group explicitly if you need it.

There is **no** `%` (modulo), no `**` (power), no ternary `?:`, no
bitwise operators (`& | ^ << >> ~`), no nullish coalescing, and no
`?.` safe access. The only operators are the ones in the table above.

---

## 8.2 Logical operators (`||`, `&&`, `!`)

```sol
a || b
a && b
!a
```

At runtime (`And` / `Or` in `sol/src/vm.rs`) both `&&` and `||` require
`Bool` operands and return a `Bool`. Non bool operands raise a runtime
error such as `cannot 'and' <value> and <value>`. Both operands are
evaluated before the operator runs; these are not lazy short circuiting
guards. To guard a value that could trap, nest `if` statements (see
chapter 07 §7.6).

Unary `!` (the `Not` instruction) requires a `Bool` and flips it. A non
bool operand raises `cannot apply 'not' to <value>`.

---

## 8.3 Comparison (`== != < > <= >=`)

```sol
a == b
a != b
a < b
a <= b
a > b
a >= b
```

Comparisons return a `Bool` (`cmp_op` in `sol/src/vm.rs`). The runtime
accepts numeric operands directly: int with int, float with float, or a
mixed int/float pair (the int is coerced to float for the comparison).
`==` and `!=` also work structurally across the value types they are
given. Comparing incompatible types raises
`cannot compare <value> and <value>` at runtime.

Comparison is non associative at the parser level (§8.1): write at most
one comparison per group.

---

## 8.4 Arithmetic (`+ - * /` and unary `-`)

```sol
a + b
a - b
a * b
a / b
-a
```

Arithmetic runs through `bin_op` in `sol/src/vm.rs`:

- `int op int` yields an `int`.
- `float op float` yields a `float`.
- A mixed `int` and `float` pair coerces the int to float, so the result
  is a `float`.
- For `+` only, two `Str` operands concatenate into a new string.
- Any other operand pairing raises a runtime error such as
  `cannot add <value> and <value>`.

```sol
let n: int = 2 + 3 * 4;     # 14 — `*` binds tighter than `+`
let m: int = (2 + 3) * 4;   # 20 — parentheses override
let s: str = "ab" + "cd";   # "abcd" — string concatenation
let f: float = 1 + 2.0;     # 3.0 — int/float mix coerces to float
```

### Division by zero

Division by zero is a runtime error for both integers and floats
(`Div` in `sol/src/vm.rs`):

```
division by zero
```

This holds for `int / 0`, `float / 0.0`, and the mixed forms. There is
no compile time guard.

### Unary minus

Unary `-` (the `Neg` instruction) negates an `Int`. Applying it to a non
int value raises `cannot negate <value>`. It is a prefix operator and
binds tighter than the binary arithmetic operators.

```sol
let n: int = -42;
let m: int = -(-10);     # 10
```

---

## 8.5 Postfix operators

### Field access (`.field`)

```sol
expr.field
```

Left associative. At runtime (`MemberAccess`) the left value must be a
`Struct`; the result is the field's value. Chained access works left to
right:

```sol
let value: int = nested.point.x;
```

If the left value is not a struct: `cannot access field '<f>' on <value>`.
If the field is absent: `field '<f>' not found`.

### Index access (`[i]`)

```sol
expr[index]
```

Left associative. At runtime (`Index`) the left value must be an `Array`
and the index must be an `Int`; the result is the element. Out of bounds
raises `index <n> out of bounds`. Indexing a non array, or with a non
int index, raises `cannot index <value> with <value>`.

Note that index access reads only. Index **assignment** (`a[i] = x;`) is
not supported by the compiler; see chapter 06 §6.2.

### Enum variant (`Enum::Variant`)

```sol
Color::Red
```

When `Name::Ident` is not followed by `(`, it parses as an enum variant
reference and evaluates to an `Enum` value. See chapter 10.

### Namespace / RPC call (`module::rpc(args)`)

```sol
discord::send({ channel: "ops", text: "hi" })
```

When `expr::name` is followed by `(args)`, it parses as a namespace
call. At runtime it becomes a `RemoteCall` with the capability string
`"module::rpc"` and a single params value. See chapter 05 §5.4 and
chapter 12.

### Call (`callee(args)`)

```sol
print("hi")
len(items)
discord.send({ text: "hi" })
```

A trailing `(args)` is a call. The callable surface that actually runs
inside a workflow is the VM builtins (`print`, `len`, `to_str`,
`type_name`), host registered natives, and the capability forms; see
chapter 05 §5.2. An imported `module.func(args)` becomes a remote
capability call.

---

## 8.6 Primary expressions

`parse_primary` accepts:

- **Literals**: int, float, bool (`true`/`false`), char (`'x'`),
  and string (`"..."`).
- **Array literal**: `[a, b, c]`. Empty `[]` is allowed. A trailing
  comma is tolerated by the element loop.
- **Struct literal**: `Name { f: v, g: w }`, or an anonymous
  `{ f: v }` with no leading name. Fields are comma separated and bound
  by name, so field order is independent of declaration order.
- **Grouping**: `( expr )`, pure grouping with no tuple effect.
- **The `call` builtin**: `call("cap.name", params)`, which carries a
  single params value (see chapter 05 §5.4).
- **Identifiers**: resolved as a local slot first, otherwise as a
  runtime name lookup (see chapter 06 §6.3).

```sol
let xs: []int = [1, 2, 3];
let p: Point = Point { x: 11, y: 99 };
let anon = { ok: true, count: 3 };
let grouped: int = (2 + 3) * 4;
```

---

## 8.7 Operand rule summary

All rules below are enforced at runtime and reported as plain strings.
There is no analyzer and there are no `E00xx` / `T90xx` codes.

| Op | Operand rule | Result | Error on violation |
|---|---|---|---|
| `+` | int/int, float/float, mixed int/float, or str/str | numeric or str | `cannot add <a> and <b>` |
| `-` `*` | int/int, float/float, mixed int/float | numeric | `cannot subtract/multiply <a> and <b>` |
| `/` | numeric, divisor nonzero | numeric | `division by zero` / `cannot divide <a> and <b>` |
| `== !=` | comparable values | `bool` | `cannot compare <a> and <b>` |
| `< <= > >=` | int/int, float/float, mixed | `bool` | `cannot compare <a> and <b>` |
| `&&` `\|\|` | `bool` / `bool` | `bool` | `cannot 'and'/'or' <a> and <b>` |
| `-` (unary) | `int` | `int` | `cannot negate <value>` |
| `!` (unary) | `bool` | `bool` | `cannot apply 'not' to <value>` |
| `.field` | struct | field value | `cannot access field '<f>' on <value>` |
| `[i]` | array + int index | element | `cannot index <value> with <value>` |

---

## 8.8 Sources cited in this chapter

- `sol/src/parser.rs` — `parse_or`, `parse_and`, `parse_comparison`,
  `parse_term`, `parse_factor`, `parse_unary`, `parse_postfix`,
  `parse_primary`
- `sol/src/ast.rs` — `Expr`, `BinOp`, `UnaryOp`
- `sol/src/compiler.rs` — `compile_expr`
- `sol/src/vm.rs` — `bin_op`, `cmp_op`, `Div`, `And`/`Or`, `Neg`,
  `Not`, `MemberAccess`, `Index`
