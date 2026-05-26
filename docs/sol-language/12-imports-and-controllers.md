# 12 — Imports, External Functions, and the Host Runtime

> **Status:** Scope statement only. Substantive content lands in
> commit 4. The runtime-integration sections are **snapshots** of one
> host implementation observed on 2026-05-26; the language-level rules
> are stable.

## What this chapter answers

- How does a SOL file declare functions that live outside itself?
- How does a SOL file mark its own functions as callable from the
  outside?
- What contract does the host runtime honor when it loads a session?
- What does "external function" mean at the value level — is a call
  to one syntactically distinguishable from a call to a local one?

## Topics covered

### Language surface (stable)

1. **`ext function name(params) -> T;`** — declares that the host
   runtime will supply a function with this signature. No body;
   terminated with `;`. Calls to it look exactly like calls to any
   other function.
2. **`export function name(params) -> T { … }`** — declares a
   function that the host runtime may invoke from outside the
   program. Carries a body.
3. **Name resolution.** Whether `ext` names share the same namespace
   as plain `function` names; collision behavior.
4. **Type contract.** Argument types and return type must match
   what the host actually supplies; mismatch behavior is described
   here and in chapter 15.

### Host-runtime wiring (snapshot)

5. **Session configuration.** A host typically declares which
   `.sol` file backs a given session via a configuration file.
   The example observed uses a TOML layout in which a `[session.<name>]`
   block sets `source = "path/to/file.sol"` and a separate `[ext]`
   block maps each external function name to a remote endpoint.
6. **Endpoint mapping.** External names declared with `ext function`
   in the source must be present in the host's external-mapping
   table. Names that are declared but not mapped become unresolved at
   load time; names that are mapped but not declared are simply
   unused.
7. **Calls at runtime.** An `ext` call is dispatched by the runtime;
   the SOL program does not see the transport. From the program's
   point of view, the call returns a value (or completes) and
   execution continues.

### Cross-references

- Syntax of `ext` / `export` is also covered in chapter 03 and
  enumerated in [`GRAMMAR.md`](./GRAMMAR.md).
- Type rules for the parameters and return values come from chapter
  04.
- Failure modes — "declared but not mapped", "mapped but the host
  returned the wrong type" — appear in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

## What this chapter deliberately does *not* cover

- The transport, addressing scheme, or peer model of any specific
  host runtime. Those details live outside the SOL language and
  outside this manual.
- Any specific product's deployment model.

## Sources to be cited

- `parser.rs` `ext function` / `export function` productions
- `analyzer.rs` symbol-table handling of `ext` declarations
- `init.rs`, `cli.rs` for the snapshot of one host's load flow
- An example host configuration file (TOML layout above) — surfaced
  with field names but no host-specific branding
- Fixtures: `syntax_test.sol` (canonical `ext` + `export` example),
  `gemini_long.sol` (longer practical use), `s1.sol`, `s2.sol`
