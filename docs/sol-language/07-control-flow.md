# 07 — Control Flow

> **Status:** Scope statement only. Substantive content lands in
> commit 3.

## What this chapter answers

- What control-flow constructs does SOL admit?
- What does an `if` without an `else` do? What does a `while` with a
  false initial condition do? What does a `for-in` over an empty
  array do?
- Are `break` and `continue` accepted?
- How does `return` interact with surrounding control flow?
- What is the evaluation order inside a condition?

## Topics covered

1. **`if` and `if … else`.** Condition is a `bool`-typed expression
   (type rule from chapter 04). Branch bodies are blocks. Whether an
   `else if` chain is admitted via the natural sequence, vs. an
   explicit `else if` token.
2. **`while`.** Condition evaluated before each iteration; body
   executes zero or more times. Loop body is a block.
3. **`for-in`.** Iteration over an array; introduces an iteration
   variable bound for the body block. Cross-referenced to chapter
   11. Cross-referenced to `test_array.sol`, `test_control.sol`.
4. **Early `return`.** Permitted inside any block, including loops
   and `if` arms. Terminates the surrounding function immediately;
   subsequent code in the same block is unreachable.
5. **`break` and `continue`.** **Confirmed/Uncertain** marker to be
   set in the substantive pass based on a direct grep of `parser.rs`.
   Until then, treat as *uncertain*.
6. **Reachability.** Whether the analyzer flags unreachable code
   after a `return` or after an exhaustive `if/else` that both
   return.
7. **Side effects.** Order of operations inside conditions and inside
   call arguments — important for `print` and `ext` calls.

## Common mistakes

- A non-`bool` expression in a condition (e.g. an `int` — see
  chapter 04 for the type-mismatch rule)
- A `for` written in a form the parser doesn't accept (e.g. the
  C-style three-clause form, if the parser only admits `for-in`)
- Returning a value of the wrong type from inside a nested branch

## Sources to be cited

- `parser.rs` if / while / for productions; `return` handling
- `analyzer.rs` reachability / return-path checks
- `bytecode.rs` jump instruction set (`Jump`, `JumpIfFalse`, etc.)
- `vm.rs` runtime control-flow handlers
- Fixtures: `test_control.sol`, `test_array.sol`, `test_func.sol`,
  `jj_comp.sol` (while-loop pattern)
