# 04 — Types

> **Status:** Substantive (commit 2). Cross-checked against
> `parser.rs` (`Type` enum + `parse_type`), `analyzer.rs` (every
> per-construct type check), and `vm.rs` arithmetic ops.

SOL has a small, static, nominal type system. There is no
inference at use sites and no implicit coercion between types.
Every `let`, every parameter, and every return type carries an
explicit annotation; every binary operator requires its operands to
have already-matching types. A type mismatch is a compile-time
error that the analyzer prints to stderr.

This chapter enumerates every type the language admits, the
operations that are valid on each, and the rules the analyzer
enforces between them.

---

## 4.1 Type universe

The parser's `Type` enum (`parser.rs:5–24`) lists every type
form. Removing the parser-internal variants that the surface syntax
cannot construct, the user-visible set is:

| Form | Spelling | Notes |
|---|---|---|
| `int` | `int` | 128-bit at parse, 64-bit at runtime (§4.2.1) |
| `float` | `float` | IEEE-754 binary64 (§4.2.2) |
| `bool` | `bool` | `true` / `false` |
| `str` | `str` | UTF-8 string, heap-allocated (§4.2.4) |
| `char` | `char` | Single Unicode scalar (§4.2.5) |
| array | `[N]T` or `[]T` | Fixed or unsized; element type required (§4.3) |
| tuple type | `(T1, T2, …)` | Accepted in type position, **no value form** (§4.4) |
| struct ref | `IdentName` | Nominal type; declared via `struct` (chapter 09) |
| enum ref | `IdentName` | Nominal type; declared via `enum` (chapter 10) |
| void | (omitted return) | The absence of a value (§4.5) |

There is **no `any` type** in SOL. The visual editor uses an "any"
marker in its graph schema, but the language itself does not. There
is also **no nullable / optional type** today; `Type::Ident` is
nominal-only.

The parser also has a `Type::Function { params, ret }` variant that
the surface syntax cannot produce — it appears only in the
analyzer's symbol table to record function signatures
(`analyzer.rs:85–89`). There is no first-class function type at the
language level.

---

## 4.2 Primitives

### 4.2.1 `int`

Source-level integer literals are parsed as `i128`
(`lexer.rs:383`). The runtime stores values as `u64` slots on the
stack and interprets them as `i64` when performing arithmetic
(`vm.rs:143–146`):

```rust
let b = self.pop() as i64; let a = self.pop() as i64;
self.push((a + b) as u64);
```

Two consequences:

1. **Integers compile to 64-bit signed at runtime.** Literals that
   exceed `i64::MAX` parse cleanly but produce truncated values when
   they reach arithmetic instructions.
2. **No `Integer` overflow check.** Rust release-mode arithmetic
   wraps on overflow; the VM uses these ops directly. Debug builds
   of the compiler may panic on overflow.

**Allowed operations** (`analyzer.rs:247–289`):

- `+`, `-`, `*`, `/` — arithmetic; operands must both be `int`
- `==`, `!=`, `<`, `<=`, `>`, `>=` — comparisons → `bool`
- `&`, `|`, `^`, `<<`, `>>` — bitwise; operands must both be `int`
- `-x` — unary negation
- `~x` — bitwise complement
- `!x` — accepted by the analyzer (treats `int` as truthy/falsy);
  prefer `bool` for boolean negation

**Division by zero** is a **runtime** error, not a compile-time one
(see [chapter 14](./14-runtime-semantics.md) and `error_runtime.sol`).
Float division is documented in §4.2.2.

### 4.2.2 `float`

Stored as `f64`. Literal form requires a digit on each side of the
decimal point — `1.0`, not `1.` (§3.2).

**Allowed operations:**

- `+`, `-`, `*`, `/` — arithmetic; operands must both be `float`
- `==`, `!=`, `<`, `<=`, `>`, `>=` — IEEE-754 ordering
- `-x` — unary negation
- `!x` — accepted by the analyzer; same caveat as for `int`

**Float division by zero** does **not** trap in the VM; it produces
IEEE `inf` / `-inf` / `NaN` per the standard (`vm.rs:159`).

There is **no mixed-precision arithmetic**: `int + float` is a
compile-time error because the analyzer requires both sides to
have the same type (`analyzer.rs:248–251`). There is also **no
conversion operator** — the language ships no built-in to cast
between `int` and `float`. If you need a value in the other type,
you must add an `ext function` that performs the conversion in the
host.

### 4.2.3 `bool`

Stored as `0` (false) or `1` (true). Produced by every comparison
operator and by the `true` / `false` literals.

**Allowed operations** (`analyzer.rs:273–280`):

- `&&` — logical and; both operands must be `bool`
- `||` — logical or; both operands must be `bool`
- `!x` — logical not; operand must be `bool` (the analyzer also
  accepts `int` / `float`; prefer `bool`)
- `==`, `!=` — equality

**`&&` and `||` are not short-circuiting at the bytecode level.**
Both operands are evaluated before the `LogAnd` / `LogOr` op runs
(`vm.rs:177–178`). If either operand has a side effect or could
itself fail at runtime, that effect happens regardless of whether
the other operand would have made the result deterministic. This is
a footgun; document the desired ordering explicitly with nested
`if` statements when it matters.

### 4.2.4 `str`

The lexer's `Token::String` carries a raw `String` (`lexer.rs:25`);
the VM stores string values as `HeapObject::String` indices
(`vm.rs:7–11, 109–112`). String values are heap-allocated and
referenced by index.

**Allowed operations:**

- `==` / `!=` — the analyzer accepts them (requires both operands
  to be `str`), but **the VM only emits integer / float / char
  comparisons** — string equality is not implemented at the bytecode
  level today. Treat `str == str` as *uncertain*. Use it sparingly
  until a fixture confirms the behavior.

There is **no string concatenation operator**. There is **no string
indexing**. There are **no length / slice / find / split builtins**.
The only thing the language guarantees you can do with a `str` is
print it.

### 4.2.5 `char`

A single Unicode scalar (`lexer.rs:24`). Stored as `u32` at runtime
(`vm.rs:321`).

**Allowed operations:**

- `==`, `!=`, `<`, `<=`, `>`, `>=` — char-level comparison, ordered
  by code point

### 4.2.6 Voids and absences

The `Void` type appears in three places:

1. As the return type of any function declared without `-> T`.
2. As the result type of statements (`if`, `while`, `for-in`,
   assignment, `print`).
3. As the type the analyzer registers for `import` aliases
   (`analyzer.rs:166–171`).

`Void` is **not** something you can write at the source level — it
has no spelling. It only exists implicitly when a return type is
omitted.

---

## 4.3 Arrays

### Type syntax

```
[N]T     // fixed-size array of N elements of type T
[]T      // dynamic / unsized array of T
```

The size `N` must be an integer literal (`parser.rs:213–219`); the
parser rejects any non-`Token::Integer` with:

```
only integers can be used to specify an array size
```

The element type `T` is any type, including another array type, a
struct, an enum, or a primitive.

### Operations

- Index read: `a[i]` — required `i` is an integer or *float* per
  the analyzer (`analyzer.rs:459`); the float-index case is almost
  certainly a bug in the analyzer (it should be `int`-only). The
  VM treats the index value as a `u64`.
- Index write: `a[i] = expr;` — only valid as a statement (the
  parser admits assignment as an expression but the only
  meaningful use is in statement position).
- Iteration: `for x in a { … }` — chapter 11.

Arrays do not have a `.length` field (the analyzer's `ExprMemAcc`
requires the LHS to be a struct, `analyzer.rs:409–414`). If you need
a length, your host must supply an `ext function` that returns it.

### Construction

```sol
let xs: [3]int = [1, 2, 3];
```

The array literal expression is documented in chapter 11. Element
types should match; the analyzer does not validate this for literal
shapes today (`ExprArrayInit` falls through `todo!()` at
`analyzer.rs:500`), but the bytecode expects homogeneous arrays at
load time.

---

## 4.4 Tuples (type-only)

The parser admits tuple *types* — `(T1, T2)` — at every type
position (`parser.rs:227–242`). There is, however, **no tuple value
literal** in the expression grammar. You can declare a parameter or
return type as a tuple, but you cannot construct one. Treat tuple
types as a parser feature with no current use, and avoid them in
hand-written SOL until a value form is added.

---

## 4.5 Structs and enums (nominal types)

A `struct Foo { … }` or `enum Bar { … }` declaration registers a
named type in the global scope. Subsequent uses of `Foo` / `Bar` in
type positions resolve to the declared shape. Nominal equality
applies — two structs with identical fields but different names are
*not* the same type.

Detail lives in chapter 09 (structs) and chapter 10 (enums).

---

## 4.6 Type equality

The analyzer uses a single `type_eq` helper (`util.rs:1–42`) to
decide whether two `Type` values are compatible. The full source
is small enough to reproduce, and several of its rules are
non-obvious or buggy in ways that matter to anyone writing
production SOL.

### The actual rules

The helper returns `Result<(), TypeMismatch>` where
`TypeMismatch` has two variants — `Inequal` (the generic
mismatch) and `ArraySize` (specifically a size disagreement on
otherwise-matching array types). At every analyzer call site
today both variants collapse into the same "cannot ... mismatched
types" diagnostic (the call sites use `.is_err()`), but the
underlying distinction is real and a future analyzer could lift
it into a more precise message.

| `lhs` | `rhs` | Result |
|---|---|---|
| Same primitive (`Integer/Float/String/Char/Bool/Void`) | Same | `Ok(())` |
| `Ident(a)` | `Ident(b)` | `Ok(())` iff `a == b` |
| `Array { size: s1, inner: i1 }` | `Array { size: s2, inner: i2 }` | `Ok(())` iff `type_eq(i1, i2)` *and* `s1 == s2`; if inner matches but sizes differ, `Err(ArraySize)` |
| `Tuple(ts1)` | `Tuple(ts2)` | `Ok(())` iff every zipped pair is `Ok` (see "tuple bug" below) |
| `Function { params: p1, ret: r1 }` | `Function { params: p2, ret: r2 }` | see "function bug" below |
| Anything else | — | `Err(Inequal)` |

### Confirmed — arrays DO compare sizes

```sol
let a: [3]int = [1, 2, 3];
let b: [5]int = a;            // analyzer: cannot assign mismatched types
```

This is **opposite** of what an earlier draft of this manual
claimed. The size comparison happens at `util.rs:12`. If you
want a function that accepts arrays of any length, declare its
parameter as `[]T` (unsized) — `Array { size: None, inner: T }`
treats `None == None` as equal, and the call site can pass any
sized or unsized array provided the inner type matches.

Sized-to-unsized matching is **also size-sensitive** because the
sizes (`Some(N)` vs. `None`) are not equal. A `[3]int`-typed
literal cannot be passed where `[]int` is declared, and vice
versa. To be safely portable, pick `[]T` everywhere unless the
size is part of the contract.

### Buggy — tuple equality truncates to the shorter length

```rust
// util.rs:17–23
Type::Tuple(types_lhs) => {
    if let Type::Tuple(types_rhs) = rhs {
        if types_lhs.iter().zip(types_rhs).any(|(l, r)| type_eq(l.to_owned(), r).is_err()) {
            Err(TypeMismatch::Inequal)
        } else { Ok(()) }
    } else { Err(TypeMismatch::Inequal) }
}
```

`iter().zip()` truncates to the shorter operand. So
`(int, int)` and `(int, int, int)` are **considered equal** —
the third element is silently ignored. There is no
length-comparison guard.

Since the surface syntax has no tuple value form (chapter 04
§4.4), this bug is currently latent — it can only fire if you
declare differently-arity tuple types in function signatures, and
the call-site rule then doesn't catch the arity mismatch. Logged
as T9007.

### Buggy — function equality ignores return types

```rust
// util.rs:24–32
Type::Function { params: params_lhs, ret: ret_lhs } => {
    if let Type::Function { params: params_rhs, ret: ret_rhs } = rhs {
        match (type_eq(*ret_lhs, Type::Void).is_ok(),
               type_eq(*ret_rhs, Type::Void).is_ok()) {
            (true, false) | (false, true) => Err(TypeMismatch::Inequal),
            _ => if params_lhs.iter().zip(params_rhs)
                    .any(|(l, r)| type_eq(l.to_owned(), r).is_err()) {
                Err(TypeMismatch::Inequal)
            } else { Ok(()) }
        }
    } else { Err(TypeMismatch::Inequal) }
}
```

Reading the match: the return-type comparison checks only
"is one void and the other non-void?". If both are void or both
are non-void, **the actual return types are not compared.** So
`function() -> int` and `function() -> str` are considered
equal — both have void-ness `false`, both have zero params.

Combined with the tuple zip-truncation, this means: two
function types with different return types and different param
counts can compare equal as long as the **non-void-ness flags
match** and the **prefix params match**.

Since function types have no surface spelling in SOL, this bug
is also currently latent — function symbols are introduced via
the analyzer's pass-1 registration (`analyzer.rs:84–89`) and
compared only when a name is resolved, not when function values
are exchanged. Logged as T9008.

### Practical takeaway

For day-to-day SOL programming, type equality behaves as you
expect for primitives and named types. The hazards above only
appear in:

- Array types — sizes are compared; pick `[]T` when you mean
  "any length".
- Tuple and function types — which today have no source-level
  user-facing role; the bugs are real but latent.

---

## 4.7 No coercion

Coercion is documented exhaustively because the language has *none*:

| From → To | Allowed? |
|---|---|
| `int` → `float` | No |
| `float` → `int` | No |
| `int` → `bool` | No (only via `== 0` etc., and the comparison's RHS must already be `int`) |
| `bool` → `int` | No |
| `char` → `int` | No |
| Any → `str` | No |

Every conversion has to be done by the host via an `ext function`.
A common pattern is to declare:

```sol
ext function to_str(n: int) -> str;
```

and call it explicitly.

---

## 4.8 Where types appear

Every site that requires a type annotation:

| Site | Required | Form |
|---|---|---|
| `let` declaration | yes | `let name: T;` or `let name: T = expr;` |
| function parameter | yes | `name: T` |
| function return | optional | `-> T` (omit ⇒ `Void`) |
| `ext function` parameter | yes | `name: T` |
| `ext function` return | optional | `-> T` (omit ⇒ `Void`) |
| `struct` field | yes | `name: T,` |
| array size | yes | integer literal between `[ ]` |

Type inference is **never** performed at the use site. There is no
`let x = 5;` form; the colon-T is mandatory.

---

## 4.9 Diagnostics related to types

| Code | Source | When |
|---|---|---|
| (parse) only integers can be used to specify an array size | `parser.rs:215` | Non-integer in `[N]T` |
| (parse) `<TOKEN>` is not valid in a type specifier | `parser.rs:245` | Type position has a token that isn't an identifier, `[`, or `(` |
| (semantic) mismatched types in arithmetic: ... | `analyzer.rs:249` | `+`/`-`/`*`/`/` with mismatched operands |
| (semantic) arithmetic operation ... not supported for type ... | `analyzer.rs:256` | Arithmetic on a non-numeric type |
| (semantic) cannot compare mismatched types | `analyzer.rs:267` | `==`/`!=`/`<`/`<=`/`>`/`>=` with mismatched operands |
| (semantic) logical operation ... requires boolean operands | `analyzer.rs:276` | `&&` / `\|\|` on non-bool |
| (semantic) bitwise operation ... requires integer operands | `analyzer.rs:285` | `&` / `\|` / `^` / `<<` / `>>` on non-int |
| (semantic) cannot negate a non number type | `analyzer.rs:311` | `-x` where `x` is not `int` / `float` |
| (semantic) can't not this type | `analyzer.rs:319` | `!x` where `x` is not `int` / `float` / `bool` |
| (semantic) cannot bitwise invert a non integer type | `analyzer.rs:327` | `~x` where `x` is not `int` |
| (semantic) cannot assign mismatched types | `analyzer.rs:293` | `=` between mismatched types |
| (semantic) Array index must be an integer or float | `analyzer.rs:460` | Index expression of the wrong type |
| (semantic) Cannot index into a non-array type | `analyzer.rs:465` | `e[i]` where `e` is not an array |

Each of these gets a full entry — bad example, fixed example, fix —
in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 4.10 Sources cited in this chapter

- `parser.rs:5–24` — `Type` enum
- `parser.rs:196–248` — type parser
- `analyzer.rs:241–303` — binary operator type rules
- `analyzer.rs:305–337` — unary operator type rules
- `analyzer.rs:455–466` — array index type rule
- `analyzer.rs:409–435` — field access type rule
- `util.rs:1–42` — `type_eq` helper (full source — including
  the tuple/function bugs documented in §4.6)
- `vm.rs:143–146` — integer arithmetic (i64)
- `vm.rs:156–166` — float arithmetic
- `vm.rs:177–178` — non-short-circuiting logical ops
- `vm.rs:7–11, 50–54, 109–112` — heap-stored strings
- `lexer.rs:21–25` — primitive literal token shapes
- Fixtures: `test_arith.sol`, `test_edge.sol`, `test_array.sol`,
  `test_struct.sol`, `error_runtime.sol`, `largemini.sol`
  (uses `string` as a type name — see chapter 20 §20.5)
