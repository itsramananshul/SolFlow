# 09 — Structs

> **Status:** Scope statement only. Substantive content lands in
> commit 3.

## What this chapter answers

- How is a struct declared?
- How is a struct value constructed?
- How are fields read and written?
- Can structs be nested? Can arrays hold structs?
- What is the value/identity semantics of a struct — copied on
  assignment, or aliased?

## Topics covered

1. **Declaration.** `struct Name { field: T, field: T, … }` —
   form, allowed field types, empty structs (`test_struct.sol`
   includes an empty case).
2. **Field order.** **Important hazard.** The current compiler stores
   fields in a `HashMap` internally (`parser.rs:43`, per the audit in
   `reference/SOL_CRATE_IDE_READINESS_PLAN.md` §1), which means the
   declared order is **not** preserved across an open/save cycle in
   tools that round-trip the AST. The language is best read as
   *order-sensitive* (literals refer to fields by position is wrong;
   they refer by name — but the canonical print order may shift).
   The language docs treat fields as named.
3. **Struct literals.** `Name { a: 1, b: "hello" }`. Every field
   must be supplied; partial literals are not accepted.
4. **Field access.** `s.a` as expression, `s.a = v;` as statement.
5. **Mutation.** Whether a struct value held in a `let` binding can
   have its fields written. Verified against `test_struct.sol`.
6. **Nesting.** Structs whose fields are structs. Construction and
   access patterns demonstrated by `test_struct.sol` and
   `gemini_long.sol`.
7. **Arrays of structs.** Constructed via array literals; iterated
   via `for-in` (chapter 11). Demonstrated by `test_array.sol`.
8. **Common mistakes.** Missing a field in a literal; referring to
   an undeclared struct; referring to a field that doesn't exist on
   the struct (chapter 15 lists the diagnostic).

## Sources to be cited

- `parser.rs` struct-declaration production
- `analyzer.rs` struct-symbol handling, field lookup, literal type
  checking
- `bytecode.rs` instructions for struct construction and field
  load/store
- Fixtures: `test_struct.sol`, `gemini_long.sol`, `jjsi.sol`
