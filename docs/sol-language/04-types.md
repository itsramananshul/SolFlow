# 04. Types

> **Status:** Rewritten against the canonical `openprem-sol-v2`
> crate (`sol/`). Cross-checked against `sol/src/ast.rs` (`Type`
> enum), `sol/src/parser.rs` (`parse_type`), `sol/src/vm.rs`
> (arithmetic and value ops), and `sol/src/value.rs` (runtime
> `Value`).

SOL has a small set of types. Type annotations are written on
`let` bindings, on function parameters, and on the optional return
type of a function. There is **no type-checker and no analyzer
phase** in the canonical crate. The pipeline is

```
source -> Lexer -> Parser (AST) -> Compiler (bytecode) -> Vm
```

and every fallible step returns a plain `Result<_, String>`. Type
mismatches are **not** caught at compile time. They surface at
runtime as a `Failed(string)` step result. There are no `E0xxx` or
`T90xx` error codes in the language pipeline.

This chapter enumerates every type the language admits, the
runtime values they map to, and the runtime operations defined on
them.

---

## 4.1 Type universe

The AST `Type` enum (`sol/src/ast.rs`) lists every type form:

| Form | Spelling | Notes |
|---|---|---|
| `bool` | `bool` | `true` / `false` |
| `int` | `int` | signed 64-bit, `Value::Int(i64)` (§4.2.1) |
| `float` | `float` | IEEE-754 binary64, `Value::Float(f64)` (§4.2.2) |
| `char` | `char` | single Unicode scalar, `Value::Char(char)` (§4.2.5) |
| `str` | `str` | UTF-8 string, `Value::Str(String)` (§4.2.4) |
| array | `[]T` | prefix bracket, element type required (§4.3) |
| named | `IdentName` | a struct or enum name (§4.5) |

The five primitive keywords are `bool int float char str`. Arrays
are written with a **prefix** bracket: `[]int`, `[][]float`. A
named type is any identifier, which the runtime resolves to a
struct or enum.

There is **no `any` type** in SOL. The visual editor uses an "any"
marker in its graph schema, but the language itself does not. There
is **no nullable / optional type**, **no tuple type**, and **no
first-class function type**. The runtime `Value::Unit` represents
the absence of a value (§4.6); it has no source-level spelling.

The default-type quirk to know: if a `let` omits its type
annotation, the parser records `Type::Bool` as a placeholder
(`sol/src/parser.rs`, `parse_stmt`). Since nothing type-checks the
annotation against the initializer, this placeholder is harmless at
runtime, but you should always annotate `let` bindings for clarity.

---

## 4.2 Primitives

### 4.2.1 `int`

Integers are `Value::Int(i64)` at runtime (`sol/src/value.rs`).
Integer literals lex as `i64` (`sol/src/lexer.rs`).

**Runtime operations** (`sol/src/vm.rs`):

- `+`, `-`, `*`, `/`: arithmetic. Two ints produce an int.
  Mixing an int and a float coerces the result to float.
- `==`, `!=`, `<`, `<=`, `>`, `>=`: comparisons producing `bool`.
- `-x`: unary negation.

**Division by zero** is a runtime error (a string message), for
both int and float division. It is not a compile-time diagnostic.

There are no bitwise operators in the canonical language. The
operator set is `+ - * /`, `== != < > <= >=`, `&& || !`.

### 4.2.2 `float`

Floats are `Value::Float(f64)`. A float literal needs a digit on
each side of the decimal point: `1.0`, not `1.` and not `.5`
(`sol/src/lexer.rs`).

**Runtime operations:**

- `+`, `-`, `*`, `/`: arithmetic.
- `==`, `!=`, `<`, `<=`, `>`, `>=`: IEEE-754 ordering.
- `-x`: unary negation.

Mixed arithmetic is allowed at runtime: `int op float` coerces the
int to float and produces a float (`sol/src/vm.rs`). There is no
explicit cast operator in the language; if you need an explicit
conversion, a host can register a native function for it.

### 4.2.3 `bool`

Booleans are `Value::Bool(bool)`, produced by the `true` / `false`
literals and by every comparison operator.

**Runtime operations:**

- `&&`: logical and.
- `||`: logical or.
- `!x`: logical not.
- `==`, `!=`: equality.

Truthiness for `if` / `while` conditions accepts `Bool`, or `Int`
where nonzero is true (`sol/src/vm.rs`). Using any other type as a
condition is a runtime error.

### 4.2.4 `str`

Strings are `Value::Str(String)` (`sol/src/value.rs`), an owned
UTF-8 string.

**Runtime operations:**

- `+` on two strings concatenates them (`sol/src/vm.rs`).
- `==` / `!=` compare string content.

Length is available through the `len` builtin (§4.7), which accepts
a string or an array and returns an int. There is no string
indexing operator and no slice / find / split builtin in the
language; a host can register natives for richer string work.

### 4.2.5 `char`

A single Unicode scalar, `Value::Char(char)`. A char literal is
exactly one character between single quotes, with no escape
processing in the lexer (`sol/src/lexer.rs`).

**Runtime operations:**

- `==`, `!=`, `<`, `<=`, `>`, `>=`: comparison by code point.

### 4.2.6 The unit value

`Value::Unit` is the runtime "no value". It is produced by
statements and by builtins that return nothing (such as `print`),
and it is the implicit result of a function declared without a
return type. There is no source-level spelling for unit; you cannot
write it directly.

---

## 4.3 Arrays

### Type syntax

```
[]T
```

The bracket is a **prefix**: `[]int` is an array of int, `[][]float`
is an array of arrays of float (`sol/src/parser.rs`, `parse_type`,
which reads `[`, then `]`, then the inner type). There is no sized
array form; the element count is never part of the type.

The element type `T` may be any type, including another array, a
struct, or an enum.

### Operations

- Index read: `a[i]`. The index must evaluate to an int; the `Index`
  instruction does a runtime bounds check and errors as a string
  (`index N out of bounds`) when out of range (`sol/src/vm.rs`).
- Index write: `a[i] = expr;` as a statement.
- Iteration: `for x in a { … }` (chapter 11).
- Length: `len(a)` returns the element count as an int (§4.7).

There is no `.length` field on arrays; use the `len` builtin.

### Construction

```sol
let xs: []int = [1, 2, 3];
```

Array literals are detailed in chapter 11. At runtime an array is
`Value::Array(Vec<Value>)`, which is heterogeneous in principle;
keep your arrays homogeneous so that index reads and `for-in`
bodies see a consistent element type.

---

## 4.4 No tuples

The canonical language has no tuple type and no tuple value form.
If you see `(T1, T2)` in older docs, it does not exist in the
`openprem-sol-v2` crate. Use a struct (chapter 09) to group
heterogeneous values.

---

## 4.5 Structs and enums (named types)

A `struct Foo { … }` or `enum Bar { … }` declaration introduces a
named type. Subsequent uses of `Foo` or `Bar` in a type position
parse as `Type::Named("Foo")` / `Type::Named("Bar")`. Because there
is no type-checker, a name in a type position is not validated
against the declared structs and enums at compile time; mismatches
surface only when the runtime tries to use the value.

Detail lives in chapter 09 (structs) and chapter 10 (enums).

---

## 4.6 No coercion at the type level

There is no implicit coercion **between distinct types** at the
language level, and there is no cast operator. The one place values
change representation is numeric arithmetic: when one operand is an
`int` and the other a `float`, the int is promoted to float for that
operation (`sol/src/vm.rs`). Beyond that, if you need to convert a
value, register a native function on the host or use the `to_str`
builtin (§4.7) for stringification.

---

## 4.7 Builtins that touch types

The complete set of VM builtins (`sol/src/vm.rs`):

| Builtin | Signature | Behavior |
|---|---|---|
| `print(...)` | variadic `<- unit` | space-joins its arguments, appends a newline, writes to the output buffer |
| `len(x)` | `str` or array `<- int` | element count of an array, or character count of a string |
| `to_str(x)` | any `<- str` | string form of any value |
| `type_name(x)` | any `<- str` | one of `"bool" "int" "float" "char" "str" "array" "struct" "enum" "unit" "module" "remote_ref"` |

`type_name` is the closest thing the language has to runtime type
introspection. It returns the value's runtime category as a string.

---

## 4.8 Where types appear

| Site | Required | Form |
|---|---|---|
| `let` declaration | optional (defaults to `bool` placeholder) | `let name: T = expr;` or `let name = expr;` |
| function parameter | yes | `name: T` |
| function return | optional | `<- T` (omit for no declared return type) |
| `struct` field | yes | `name: T;` |

The function return arrow is `<-`, written before the return type:

```sol
fn square(n: int) <- int {
    return n * n;
}
```

There is no `->` token in SOL. Writing `->` lexes as two separate
tokens and fails to parse.

---

## 4.9 The error model for types

There are no compile-time type errors and no error codes in the
language pipeline. A type mismatch (for example adding an int to a
struct, indexing a non-array, or dividing by zero) produces a
runtime `Failed(string)` step result with a plain message. The
editor bridge (`compiler-wasm/src/lib.rs`) wraps these into a JSON
envelope whose diagnostic vocabulary is limited to five codes
(`E_PARSE`, `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001`);
none of them is a per-type error code.

Separately, the visual editor runs structural checks on the graph
before it ever reaches the runtime (`src/graph/validate.ts`), using
kebab-case codes such as `type-mismatch`. Those are editor-side
hints, not compiler diagnostics.

---

## 4.10 Sources cited in this chapter

- `sol/src/ast.rs`: the `Type` enum (`Bool Int Float Char Str
  Array Named`) and `Expr`
- `sol/src/parser.rs`: `parse_type` (prefix `[]T`, named types),
  `parse_stmt` (`let` default type)
- `sol/src/value.rs`: the runtime `Value` enum
- `sol/src/vm.rs`: arithmetic, comparison, truthiness, `Index`
  bounds check, the builtins (`print`, `len`, `to_str`, `type_name`)
- `sol/src/lexer.rs`: literal forms, the `<-` arrow token
- `compiler-wasm/src/lib.rs`: the editor bridge and its five
  diagnostic codes
