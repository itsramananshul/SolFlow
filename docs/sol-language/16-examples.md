# 16 — Examples

> **Status:** Scope statement only. Substantive content lands in
> commit 4.

## What this chapter contains

A small set of *annotated walkthroughs* of canonical sample programs.
Each walkthrough:

- shows the full source
- explains it block by block
- maps every interesting line to the chapter(s) that explain the
  rule it relies on
- lists what could go wrong and points at the matching diagnostic

These walkthroughs are designed to be read after chapters 02 – 11.
A reader who has worked through the manual once can use them to
build intuition for "what a real SOL program looks like"; a reader
who is reverse-engineering an existing program can use them as a
gloss.

## Programs covered

1. **`retest.sol`** — *minimal viable program.* Demonstrates
   `function start`, an integer literal, a `print`, and a `return`.
2. **`s1.sol`** — *small orchestration.* `let` bindings, `print`
   side effects, sequenced calls.
3. **`s2.sol`** — *small orchestration with external calls.*
   Introduces `ext` and an `export` entry.
4. **`jjsi.sol`** — *struct + helper + start pattern.* The
   simplest realistic shape.
5. **`jj_comp.sol`** — *monitoring loop.* `while`, struct mutation,
   `print`.
6. **`test_control.sol`** — *control-flow exhaustive.* Reference for
   chapter 7 patterns.
7. **`test_struct.sol`** — *struct exhaustive.* Reference for
   chapter 9 patterns, including the field-order hazard.
8. **`gemini_long.sol`** — *combined showcase.* Imports, enum,
   struct, orchestration.
9. **`largemini.sol`** — *broad-coverage harness.* Used as a stress
   test; not all of it is idiomatic and that is noted inline.

## Cross-references

- The long-form catalogue lives in [`EXAMPLES.md`](./EXAMPLES.md);
  this chapter is the *guided tour* and that file is the index.
- Style commentary on each example moves to chapter 17.

## Sources

All examples come from real test fixtures. No fabricated programs
appear in this chapter; if a useful illustrative point requires an
invented snippet, it goes into the chapter where the rule lives and
is labeled *(illustrative)*.
