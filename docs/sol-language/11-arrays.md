# 11. Arrays

> **Status:** Rewritten against the canonical `openprem-sol-v2`
> crate (`sol/`). Cross-checked against `sol/src/parser.rs`
> (`parse_type` for array types, array-literal parsing), `sol/src/vm.rs`
> (the `Index` instruction and `for-in` execution), `sol/src/value.rs`
> (`Value::Array`), and `sol/src/ast.rs`.

A SOL array is an integer-indexed sequence of runtime values. Arrays
are constructed with a literal expression, read with indexing,
written with index assignment, and iterated with `for-in`. The `len`
builtin returns the element count. There is no `.length` field and
no slice or concat helper.

---

## 11.1 Array type syntax

One form:

```
[]T
```

The bracket is a **prefix** and is always empty: `[]int`, `[]str`,
`[]Point`. The parser reads `[`, then `]`, then the element type
(`sol/src/parser.rs`, `parse_type`), producing
`Type::Array(Box<Type>)` (`sol/src/ast.rs`). There is no sized array
form (`[N]T` does not exist) and no postfix form (`T[]` does not
exist).

Nested arrays stack the prefix: an array of arrays of int is
`[][]int`, an array of arrays of `Point` is `[][]Point`.

The element type `T` may be any type: a primitive, another array, a
struct, or an enum.

---

## 11.2 Array literal

```sol
[expr1, expr2, expr3]
[]
```

Array literals are comma-separated expressions between `[ ]`
(`sol/src/parser.rs`), producing `Expr::Array(Vec<Expr>)`. The
empty literal `[]` is accepted; its element type comes from the
surrounding `let` annotation:

```sol
let xs: []int = [];
```

### Type uniformity

There is no type-checker, so the parser and compiler do not validate
that every element shares a type. At runtime an array is
`Value::Array(Vec<Value>)`, which can technically hold mixed values.
Keep array literals homogeneous so that index reads and `for-in`
bodies behave consistently; a mixed array will only error if a later
operation encounters an unexpected element type.

---

## 11.3 Indexing (read)

```sol
arr[i]
```

Postfix index access (`sol/src/parser.rs`), producing
`Expr::Index(Box<Expr>, Box<Expr>)`. At runtime the `Index`
instruction pops the index and the array, requires the array to be
`Value::Array` and the index to be `Value::Int`, and reads the
element (`sol/src/vm.rs`):

```sol
let arr: []int = [10, 20, 30];
let middle: int = arr[1];      # 20
```

### Out-of-bounds is a runtime error

The index is dynamic, so range validity is a **runtime** check, not
a compile-time one. The `Index` instruction does a bounds check and,
when the index is out of range, fails the step with a plain string
message:

```
index N out of bounds
```

Indexing a non-array value, or indexing with a non-int, likewise
fails at runtime with a string message such as `cannot index <v>
with <i>`. None of these are compile-time diagnostics; they appear
as a `Failed(string)` step result.

---

## 11.4 Indexing (write)

```sol
arr[i] = expr;
```

Index assignment is a statement whose target is `Target::Index`
(`sol/src/ast.rs`). The runtime evaluates the array, the index, and
the value, and stores the value into the array slot. As with reads,
an out-of-range or wrong-typed index is a runtime error, not a
compile-time one.

---

## 11.5 `for-in` iteration

```sol
for elem_name in array_expr {
    body
}
```

`for-in` has **no parentheses** around the header (`sol/src/parser.rs`).
The runtime expects `array_expr` to evaluate to an array and binds
`elem_name` to each element in order (`sol/src/vm.rs`). There is no
`break` or `continue`; use `return` to exit a function early.

```sol
let xs: []int = [10, 20, 30];
let sum: int = 0;
for item in xs {
    sum = sum + item;
}
```

```sol
let people: []Person = [];
for p in people {
    if (p.active) {
        print(p.name);
    }
}
```

---

## 11.6 Arrays of structs

Arrays may hold struct values:

```sol
struct Point { x: int; y: int; }

workflow "demo" {
    let arr: []Point = [
        Point { x: 1, y: 2 },
        Point { x: 3, y: 4 }
    ];

    let total: int = arr[0].x + arr[0].y + arr[1].x + arr[1].y;
}
```

Field access chains through index access naturally: `arr[i].field`.
Note the struct fields are declared with **semicolons**
(`x: int; y: int;`) while the struct-literal and array-literal
elements are separated by **commas**.

---

## 11.7 Length and what arrays do not have

| Feature | Status |
|---|---|
| `len(arr)` | Supported. The `len` builtin returns the element count as an int (`sol/src/vm.rs`) |
| `.length` field | Not supported; use `len(arr)` |
| `.push(x)` / `.pop()` | No methods; the language has no method-call syntax |
| Slicing (`arr[1:3]`) | Not supported |
| Repeat literal (`[0; 10]`) | Not supported |
| Concatenation (`arr1 + arr2`) | Not supported; `+` is defined on numbers and on two strings |
| Comparison (`arr1 == arr2`) | Not part of the documented value operations; do not rely on it |
| Negative indexing | Not supported; a negative index fails the runtime bounds check |
| Range iteration (`for i in 0..n`) | No range syntax; use a `while` loop with a counter |

If you need richer array behavior, a host can register native
functions that the workflow calls.

---

## 11.8 Runtime layout

At runtime an array is `Value::Array(Vec<Value>)` (`sol/src/value.rs`).
Each element is itself a full `Value`, so arrays can nest arrays,
hold structs, hold enums, and so on. The `len` builtin reads the
vector length; the `Index` instruction reads or bounds-checks a
single slot; `for-in` walks the vector in order.

---

## 11.9 Error model for arrays

Array errors are runtime string errors, surfaced as a
`Failed(string)` step result. There are no compile-time array
diagnostics and no error codes. Representative runtime messages:

| Cause | Runtime message |
|---|---|
| Index out of range | `index N out of bounds` |
| Index on a non-array value | `cannot index <value> with <index>` |
| Non-int index | `cannot index <value> with <index>` |

The visual editor may flag some array-shape problems before runtime
through its graph checks (`src/graph/validate.ts`), but those are
editor-side hints, not compiler output.

---

## 11.10 Sources cited in this chapter

- `sol/src/ast.rs`: `Type::Array`, `Expr::Array`, `Expr::Index`,
  `Target::Index`, `Stmt::For`
- `sol/src/parser.rs`: array type (`parse_type`), array literal,
  index access, `for-in` header
- `sol/src/vm.rs`: the `Index` instruction (bounds check), `for-in`
  execution, the `len` builtin
- `sol/src/value.rs`: `Value::Array(Vec<Value>)`
