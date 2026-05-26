# Examples Catalogue

> **Status:** Substantive (commit 6). Lookup index of every `.sol`
> fixture cited by this manual. The guided tour lives in
> [chapter 16](./16-examples.md).

## Index format

Each entry lists:

- **Role** — positive (idiomatic program) or negative (intentional error).
- **Demonstrates** — the language features the fixture exercises.
- **See also** — the chapter(s) that explain those features.

The full source of each fixture is mirrored at `reference/sol files/`
inside this repository. For a guided walkthrough of selected
fixtures with annotations, read [chapter 16](./16-examples.md).

---

## Positive fixtures

### `retest.sol`

- **Role:** Positive — minimal viable program.
- **Demonstrates:** `function` declaration, `let`, integer
  multiplication, `print`, missing-return idiom.
- **See also:** chapters [03](./03-syntax.md), [05](./05-functions.md),
  [16 §16.1](./16-examples.md#161-retestsol--minimal-viable-program).

### `jjsi.sol`

- **Role:** Positive — struct + helper + start pattern.
- **Demonstrates:** struct declaration, struct literal, `bool`-returning
  helper with early `return`, function parameter passing a struct,
  void-returning helper.
- **See also:** [05](./05-functions.md), [09](./09-structs.md),
  [16 §16.2](./16-examples.md#162-jjsisol--struct--helper--start).

### `s1.sol`

- **Role:** Positive — small orchestration.
- **Demonstrates:** `import`, `enum` with mixed implicit/explicit
  values, struct with composite field (`[4]int`), nested `if`/`else`,
  enum comparison, void-returning side-effect helpers, struct passing.
- **Caveat:** Runtime behavior is distorted by T9002 (enum-variant
  hash collisions) — see chapter 10 §10.5.
- **See also:** [10](./10-enums.md), [12](./12-imports-and-controllers.md),
  [16 §16.3](./16-examples.md#163-s1sol--small-orchestration).

### `s2.sol`

- **Role:** Positive — payment-style orchestration.
- **Demonstrates:** `ext function` declaration pattern (in spirit;
  uses internal helpers in this fixture); `deploy` / `shutdown`
  style of conditional dispatch.
- **See also:** [05](./05-functions.md), [12](./12-imports-and-controllers.md).

### `jj_comp.sol`

- **Role:** Positive — monitoring loop.
- **Demonstrates:** `while` loop, struct mutation, `print` cadence.
- **See also:** [07 §7.2](./07-control-flow.md), [09 §9.4](./09-structs.md).

### `fwdecl.sol`

- **Role:** Positive — forward declaration / call-before-definition.
- **Demonstrates:** Two-pass analyzer design that registers all
  function signatures before any body is walked.
- **See also:** [05 §5.5](./05-functions.md).

### `test_arith.sol`

- **Role:** Positive — exhaustive arithmetic / comparison / logical
  / bitwise regression.
- **Demonstrates:** Every binary op the language admits; operator
  precedence; unary `-`; `print` dispatch by argument type.
- **See also:** [04](./04-types.md), [08](./08-expressions.md),
  [13 §13.1](./13-builtins-and-stdlib.md).

### `test_array.sol`

- **Role:** Positive — array exhaustive.
- **Demonstrates:** Array literal, indexed read and write, `for-in`
  iteration (including over empty, single, and nested arrays),
  arrays of structs.
- **See also:** [11](./11-arrays.md), [07 §7.3](./07-control-flow.md).

### `test_control.sol`

- **Role:** Positive — control-flow exhaustive.
- **Demonstrates:** `if` / `if-else` / nested `if`, `while`
  (with zero-trip and nested cases), `for-in` (basic / empty /
  single / nested), early `return`, chained boolean conditions.
- **See also:** [07](./07-control-flow.md), [16 §16.4](./16-examples.md#164-test_controlsol--control-flow-exhaustive).

### `test_edge.sol`

- **Role:** Positive — edge cases regression.
- **Demonstrates:** Large integer literals, `-0`, chained assignment
  (`a = b = c = 42`), assignment-as-expression-result
  (`let y = (x = 5)`), complex bitwise expressions, enum variant
  values (showing T9002), multi-parameter functions, deeply
  nested returns.
- **See also:** [08 §8.2](./08-expressions.md), [10 §10.5](./10-enums.md).

### `test_func.sol`

- **Role:** Positive — function exhaustive.
- **Demonstrates:** Recursive functions, nested calls, void-style
  returns, multiple parameters of mixed types.
- **See also:** [05](./05-functions.md).

### `test_scope.sol`

- **Role:** Positive — scope exhaustive.
- **Demonstrates:** Block-scoping rules, what is visible where
  across nested blocks, parameter scope, shadowing across scopes.
- **See also:** [06](./06-variables-and-scope.md).

### `test_struct.sol`

- **Role:** Positive — struct exhaustive.
- **Demonstrates:** Empty struct, multi-field struct (`Point`,
  `Person`), nested struct (`Nested`), field-name reordering in
  literals (`Point { y: 99, x: 11 }`), field mutation, struct in
  loop, struct passed to function, swap-via-temp pattern.
- **See also:** [09](./09-structs.md), [16 §16.5](./16-examples.md#165-test_structsol--struct-exhaustive).

### `gemini_long.sol`

- **Role:** Positive — combined showcase.
- **Demonstrates:** Imports, enum, struct, orchestration patterns
  layered together. Useful as a self-test for documentation
  coverage.
- **See also:** [16 §16.6](./16-examples.md#166-reading-the-larger-fixtures).

### `largemini.sol`

- **Role:** Positive — broad coverage harness.
- **Demonstrates:** A large surface; not all of it is idiomatic
  (the file is structured as a regression harness, not a model
  program).
- **See also:** [16 §16.6](./16-examples.md#166-reading-the-larger-fixtures).

---

## Negative fixtures

### `error_parse1.sol`

- **Role:** Negative — parse error.
- **Triggers:** Empty initializer in `let`: `let x: int = ;`.
- **Diagnostic family:** `E0001 — Empty initializer in let`.
- **See also:** [`ERROR_REFERENCE.md#E0001`](./ERROR_REFERENCE.md#e0001--empty-initializer-in-let).

### `error_parse2.sol`

- **Role:** Negative — parse error.
- **Triggers:** Missing semicolon on a `let`.
- **Diagnostic family:** `E0002 — Missing semicolon on a statement`.
- **See also:** [`ERROR_REFERENCE.md#E0002`](./ERROR_REFERENCE.md#e0002--missing-semicolon-on-a-statement).

### `error_semantic1.sol`

- **Role:** Negative — semantic error.
- **Triggers:** `return undefined_var;` where `undefined_var` is
  not declared.
- **Diagnostic family:** `E1001 — Variable not in scope`.
- **See also:** [`ERROR_REFERENCE.md#E1001`](./ERROR_REFERENCE.md#e1001--variable-not-in-scope).

### `error_semantic2.sol`

- **Role:** Negative — semantic error.
- **Triggers:** Duplicate `let` of `x` in the same scope.
- **Diagnostic family:** `E1002 — Redefinition of name`.
- **See also:** [`ERROR_REFERENCE.md#E1002`](./ERROR_REFERENCE.md#e1002--redefinition-of-name-variable--parameter--function--struct--enum).

### `error_semantic3.sol`

- **Role:** Negative — semantic error.
- **Triggers:** Duplicate top-level `function foo` declaration.
- **Diagnostic family:** `E1002 — Redefinition of name` (same
  diagnostic as duplicate `let`).
- **See also:** [`ERROR_REFERENCE.md#E1002`](./ERROR_REFERENCE.md#e1002--redefinition-of-name-variable--parameter--function--struct--enum).

### `error_runtime.sol`

- **Role:** Negative — runtime error.
- **Triggers:** `return 1 / 0;` — integer division by zero panics
  at runtime.
- **Diagnostic family:** `E2001 — Integer division by zero`.
- **See also:** [`ERROR_REFERENCE.md#E2001`](./ERROR_REFERENCE.md#e2001--integer-division-by-zero).

---

## Compiler-side smoke test (with caveat)

### `syntax_test.sol` (lives in the compiler crate, not the fixture mirror)

- **Role:** Mixed-syntax driver kept beside the compiler.
- **Caveat:** At time of writing, the file uses `export function` —
  a form the parser rejects (no `export` keyword exists; see
  chapter 03 §3.6 and `T9001`-class `E0003`). Treat the file as
  illustrative of *intent*, not as a positive conformance fixture.
- **See also:** [03 §3.6](./03-syntax.md), [12 §12.2](./12-imports-and-controllers.md).
