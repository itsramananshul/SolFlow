# 15 — Errors and Diagnostics

> **Status:** Substantive (commit 4). The narrative companion to
> [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md), which is the
> lookup catalogue.

A SOL program can be wrong in three places:

1. **Lexically / syntactically** — the source text is not a
   well-formed SOL program. The lexer or parser refuses it.
2. **Semantically** — the source parses, but a rule is violated.
   The analyzer refuses it.
3. **At runtime** — the source compiles and starts running, but a
   precondition fails inside the VM. The session terminates.

This chapter explains how each category surfaces today, what
tooling can do with the resulting diagnostics, and what the planned
direction for diagnostics looks like.

---

## 15.1 What a diagnostic looks like today

The current compiler prints diagnostics to **stderr** as plain
lines of text and then calls `std::process::exit(1)`. Examples
captured directly from the source:

```
error: redefinition of `x`
mismatched types in arithmetic: Integer + String
function `lookup` expected Integer in position 0 but was passed String
```

Two consequences for tooling:

- **No structured codes.** The compiler does not yet emit
  numerical or alphabetic error codes. The provisional codes used
  in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) are a
  documentation-side convention, intended to give consumers a
  stable name to refer to until the compiler adopts an equivalent
  scheme.
- **No source spans.** Tokens and AST nodes don't carry source
  positions today. A diagnostic can name a symbol (`x` /
  `lookup`) but cannot point at a byte offset, line number, or
  column. This is the single most important diagnostic gap for IDE
  use; the audit lists it as blocker #3 (`SOL_CRATE_IDE_READINESS_PLAN.md`
  §1).

A future version of the compiler is expected to emit structured
`Diagnostic` values with severities, codes, and `Span` values
(see [`SPEC.md`](./SPEC.md) §11 for the target shape). Until then,
plain text on stderr is the only signal.

---

## 15.2 Categories

### Parse errors

Triggered when the source text doesn't match the grammar. Sources:
`lexer.rs` (lexical, rare today — the lexer's only fatal lex error
is "unrecognized character") and `parser.rs` (frequent).

Canonical fixtures: `error_parse1.sol` (`let x: int = ;`), `error_parse2.sol`
(`let x: int = 5` missing semicolon).

Recovery: **none today.** The parser exits on the first error. A
tool that wants to show every problem in a file at once has to
restart compilation after each fix.

### Semantic errors

Triggered when the source parses but a rule is violated.

Canonical fixtures: `error_semantic1.sol` (undefined variable),
`error_semantic2.sol` (duplicate `let`), `error_semantic3.sol`
(duplicate function name).

Sources span the analyzer's per-construct match arms. The full
catalogue lives in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md);
chapters 04 – 11 list the diagnostics that fire for each
construct.

Recovery: **also none today.** Same exit-on-first-error model.

### Runtime errors

Triggered after compilation, when the VM is executing.

Canonical fixture: `error_runtime.sol` (integer division by zero
via `1 / 0`). Other classes — array out-of-bounds, panic from
malformed RPC JSON — exist but are not currently exercised by
named fixtures.

Recovery: **none.** A runtime panic terminates the session. There
is no `try` / `catch`, no `Result<T,E>` type, no error propagation
at the language level. To handle failure gracefully, do the
checking in SOL itself (defensive `if`) or move it into an
`ext function` that the host can express as a typed return.

---

## 15.3 Tool-side mismatches

A fourth category exists not because the language is wrong, but
because a tool produces something the compiler would reject (or
vice versa). These get a `T9xxx`-style code in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) to make them findable
without conflating them with language diagnostics.

Examples documented today:

| Code | Description |
|---|---|
| T9001 | Editor emits `// @trigger …` annotations (parser tolerates as comments; not part of canonical SOL) |
| T9002 | Bytecode emits enum variants as a first-character hash, not the parsed iota — see chapter 10 §10.5 |
| T9003 | `print(a, b, c)` is analyzer-accepted but the bytecode emits only the first argument — see chapter 13 §13.1 |

These mismatches should ultimately be fixed in either the tool or
the compiler. Until then they are documented so consumers don't
chase a SOL-language problem that is really a tool problem.

---

## 15.4 How tools should display SOL diagnostics

Pending source spans, the recommended UX is:

1. **Severity.** Render `error` lines in red, `warning` lines in
   amber, `note` lines in dim. The compiler today emits only
   errors; the `warning` and `note` tiers are present in the
   diagnostic shape but underused.
2. **Source location.** Until the compiler provides spans, the
   tool's best move is to show the diagnostic alongside the *full
   compiler output*, not synthesize a guessed location. False
   pointers are worse than no pointers.
3. **Single-error mode.** Because the compiler exits on the first
   error, "show me everything wrong with this file" is not yet
   achievable. Tooling should set expectations accordingly:
   *"compiler reports the first error; fix and re-run."*
4. **Related context.** Several semantic diagnostics name a prior
   declaration (e.g. `error: redefinition of x` implies an earlier
   `let x`). When the compiler eventually emits related spans
   (see `SOL_CRATE_IDE_READINESS_PLAN.md` Appendix B), tools should
   render them as secondary highlights.

---

## 15.5 Diagnostic-quality issues to be aware of

A short list of diagnostics that print today with imprecise or
misleading text. None of these is incorrect about *whether* the
program is invalid — they are incorrect about *why*. They are
documented here so consumers don't take the wording too literally:

- `condition of if statement must be of type bool, got <T>` — also
  fires for `while` conditions. The string "if statement" is
  hard-coded in `analyzer.rs:191`.
- `could not find struct <NAME> in scope` — also fires for enum
  lookups (`analyzer.rs:438`). The "struct" word is wrong for
  enum cases.
- `variable <NAME> is assigned to before initialization` — fires
  both for true uninitialized-use *and* for type-mismatched
  assignments (`analyzer.rs:222, 236`). The first matches the
  text; the second does not.
- `arithmetic operation <op> not supported for type <T>` — fires
  when both operands match each other but are non-numeric (e.g.
  `bool + bool`). The text is fine; the underlying cause is the
  type rule, not the operator.

Each is noted again at the corresponding entry in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 15.6 Sources cited in this chapter

- `parser.rs` — every `eprintln!` / `panic!` site is a parse-error
  source
- `analyzer.rs` — every `eprintln!` / `process::exit` site is a
  semantic-error source
- `vm.rs` — every `panic!` / `expect(...)` site is a runtime-error
  source
- `bytecode.rs:458` — only compile-time fatal in the bytecode
  emitter (unresolved `ext function` endpoint)
- Fixtures: every `error_*.sol`
