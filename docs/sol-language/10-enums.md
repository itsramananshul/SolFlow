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

## 10.5 Fixture inconsistency to be aware of

`test_edge.sol` declares:

```sol
enum MyEnum {
    Inactive,
    Active,
    Busy,
}
```

By the iota rule documented above, these resolve to:

| Variant | Iota value |
|---|---|
| `Inactive` | 0 |
| `Active` | 1 |
| `Busy` | 2 |

The `start` function in the same fixture asserts:

```sol
if (test_enum_var() != 5) { return 9; }       // expects Active == 5
if (test_enum_inactive() != 3) { return 10; }  // expects Inactive == 3
if (test_enum_busy() != 6) { return 11; }      // expects Busy == 6
```

These expectations contradict the parser's iota rule and the
behavior of `analyzer.rs` / `bytecode.rs` as read in commit 2 –
3. Either the fixture's expected values are stale (likely — left
over from an older iota convention) or there is a different
numbering source the docs have not yet found.

**Treat the iota rule above as authoritative** — it is what the
parser source code does. Until the contradiction is resolved by
inspecting `bytecode.rs` for an alternative enum-numbering pass,
the discrepancy is logged as a known inconsistency in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md). Re-resolve in
commit 4 once the bytecode is fully read.

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
- `parser.rs:530–550` — iota algorithm
- `parser.rs:699–706` — variant reference parser
- `analyzer.rs:146–149` — enum symbol registration
- `analyzer.rs:263–271` — comparison-op type rule
- `analyzer.rs:437–454` — variant lookup
- Fixtures: `test_edge.sol` (with the inconsistency noted above),
  `gemini_long.sol`
