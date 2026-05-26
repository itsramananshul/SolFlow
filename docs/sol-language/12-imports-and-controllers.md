# 12 — Imports, External Functions, and the Host Runtime

> **Status:** Substantive (commit 4). Language-level rules sourced
> from `parser.rs:250–287, 440–474`, `analyzer.rs:84–89, 166–171`,
> and `bytecode.rs:454–466`. The host-runtime integration is a
> **snapshot** of one host observed on 2026-05-26 and may evolve
> independently of the language.

A SOL program is a self-contained file: it cannot read other
`.sol` files at compile time, and there is no module system in the
language proper. What a SOL program *can* do is declare names that
the host runtime will supply (`ext function`) and use names that
the host runtime will arrange to call back into (the conventional
`start` function plus any other plain `function`).

This chapter has two parts:

- **§12.1 – §12.3** — the language surface that is stable across
  any host.
- **§12.4** — a snapshot of one specific host's wiring (TOML
  configuration shape, endpoint mapping rules). Anything in §12.4
  is not part of SOL and may change with the host.

---

## 12.1 `ext function` — declaring an external function

```sol
ext function fetch_orders(query: str) -> str;
ext function notify(channel: str, body: str);
```

Parsed at `parser.rs:250–287`. Syntactic differences from a normal
`function`:

- The keyword `ext` precedes `function`.
- **No body**; the declaration ends with `;`.
- Otherwise the parameter list and return type are identical.

### Analyzer behavior

At the analyzer level (`analyzer.rs:84–89`) an `ext function`
declaration is registered in the global type table exactly like a
regular function — same signature shape, same name lookup, same
duplicate-name rule. The call-site type checks (chapter 05 §5.2)
make no distinction between calling an `ext` function and calling
a local one.

```sol
ext function lookup(id: int) -> str;

function start() -> int {
    let name: str = lookup(42);   // call site is indistinguishable
    print(name);
    return 0;
}
```

### Bytecode emission

The bytecode emitter does distinguish — for each call it checks
whether the target name is in the `ext_functions` set
(`bytecode.rs:454–466`). If yes:

1. Each argument is compiled (pushed onto the stack in source
   order).
2. The function name and its bound endpoint URL are pushed as
   string constants.
3. An `Inst::ExtCall(arg_types, ret_type)` op is emitted.

The runtime dispatches the call through the host's transport layer
(opaque to the program) and pushes the return value back onto the
stack. From the program's point of view the call is synchronous
and returns a single value (or `Void`).

### Endpoint resolution at compile time

The host runtime supplies a *function-name → endpoint-URL* mapping
when it constructs the code generator
(`bytecode.rs:93–96, 457–460`). If an `ext function` is **declared
in the source but not in the host's mapping**, the bytecode emitter
exits at compile time with:

```
no endpoint configured for ext function `<name>`
```

This is a deliberate fail-fast: a program that calls an external
function with no transport bound to it would otherwise crash at
runtime with no explanation.

### What `ext function` is *not*

- Not asynchronous. The compiled call sites block until the
  runtime returns.
- Not a typeclass or trait — there is no way to declare multiple
  `ext function` names with the same signature and a dispatch
  rule.
- Not an opaque type — the language enforces the declared
  signature on every call site; the host must honor it.

---

## 12.2 No `export` keyword

SOL has no `export` keyword. The lexer's keyword table
(`lexer.rs:341–356`) is fifteen entries; `export` is not among
them. The host runtime calls back *into* the program by invoking a
named regular `function`; the convention is `start`. There is no
language-level mechanism for marking a function as "visible from
outside" — every top-level `function` is callable by the host that
loaded the program.

If you encounter a source that uses `export function`, it will fail
at parse time with:

```
unknown declaration: Ident("export")
```

Treat such sources as bugs; rewrite as a plain `function`.

---

## 12.3 `import path.to.thing [as alias];`

```sol
import controllers.warehouse;
import controllers.warehouse as wh;
```

Parsed at `parser.rs:440–474`. The path is one or more
dot-separated identifiers; the `as alias` clause is optional.

### Current semantics — mostly inert

At the analyzer level (`analyzer.rs:166–171`) the import only adds
the alias as a `Void`-typed variable in the global scope:

```rust
if let Some(a) = alias {
    self.add_entry(a.to_owned(), Symbol::Variable { kind: Box::from(Type::Void) });
}
```

That's it. The path is parsed and stored in the AST but not used
by the analyzer or the bytecode emitter. There is **no module
resolution**, no namespace, no symbol re-export. The import
statement exists as a grammar slot for future development; treat
it as inert in the current language.

### Recommendation

Until the import system is implemented, don't write `import` in
production SOL. The form parses cleanly and won't break anything,
but it also doesn't do anything useful, and the alias-as-Void
binding can shadow a real variable name if you reuse the alias
later in scope.

---

## 12.4 Host runtime wiring (snapshot, 2026-05-26)

> **Snapshot.** This section describes how one specific host
> arranges the compile- and run-time integration of SOL programs.
> The shape may evolve independently of the language. Anything in
> this section that depends on configuration-file keys is **not**
> part of SOL.

The host loads a SOL program in three stages:

1. **Read the host configuration file.** The file is TOML; the
   relevant top-level keys are summarized below.
2. **For each session, compile its `.sol` source** with the
   per-controller external-function mapping passed to the
   bytecode emitter. The compiled VM is associated with the
   session name.
3. **Hold the compiled VM ready** to be invoked by name (or to
   auto-start, depending on session configuration). When the host
   serves a request, it dispatches to the named session's VM,
   which begins executing at `start` (or the named function the
   host chose).

### Configuration shape

```toml
[controller]
name = "my-controller"
api_url = "http://0.0.0.0:3000"

[access_points]
some-access-point = { type = "http", method = "POST", url = "..." }
other-access-point = { type = "sdk", endpoint = "...", sdk_lib = "..." }

[sessions]
default = "my-session"

[session.my-session]
source = "tests/my_program.sol"
start_on_init = true

[nodes]
"warehouse" = "http://192.168.1.50:3000/rpc"
"shipping" = "http://192.168.1.51:3000/rpc"

[ext]
"warehouse" = [ "products_list", "stock_check", "is_available" ]
"shipping"  = [ "schedule_pickup" ]
```

Source: the host's loader (an `init.rs` of approximately 130
lines that consumes the TOML and constructs the controller). The
TOML keys above are:

| Section | Meaning |
|---|---|
| `[controller]` | Identity of this controller process and the URL it serves |
| `[access_points]` | Inbound surfaces — HTTP endpoints or SDK shims — the host exposes |
| `[sessions]` | Default session for this controller |
| `[session.<name>]` | A session — `source` points to a `.sol` file; `start_on_init` is an optional boolean |
| `[nodes]` | A directory of *node-name → URL* for remote services |
| `[ext]` | For each node, the list of function names that node provides |

### How `ext` resolves to a URL

The host flattens `[ext]` × `[nodes]` into a single
*function-name → URL* mapping (`init.rs:96–105`):

```text
for (node, funcs) in [ext]:
    url = [nodes][node]    // panic if node not in [nodes]
    for func in funcs:
        ext_flat[func] = url
```

The flattened map is what is handed to the SOL bytecode emitter
via `Codegen::with_ext_endpoints(...)` (chapter 12 §12.1, third
paragraph). If a SOL source declares `ext function f();` and `f`
is not present in this map, compilation fails with:

```
no endpoint configured for ext function `f`
```

### How the program is started

The bytecode emitter, at the end of code generation, appends a
single `Inst::Call(start_addr, 0)` for the function named `start`
(`bytecode.rs:159–161`). If no `start` exists, no startup call is
appended and the host's session VM is "ready but idle" — the host
may still invoke the program by selecting a different entry
function and calling `VM::call_entry`. The conventional shape is
to have `start` and let the host call it implicitly.

### Snapshot caveats

Everything in §12.4 is host-specific and may change. The pieces of
behavior that are **language**-level, and therefore stable, are:

- The form `ext function name(…) -> T;`
- The form `function name(…) -> T { … }` and the convention of
  using `start` as the host-invoked entry
- The compile-time fail-fast when an `ext` declaration has no
  configured endpoint (this happens inside the SOL compiler, so it
  is part of the language toolchain regardless of host)

---

## 12.5 Common diagnostics

| Diagnostic | Cause | Where |
|---|---|---|
| `unknown declaration: Ident("export")` | `export function …` at top level | parser |
| `expected `function` keyword after `ext`` | `ext` followed by anything other than `function` | parser |
| `expected semicolon after ext function declaration` | `ext function …` body or missing `;` | parser |
| `no endpoint configured for ext function `<name>`` | `ext function` declared but no host mapping | bytecode emitter |
| `attempting to make a function call on an undefined name `<name>`` | Call site refers to a name not declared as `function` or `ext function` | analyzer |
| `error: redefinition of `<name>`` | Same name used for two top-level functions (or `ext function` and `function`) | analyzer |
| `node `<node>` defined in [ext] but not found in [nodes]` | Host TOML refers to an undefined node | host (`init.rs:99–101`) |

Full entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 12.6 Sources cited in this chapter

- `parser.rs:250–287` — `ext function` parser
- `parser.rs:440–474` — `import` parser
- `analyzer.rs:84–89` — `ext function` registration
- `analyzer.rs:166–171` — `import` alias registration
- `bytecode.rs:93–96, 132–138, 454–466` — codegen ext handling
- `bytecode.rs:159–161` — `start` autocall
- (Snapshot) `init.rs:10–127` — host configuration loader
- Fixtures: `gemini_long.sol`, `s1.sol`, `s2.sol`
