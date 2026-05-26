# 15 — Errors and Diagnostics

> **Status:** Scope statement only. Substantive content lands in
> commit 4.

## What this chapter answers

- What categories of error does SOL have?
- How is each surfaced — at parse time, at semantic-check time, at
  runtime?
- For each category, what does the user-facing message look like,
  and how should tooling display it?
- What can a SolFlow / IDE consumer do programmatically with these
  errors?

This chapter is the *narrative* companion to
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md), which is the lookup
catalogue.

## Topics covered

1. **Error categories.**
   - *Parse errors.* The source text is not a well-formed SOL
     program — `error_parse1.sol` (missing initializer) and
     `error_parse2.sol` (missing semicolon) are canonical.
   - *Semantic errors.* The program parses but violates a rule —
     undefined variable (`error_semantic1.sol`), duplicate `let`
     (`error_semantic2.sol`), duplicate function
     (`error_semantic3.sol`), type mismatches, missing return paths,
     etc.
   - *Runtime errors.* The program parses, type-checks, and
     compiles, but a precondition fails at execution time —
     `error_runtime.sol` is the canonical division-by-zero case.
2. **Severity.** The compiler today uses errors and warnings; notes
   exist as a planned third tier. The audit doc
   (`reference/SOL_CRATE_IDE_READINESS_PLAN.md` §1) records the
   ongoing transition from `eprintln!`/`process::exit` toward
   structured `Diagnostic` values.
3. **Source spans.** Today most diagnostics do not carry byte
   ranges; this is recorded as a known limitation. When spans land,
   this chapter will document the wire-level `Span` shape.
4. **Recovery.** Parser recovery (today: minimal) and analyzer
   recovery (today: stops on first error in many paths). What that
   means for editor UX is documented here so that consumers do not
   build experiences that assume "all errors at once".
5. **How tools should display diagnostics.** Recommended UX —
   severity color, source-location prefix, "previous definition
   here" related spans — matches how SolFlow already renders them.

## Cross-references

- The full catalogue: [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)
- Type-mismatch diagnostics: chapter 04
- Scope diagnostics: chapter 06
- Runtime traps: chapter 14

## Sources to be cited

- `parser.rs` — every parse-error site
- `analyzer.rs` — every semantic-error site
- `vm.rs` — every runtime-trap site
- `lexer.rs` — lexical errors
- Fixtures: every `error_*.sol`
