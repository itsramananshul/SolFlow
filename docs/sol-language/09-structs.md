# 09 — Structs

> **Status:** Substantive (commit 3). Cross-checked against
> `parser.rs:487–517` (declaration), `parser.rs:683–697` (literal
> form), `analyzer.rs:142–145, 409–435, 499` (semantic handling),
> `vm.rs:7–11, 189–214` (runtime layout), and `test_struct.sol`.

A struct is a named record type: a set of fields, each with its
own name and type. SOL's struct system is small but complete enough
to model the data a workflow needs to thread through its steps.
Structs are value types from the language's point of view; the
runtime stores them on a heap and references them by index.

This chapter covers declarations, literal construction, field
access, mutation, nesting, and the practical hazards a writer
should know about.

---

## 9.1 Declaration

```sol
struct Name {
    field_a: T,
    field_b: T,
    field_c: T,
}
```

Parsed at `parser.rs:487–517`. The body is comma-separated `name: T`
pairs; a trailing comma is optional. The empty body `struct Empty {}`
is accepted (`test_struct.sol::test_empty_struct`).

Each field is a `name: type` pair. Field types may be primitives,
arrays, or other struct/enum types. Forward references between
structs work at the analyzer level — the two-pass design that lets
functions call each other before declaration also lets structs
mention each other, but the *use site* of a struct type must be
inside a function body or a later top-level decl, not in another
struct's field type (the parser walks struct decls top-down). In
practice, declare leaf structs before composites:

```sol
struct Point { x: int, y: int }
struct Nested { p: Point, label: str }
```

### Field-order hazard

The parser stores fields in a `HashMap<String, Type>`
(`parser.rs:48`). Two consequences:

1. **Declaration order is not preserved internally.** Any tool that
   reads the AST and writes it back may reorder fields. Hand-written
   source preserves order on disk, but a round-trip through the AST
   does not.
2. **Variant iteration order is unspecified.** Code that pretty-prints
   a struct cannot rely on fields appearing in declaration order
   unless it carries its own ordering metadata.

This is recorded as blocker #5 in `SOL_CRATE_IDE_READINESS_PLAN.md`
§1. Until the compiler switches to an order-preserving container,
treat struct fields as **named, not positional**, and never write
code (or tools) that depends on a particular print order.

---

## 9.2 Struct literals

```sol
TypeName { field_a: expr, field_b: expr, … }
```

Parsed at `parser.rs:683–697`. Every literal supplies field-name /
expression pairs, comma-separated. The order in the literal need
not match the order in the declaration:

```sol
struct Point { x: int, y: int }

let a: Point = Point { x: 10, y: 20 };
let b: Point = Point { y: 99, x: 11 };   // also fine
```

Demonstrated by `test_struct.sol::test_field_order`.

### Partial literals

The parser does not enforce that every declared field appear in a
literal. The analyzer's `ExprStructInit` branch is currently a
fall-through to `todo!()` (`analyzer.rs:499`), so missing-field
detection is not implemented today. A struct literal that omits a
field will leave that field's underlying slot zero-initialized at
runtime, which is almost never what you want. **Always supply
every field.**

This gap is queued for fix in the upstream audit
(`SOL_CRATE_IDE_READINESS_PLAN.md` §1, blocker #18).

### Literals in conditions — parens required

Struct literals are disabled inside `if` / `while` / `for-in`
conditions because of an unavoidable ambiguity with the body block.
Wrap in parentheses to use:

```sol
if (Point { x: 0, y: 0 }) { … }    // explicit grouping
```

See [chapter 03 §3.5](./03-syntax.md) and [chapter 07 §7.1](./07-control-flow.md).

---

## 9.3 Field access

```sol
s.field
```

Left-associative postfix (`parser.rs:608–617`). The analyzer's
`ExprMemAcc` (`analyzer.rs:409–435`) requires the LHS to be of type
`Type::Ident(struct_name)`. Chained access works left-to-right:

```sol
let v: int = nested.point.x;
```

Demonstrated by `test_struct.sol::test_struct_in_struct`.

### Diagnostics

| Cause | Diagnostic |
|---|---|
| LHS is not a struct | `<TYPE> is not a struct with members` |
| Struct name unresolved | `could not find struct <NAME> in scope` |
| Name resolved but not a struct | `<NAME> is not a struct` |
| Field absent | `<STRUCT> has no member <FIELD>` |

---

## 9.4 Field mutation

```sol
s.field = expr;
```

Parsed as an `ExprBinary` with `=` operator on top of a `ExprMemAcc`
LHS. The type-check rule is the same as plain assignment: LHS and
RHS types must match (chapter 06 §6.2 + chapter 08 §8.2).

Demonstrated by `test_struct.sol::test_mutate_field` and
`::test_mutate_person`.

---

## 9.5 Nested structs

A field's type may itself be a struct:

```sol
struct Point { x: int, y: int }
struct Nested {
    p: Point,
    label: str,
}

let n: Nested = Nested { p: Point { x: 7, y: 8 }, label: "point" };
let value: int = n.p.x;
```

Construction nests; access chains. Demonstrated by
`test_struct.sol::test_struct_in_struct`.

---

## 9.6 Structs in arrays

Arrays of struct values are built with the array-literal syntax of
[chapter 11](./11-arrays.md):

```sol
let arr: []Point = [
    Point { x: 1, y: 2 },
    Point { x: 3, y: 4 },
];

let sum: int = arr[0].x + arr[1].y;
```

Demonstrated by `test_array.sol::test_array_of_struct`.

---

## 9.7 Structs as parameters and returns

```sol
function offset(p: Point, dx: int, dy: int) -> Point {
    p.x = p.x + dx;
    p.y = p.y + dy;
    return p;
}
```

Passing a struct as an argument and returning one work as
expected. **Mutation semantics are *uncertain* today** — the
runtime stores structs as heap objects (`vm.rs:7–11`) addressed by
heap-index references, so the question is whether passing a struct
to a function copies the heap object or shares the reference. The
fixture `test_struct.sol::test_pass_struct` exercises the pass case
but does not write to the struct in the callee, so it doesn't
distinguish copy from share. Treat parameter passing as **likely
reference semantics** until a fixture or bytecode reading confirms.

> **Uncertain.** A future commit will inspect `bytecode.rs` for the
> struct-load and struct-store ops to answer this definitively. The
> safe defensive habit until then: don't write to struct parameters
> if the caller relies on the original being unchanged.

---

## 9.8 Runtime layout

At runtime a struct value is a `HeapObject::Struct(Vec<u64>)`
(`vm.rs:7–11`). Construction (`Inst::NewStruct(n)`) pops `n`
field values from the stack, packs them into a `Vec<u64>`, pushes
the heap onto the end, and pushes the new heap index onto the
stack (`vm.rs:189–196`). Field access (`Inst::GetField(idx)`) pops
the heap index, indexes into the field vector by *positional*
index, and pushes the field value.

The use of a positional field index in the bytecode is what makes
the **field-order hazard in §9.1** so consequential: the order in
which fields are emitted into the struct vector is determined by
the iteration order of the parser's field `HashMap`, which is not
stable across runs. The current compiler appears to work because
emission and access happen in the same program run, so the order
is internally consistent — but two separate compilations of the
same source may pick different orders, and any external tool that
constructs a struct value via a separate emission step risks
mismatching the order.

> **Confirmed.** The hazard exists. The current compiler protects
> itself by emission/access being in lockstep within one run. Any
> serialization, persistence, or cross-compile sharing of struct
> values would expose the bug.

---

## 9.9 Common diagnostics

| Diagnostic | Cause | Fixture |
|---|---|---|
| `<TYPE> is not a struct with members` | `.field` on a non-struct | n/a |
| `could not find struct <NAME> in scope` | Field access on an unknown type name | n/a |
| `<STRUCT> has no member <FIELD>` | Field name not in the struct | n/a |
| `error: redefinition of <NAME>` | Duplicate `struct` declaration | n/a |
| `expected `{` after enum declaration` (yes, the error misnames "enum") | Struct body missing `{` | n/a |
| `expected identifier for a field name in struct declaration` | Field-list entry isn't an identifier | n/a |
| `expected colon after field name` | Missing `:` between field name and type | n/a |
| `expected `}` to close struct declaration` | Missing closing brace | n/a |

Full entries (bad / fixed examples) live in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 9.10 Sources cited in this chapter

- `parser.rs:48` — struct field storage (`HashMap`)
- `parser.rs:487–517` — struct declaration parser
- `parser.rs:683–697` — struct literal parser
- `analyzer.rs:142–145` — struct symbol registration
- `analyzer.rs:409–435` — field access type rule
- `analyzer.rs:499` — `ExprStructInit` is `todo!()` fallthrough
- `vm.rs:7–11` — heap object layout
- `vm.rs:189–214` — struct construction and field load/store
- Fixtures: `test_struct.sol`, `test_array.sol` (arrays of structs)
