# 00 — Source Audit

> **Status:** Complete for the initial documentation pass (2026-05-26).
> Re-run whenever the canonical SOL compiler changes shape or new
> language features land.

This chapter exists to make the rest of the manual *checkable*. It
states exactly which files were read to write each claim, what
counts as primary vs. secondary signal, and which questions remain
open. If a future maintainer suspects a doc has drifted from the
implementation, the entry points to verify are listed here.

---

## 1. Authoritative sources

The SOL language is defined by the behavior of its compiler and
runtime. The following Rust modules are treated as the **single
source of truth** for every claim in this manual.

| Module | Approx. size | What it defines |
|---|---|---|
| `lexer.rs` | ~390 lines | Token set, keywords, identifier rules, literal forms, comment syntax, whitespace handling |
| `parser.rs` | ~750 lines | Full grammar — declarations, statements, expressions, the Pratt-style precedence table |
| `analyzer.rs` | ~500 lines | Semantic rules — scoping, types, mutability, duplicate-decl checks, forward-declaration semantics, every semantic diagnostic |
| `bytecode.rs` | ~710 lines | Instruction set; implicitly defines what operations the language admits at the value level |
| `vm.rs` | ~580 lines | Runtime semantics — evaluation, stack frames, dispatch, runtime errors (e.g. division by zero) |
| `mod.rs` | ~10 lines | Module re-exports; confirms what is exposed as the library surface |
| `init.rs`, `cli.rs` | ~90 lines combined | How a SOL file is loaded; how a `session` maps to a source file; how `ext` function names are wired to controller-provided endpoints |

Source citations in the manual use short forms — e.g.
`(parser.rs:540–558)` — that map into these files.

---

## 2. Test fixtures (positive and negative)

The compiler crate ships a corpus of `.sol` fixtures. A curated
subset is mirrored into this repo at `reference/sol files/`. Both
mirrors are treated as primary signal — a claim in the manual that
contradicts a fixture is a claim that needs revision.

### Positive fixtures (valid programs)

| Fixture | Demonstrates |
|---|---|
| `test_arith.sol` | Arithmetic, comparison, logic, bitwise operators; numeric literal forms |
| `test_array.sol` | Dynamic arrays, `for-in` iteration, arrays of structs |
| `test_control.sol` | `if` / `else`, `while`, `for`, early `return` |
| `test_edge.sol` | Large integers, enum behavior, chained assignment |
| `test_func.sol` | Function declarations, recursion, nested calls, void-style returns |
| `test_scope.sol` | Block scope, lexical scope, what is and is not visible across blocks |
| `test_struct.sol` | Empty, nested, and mutated structs; field-order behavior |
| `fwdecl.sol` | Forward declaration / call before definition |
| `jjsi.sol`, `jj_comp.sol` | Struct + helper + entry pattern; while-loop monitoring style |
| `retest.sol` | Minimal function-call + `print` (notes a missing `return` in `start`) |
| `s1.sol`, `s2.sol` | Orchestration-style programs with `print` side effects |
| `gemini_long.sol` | Long sample exercising imports, enums, structs, and orchestration together |
| `largemini.sol` | Largest integration-style fixture; broad coverage of the surface |

### Negative fixtures (intentional errors)

| Fixture | Diagnoses |
|---|---|
| `error_parse1.sol` | Parse error — missing initializer in a `let` |
| `error_parse2.sol` | Parse error — missing semicolon |
| `error_semantic1.sol` | Semantic — undefined variable |
| `error_semantic2.sol` | Semantic — duplicate `let` (shadowing forbidden in the same scope) |
| `error_semantic3.sol` | Semantic — duplicate function name |
| `error_runtime.sol` | Runtime — division by zero |

Every error documented in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)
must cite at least one fixture and one source location.

---

## 3. Secondary signal

Two artifacts inside this repository are useful as **secondary**
signal — they describe SOL's surface syntax via a second
implementation, and where they agree with the compiler that
agreement strengthens confidence. Where they disagree, the compiler
wins and the disagreement is recorded as a tool-side mismatch.

| Artifact | Role |
|---|---|
| `src/emit/emit.ts` | The visual editor's Graph → SOL emitter — implements (part of) the syntax independently. Used to confirm shape of declarations and statements. Any divergence from the compiler is flagged in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) as an emitter bug, **not** as canonical SOL behavior |
| `src/graph/factory.ts`, `src/graph/validate.ts` | The graph node port shapes and validation rules; useful only for chapter 18 (SolFlow mapping) — these reflect editor opinion, not language law |
| `src/samples/*.ts` | Programmatic constructions of canonical workflows — useful as cross-checks for the mapping chapter |

---

## 4. External-runtime integration (snapshot)

A controller / host runtime is the environment that loads and
executes a `.sol` file and supplies the endpoints declared with
`ext function`. The wiring lives outside the language proper —
typically a configuration file declares which session a controller
serves, which `.sol` source backs that session, and which remote
endpoints back the names declared as `ext`.

The runtime's specific wire format is **not** part of the SOL
language. Chapter 12 documents it as a *snapshot* of one host
implementation, clearly labeled as such. Anything that depends on
the wire format should be considered subject to drift; anything
that depends only on what SOL syntactically permits is stable.

Snapshot date for the runtime integration sections: **2026-05-26**.

---

## 5. Conventions used in the manual

Three labels appear throughout the chapters to keep claims honest.

- **Confirmed** — supported by direct reading of the source files
  listed in §1 and reproduced by at least one fixture in §2. The
  expected default state for normative claims.
- **Uncertain** — partially supported. The block explains what
  evidence does exist and what is missing. Anything in this state
  should either be promoted to Confirmed or removed in a subsequent
  documentation pass.
- **Snapshot** — describes behavior of an external integration that
  may evolve independently of the language. Always carries the date
  of the observation.

Source citations look like:

```
(parser.rs:540–558)            ← line range in the lexer/parser/analyzer module
(test_struct.sol)              ← test fixture in the compiler crate / mirror
(error_semantic2.sol)          ← negative fixture by name
```

Examples come from real fixtures wherever possible. Fabricated
examples are labeled *(illustrative)* in the chapter where they
appear.

---

## 6. Open questions

These items are flagged here so they don't get lost. They need
either a source reading pass or a fixture before they can move from
*uncertain* to *confirmed*.

| # | Question | What would resolve it | Status |
|---|---|---|---|
| 1 | Are `break` / `continue` accepted by the parser? | Grep `parser.rs` for the tokens; check for a fixture that uses them | **Resolved (commit 2).** Neither keyword exists in `lexer.rs:341–356`. The analyzer carries a `can_break` flag but no AST node ever sets it. Documented in chapter 03 §3.6 and chapter 07. |
| 2 | Does the language admit a nullable / optional type? | Inspect `Type` enum variants in `parser.rs` and `analyzer.rs` | **Resolved (commit 2).** The `Type` enum (`parser.rs:5–24`) has no `Option` / `Nullable` variant. Documented in chapter 04 §4.1. |
| 3 | What is the exact set of bitwise operators? | The Pratt table in `parser.rs:540–558` enumerates them — to be transcribed verbatim into `GRAMMAR.md` | **Resolved (commit 2).** Set: `& \| ^ << >> ~`. Documented in `GRAMMAR.md` §1 and chapter 04. The full precedence table moves to `GRAMMAR.md` §4 in commit 3. |
| 4 | What does `print` accept — single value, varargs, a single string? | Read the analyzer's handling of the `print` identifier and confirm against `s1.sol`, `s2.sol`, `retest.sol` | **Partially resolved (commit 2).** `analyzer.rs:340–345` accepts any number of args of any types and returns `Void`. The VM has separate `PrintInt` / `PrintFloat` / `PrintChar` / `PrintString` ops; the dispatch from `print` to the right op happens at bytecode emission time (`bytecode.rs`, to be read in commit 4). Full treatment in chapter 13. |
| 5 | Does `for` admit C-style three-clause form, or only `for-in`? | Inspect the parser's `for`-handling block; cross-check with `test_control.sol` and `test_array.sol` | **Resolved (commit 2).** Only the `for IDENT in expr block` form exists (`parser.rs:383–404`). Documented in chapter 03 §3.6 and chapter 07. |
| 6 | Are integer overflows defined, wrapping, or trapping? | Read the VM's arithmetic instruction handlers; check `test_edge.sol` which uses large integers | **Partially resolved (commit 2).** Runtime arithmetic uses native Rust `i64` ops (`vm.rs:143–146`), which wrap in release builds and panic in debug builds. Literal parsing is `i128` (`lexer.rs:383`); literals above `i64::MAX` are truncated at runtime. Full treatment in chapter 14. |
| 7 | Does the runtime guarantee left-to-right evaluation of function arguments? | Read the bytecode emission for `Call` | **Resolved (commit 4).** Yes — `bytecode.rs:467–481` compiles each argument in order and pushes onto the stack; `Inst::Call(addr, n)` then sets `fp = stack.len() - n` so arg 0 sits at `fp + 0`. Side effects happen left-to-right. Documented in chapter 14 §14.3. |
| 8 | What is the precise lifetime of a struct value — value-semantics or reference-semantics? | Read the `Inst` ops that load/store struct fields, plus VM handling | **Partially resolved (commit 2).** `vm.rs:7–11, 189–196` shows structs live on the heap as `HeapObject::Struct(Vec<u64>)`, addressed by a heap-index reference. Stack values for struct-typed variables therefore carry heap indices, not field contents. Full treatment in chapter 09 + chapter 14. |
| 9 | Does the analyzer ever check `let` initializer types against the declared type? | Read `analyzer.rs` `DeclVar` branch | **Resolved (commit 2).** No — the analyzer ignores the initializer expression (`analyzer.rs:138–141`). Documented as a known hole in chapter 06 §6.1 and queued in the upstream audit (`SOL_CRATE_IDE_READINESS_PLAN.md` §1, blocker #18). |
| 10 | Are `&&` and `||` short-circuiting? | Read the VM's `LogAnd` / `LogOr` handlers | **Resolved (commit 2).** No — both operands are evaluated before the op runs (`vm.rs:177–178`). Documented in chapter 04 §4.2.3. |
| 11 | Does `export` exist as a keyword? | Grep `lexer.rs` for `export` | **Resolved (commit 2).** No. The keyword set is fifteen entries (`lexer.rs:341–356`); `export` is not among them. Sources observed using `export function` are broken; documented in chapter 03 §3.6 and chapter 05 §5.1. |
| 12 | Does string equality (`str == str`) actually work? | Read the bytecode emission for `==` on strings | **Resolved (commit 4).** Yes — `bytecode.rs:683` emits `Inst::EqStr` for `str == str` (and `EqStr` + `LogNot` for `str != str`). The VM has a corresponding op. String concatenation via `Inst::ConcatStr` is also present but unreachable from source because the analyzer rejects `str + str` — logged as T9005. Documented in chapter 04 §4.2.4 and chapter 08 §8.5. |
| 13 | What is the actual runtime value of an enum variant? | Read the bytecode emission for `ExprEnumVar` | **Resolved (commit 4).** `bytecode.rs:538–541` emits `(first_char of variant name) % 10`, not the parser-computed iota. This is a bug (T9002) — variants that share a first character collide. Documented in chapter 10 §10.5 and chapter 17 §17.1. |
| 14 | Does `print(a, b)` print both arguments? | Read the bytecode emission for `print` | **Resolved (commit 4).** No — `bytecode.rs:425` compiles only `args[0]`. Extra arguments are silently dropped. This is T9003. Documented in chapter 13 §13.1. |

Each of these is queued for resolution in the chapters where the
behavior matters (types / control flow / runtime semantics). Items
that survive all upcoming commits should appear in the open-questions
section of [`SPEC.md`](./SPEC.md) so they remain visible.

---

## 7. Methodology for each chapter

To keep drift to a minimum, every chapter is built the same way:

1. **Read** the relevant compiler source end-to-end. Note line
   ranges for the constructs the chapter covers.
2. **Reproduce** each rule against at least one positive fixture
   from §2, and (where the rule has failure modes) at least one
   negative fixture.
3. **Write** the chapter as: *rule → why → minimal valid example →
   minimal invalid example → matching diagnostic*. Cite sources.
4. **Cross-check** against `src/emit/emit.ts`. Where the emitter
   agrees, the rule is reinforced; where it disagrees, the
   disagreement is logged in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).
5. **Mark** every remaining gap as *Uncertain* with the precise
   evidence needed to close it.

Anything that cannot be sourced this way does not belong in the
manual.
