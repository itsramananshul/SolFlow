# Simulator ↔ Compiler Parity Audit

Status as of B.10 c27 (2026-05-27). **Audit only — no code changes
in this phase.** The simulator stays as-is; this document is the
drift report that informs future B.10 rewrite work.

## Layout

- **Simulator** — `src/runtime/interpret.ts`. In-browser
  graph-walking interpreter. ~770 lines. Used by the Run modal.
- **Compiler** — `compiler/` Rust crate. Authoritative SOL
  semantics. The TS importer + WASM diagnostics already run
  against it.
- **Editor validator** — `src/graph/validate.ts` +
  `src/graph/expressionLint.ts`. Runs alongside the simulator;
  catches a lot but not all parity drifts.

## Drifts that matter

Each row is a place where running the simulator on a given
workflow produces a different observable result than compiling +
running through the canonical SOL VM would.

### Type coercion in `applyBinaryOp` / `applyUnaryOp`

| Operation | Simulator | Canonical SOL | Notes |
|---|---|---|---|
| `true + 1` | `2` (bool→1, then add) | **Compile error E1006** | sim's `num()` coerces bool to 0/1 |
| `"42" + 1` | `"421"` (string concat) | **Compile error E1006** | sim's `numOrConcat()` treats `+` as string concat when either operand is string |
| `"a" + "b"` | `"ab"` | **Compile error T9023 / E1006** | str+str is rejected by analyzer; cataloged as bug T9023 |
| `"hi" / 2` | NaN or RuntimeError | Compile error E1006 | sim attempts numeric coerce |
| `!"hello"` | `false` (toBool of non-empty string) | Compile error E1012 | sim's `toBool` accepts strings; compiler restricts `!` to int/float/bool |
| `~3.5` | RuntimeError (no ~ in sim) | Compile error E1013 | sim doesn't implement bitwise NOT at all |

**Impact:** workflows that "run" in the sim may fail to compile.
Surface: import → analyze → see compiler errors that didn't show
up in the sim's diagnostics. This already partially shows up
through B.5 live diagnostics; future B.10 work could lift the
simulator's evaluator to use canonical semantics.

### Truthy / falsy in control flow

| Construct | Simulator condition | Canonical SOL condition |
|---|---|---|
| `if (cond)` | `toBool(cond)` — accepts non-zero numbers, non-empty non-"false" strings | Must be `bool`, else E1003 |
| `while (cond)` | `toBool(cond)` | Must be `bool`, else E1003 |

**Impact:** `while (1)` and `if ("yes")` both run forever / take
the true branch in the sim; both are E1003 compile errors. Users
who simulate-then-import then-fix is the loop today.

### Inline expressions evaluated as JS

The sim's `evalInline()` translates `E::V` → `"E::V"` then
constructs a `new Function(...)` to eval the expression with
scope bindings injected as arguments. That means:

- **Integer division** — JS `/` is float division. SOL `int /
  int` is integer division (truncated). `7 / 2` is `3.5` in the
  sim, `3` in canonical SOL.
- **Bitwise** — JS bitwise ops cast operands to int32 (signed,
  32-bit). SOL integers are i128 (per parser) and the VM's bitwise
  ops operate on those. Large or negative shifts will diverge.
- **Operator precedence** — JS's table is not identical to SOL's.
  The always-parenthesize-everything emit rule masks most cases,
  but hand-edited inline expressions can hit this. The validator
  + lint accept many such expressions today.
- **Float formatting** — JS `Number.prototype.toString()` drops
  trailing `.0`; SOL's `print` for Float emits a fixed
  representation.

### Equality

| Comparison | Simulator | Canonical SOL |
|---|---|---|
| `a == b` | JS `===` then string-norm for `E::V` shapes | Per-type dispatch: `IntEq`, `FloatEq`, `CharEq`, `EqStr` |
| Enum variants stored as strings (`"E::V"`) | String equality with normalization (`E::V(N)` ↔ `E::V`) | Integer hash equality on `(char)first_variant % 10` — see T9002 |

**T9002 redux:** the variants-by-first-char hash isn't caught by
the simulator at all — two variants `Active` and `Aborted` compare
equal in the canonical VM but unequal in the sim. The validator
already warns on this collision class.

### Print

| Aspect | Simulator | Canonical SOL |
|---|---|---|
| Multi-arg `print(a, b)` | Importer rewrites as `print([a, b])`; sim then prints array repr | Compiler's `print` takes one arg; per-type instruction (`PrintInt` / `PrintFloat` / `PrintChar` / `PrintString`) |
| Value formatting | `formatValue()` — adds type-appropriate quoting | Format embedded in the per-type print instruction |
| Newlines | Each `print` adds a newline | Same |

Acceptable drift; both produce human-readable output.

### Runtime errors

| Condition | Simulator | Canonical SOL |
|---|---|---|
| Division by zero | Throws RuntimeError (sim) | VM panics (not yet a structured diagnostic) |
| Array OOB | Throws RuntimeError | VM panics |
| Stack overflow | "Max call stack" RuntimeError after 1000 frames | VM stack overflow |
| Wall-clock timeout | RuntimeError after 60s | No timeout (host's problem) |
| Step limit | RuntimeError after 100k steps | No step limit |

The sim's safety rails are editor-side conveniences; the canonical
VM doesn't have them. Workflows that rely on sim's auto-abort
won't behave identically in deployment.

### Security note

`evalInline()` runs through `new Function(...)` after a
`lintInlineExpression()` gate. The lint is the only thing
preventing arbitrary JS execution; do NOT weaken it. The B.10
ideal is to replace the JS-eval path with a deterministic SOL
mini-evaluator that respects compiler semantics.

## What's safe today

The simulator IS faithful for these areas:

- Pure-bool conditions (no coercion in play)
- Integer arithmetic where all operands are int
- `for-in` over array literals
- `let` / `assign` for primitives
- `return` semantics
- Function call dispatch (graph-resolved)
- Struct field access + mutation
- Array indexing (bounds-checked in sim, undefined behavior in
  canonical VM — but bounds errors don't change well-formed
  workflows)

The validator already catches most divergent expressions through
`lintInlineExpression`; a clean validator + clean compiler
diagnostics gives high confidence the sim and canonical execution
will agree.

## Recommendations for B.10

Not promises — just the path that follows from this audit:

1. **Replace `evalInline` with a SOL mini-interpreter** that
   respects compiler-semantic int division, bitwise i128, and
   bool-only `!`. Eliminates the `new Function` security surface
   AND the operator-semantic drift in one move.
2. **Reject coerced types at sim boundary** rather than masking
   them with `num()`/`toBool()`. Better to surface the same
   errors the compiler will surface.
3. **Replace enum string normalization with int-by-position** so
   the sim catches T9002-class collisions too.
4. **Or:** ship the canonical VM as a second WASM target (`vm.rs`
   from upstream) and have the sim invoke it. Largest scope but
   removes all of this drift at once.

For SolFlow's current phase, the live compiler diagnostics from
B.5 already surface most of these mismatches at edit time. The
sim is increasingly a convenience layer rather than a source of
truth — which is the correct trajectory.
