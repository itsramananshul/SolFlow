# 10. Enums

> **Status:** Rewritten against the canonical `openprem-sol-v2`
> crate (`sol/`). Cross-checked against `sol/src/parser.rs`
> (`parse_enum`, variant-reference parsing), `sol/src/ast.rs`
> (`EnumDecl`, `Expr::EnumVariant`), `sol/src/vm.rs` (the `MakeEnum`
> instruction and enum values), `sol/src/value.rs`
> (`Value::Enum(String, String)`), and the editor check in
> `src/graph/validate.ts`.

A SOL enum is a set of named variants gathered under one type name.
Each variant is a plain tag. There is **no payload form** (no
`Variant(T1, T2)`), **no pattern matching**, and **no exhaustiveness
checking**. Variants are referenced as `EnumName::VariantName` and
compared with `==` / `!=`.

This chapter documents the declaration syntax, how variants are
referenced, the runtime value model, and one important runtime
hazard you must design around: the **first-character collision**.

---

## 10.1 Declaration

```sol
enum Name {
    Variant1;
    Variant2;
    Variant3;
}
```

The enum body is a list of variant identifiers, each terminated by a
**semicolon** (`sol/src/parser.rs`, `parse_enum`: it reads an
identifier, then expects `;`). An empty body `enum Empty {}` is
accepted. There is no `= N` value form; the AST stores a plain
`Vec<String>` of variant names (`sol/src/ast.rs`, `EnumDecl`).

```sol
enum Status {
    Active;
    Inactive;
}
```

Variant declaration order is preserved in the AST `Vec<String>`, so
source order is stable.

---

## 10.2 Variant reference

```sol
EnumName::VariantName
```

A variant reference uses the `::` operator (`sol/src/parser.rs`),
producing `Expr::EnumVariant { enum_name, variant }`
(`sol/src/ast.rs`). At runtime the `MakeEnum` instruction builds a
`Value::Enum(enum_name, variant)`, which carries both names as
strings (`sol/src/vm.rs`, `sol/src/value.rs`).

```sol
let s: Status = Status::Active;
```

---

## 10.3 Comparison

Because the runtime value carries the enum name and the variant
name, comparing two variants of the same enum works:

```sol
enum Status { Active; Inactive; }

workflow "demo" {
    let s: Status = Status::Active;
    if (s == Status::Active) {
        print("on");
    }
}
```

There is no type-checker, so comparing variants of two different
enums is not rejected at compile time; it simply compares the two
`Value::Enum` values at runtime. Keep comparisons within a single
enum.

---

## 10.4 No pattern matching

SOL has no `match` construct. Dispatch on a variant with `if` /
`else` chains:

```sol
if (status == Status::Active) {
    handle_active();
} else if (status == Status::Inactive) {
    handle_inactive();
} else {
    handle_other();
}
```

There is no exhaustiveness check; nothing warns that you forgot a
variant.

---

## 10.5 The first-character collision hazard

This is the most important thing to know about SOL enums.

The simulator's by-name semantics and the canonical wire bytecode
disagree, and the disagreement is silent. The runtime `Value::Enum`
stored by `sol/src/vm.rs` carries the variant name, so a by-name
simulator compares variants exactly as written and runs your program
correctly. But the **canonical SOL bytecode dispatches each enum
variant by its first character**, computed as

```
(first_char as i128) % 10
```

So two variants whose first characters fall into the same mod-10
residue class compare **equal at runtime**, even though the by-name
simulator ran them as distinct. This is documented in the editor
check at `src/graph/validate.ts` (the `enum-first-char-collision`
warning, historically tracked as `T9002`).

### Worked example

```sol
enum Status {
    Active;
    Aborted;
}
```

Both `Active` and `Aborted` start with `A` (ASCII 65, `65 % 10 = 5`),
so `Status::Active == Status::Aborted` is **true** under the
canonical bytecode, even though the simulator treats them as
different. The residue is taken mod 10, so non-identical first
characters can also collide: `A` (65) and `K` (75) both map to 5,
`B` (66) and `L` (76) both map to 6, and so on.

### The rule to follow

Make every variant in an enum start with a **distinct first
character whose code points do not share a mod-10 residue**. The
editor surfaces a `warning` (not an error) when it detects a
collision, listing the colliding variant names and telling you to
rename one. Heed that warning: the simulator will say "all good"
while the canonical runtime silently misdispatches.

### Why this matters

The simulator runs the intended by-name behavior, so a collision is
invisible during in-browser testing. The warning at the editor level
is what saves you from a deploy-time surprise where the simulator
said the workflow was correct but the canonical runtime compares
two variants as equal.

---

## 10.6 Error model for enums

Enum behavior produces no compile-time type errors and no error
codes in the language pipeline. A misuse (for example comparing an
enum to an incompatible value) surfaces, if at all, as a runtime
string error in the `Failed(string)` step result.

The one enum-specific diagnostic worth calling out is editor-side,
not compiler-side: `enum-first-char-collision`
(`src/graph/validate.ts`), a `warning` that flags the runtime
collision hazard of §10.5. Other editor-side enum checks include
`unknown-enum`, `unset-enum`, `unset-variant`.

---

## 10.7 Sources cited in this chapter

- `sol/src/ast.rs`: `EnumDecl` (a `Vec<String>` of variant names),
  `Expr::EnumVariant`
- `sol/src/parser.rs`: `parse_enum` (semicolon-terminated
  variants), `::` variant reference
- `sol/src/vm.rs`: the `MakeEnum` instruction, `Value::Enum`
  construction
- `sol/src/value.rs`: `Value::Enum(String, String)`
- `src/graph/validate.ts`: the `enum-first-char-collision`
  warning (the `(first_char as i128) % 10` hazard, historically
  `T9002`)
