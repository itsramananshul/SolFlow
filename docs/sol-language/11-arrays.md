# 11 — Arrays

> **Status:** Substantive (commit 3). Cross-checked against
> `parser.rs:210–225` (array type parser), `parser.rs:726–739`
> (array literal parser), `parser.rs:618–623` (index access),
> `analyzer.rs:201–217, 455–466` (semantic handling),
> `vm.rs:7–11` (heap layout), and the fixtures `test_array.sol`,
> `test_control.sol`.

A SOL array is a homogeneous, integer-indexed sequence of values
stored on the runtime heap. Arrays are constructed with a literal
expression, written into via assignment, read out of via indexing,
and iterated with `for-in`. There is no built-in `length`
operator and no slice or concat helper.

---

## 11.1 Array type syntax

Two forms (`parser.rs:210–225`):

```
[N]T      // size N (an integer literal), element type T
[]T       // unsized — element count not part of the type
```

The size `N` must be an `INTEGER` literal token. The parser refuses
anything else:

```
only integers can be used to specify an array size
```

The element type `T` is any type — primitive, array, struct, or
enum. Nested arrays are written as `[][]int`, `[3][]Point`, etc.

### When to use sized vs unsized

The two forms are interchangeable for the analyzer's `type_eq`
helper, which compares only the inner element type and ignores the
size (chapter 04 §4.6). Idiomatic SOL uses `[]T` everywhere the
literal supplies the size implicitly. Reserve `[N]T` for cases
where the size is part of the contract — e.g. "this function takes
a 3-element vector".

> **Uncertain.** The runtime's behavior when a sized `[3]int`
> variable is given a 5-element literal hasn't been audited. The
> bytecode emitter (commit 4) will resolve this.

---

## 11.2 Array literal

```sol
[expr1, expr2, expr3]
[]
```

Parsed at `parser.rs:726–739`. Comma-separated expressions inside
`[ ]`. The loop terminates the first time it encounters a non-comma
token, so a trailing comma is **not** accepted (the loop would
break after the last comma's element with the next token being `]`,
which is the close).

The empty literal `[]` is accepted; the element type is determined
by the surrounding context (typically a `let` annotation):

```sol
let xs: []int = [];
```

Demonstrated by `test_control.sol::test_for_empty`.

### Type uniformity

All elements of an array literal *should* share a type. The
analyzer does not currently validate this — `ExprArrayInit` falls
through the analyzer's `todo!()` catch (`analyzer.rs:500`). The
bytecode emitter and VM expect homogeneous arrays; passing a
heterogeneous literal is undefined behavior at runtime.

---

## 11.3 Indexing (read)

```sol
arr[i]
```

Postfix, left-associative (`parser.rs:618–623`). The LHS must be of
type `Type::Array { … }`; the index expression must be `int` (the
analyzer also accepts `float` by what is almost certainly a typo;
`analyzer.rs:459`).

```sol
let arr: []int = [10, 20, 30];
let middle: int = arr[1];      // 20
```

Demonstrated by `test_array.sol::test_array_basic`.

### Diagnostics

| Cause | Diagnostic |
|---|---|
| LHS not an array | `Type Error: Cannot index into a non-array type` |
| Index wrong type | `Type Error: Array index must be an integer or float` |

(Both at `analyzer.rs:459–466`.)

### Out-of-bounds

The analyzer cannot tell whether an index is in range — the value
is dynamic. The runtime behavior of `arr[N]` where `N` is out of
bounds is *uncertain* — to be resolved by reading the bytecode op
that backs index access in commit 4. Until then, treat
out-of-bounds as undefined behavior and validate ranges
defensively.

---

## 11.4 Indexing (write)

```sol
arr[i] = expr;
```

Parses as a `ExprBinary { op: Eq }` over a `ExprArrAcc` LHS. The
analyzer's `Eq` rule (`analyzer.rs:291–297`) requires matching
types between the element type and the RHS.

Demonstrated by `test_array.sol::test_array_write`,
`::test_array_mutate_in_loop`.

---

## 11.5 `for-in` iteration

```sol
for elem_name in array_expr {
    body
}
```

Covered in detail in [chapter 07 §7.3](./07-control-flow.md).
Recap relevant for array users:

- `array_expr` is type-checked as an array; the iteration variable
  inherits the element type.
- The iteration variable *leaks into the enclosing scope* (chapter
  06 §6.5). Wrap the loop in an extra block if you want the
  binding tightly scoped.
- There is no `break` or `continue`. Use `return` to exit early.

Examples:

```sol
let xs: []int = [10, 20, 30];
let sum: int = 0;
for item in xs {
    sum = sum + item;
}
```

```sol
let people: []Person = [...];
for p in people {
    if (p.active) {
        print(p.name);
    }
}
```

---

## 11.6 Arrays of structs

Arrays may hold struct values. Construction uses the obvious form:

```sol
struct Point { x: int, y: int }

let arr: []Point = [
    Point { x: 1, y: 2 },
    Point { x: 3, y: 4 },
];

let total: int = arr[0].x + arr[0].y + arr[1].x + arr[1].y;
```

Demonstrated by `test_array.sol::test_array_of_struct`.

Field access chains through index access naturally: `arr[i].field`.

---

## 11.7 What arrays do not have

| Feature | Status |
|---|---|
| `.length` | Not supported. Field access requires the LHS to be a struct (`analyzer.rs:409–414`). Maintain a counter alongside the array, or get the length from an `ext function` |
| `.push(x)` / `.pop()` | No mutating methods exist; no method syntax in the language at all |
| Slicing (`arr[1:3]`) | Not supported |
| Repeat literal (`[0; 10]`) | Not supported |
| Concatenation (`arr1 + arr2`) | Not supported; `+` requires `int` or `float` operands |
| Comparison (`arr1 == arr2`) | Parser accepts; runtime not implemented; treat as `Uncertain` |
| Negative indexing | Not supported semantically; behavior is undefined |
| Iterator-from-range (`for i in 0..n`) | No range syntax exists; use `while` with a counter |

If you need any of these, the convention is to define an
`ext function` that the host implements.

---

## 11.8 Runtime layout

Arrays live on the runtime heap as `HeapObject::Array(Vec<u64>)`
(`vm.rs:7–11`). Each `u64` slot is either a primitive value
(int / float / bool / char) or a heap index pointing to a string
or composite. Stack values for array-typed variables carry the
heap index of the array object.

This means:

- Passing an array to a function passes the heap reference, not a
  copy. Mutation through one binding is visible through the other.
  (*Uncertain* — confirmed indirectly by the heap-index model;
  to be cross-checked against `bytecode.rs` in commit 4.)
- Two array literals always allocate distinct heap entries, so they
  are never reference-equal.
- The array's slot vector is heap-resident; iteration via `for-in`
  reads slots in order.

---

## 11.9 Common diagnostics

| Diagnostic | Cause | Fixture |
|---|---|---|
| `only integers can be used to specify an array size` | `[N]T` with non-integer N | n/a |
| `expected `]` after array size` | Missing `]` in array type | n/a |
| `expected `]` to close an array initializer` | Missing `]` in array literal | n/a |
| `array in which for loop is iterating over must have the known type `Array`` | Iterable isn't an array | n/a |
| `Type Error: Array index must be an integer or float` | Bad index type | n/a |
| `Type Error: Cannot index into a non-array type` | `[ ]` on a non-array | n/a |

Full entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 11.10 Sources cited in this chapter

- `parser.rs:210–225` — array type parser
- `parser.rs:618–623` — index access (postfix)
- `parser.rs:726–739` — array literal parser
- `analyzer.rs:201–217` — `for-in` type rule
- `analyzer.rs:455–466` — index access type rule
- `analyzer.rs:500` — `ExprArrayInit` is `todo!()` fallthrough
- `vm.rs:7–11` — heap object layout
- Fixtures: `test_array.sol`, `test_control.sol` (empty / single /
  nested for-in)
