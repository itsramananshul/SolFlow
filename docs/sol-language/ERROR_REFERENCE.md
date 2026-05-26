# Error Reference

> **Status:** Scope statement only. Substantive entries land in
> commit 4 and are extended in commits 5 – 6.

This file is the lookup catalogue for every diagnostic the SOL
compiler and runtime emit. The narrative companion is
[chapter 15](./15-errors-and-diagnostics.md).

## Entry format

Each diagnostic entry has the following shape:

```
### E<NNNN> — short name

**Severity:** error | warning | note
**Category:** parse | semantic | runtime | tool
**Where it fires:** brief description + source citation
**Cause:** the rule that was violated, in one sentence
**Bad example:**
    <minimal program that triggers it>
**Diagnostic shape:**
    <what the compiler prints, today>
**Fix:**
    <minimal change to make the program valid>
**Fixture:** <fixture-name.sol>
**Related chapters:** <links into the manual>
```

The `E<NNNN>` codes are *forward-looking* — the current compiler
does not yet emit numeric codes (see
`reference/SOL_CRATE_IDE_READINESS_PLAN.md` §1, blocker #2 and #8).
This catalogue assigns provisional codes so consumers (SolFlow,
Sol Man) can refer to a diagnostic by stable name today, with the
expectation that the compiler will adopt these or a compatible
scheme.

## Categories

### Parse errors (E0001 – E0099)

Triggered by `parser.rs` / `lexer.rs`. The source text is not a
well-formed SOL program.

Canonical fixtures: `error_parse1.sol`, `error_parse2.sol`.

### Semantic errors (E1000 – E1999)

Triggered by `analyzer.rs`. The source parses, but a rule is
violated. Examples:

- E1001 — Undefined variable (`error_semantic1.sol`)
- E1002 — Duplicate `let` in the same scope (`error_semantic2.sol`)
- E1003 — Duplicate function declaration (`error_semantic3.sol`)
- E1004+ — Type mismatch family; missing return path; unresolved
  function call; struct field-missing; unknown enum variant; etc.

(Exact numbering is finalized in commit 4 once every site is
walked.)

### Runtime errors (E2000 – E2999)

Triggered by `vm.rs`. The program compiles but a precondition fails
at execution time. Examples:

- E2001 — Division by zero (`error_runtime.sol`)
- E2002+ — Out-of-bounds (if confirmed at runtime); call to
  unresolved external; etc.

### Tool-side mismatches (T9000 – T9999)

Triggered when a tool produces something the canonical compiler
would reject, or vice versa. These are **not** part of SOL itself;
they live here so SolFlow / Sol Man / IDE authors have a single
place to look. Each entry names the tool, the construct, and the
expected behavior. Example:

- T9001 — Editor emits `// @trigger` annotation (not part of
  canonical SOL; tolerated as a comment by the parser).

## Maintenance

- New diagnostics are added in this file *first*, then linked from
  the relevant chapter.
- Each entry must cite a source location and a fixture (positive or
  negative).
- When the compiler adopts numeric codes, this file is the
  reconciliation point.
