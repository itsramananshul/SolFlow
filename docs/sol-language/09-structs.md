# 09. Structs

> **Status:** Rewritten against the canonical `openprem-sol-v2`
> crate (`sol/`). Cross-checked against `sol/src/parser.rs`
> (`parse_struct`, struct-literal parsing), `sol/src/ast.rs`
> (`StructDecl`, `Field`, `Expr::StructInstance`), `sol/src/vm.rs`
> (struct construction, `MemberAccess`, `StoreField`), and
> `sol/src/value.rs` (`Value::Struct`).

A struct is a named record type: a set of fields, each with a name
and a type. Structs let a workflow thread structured data through
its steps. At runtime a struct is a map from field name to value
(`Value::Struct(HashMap<String, Value>)`), so fields are addressed
**by name**, not by position.

This chapter covers declarations, literal construction, field
access, mutation, nesting, and arrays of structs.

---

## 9.1 Declaration

```sol
struct Name {
    field_a: T1;
    field_b: T2;
    field_c: T3;
}
```

The struct body is a list of `name: Type` fields, each terminated by
a **semicolon** (`sol/src/parser.rs`, `parse_struct`: it reads a
field name, a colon, a type, then expects `;`). An empty body
`struct Empty {}` is accepted.

Each field type may be a primitive, an array, or another struct or
enum name. Declare leaf structs before composites so the names you
reference already exist:

```sol
struct Point { x: int; y: int; }
struct Nested { p: Point; label: str; }
```

Field declaration order is preserved in the AST as a `Vec<Field>`
(`sol/src/ast.rs`), so source order is stable. At runtime the value
is a `HashMap<String, Value>` keyed by field name, so field lookup
is by name and never depends on declaration order.

---

## 9.2 Struct literals

```sol
TypeName { field_a: expr, field_b: expr }
```

A struct literal supplies field-name / expression pairs separated by
**commas** (`sol/src/parser.rs`), producing
`Expr::StructInstance { name, fields }`. Note the contrast with the
declaration: declarations separate fields with semicolons, literals
separate them with commas.

The order of fields in a literal need not match the declaration,
because the runtime stores them in a name-keyed map:

```sol
struct Point { x: int; y: int; }

workflow "demo" {
    let a: Point = Point { x: 10, y: 20 };
    let b: Point = Point { y: 99, x: 11 };   # also fine
}
```

### Anonymous struct literals

The struct name is optional. An anonymous literal omits the name and
produces a `Value::Struct` with no associated nominal type:

```sol
let p = { x: 1, y: 2 };
let v: int = p.x;
```

In the AST this is `Expr::StructInstance` with an empty `name`
(`sol/src/ast.rs`).

### Partial literals

Because there is no type-checker, nothing forces a literal to supply
every declared field. A literal builds exactly the fields you write
into the runtime map; a field you omit simply will not be present,
and reading it later fails at runtime with `field '<name>' not
found` (`sol/src/vm.rs`, `MemberAccess`). Supply every field you
intend to read.

---

## 9.3 Field access

```sol
s.field
```

Postfix member access (`sol/src/parser.rs`), producing
`Expr::MemberAccess(Box<Expr>, String)`. At runtime the
`MemberAccess` instruction requires the value to be a
`Value::Struct` and looks the field up by name in the map
(`sol/src/vm.rs`). Chained access works left to right:

```sol
let v: int = nested.p.x;
```

If the value is not a struct, the runtime errors with `cannot access
field '<name>' on <value>`. If the field is absent, it errors with
`field '<name>' not found`. Both are runtime string errors, not
compile-time diagnostics.

---

## 9.4 Field mutation

```sol
s.field = expr;
```

Field assignment is a statement whose target is
`Target::MemberAccess` (`sol/src/ast.rs`). The runtime path for it
is the `StoreField` instruction: it pops the value and the struct,
inserts the field into the struct's map, and pushes the updated
struct back (`sol/src/vm.rs`). Assigning to a field of a non-struct
value errors at runtime with `cannot assign to field of non-struct`.

```sol
struct Point { x: int; y: int; }

workflow "demo" {
    let p: Point = Point { x: 0, y: 0 };
    p.x = 10;
    p.y = p.x + 5;
}
```

---

## 9.5 Nested structs

A field's type may itself be a struct:

```sol
struct Point { x: int; y: int; }
struct Nested {
    p: Point;
    label: str;
}

workflow "demo" {
    let n: Nested = Nested { p: Point { x: 7, y: 8 }, label: "point" };
    let value: int = n.p.x;
}
```

Construction nests; access chains.

---

## 9.6 Structs in arrays

Arrays of struct values use the array-literal syntax of
[chapter 11](./11-arrays.md):

```sol
let arr: []Point = [
    Point { x: 1, y: 2 },
    Point { x: 3, y: 4 }
];

let sum: int = arr[0].x + arr[1].y;
```

Each array element is a comma-free struct literal; the array
elements themselves are comma-separated.

---

## 9.7 Structs as parameters and returns

```sol
fn offset(p: Point, dx: int, dy: int) <- Point {
    p.x = p.x + dx;
    p.y = p.y + dy;
    return p;
}
```

Note the return arrow is `<-`, written before the return type. A
struct can be passed as an argument and returned as a result. Within
a function, mutating a struct binding updates that local struct
value; pass the modified struct back via `return` when the caller
needs the result.

---

## 9.8 Runtime layout

At runtime a struct is `Value::Struct(HashMap<String, Value>)`
(`sol/src/value.rs`). Construction packs the field name / value
pairs into the map; `MemberAccess` reads a field by name;
`StoreField` writes a field by name (`sol/src/vm.rs`). Because the
map is keyed by name, field operations never depend on a positional
index, so there is no field-order hazard at runtime.

---

## 9.9 Error model for structs

Struct errors are runtime string errors surfaced as a
`Failed(string)` step result. There are no compile-time struct
diagnostics and no error codes. Representative runtime messages:

| Cause | Runtime message |
|---|---|
| Field access on a non-struct | `cannot access field '<name>' on <value>` |
| Field not present | `field '<name>' not found` |
| Field assignment on a non-struct | `cannot assign to field of non-struct` |

The visual editor runs structural checks before runtime
(`src/graph/validate.ts`) with kebab-case codes such as
`unknown-struct`, `unset-struct`, `unset-field`. Those are
editor-side hints, not compiler output.

---

## 9.10 Sources cited in this chapter

- `sol/src/ast.rs`: `StructDecl`, `Field`, `Expr::StructInstance`,
  `Target::MemberAccess`, `Expr::MemberAccess`
- `sol/src/parser.rs`: `parse_struct` (semicolon-terminated
  fields), struct-literal parsing (comma-separated, optional name)
- `sol/src/vm.rs`: struct construction, `MemberAccess`,
  `StoreField`
- `sol/src/value.rs`: `Value::Struct(HashMap<String, Value>)`
- `src/graph/validate.ts`: editor-side struct checks
