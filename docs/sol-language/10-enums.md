# 10 — Enums

> **Status:** Scope statement only. Substantive content lands in
> commit 3.

## What this chapter answers

- How is an enum declared?
- How is a variant referenced?
- Do variants carry data, or are they pure tags?
- What value (if any) backs each variant — auto-assigned or explicit?
- How do enums interact with `==` and with control flow?

## Topics covered

1. **Declaration.** `enum Name { Variant, Variant = N, … }` —
   syntax sourced from `parser.rs`; auto-numbering vs. explicit
   integer values verified against `test_edge.sol` and the analyzer.
2. **Field order.** Same hazard as structs (chapter 09): variants
   are currently stored in a `HashMap` (`parser.rs:47` per the audit
   doc), so round-trip via the AST does not guarantee declaration
   order. Code should not assume a printed order.
3. **Variant reference.** `Name::Variant` as an expression.
4. **Comparison.** Two enum values of the same enum compared with
   `==` / `!=` — the canonical use case in conditionals.
5. **No pattern-matching today.** SOL does not (currently) provide a
   `match` construct; decisions on enum values are made with `if /
   else` chains. This is stated explicitly so consumers know what
   they cannot rely on.
6. **Common mistakes.** Comparing variants from two different enums;
   referencing a variant that doesn't exist; redeclaring a variant
   name within an enum.

## Sources to be cited

- `parser.rs` enum-declaration production
- `analyzer.rs` enum-symbol handling
- `bytecode.rs` instructions for variant load and comparison
- Fixtures: `test_edge.sol`, `gemini_long.sol`
