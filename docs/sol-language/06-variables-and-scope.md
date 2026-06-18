# 06 — Variables and Scope

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate.
> Cross-checked against `sol/src/parser.rs` (`parse_stmt` for `let` and
> assignment, `expr_to_target`), `sol/src/ast.rs` (`Stmt::Let`,
> `Stmt::Assign`, `Target`), `sol/src/compiler.rs` (`get_or_add_local`,
> `find_local`, target handling), and `sol/src/vm.rs`
> (`LoadLocal`/`StoreLocal`/`LoadName`).

SOL has one kind of variable: a binding introduced by `let`, written by
assignment, and resolved by name. There is no `const` keyword and no
mutable/immutable distinction; every `let` is freely reassignable.

There is no type checker. A `let` records a declared type on the AST,
but nothing validates it against the initializer. Mismatches, when they
matter at all, surface at runtime as plain string errors.

---

## 6.1 Declaration

```sol
let name: Type = value;     # declaration with type and initializer
let name = value;           # type omitted (see the bool default below)
```

Parsed by `parse_stmt` in `sol/src/parser.rs`. An initializer is
**required**: the parser reads `let`, then the name, then an optional
`: Type`, then expects `=` and an expression. The trailing `;` is
consumed when present.

### The type annotation defaults to bool

If you omit `: Type`, the parser does not infer the type. It records
`Type::Bool` on the `Stmt::Let` node by default:

```sol
let x = 42;     # the AST type is `bool`, even though the value is an int
```

The declared type is otherwise inert (no type checking happens), so this
quirk does not change runtime behavior. But it is misleading to read.
**Annotate the type whenever the value is not a bool** so the source
reflects intent:

```sol
let x: int = 42;
```

### Examples

```sol
let amount: int = 100;
let label: str = "ready";
let p: Point = Point { x: 1, y: 2 };
let flag = true;            # bool, annotation legitimately omitted
```

### What is not allowed

```sol
let x: int;          # parse error: `=` and an initializer are required
let x: int = ;       # parse error: an expression must follow `=`
```

There is no top-level `let`. A `let` is a statement and is only valid
inside a workflow body (or a block within it). Top-level items are
limited to imports, functions, structs, enums, and the workflow.

---

## 6.2 Assignment

```sol
name = value;
obj.field = value;
arr[i] = value;       # parses, but see the codegen note below
```

Parsed in `parse_stmt`: an expression is parsed, and if `=` follows, the
parsed expression is converted to an assignment target via
`expr_to_target`. A valid target is one of three shapes
(`Target` in `sol/src/ast.rs`):

- an identifier, `Target::Ident`
- a member access, `Target::MemberAccess` (`obj.field`)
- an index, `Target::Index` (`arr[i]`)

Any other left hand side fails with `invalid assignment target: ...`.

Assignment is a statement, not an expression. There is no chained
`a = b = c` form: the right hand side is parsed as an expression, and
`=` is not part of the expression grammar.

### Plain assignment to a variable

```sol
let count: int = 0;
count = count + 1;       # OK
```

The compiler resolves the target name to a local slot with
`find_local`. Assigning to a name that was never introduced by a `let`
fails at compile time with `variable '<name>' not found for assignment`.

### Assignment to a struct field

```sol
obj.field = value;
```

Single level member assignment is supported by the compiler: it loads
the root local, stores the new value into the field with `StoreField`,
and writes the modified struct back to the root local. The root of the
member chain must be a local variable.

### Assignment to an array element

```sol
arr[i] = value;
```

This parses into a `Target::Index`, but the compiler does NOT emit code
for it. Index assignment compiles to the error
`index assignment not supported`. Avoid `arr[i] = value;` in canonical
SOL; rebuild the array or model the data with a struct instead.

---

## 6.3 Scope and the locals model

The VM does not use lexical block scopes with push/pop frames. Instead,
the compiler maintains a flat list of named local slots for the whole
workflow body (`get_or_add_local` / `find_local` in
`sol/src/compiler.rs`). Each distinct name gets one slot index; the VM
holds a `locals` vector of values and reads/writes those slots with
`LoadLocal` and `StoreLocal`.

Consequences worth knowing:

- **Names are workflow wide.** A `let` for a name reuses the same slot
  everywhere that name appears, including inside `if`, `while`, and
  `for` bodies. There is no block local shadowing; re-using a name in a
  nested block writes the same slot.
- **A `let` after an `if`/`while`/`for` is in the same flat namespace.**
  Bodies of control-flow statements do not open a new slot namespace.
- **`for item in ...` introduces `item` as a local slot** plus two
  internal helper slots for the iterator and index (see chapter 07).
  The `item` slot persists after the loop, since slots are not popped.

### Reading a name

When the compiler encounters an identifier, it first tries to resolve it
to a local slot (`LoadLocal`). If the name is not a known local, it
emits `LoadName`, which the VM resolves by searching the recorded local
names at runtime. If the name is not found there either, the VM raises
the runtime string error `variable '<name>' not found`.

```sol
workflow "demo" {
    print(x);            # x was never declared; runtime error at this read
    let x: int = 5;
}
```

There is no compile time use before declaration diagnostic for a read.
The error appears at runtime when the unknown name is loaded.

---

## 6.4 Mutability

There is no immutability marker. Every `let` is reassignable with `=`,
and no type rule restricts the new value. If you want a constant, the
only convention available is naming (for example SCREAMING_SNAKE_CASE)
and discipline. Nothing in the language enforces it.

---

## 6.5 Common runtime and compile errors

These are plain string messages. The editor bridge classifies them
(see chapter 8 and chapter 12): codegen failures surface as
`E_CODEGEN`, runtime failures as `E_RUNTIME`. There are no `E00xx` or
`T90xx` codes.

| Message | When |
|---|---|
| `variable '<name>' not found` | Runtime: a read of a name with no local slot |
| `variable '<name>' not found for assignment` | Compile time: assigning to an undeclared name |
| `index assignment not supported` | Compile time: `arr[i] = value;` |
| `invalid assignment target: ...` | Compile time: `=` after a non assignable expression |
| `cannot access field '<f>' on <value>` | Runtime: member access on a non struct |

---

## 6.6 Sources cited in this chapter

- `sol/src/parser.rs` — `parse_stmt` (`let`, assignment), `expr_to_target`
- `sol/src/ast.rs` — `Stmt::Let`, `Stmt::Assign`, `Target`
- `sol/src/compiler.rs` — `get_or_add_local`, `find_local`,
  `compile_stmt` (`Let`, `Assign`), member/index target handling
- `sol/src/vm.rs` — `LoadLocal`, `StoreLocal`, `LoadName`, `StoreField`
- `compiler-wasm/src/lib.rs` — `E_CODEGEN` / `E_RUNTIME` classification
