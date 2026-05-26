# 19 — Sol Man Generation Guide

> **Status:** Scope statement only. Substantive content lands in
> commit 5.

## What this chapter answers

When an LLM-driven generator produces a SOL workflow — either as a
SolFlow graph spec or directly as `.sol` source — what does it need
to honor for the result to be valid, runnable, and editable?

This chapter is the *generation contract*. It is written so an LLM
prompt can quote it verbatim and so a post-generation validator can
turn each rule into a check.

## Topics covered

### Hard rules (validator-enforceable)

A complete list of rules that must hold for any generated artifact.
Each rule has:

- a one-line statement of the rule
- the diagnostic that fires when it is broken
- the chapter that contains the underlying language reason

Examples that will appear:

- Every `let` has an initializer expression.
- Every `branch` / `while` has a `bool`-typed condition expression.
- Every `print` / `assign` / non-void `return` has a value
  expression.
- Every call resolves to a function known to the graph
  (`function`, `ext function`, or one already declared in the
  workflow). Unresolved calls are not produced.
- Every `for-in` has an array-typed iteration source.
- No declarations are duplicated within their scope.
- Field literals supply every declared field of the struct.

### Soft rules (quality-bar)

Rules that produce *valid* programs but should be honored for
readable, editable workflows:

- Prefer named intermediate `let`s over long inline expressions.
- Prefer one entry function (`start`); do not duplicate it.
- Use `snake_case` and `PascalCase` per chapter 17.
- Provide at least one `assumption` per generated workflow when the
  prompt is under-specified.

### Repair pass

When a generated artifact fails a hard rule, the generator (or a
post-pass) should attempt one of:

- **Replace unresolved calls with `print` placeholders.** Encode the
  intended action as a SOL string literal — the result is a valid
  program that documents the missing piece.
- **Drop edges that reference ports that don't exist.** Surface a
  warning.
- **Add missing inline expressions or refuse to apply.** Never
  silently materialize a broken graph.

### Prompting patterns

Concrete recipes:

- "Action with no real endpoint" → `print` with a string literal.
- "External call" → `ext function` declaration + a normal call.
- "Threshold check" → `branch` with an inline boolean expression.
- "Fan-out across N items" → `for-in` over an array literal or an
  array returned from an `ext` call.

### Failure modes observed

A short catalogue of failure modes seen in practice (e.g. the
"unresolved call" failure that motivated the current Sol Man
auto-repair pass) with the exact repair that turns each one into a
valid graph.

## Cross-references

- The validator that enforces hard rules lives in
  `src/graph/validate.ts`.
- The auto-repair pass lives in `src/sol-man/applyGraph.ts`.
- The system prompt the LLM sees lives in `api/sol-man/_prompt.ts`.
- The visual-editor mapping is chapter 18; this chapter consumes it
  rather than re-deriving it.

## Sources to be cited

- `src/sol-man/applyGraph.ts` (current repair pass)
- `src/graph/validate.ts` (current validator)
- `api/sol-man/_prompt.ts` (current prompt)
- All chapters that ground each hard rule in the language
