# 02 — File Structure

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

- What does the top level of a `.sol` file look like?
- What can appear at the top level, and is there a required order?
- How does the compiler decide where execution begins?
- What does the parser accept as whitespace, comments, and trivia
  between declarations?

## Topics covered

1. **The top-level grammar.** A SOL file is a flat sequence of
   declarations: `struct`, `enum`, `function`, `ext function`,
   `export function`. There are no nested modules; there is no
   package header.
2. **Entry point.** The host runtime calls a function the source
   advertises as the entry — by convention `start` — when it
   executes a session. The exact rule for entry selection is
   verified against `analyzer.rs` / `vm.rs`.
3. **Ordering rules.** Whether forward references between
   declarations are admitted, with reference to `fwdecl.sol` and
   `error_semantic3.sol`. Whether duplicate names within the file
   are rejected (they are — see chapter 15).
4. **Comments and whitespace.** Line-comment syntax (`//`), block
   comments (presence/absence to be confirmed against `lexer.rs`),
   significant vs. insignificant whitespace, and how semicolons
   terminate statements.
5. **External declarations.** `ext function …;` (no body) and
   `export function …` (with body) are surface-level markers — what
   they mean to the language vs. what they mean to the host runtime
   is split between this chapter and chapter 12.
6. **Canonical layouts.** A "small program" template, a "library
   helper" template, and an "orchestration with external endpoints"
   template, all drawn from real fixtures.

## Sources to be cited in the substantive pass

- `parser.rs` top-level declaration loop
- `lexer.rs` comment and whitespace handling
- `analyzer.rs` duplicate-name / forward-decl handling
- Fixtures: `s1.sol`, `s2.sol`, `jjsi.sol`, `largemini.sol`,
  `fwdecl.sol`, `error_semantic3.sol`
