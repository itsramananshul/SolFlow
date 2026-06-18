# 20 — Implementation Notes

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate
> (the `sol/` directory) and the `compiler-wasm` bridge. The old
> standalone compiler and runtime crates were deleted; this chapter no
> longer describes their analyzer, bytecode emitter, or `E0xxx` / `T90xx`
> codes. Everything below cites real files under `sol/src/*`,
> `compiler-wasm/src/lib.rs`, or the editor `src/*`.

This chapter is the maintainer-facing overview of how the canonical SOL
implementation is wired. It is the place to look when "the docs say X but
I am seeing Y at runtime" and the discrepancy is rooted in how the
compiler emits bytecode or how the VM steps it.

---

## 20.1 The pipeline in one paragraph

The canonical language lives entirely in the `sol/` crate (package
`openprem-sol-v2`). The pipeline is:

```text
source text
  →  Lexer            // sol/src/lexer.rs   — chars to tokens
  →  Parser           // sol/src/parser.rs  — tokens to a Program (AST)
  →  Compiler         // sol/src/compiler.rs — Program to a bytecode Chunk
  →  Vm               // sol/src/vm.rs      — stack machine over the Chunk
```

`WorkflowExecutor` (`sol/src/workflow.rs`) ties parse, compile, and run
together for a single named workflow. There is **no separate type-checker
or semantic-analyzer phase** and **no error-code system**. Every fallible
step returns `Result<_, String>` — a plain message. Type mismatches are
not caught at compile time; they surface at runtime as
`StepResult::Failed(String)`.

A tree-walking `interpreter` module still exists but is `#[deprecated]`
(`sol/src/lib.rs:12`); the bytecode `vm` is the canonical execution path.

The editor never touches the crate directly. It talks to the language
through the `compiler-wasm` bridge (`compiler-wasm/src/lib.rs`), which
wraps the crate and returns a stable JSON envelope.

---

## 20.2 The modules, and what each one does

The crate re-exports its public surface from `sol/src/lib.rs`. The
modules are:

| Module | File | Responsibility |
|---|---|---|
| `lexer` | `sol/src/lexer.rs` | `Lexer` turns source chars into a `Token` stream. `#` line comments, `<-` arrow, `fn`/`workflow` keywords, `i64`/`f64`/`bool`/`char`/`str` literals. Never hard-errors. |
| `parser` | `sol/src/parser.rs` | Recursive-descent `Parser` consuming tokens into a `Program`. Precedence climbing for expressions; prefix `[]T` array types. |
| `ast` | `sol/src/ast.rs` | The AST node types: `Program`, `TopLevel`, `Stmt`, `Expr`, `Type`, `Target`, plus the declaration structs (`FunctionDecl`, `WorkflowDecl`, `StructDecl`, `EnumDecl`, `ImportDecl`). All `Serialize`. |
| `compiler` | `sol/src/compiler.rs` | `Compiler` lowers the workflow body into a bytecode `Chunk`. |
| `instruction` | `sol/src/instruction.rs` | The `Instruction` enum (the bytecode op set) and the `Chunk` container. |
| `vm` | `sol/src/vm.rs` | `Vm`, a stack machine that executes a `Chunk`. Owns the builtin table, the native-function table, and the statement-budget stepper. |
| `value` | `sol/src/value.rs` | The runtime `Value` enum — the universal value type, `Serialize`/`Deserialize` for network transfer. |
| `workflow` | `sol/src/workflow.rs` | `WorkflowExecutor` and `WorkflowState` — drive a run and snapshot/restore it. |
| `analysis` | `sol/src/analysis.rs` | `extract_capabilities` and `analyze_workflow` — static capability extraction from a source string. |
| `crypto` | `sol/src/crypto.rs` | `Keypair` (ed25519 sign/verify, sha512). Exported by the crate but **not** a SOL builtin. |
| `format` | `sol/src/format.rs` | `format_source` / `format_program` — reparse and pretty-print. |
| `interpreter` | `sol/src/interpreter.rs` | Deprecated tree-walker; retained but not on the canonical path. |

---

## 20.3 The bytecode model: `Instruction` and `Chunk`

The compiler produces a single `Chunk` (`sol/src/instruction.rs:5`):

```rust
pub struct Chunk {
    pub instructions: Vec<Instruction>,
    pub constants:    Vec<Value>,
    pub locals_count: u16,
    pub locals_names: Vec<String>,
}
```

`constants` holds string/struct/enum payloads referenced by index;
`locals_names` is the ordered list of local-variable slots (workflow
`let` bindings plus the synthetic `__for_iter_*` / `__for_idx_*` slots a
`for` loop allocates). `locals_count` is the slot count the VM
pre-allocates with `Value::Unit`.

The full op set (`sol/src/instruction.rs:30`):

- **Push literals:** `PushInt(i64)`, `PushFloat(f64)`, `PushBool(bool)`,
  `PushChar(char)`, `PushStr(u16)` (constant index), `PushUnit`.
- **Locals:** `LoadLocal(u16)`, `StoreLocal(u16)`, and `LoadName(u16)`
  (resolve a name against `locals_names` at runtime; errors
  `variable '<name>' not found` if absent).
- **Composites:** `MakeArray(u16)`, `MakeStruct(u16)` (consumes
  key/value pairs), `MakeEnum(u16, u16)` (enum-name and variant-name
  constant indices), `MemberAccess(u16)`, `StoreField(u16)`, `Index`,
  `Len`.
- **Arithmetic / logic:** `Neg`, `Not`, `Add`, `Sub`, `Mul`, `Div`,
  `Eq`, `Ne`, `Lt`, `Gt`, `Le`, `Ge`, `And`, `Or`.
- **Control flow:** `Jump(u32)`, `JumpIfFalse(u32)`, `Pop`, `Return`,
  `Halt`.
- **Calls:** `Call(u16, u8)` (name-constant index, arg count) for
  builtin / native dispatch; `WorkflowCall` and `ModuleCall(u16)` for
  external capability calls.
- **Stepping:** `StmtBoundary` — a no-op marker the stepper counts (see
  §20.5).

`Jump` and `JumpIfFalse` carry absolute instruction indices, patched by
the compiler after the branch body is emitted (`sol/src/compiler.rs:88`
for `if`, `:125` for `while`, `:146` for `for`).

---

## 20.4 What the compiler actually compiles

This is the single most important implementation fact: **the compiler
only emits the body of one workflow.** `Compiler::compile`
(`sol/src/compiler.rs:19`) does a first pass to record import and
function names into `HashSet`s, then:

```rust
let workflow = program.items.iter().find_map(|item| {
    if let TopLevel::Workflow(w) = item { Some(w.clone()) } else { None }
}).ok_or_else(|| "no workflow found in program".to_string())?;

for stmt in &workflow.body.stmts {
    self.compile_stmt(stmt, &mut chunk, &mut locals)?;
}
chunk.instructions.push(Instruction::Halt);
```

Consequences worth internalizing:

- **Top-level `fn` bodies are never emitted.** Function declarations are
  recorded by name only. There is no per-function frame, no inline
  function body, no call/return into user functions.
- **Calling a plain `fn` from the workflow body** lowers to
  `Instruction::Call(name_idx, argc)` (`sol/src/compiler.rs:318`). At
  runtime the VM first checks the native-function table, then the builtin
  table (`sol/src/vm.rs:126`). A user `fn` is in neither, so the call
  ends in `exec_builtin`'s catch-all and fails with
  `function '<name>' not found` (`sol/src/vm.rs:503`) — unless the host
  registered a native of that name first.
- **A program with no workflow does not compile** (the `ok_or_else`
  message above), surfaced by the bridge as `E_CODEGEN`.

The set of names that resolve in the workflow body is therefore: the
builtins, any host-registered natives, and capability calls (which are
not "calls" in the bytecode sense — they are `WorkflowCall` /
`ModuleCall`, see §20.7).

---

## 20.5 The statement-budget step model

`Vm::step(budget)` (`sol/src/vm.rs:83`) does not run the whole program
and does not count raw instructions. `budget` is a **statement budget**:
the stepper runs instructions until it has crossed `budget`
`StmtBoundary` markers, then yields.

The compiler appends a `StmtBoundary` after each statement it lowers
(after `let`, after `assign`, after the `if`/`while`/`for` body, after an
expression statement, after `emit` — see the `StmtBoundary` pushes
throughout `sol/src/compiler.rs:50`–`208`). `return` does not get a
boundary; it emits `Return` and unwinds.

The loop (`sol/src/vm.rs:99`):

```rust
while stmts_ran < budget && self.pc < self.chunk.instructions.len() {
    let instr = self.chunk.instructions[self.pc].clone();
    let is_boundary = matches!(instr, Instruction::StmtBoundary);
    match self.exec_instruction(&instr)? {
        InsResult::Continue        => { self.pc += 1; if is_boundary { stmts_ran += 1; } }
        InsResult::ContinueNoAdvance => {}            // a Jump already set pc
        InsResult::Returned(val)   => return Completed(val),
        InsResult::RemoteCall(..)  => return RemoteCall { .. },
        InsResult::CallFunc(name, args) => { /* dispatch native/builtin, push result */ }
    }
}
```

`step` returns a `StepResult` (`sol/src/vm.rs:546`):

- `Completed(Value)` — the workflow finished (`Halt`, `Return`, or `pc`
  ran off the end). The value is whatever is left on top of the stack, or
  `Unit`.
- `Yielded(steps)` — the budget was exhausted but the program is not
  done. The host loops and calls `step` again.
- `RemoteCall { capability, params }` — the program hit a `WorkflowCall`
  or `ModuleCall`. The VM has parked the call in `pending_call`; the host
  resolves it and resumes with `resolve_remote_call` (`sol/src/vm.rs:148`),
  which feeds the result back in via `pending_result` and skips the next
  boundary so the resumed statement is not double-counted.
- `Failed(String)` — an instruction returned an error (the plain string
  is the only diagnostic).

This budgeted, yield-and-resume design is what makes a SOL run pausable
across an `await` for a remote capability and across a host scheduler
tick.

---

## 20.6 How `WorkflowExecutor` drives a run

`WorkflowExecutor::new(source, workflow_name)` (`sol/src/workflow.rs:36`)
parses the source, compiles it once into a `Chunk`, finds the workflow
with the requested name, and builds a `Vm` over the chunk. It caches
`locals_names` so it can map VM local slots back to named bindings.

Each `WorkflowExecutor::step(budget)` (`sol/src/workflow.rs:129`)
delegates to `Vm::step`, then refreshes its `bindings` map from the VM's
locals (skipping `Unit` slots) so a host can observe live variable
values. `save()` / `from_state()` snapshot and restore the full VM
(`pc`, stack, locals, `pending_call`, `pending_result`) into a
serializable `WorkflowState`, which is how a run survives being forwarded
between controllers.

`register_native(name, func)` (`sol/src/workflow.rs:155`) installs a host
function callable from the workflow body — this is the only way to make a
plain-`fn`-style call name resolve (see §20.4), and the only way to
expose `crypto` or any other host capability to SOL code.

---

## 20.7 External capability calls

There is no `ext fn`. A workflow reaches the outside world three ways,
all of which become a `RemoteCall`:

- `call("module.cap", params)` — the `call` keyword. Lowers to
  `WorkflowCall` with the capability string and one params value.
- `module.func(params)` for an imported `module`. The compiler detects
  the imported-module receiver (`sol/src/compiler.rs:294`) and lowers it
  to `WorkflowCall` with capability `"module.func"`.
- `expr::rpc(params)` — a namespace call. Lowers to `ModuleCall`
  (`sol/src/compiler.rs:335`), producing capability `"module::rpc"`.

There is also one built-in capability shim: a bare `sleep(...)` call
lowers to a `WorkflowCall` on `"__system.sleep"`
(`sol/src/compiler.rs:297`).

In every case the VM parks the call in `pending_call` and returns
`StepResult::RemoteCall`. The host is responsible for resolving the
capability and resuming.

`analysis.rs` mirrors this lowering statically:
`analyze_workflow(source, name)` (`sol/src/analysis.rs:30`) walks the AST
and collects each capability string (`call("…")` literals and
imported-module calls) into a `WorkflowAnalysis` with `workflow_name`,
`call_graph` (`{module, capability}`), `imported_modules`, and a sorted,
deduped `capabilities` list. It never compiles or runs anything.

---

## 20.8 The runtime value model

`Value` (`sol/src/value.rs:20`) is the one runtime type:

`Bool(bool)`, `Int(i64)`, `Float(f64)`, `Char(char)`, `Str(String)`,
`Array(Vec<Value>)`, `Struct(HashMap<String, Value>)`,
`Enum(name, variant)`, `Unit`, `Module(name)`, and
`RemoteRef { id, owner }` (a handle to a value owned by another
controller). All variants are `Serialize`/`Deserialize`, which is what
lets a paused workflow's stack and locals travel across the wire.

Notable runtime semantics, all in `sol/src/vm.rs`:

- **Arithmetic** (`bin_op`, `:507`): int with int yields int; mixing int
  and float coerces both to float; `Add` on two `Str` concatenates. Other
  combinations error.
- **Division** (`Div`, `:321`): division by zero is a runtime error for
  both int and float (`"division by zero"`).
- **Truthiness** (`JumpIfFalse`, `:374`): only `Bool` or `Int` (nonzero
  is true) is a valid condition; anything else errors.
- **Builtins** (`exec_builtin`, `:455`): `print(...)` (variadic,
  space-joins and appends a newline into a captured output buffer,
  returns `Unit`), `len(str|array) <- int`, `to_str(any) <- str`,
  `type_name(any) <- str`. The catch-all errors `function '<name>' not
  found`.

`print` writes to a thread-local capture buffer (`SOL_OUTPUT`,
`sol/src/vm.rs:10`) rather than real stdout, so the WASM/browser host can
drain it with `take_output()` after a run.

---

## 20.9 The formatter

`format_source(src)` (`sol/src/format.rs`, re-exported at
`sol/src/lib.rs:26`) reparses the source and pretty-prints the AST:
4-space indent, the `<-` return arrow, `workflow "name" { }`,
`if (c) { } else { }`. Because the AST has no comment nodes (the lexer
discards `#` comments entirely), **a format round-trip drops all
comments.** Treat the formatter as AST-faithful, not source-faithful.

---

## 20.10 The `compiler-wasm` editor bridge

`compiler-wasm/src/lib.rs` is a thin `wasm-bindgen` wrapper over the
crate. Every exported function returns a JSON `Envelope { ok, value,
diagnostics }` (and `run_source_json` adds a `run` object). The exports:

| Export | Crate call | Value on success |
|---|---|---|
| `version` | `CARGO_PKG_VERSION` | the crate version string |
| `parse_source_json` | `Parser::parse` | the `Program` (AST) |
| `analyze_source_json` | `analyze_workflow` per workflow | program plus per-workflow capabilities/imports |
| `compile_source_json` | `Compiler::compile` | program plus `instruction_count` |
| `compile_for_wire_json` | `Compiler::compile` | the full serialized `Chunk` |
| `format_source_json` | `format_source` | the formatted source |
| `run_source_json` | `WorkflowExecutor` step loop | a `run` object (`return_value`, `output`, `steps`, `runtime_error`, `trace`, …) |

Every call is wrapped in `guarded(...)` (`compiler-wasm/src/lib.rs:52`),
which installs a panic hook and converts any panic into the
`ICE0001` internal-error envelope. The run loop drives `exec.step(64)`
(a 64-statement budget per turn) under a guard counter, draining the
`print` capture buffer at the end.

### The complete bridge diagnostic vocabulary

The bridge emits exactly five codes (`compiler-wasm/src/lib.rs`):

| Severity | Phase | Code | Raised when |
|---|---|---|---|
| Error | Parser | `E_PARSE` | `Parser::parse` returns `Err` |
| Error | Codegen | `E_CODEGEN` | `Compiler::compile` (or executor construction) returns `Err` |
| Error | Analyzer | `E_NO_WORKFLOW` | a run is requested but the program has no workflow |
| Warning | Runtime | `E_RUNTIME` | `StepResult::Failed` (or a step `Err`) during a run |
| Error | Internal | `ICE0001` | a panic was caught by `guarded` |

The browser sim emits only two runtime-error shapes
(`RtErr`, `compiler-wasm/src/lib.rs:129`): `ExtCallBlocked { function_name,
url }` (a `RemoteCall` reached during the sim, which cannot make real
network calls) and `StepLimit { limit }`. There are no `E0xxx` or `T90xx`
codes anywhere in the live pipeline; the diagnostic JSON shape (severity,
phase, code, message, span, related, help) is the stable editor contract
in `src/compiler/types.ts`, whose `DiagnosticPhase` reserves a `Lexer`
phase that is never emitted today.

---

## 20.11 Editor-side structural validation

Separate from the bridge, the editor runs structural checks on the graph
in `src/graph/validate.ts`, with kebab-case codes (`no-entry`,
`unnamed-function`, `enum-first-char-collision`, `missing-input`,
`bad-inline-expression`, `unset-struct`, `unknown-struct`, `unset-field`,
`unset-enum`, `unknown-enum`, `unset-variant`, `unset-call`,
`unknown-call`, `unset-var`, `unresolved-var`, `type-mismatch`). These are
graph-shape diagnostics, not compiler output; see chapter 21 for the
behavior each one guards against.

---

## 20.12 Sources cited in this chapter

- `sol/src/lib.rs` — module list and public re-exports
- `sol/src/lexer.rs` — tokenizer
- `sol/src/parser.rs` — recursive-descent parser, `let` type default,
  prefix `[]T` array types
- `sol/src/ast.rs` — AST node types
- `sol/src/compiler.rs` — workflow-body-only codegen, capability lowering,
  branch patching
- `sol/src/instruction.rs` — `Instruction` op set and `Chunk`
- `sol/src/vm.rs` — stepper, statement budget, builtins, arithmetic,
  capability parking, output capture
- `sol/src/value.rs` — runtime `Value` enum
- `sol/src/workflow.rs` — `WorkflowExecutor`, `WorkflowState`,
  `register_native`
- `sol/src/analysis.rs` — capability extraction
- `sol/src/crypto.rs` — `Keypair` (host-only)
- `sol/src/format.rs` — formatter (drops comments)
- `compiler-wasm/src/lib.rs` — the wasm bridge and its five diagnostic codes
- `src/compiler/types.ts`, `src/graph/validate.ts` — the editor diagnostic
  contract and structural checks
