# Simulator ↔ Compiler Parity Audit

> **B.10 update — 2026-05-27.** The audit below was originally
> written as a drift catalog motivating future work. **That work
> shipped in c28–c30.** SolFlow now executes user workflows via
> the canonical SOL VM compiled to WASM (`runtime/` crate +
> `compiler-wasm::run_source_json` export + `runSource()` in the
> TS API + RunModal integration).
>
> The drift list below is now history — every "Simulator" column
> entry is what the **legacy JS interpreter** (`src/runtime/interpret.ts`)
> does. The canonical column is what SolFlow shows users.
> `interpret.ts` is kept ONLY as an animation driver for canvas
> playback (per-node highlighting); its output is no longer
> displayed anywhere as canonical execution.
>
> **One intentional drift remains:** `ExtCallBlocked`. The
> browser refuses external network calls and surfaces a
> structured runtime error instead of silently faking success.
> This is by design — see Part 4 of the original B.10 brief.

Status as of B.10 c27 (2026-05-27 — audit) + c28–c30 (resolution).

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

## Resolution (B.10 c28–c30)

Recommendation 4 — "ship the canonical VM as a second WASM
target and have the sim invoke it" — was the chosen path.
Recommendations 1–3 became unnecessary; the legacy JS sim is no
longer the source of truth, so its quirks no longer matter.

What shipped:

- **`runtime/` sibling Rust crate** — vendored the upstream VM
  with four surgical edits: `println!` → output-buffer capture,
  `Inst::ExtCall` → structured `ExtCallBlocked` error (browser
  can't do raw TCP), step limit (default 1M), common runtime
  errors (DivByZero / IndexOOB / StackUnderflow / HeapShapeMismatch)
  returned as `RunError` values instead of panics.
- **`compiler-wasm::run_source_json`** — new export that compiles
  via the canonical compiler + runs via the canonical VM, all
  in one WASM bundle (357KB optimized).
- **`runSource()`** in `src/compiler/api.ts` — typed TS wrapper.
- **RunModal** — output panel now displays canonical-VM output;
  legacy JS interpreter kept only for canvas playback animation
  (per-node highlighting), with an honest label.

The audit table below is preserved as historical context — every
"Simulator" entry IS what the legacy JS interpreter does, and IS
the divergence the user used to see. Those entries no longer
describe what SolFlow displays; the canonical column is now
authoritative.

`interpret.ts` carries an explicit `NOT AUTHORITATIVE` banner +
a `DO NOT extend` note. Future work that needs richer simulation
semantics should land in `runtime/`, not the JS interpreter.
