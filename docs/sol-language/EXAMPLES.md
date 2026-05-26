# Examples Catalogue

> **Status:** Scope statement only. Substantive content lands in
> commit 4 (alongside chapter 16).

This file is the long-form catalogue of SOL programs that the
documentation draws on. The *guided tour* (with annotations) lives
in [chapter 16](./16-examples.md); this file is the lookup index:
one entry per fixture, with a one-paragraph description, the
language features the fixture exercises, and a pointer into the
chapter where each feature is explained.

## Index format

Each entry has the following shape:

```
### <fixture-name.sol>

**Role:** <positive | negative — and the kind of error if negative>
**Demonstrates:** <bullet list of features>
**See also:** <chapter links>

```<sol-source-here>```
```

The full source of every fixture is included so that the catalogue
is self-contained.

## Index outline (to be populated in commit 4)

### Positive fixtures

- `retest.sol`
- `s1.sol`
- `s2.sol`
- `jjsi.sol`
- `jj_comp.sol`
- `test_arith.sol`
- `test_array.sol`
- `test_control.sol`
- `test_edge.sol`
- `test_func.sol`
- `test_scope.sol`
- `test_struct.sol`
- `fwdecl.sol`
- `gemini_long.sol`
- `largemini.sol`

### Negative fixtures

- `error_parse1.sol`
- `error_parse2.sol`
- `error_semantic1.sol`
- `error_semantic2.sol`
- `error_semantic3.sol`
- `error_runtime.sol`

### Compiler-side smoke test (with caveat)

- `syntax_test.sol` (compiler crate) — mixed-syntax driver kept
  beside the compiler. **Note:** at the time of writing this file
  uses an `export function` declaration which the parser does not
  accept (no `export` keyword exists; see chapter 03 §3.6). Treat
  the file as illustrative of *intent*, not as a positive
  conformance fixture.
