# 08 — Expressions and Operators

> **Status:** Substantive (commit 3). Cross-checked against
> `parser.rs:584–751` (precedence chain + primary), `analyzer.rs:241–337`
> (per-operator type rules), and `vm.rs:143–186` (per-operator runtime).

This chapter is the complete reference for the expression sub-language
of SOL: every operator, its precedence, its associativity, its operand
type rules, and its runtime behavior. The catalogue of expression
*forms* (literals, struct literals, calls, postfix access, etc.) lives
in [chapter 03 §3.5](./03-syntax.md); this chapter is the operator
side of the story.

---

## 8.1 The precedence chain

SOL's parser uses a fourteen-level Pratt-style cascade
(`parser.rs:584–595`). Each level corresponds to a single function
that calls the next level down and applies any operators that
belong at this precedence:

| Level | Function | Operators | Associativity | Result type |
|---|---|---|---|---|
| 1 (lowest) | `assignment` | `=` | right | type of RHS |
| 2 | `logic_or` | `\|\|` | left | `bool` |
| 3 | `logic_and` | `&&` | left | `bool` |
| 4 | `bitwise_or` | `\|` | left | `int` |
| 5 | `bitwise_xor` | `^` | left | `int` |
| 6 | `bitwise_and` | `&` | left | `int` |
| 7 | `equality` | `==`, `!=` | left | `bool` |
| 8 | `relational` | `<`, `<=`, `>`, `>=` | left | `bool` |
| 9 | `shift` | `<<`, `>>` | left | `int` |
| 10 | `additive` | `+`, `-` | left | operand type |
| 11 | `multiplicative` | `*`, `/` | left | operand type |
| 12 | `unary` | `!`, `-`, `~` (prefix) | right | operand type (or `bool` for `!` on bool) |
| 13 | `postfix` | `.`, `[ ]` | left | depends on construct |
| 14 (highest) | `primary` | literals, identifier, `(…)`, calls, struct literals, enum variants, array literals | — | — |

This table is the **precedence reference**. Higher row number = higher
precedence = binds tighter. Within a row, associativity is shown.

The two-token forms (`==`, `!=`, `<=`, `>=`, `<<`, `>>`, `&&`,
`||`, `->`, `::`) are produced by the lexer as single tokens
(maximal-munch); the parser sees them at the appropriate precedence
level above.

**There is no `%` (modulo) operator.** There is no `**` (power).
There is no ternary `?:`. There is no nullish-coalescing operator.
There is no `?.` safe-access. If you need any of these you must
implement them in the host via `ext function`.

---

## 8.2 Assignment (`=`)

```sol
target = expr
```

Right-associative (`parser.rs:585`). `target` may be a variable, a
struct field (`s.field`), or an array element (`a[i]`).

The analyzer requires the LHS and RHS to have matching types
(`analyzer.rs:291–297`):

```
cannot assign mismatched types: <LHS> = <RHS>
```

Assignment is parsed as an expression and yields the RHS's value,
so chains are accepted (`test_edge.sol::test_chained_assign`):

```sol
let a: int = 0; let b: int = 0; let c: int = 0;
a = b = c = 42;        // all three become 42
```

You may also use an assignment as a `let` initializer:

```sol
let x: int = 0;
let y: int = x = 5;    // x is set to 5; y is set to 5
```

Demonstrated by `test_edge.sol::test_assign_expr_result`.

Idiomatic SOL keeps assignments on their own statement line. The
chained / initializer forms exist but are easy to misread.

---

## 8.3 Logical operators

```sol
a || b      // logical or
a && b      // logical and
!a          // logical not
```

The analyzer requires `bool` operands for `&&` / `||`
(`analyzer.rs:273–280`). For `!` the analyzer is unusually
permissive: it accepts `bool`, `int`, and `float`
(`analyzer.rs:317–324`); only `bool` is idiomatic.

### Not short-circuiting

`&&` and `||` evaluate *both* operands before the op runs
(`vm.rs:177–178`):

```rust
Inst::LogOr   => { let b = self.pop(); let a = self.pop(); self.push(if a == 1 || b == 1 { 1 } else { 0 }); }
Inst::LogAnd  => { let b = self.pop(); let a = self.pop(); self.push(if a == 1 && b == 1 { 1 } else { 0 }); }
```

If you rely on short-circuiting (e.g. to guard a divide-by-zero),
**use nested `if` instead** — see chapter 07 §7.5 for the
recommended pattern.

The runtime check `a == 1 && b == 1` means each operand must be
exactly the integer `1` to register as true. Values produced by
comparison ops are always `0` or `1`, so this works in practice for
boolean expressions. Avoid feeding raw integers (e.g. flag bits)
into `&&` / `||`; convert to `bool` with a comparison first.

---

## 8.4 Bitwise operators

```sol
a | b       // bitwise or
a ^ b       // bitwise xor
a & b       // bitwise and
a << b      // left shift
a >> b      // right shift
~a          // bitwise complement (unary, prefix)
```

The analyzer requires `int` operands (`analyzer.rs:283–289`):

```
bitwise operation <op> requires integer operands
```

For unary `~`:

```
cannot bitwise invert a non integer type
```

(`analyzer.rs:325–331`).

At runtime the VM uses native Rust `u64` operations
(`vm.rs:181–186`). Sign-extension behavior follows what `u64`
does — right-shift is logical (zero-fill), not arithmetic. This
distinction matters when working with signed-negative integers via
bit ops; the language doesn't currently provide a signed shift.

Demonstrated by `test_arith.sol::test_bit_and` (`12 & 25 == 8`),
`::test_bit_or` (`12 | 25 == 29`), `::test_bit_xor` (`12 ^ 25 == 21`),
`::test_bit_not` (`~1 == -2`), `::test_shl` (`1 << 4 == 16`),
`::test_shr` (`32 >> 2 == 8`).

---

## 8.5 Equality and comparison

```sol
a == b      // equal
a != b      // not equal
a <  b
a <= b
a >  b
a >= b
```

The analyzer accepts the comparison ops on any pair of operands of
the **same** type (`analyzer.rs:263–271`); the result is `bool`.

Demonstrated by `test_arith.sol::test_int_eq_true`, etc.
`test_struct.sol::test_person` shows `str == str` operating on
heap-stored strings — the fixture's expected return value implies
the runtime evaluates this as content equality rather than
reference equality, but the underlying bytecode site has not yet
been audited (queued as open question #12). Treat `str == str` as
*Confirmed by fixture, mechanism uncertain*.

### Float comparison

Float comparisons follow IEEE-754. `NaN` is not equal to itself;
`x == x` is `false` if `x` is `NaN`. Order operators (`<`, `<=`,
etc.) return `false` if either operand is `NaN`.

### Char comparison

Char comparisons are ordered by Unicode code-point value
(`vm.rs:169–174`).

---

## 8.6 Arithmetic

```sol
a + b
a - b
a * b
a / b
-a          // unary negation (prefix)
```

Two operands of the same numeric type (`int` or `float`); result is
the same type (`analyzer.rs:247–259`). Mixing types is a compile-time
error:

```
mismatched types in arithmetic: <T1> <op> <T2>
```

### Integer arithmetic

The VM uses Rust `i64` ops (`vm.rs:143–146`). Overflow:

- In **debug builds** of the compiler, integer overflow panics.
- In **release builds**, integer overflow wraps in two's complement.

Division by zero on integers raises a runtime panic:

```
thread '...' panicked at 'attempt to divide by zero', src/sol/vm.rs:...
```

Fixture: `error_runtime.sol`.

### Float arithmetic

The VM uses native `f64` ops (`vm.rs:156–159`). Division by zero
yields `inf` / `-inf` / `NaN` per IEEE-754; **it does not trap**.

### Unary minus

```sol
let n: int = -42;
let m: int = -(-10);    // double negation, returns 10 — test_edge.sol::test_double_neg
```

Right-associative (`parser.rs:596–604`).

### Operator precedence within arithmetic

Standard math precedence — multiplicative before additive — is
demonstrated by `test_arith.sol::test_precedence_mul_before_add`:
`2 + 3 * 4 = 14`. Parens override per `test_arith.sol::test_precedence_parens`:
`(2 + 3) * 4 = 20`.

Subtraction is left-associative
(`test_arith.sol::test_left_assoc_sub`): `10 - 5 - 2 = 3`.

---

## 8.7 Postfix operators — member access and indexing

### Field access (`.`)

```sol
expr.field
```

Left-associative (`parser.rs:608–617`). The LHS must be a struct
value (chapter 09). The result type is the field's declared type.

Chained access works left-to-right:

```sol
let n: Nested = Nested { p: Point { x: 7, y: 8 }, label: "point" };
let value: int = n.p.x;     // 7
```

Demonstrated by `test_struct.sol::test_struct_in_struct`.

If the LHS is not a struct:

```
<TYPE> is not a struct with members
```

If the field doesn't exist on the struct:

```
`<STRUCT>` has no member `<FIELD>`
```

### Index access (`[ ]`)

```sol
expr[index]
```

Left-associative. The LHS must be an array; the index must be `int`
or, by an analyzer-side quirk, `float` (`analyzer.rs:459`). The
runtime treats the index as a `u64` slot index.

```
Type Error: Array index must be an integer or float
Type Error: Cannot index into a non-array type
```

(`analyzer.rs:460, 465`.)

Demonstrated by `test_array.sol::test_array_basic`.

**Out-of-bounds is not validated at compile time.** The runtime may
panic, or — depending on the heap layout — silently misread; the
exact behavior is queued for documentation in chapter 14.

### Postfix in an LHS position

`s.field = expr;` and `a[i] = expr;` are statements that combine
postfix access with assignment. Both forms are demonstrated
extensively in `test_struct.sol` and `test_array.sol`.

---

## 8.8 Function calls

```sol
name(arg1, arg2, …)
```

Parsed in primary position (`parser.rs:668–681`). Fully covered in
chapter 05.

Calls in expression position return the function's declared return
type. A call to a `Void` function in expression position is
parser-accepted but semantically meaningless — the call still
executes for its side effects, but you cannot bind the result.

---

## 8.9 Struct literals

```sol
TypeName { field: expr, field: expr, … }
```

Parsed in primary position (`parser.rs:683–697`). Two important
caveats already mentioned:

1. Disabled inside `if` / `while` / `for-in` conditions; wrap in
   `( … )` to use.
2. Field order in the literal is independent of declaration order
   — fields are bound by name. `test_struct.sol::test_field_order`
   demonstrates `Point { y: 99, x: 11 }` working correctly.

Full chapter: [chapter 09](./09-structs.md).

---

## 8.10 Enum variant references

```sol
EnumName::VariantName
```

Parsed in primary position (`parser.rs:699–706`). The result is a
value of type `EnumName`. Compare with `==` / `!=`.

Full chapter: [chapter 10](./10-enums.md).

---

## 8.11 Array literals

```sol
[expr1, expr2, …, exprN]
```

Parsed in primary position (`parser.rs:726–739`). Trailing commas
are *not* accepted — the loop breaks the first time a non-comma
token follows an element. Empty literals `[]` are accepted; the
fixtures show `let xs: []int = [];` (chapter 11).

Full chapter: [chapter 11](./11-arrays.md).

---

## 8.12 Parenthesized expressions

```sol
( expr )
```

Pure grouping; no tuple-construction effect. Re-enables struct
literals inside the parentheses regardless of the surrounding
`can_struct` state — important for `if (Point { x: 0 }) { … }`.

---

## 8.13 Operator type-rule summary

A compact table for quick reference. Symbol of the diagnostic is the
analyzer message that fires when the rule is violated; see
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) for full entries.

| Op | LHS type | RHS type | Result | Diagnostic on mismatch |
|---|---|---|---|---|
| `+` `-` `*` `/` | `int` or `float`, both equal | same as LHS | same | "mismatched types in arithmetic" / "not supported for type" |
| `==` `!=` | any T | same T | `bool` | "cannot compare mismatched types" |
| `<` `<=` `>` `>=` | any T | same T | `bool` | "cannot compare mismatched types" |
| `&&` `\|\|` | `bool` | `bool` | `bool` | "logical operation … requires boolean operands" |
| `&` `\|` `^` `<<` `>>` | `int` | `int` | `int` | "bitwise operation … requires integer operands" |
| `=` | T | T | T | "cannot assign mismatched types" |
| `-` (unary) | `int` or `float` | — | same | "cannot negate a non number type" |
| `!` (unary) | `bool`, `int`, `float` | — | same | "can't not this type" |
| `~` (unary) | `int` | — | `int` | "cannot bitwise invert a non integer type" |
| `.` (postfix) | struct | — | field type | "is not a struct with members" / "has no member" |
| `[ ]` (postfix) | array | `int` (or `float`) | element type | "Cannot index into a non-array type" / "Array index must be …" |

---

## 8.14 Sources cited in this chapter

- `parser.rs:584–595` — precedence chain
- `parser.rs:596–604` — unary
- `parser.rs:605–629` — postfix
- `parser.rs:630–751` — primary
- `analyzer.rs:241–303` — binary op type rules
- `analyzer.rs:305–337` — unary op type rules
- `analyzer.rs:409–435` — field access type rule
- `analyzer.rs:455–466` — index access type rule
- `vm.rs:143–146` — int arithmetic
- `vm.rs:148–153` — int comparisons
- `vm.rs:156–166` — float arithmetic & comparison
- `vm.rs:169–174` — char comparison
- `vm.rs:177–186` — logical / bitwise ops
- Fixtures: `test_arith.sol`, `test_edge.sol`, `test_struct.sol`,
  `test_array.sol`, `test_control.sol`
