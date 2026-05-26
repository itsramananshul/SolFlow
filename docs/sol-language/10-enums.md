# 10 — Enums

> **Status:** Substantive (commit 3). Cross-checked against
> `parser.rs:518–558` (declaration), `parser.rs:699–706` (variant
> reference), `analyzer.rs:146–149, 437–454` (semantic handling),
> and `test_edge.sol`.

A SOL enum is a small set of named integer constants gathered
under one type name. Each variant is a plain tag — there is **no
payload** form (no `Variant(T1, T2)`), no pattern matching, and no
exhaustiveness checking. Variants compare via `==` / `!=`.

This chapter documents the declaration syntax, the auto-numbering
("iota") rule, how variants are referenced, and the small but
important contradictions between the parser-level numbering rule
and one fixture's assertions.

---

## 10.1 Declaration

```sol
enum Name {
    Variant1,
    Variant2 = 5,
    Variant3,
}
```

Parsed at `parser.rs:518–558`. Each variant is an identifier
optionally followed by `= INTEGER`. A trailing comma is optional;
empty bodies `enum Empty {}` are parser-accepted.

### Auto-numbering (the iota rule)

The parser walks variants top-down with a running counter
(`parser.rs:530–550`). Pseudocode of the algorithm:

```
iota = 0
for each variant V [ = N ] in source order:
    if V has an explicit `= N`:
        iota = N
    record V → iota
    iota += 1
```

So:

```sol
enum E {
    A,        // 0
    B,        // 1
    C = 5,    // 5
    D,        // 6
    E,        // 7
}
```

The first variant gets `0` unless it carries `= N`. An explicit
value resets the counter; the next variant gets the explicit value
plus one.

### Variant-order hazard

As with struct fields, the parser stores variants in a
`HashMap<String, isize>` (`parser.rs:52`). Printed iteration order
on round-trip is unspecified. Don't write code or tools that
depend on a particular printed order.

The numeric value assigned to each variant by the iota rule **is**
stable for a given source text — it's the source order that drives
the counter, not the `HashMap` iteration order. Only the
*printed-back* order is unspecified.

---

## 10.2 Variant reference

```sol
EnumName::VariantName
```

Parsed at `parser.rs:699–706`. The result is a value of type
`EnumName`. At the analyzer level the value is treated as having
type `Type::Ident(EnumName)`; at the runtime level it is the
underlying integer.

### Diagnostics

| Cause | Diagnostic |
|---|---|
| Enum name unresolved | `could not find struct <NAME> in scope` (the message says "struct" but applies to enum lookup too — analyzer reuses the helper) |
| Name resolved but not an enum | `<NAME> is not an enum` |
| Variant absent | `<NAME> has no variant <VAR>` |

Source: `analyzer.rs:437–454`.

---

## 10.3 Type interaction

Because variants resolve to `Type::Ident(EnumName)`, comparing two
variants of the *same* enum works:

```sol
enum Status { Active, Inactive }

let s: Status = Status::Active;
if s == Status::Active { print("on"); }
```

Comparing variants from two **different** enums fails type-checking
because the analyzer's `==` rule (`analyzer.rs:263–271`) requires
the operands to have the same type.

```sol
enum A { X, Y }
enum B { X, Y }
let a: A = A::X;
let b: B = B::X;
if a == b { … }    // cannot compare mismatched types
```

### Assignment to an `int` variable

`test_edge.sol::test_enum_var` assigns an enum variant directly into
an `int` variable:

```sol
let status: int = MyEnum::Active;
```

The analyzer's `let` branch doesn't walk the initializer
(chapter 06 §6.1), so this compiles. At runtime the value is the
integer underlying the variant. Treat this as an *implementation
detail* — the language doesn't define a coercion from enum to
`int`; the assignment works only because of the unchecked
initializer path. Idiomatic SOL keeps enum values typed as their
enum.

---

## 10.4 No pattern matching

SOL has no `match` construct. Conditional dispatch on enum
variants uses `if` / `else` chains:

```sol
if status == Status::Active {
    handle_active();
} else if status == Status::Inactive {
    handle_inactive();
} else {
    handle_other();
}
```

There is no exhaustiveness check; the compiler won't warn that you
forgot a variant.

---

## 10.5 The runtime value: parser iota vs. bytecode hash

**Resolved (commit 4).** The runtime value of an enum variant is
*not* the iota number recorded by the parser. It is a hash of the
variant's first character.

The bytecode emitter handles every `ExprEnumVar { var, … }` node
with a single line (`bytecode.rs:538–541`):

```rust
let variant_hash = var.chars().next().unwrap_or('A') as i128 % 10;
insts.push(Inst::PushConst(Ast::ExprInteger(variant_hash)));
```

So at runtime:

| Source | First char | ASCII | `% 10` |
|---|---|---|---|
| `MyEnum::Inactive` | `I` | 73 | **3** |
| `MyEnum::Active`   | `A` | 65 | **5** |
| `MyEnum::Busy`     | `B` | 66 | **6** |

That is exactly what `test_edge.sol::start` asserts. The fixture's
expectations are *correct* for the bytecode-level behavior; the
parser's iota algorithm is essentially dead — its computed values
sit in the AST but are never emitted.

### Consequences

1. **Two variants with the same first character collide at
   runtime.**
   ```sol
   enum Status { Active, Aborted }
   // Status::Active == Status::Aborted  →  true at runtime ('A' % 10 == 'A' % 10)
   ```
   This is a hard implementation bug, not a language feature. Until
   the bytecode is fixed, **make every variant in an enum start
   with a distinct first character**. With at most 10 single-byte
   first characters mapping to the same residue, the collision
   window is tighter than it looks — `A` and `K` collide (75 % 10
   = 5), `B` and `L` collide (76 % 10 = 6), etc.

   **Real-world case study.** `gemini_long.sol`'s `AppHealth`
   enum (`Offline, Initializing, Stable = 200, Overloaded = 503`)
   collides on two pairs at runtime — `Offline`/`Overloaded` both
   map to `9`, and `Stable`/`Initializing` both map to `3`. The
   program's entire enum dispatch is therefore wrong at runtime.
   See chapter 16 §16.3 for the full collision table.
2. **Explicit `= N` assignments are silently ignored.** A variant
   written `Foo = 100` will still emit `'F' % 10 = 0` at the
   bytecode level. The audit notes this in
   [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) as a known runtime
   mismatch (`T9002`).
3. **A SOL program that depends on enum values being *small,
   non-overlapping, source-ordered integers* will not behave the
   way it reads.** Use enums for tagged comparison only
   (`status == Status::Active`), not for arithmetic encoding.

### What the parser's iota *is* good for

The parser's iota is still useful for **diagnostic naming** —
`<NAME> has no variant <VAR>` checks against the parser-stored
variant set, and the iota number appears in some debug-print
paths. It just doesn't show up at runtime.

### Treat the variant-value behavior as a known bug

This documentation does not endorse the hash. It records what the
compiler does today so consumers don't trip over it. The behavior
is queued as a real bug in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) (`T9002 — Enum
variant values are a character hash, not the iota`), and is
expected to change to the iota-from-parser model in a future
compiler revision.

---

## 10.6 Common diagnostics

| Diagnostic | Cause | Fixture |
|---|---|---|
| `could not find struct <NAME> in scope` | Unknown enum name in `Name::Variant` (the error reuses the struct message) | n/a |
| `<NAME> is not an enum` | Name resolves but is a struct or variable | n/a |
| `<NAME> has no variant <VAR>` | Variant name not in the enum | n/a |
| `error: redefinition of <NAME>` | Duplicate `enum` declaration | n/a |
| `expected `{` after enum declaration` | Body block missing | n/a |
| `expected identifier for a member name in enum declaration` | Variant-list entry isn't an identifier | n/a |
| `expected an integer after equals sign in enum declaration` | `Variant = X` where X isn't an integer literal | n/a |

Full entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 10.7 Sources cited in this chapter

- `parser.rs:52` — variant storage (`HashMap`)
- `parser.rs:518–558` — enum declaration parser
- `parser.rs:530–550` — iota algorithm (parser-level only)
- `parser.rs:699–706` — variant reference parser
- `analyzer.rs:146–149` — enum symbol registration
- `analyzer.rs:263–271` — comparison-op type rule
- `analyzer.rs:437–454` — variant lookup
- `bytecode.rs:538–541` — variant value at runtime (first-char hash)
- Fixtures: `test_edge.sol` (whose expected values match the
  bytecode hash above), `gemini_long.sol`
