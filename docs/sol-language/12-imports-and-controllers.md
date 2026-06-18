# 12 — Imports, External Actions, and the Host Controller

> **Status:** Canonical. Sourced from the `sol/` crate (package
> `openprem-sol-v2`): `sol/src/parser.rs`, `sol/src/ast.rs`,
> `sol/src/vm.rs`, `sol/src/analysis.rs`, and the editor bridge
> `compiler-wasm/src/lib.rs`.

A SOL program is a self-contained source string. It cannot read other
`.sol` files, and there is no module-resolution step in the language.
What a SOL workflow *can* do is reach **external Actions** that the host
(the controller) supplies and resolves on its behalf. There is no
in-language "external function" declaration: SOL has no `ext fn` keyword
(it is not in the canonical lexer). Instead, a workflow names a
capability, the VM pauses with a `RemoteCall`, and the host resolves it.

This chapter has two parts:

- **§12.1 – §12.3** — the language surface (`import`, the three ways to
  reach an Action) that is stable across any host.
- **§12.4** — how a host controller resolves a `RemoteCall` and resumes
  the VM. This is the integration contract, not language syntax.

---

## 12.1 `import` — naming a module

The lexer has two import-related keywords, `import` and `from`. The
parser accepts exactly two forms (`sol/src/parser.rs`, `sol/src/ast.rs`
`ImportSpec`):

```sol
import system;                 # ImportSpec::Module("system")
import "get" from numbers;     # ImportSpec::Named { name: "get", module: "numbers" }
```

- `import module;` introduces a module name. A later `module.func(args)`
  call inside a workflow or function becomes an external Action with the
  capability string `"module.func"`.
- `import "name" from module;` introduces a bare named binding.

There is no `as alias` clause, no dotted path, no `export` keyword, and
no module resolution. Imports are recorded in the AST and are consumed
only by capability analysis (§12.3) and by the VM when it lowers a
qualified call into a `RemoteCall`.

---

## 12.2 Three ways a workflow reaches an external Action

Every external Action becomes a single VM `RemoteCall { capability,
params }` (`sol/src/vm.rs`, `StepResult::RemoteCall`). The `capability`
is a string; `params` is a single value, commonly a struct literal
`{ ... }`. There are three source forms, all equivalent at the VM level:

### `call("module.func", params)`

The built-in `call` form names the capability directly as a string
literal and passes one params value:

```sol
workflow "ingest" {
    let cpu = call("system.cpu", { sample: "1m" });
    print(cpu);
}
```

The VM's `WorkflowCall` instruction pops the params and the capability
string and yields `RemoteCall { capability: "system.cpu", params }`.

### Imported `module.func(args)`

After `import module;`, a member call on the module lowers to the same
`RemoteCall` with capability `"module.func"`:

```sol
import system;

workflow "ingest" {
    let cpu = system.cpu({ sample: "1m" });
    print(cpu);
}
```

### Namespace `module::rpc(args)`

A `::` namespace call (the `ModuleCall` instruction) produces a
capability of the form `"module::rpc"`:

```sol
import system;

workflow "ingest" {
    let cpu = system::cpu({ sample: "1m" });
    print(cpu);
}
```

The VM formats the capability as `"{module}::{rpc}"` and carries the
single params value (defaulting to an empty struct when no argument is
supplied). See `Instruction::ModuleCall` handling in `sol/src/vm.rs`.

In every case the call carries **one** params value. To pass several
fields, wrap them in a struct literal `{ a: 1, b: "x" }`.

---

## 12.3 Capability analysis

Static capability discovery lives in `sol/src/analysis.rs`, not in a
type checker. Two entry points:

```rust
pub fn extract_capabilities(source: &str) -> Result<Vec<String>, String>;
pub fn analyze_workflow(source: &str, name: &str) -> Result<WorkflowAnalysis, String>;
```

- `extract_capabilities` returns the sorted, deduplicated set of every
  capability a source references.
- `analyze_workflow` returns a `WorkflowAnalysis { workflow_name,
  call_graph, imported_modules, capabilities }`, where `call_graph` is an
  ordered list of `WorkflowCallSite { module, capability }`.

A "capability" is gathered from two source shapes: the string literal
inside `call("cap", ...)`, and an imported `module.func(...)` member
call (capability analysis only records the member call when `module` is
an imported module name). A `call(...)` whose capability is a dynamic
expression rather than a string literal is skipped by static analysis
and resolved at runtime. For example:

```sol
import numbers;

workflow "test" {
    let a = call("system.cpu", {});
    let b = numbers.get(42);
    print(a, b);
}
```

`extract_capabilities` returns `["numbers.get", "system.cpu"]`.

---

## 12.4 The host controller resolves the `RemoteCall`

The language does not perform external work; it pauses and hands the
host a capability plus params. The driving loop is `Vm::step(budget)`
(`sol/src/vm.rs`):

1. The host calls `step(budget)` repeatedly. `budget` is a **statement
   budget** (it counts `StmtBoundary` crossings, not raw instructions).
2. When the VM reaches an external Action it returns
   `StepResult::RemoteCall { capability, params }` and pauses.
3. The host inspects `capability` (for example `"system.cpu"` or
   `"system::cpu"`), performs the real work however it chooses
   (HTTP, SDK, local function, anything), and produces a result `Value`.
4. The host calls `resolve_remote_call(capability, result)`, which
   stashes the result, advances past the call site, and arranges for the
   resumed step to skip the next statement boundary so the budget is not
   double-counted.
5. The host resumes by calling `step(budget)` again. The stashed result
   is pushed onto the stack as the call's return value.

There is no transport baked into the language: HTTP, timeouts, retries,
and authentication are all host concerns. The VM only ever sees a
`capability` string and a single params `Value`, and only ever expects a
single result `Value` back.

### Wrapping host-native helpers

A host can also expose synchronous native helpers via
`Vm::register_native(name, func)` (`sol/src/vm.rs`). A registered native
takes precedence over the built-in dispatch when a `Call` names it. This
is how a host exposes, for example, the crate's `crypto` routines
(ed25519 sign/verify, sha512 in `sol/src/crypto.rs`), which are *not*
SOL built-ins (chapter 13). Native helpers run inline and return
immediately; they do not produce a `RemoteCall`.

---

## 12.5 What does not exist

- No `ext fn` / `ext function` declaration. SOL never declares external
  functions in-language; it names capabilities and pauses.
- No `export` keyword, no `as alias`, no dotted import paths.
- No compile-time endpoint binding or "no endpoint configured" error.
  Capability-to-host wiring happens entirely on the host side at
  resolve time.
- No type checking of Action params or results. Mismatches surface at
  runtime as `StepResult::Failed(string)` (chapter 15).

---

## 12.6 Sources cited in this chapter

- `sol/src/parser.rs`, `sol/src/ast.rs` — `import` parsing, `ImportSpec`
- `sol/src/vm.rs` — `WorkflowCall` / `ModuleCall` lowering,
  `StepResult::RemoteCall`, `step`, `resolve_remote_call`,
  `register_native`
- `sol/src/analysis.rs` — `extract_capabilities`, `analyze_workflow`,
  `WorkflowAnalysis`, `WorkflowCallSite`
- `sol/src/crypto.rs` — host-wrappable ed25519 / sha512 helpers
- `compiler-wasm/src/lib.rs` — `analyze_source_json` surfaces the
  analysis to the editor
