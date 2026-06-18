# 05 — Functions

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate.
> Cross-checked against `sol/src/parser.rs` (`parse_function`,
> `parse_params`, `parse_type`), `sol/src/ast.rs` (`FunctionDecl`,
> `Param`), `sol/src/compiler.rs` (`compile`, `compile_expr`), and
> `sol/src/vm.rs` (`exec_builtin`, `Call`/`Return` handling).

SOL has a single function kind, declared with the `fn` keyword.
Functions are not first-class values. There is no closure form, no
anonymous function literal, no method syntax, no overloading, and no
default parameter values. A program is a flat collection of top-level
items: functions, structs, enums, imports, and exactly one workflow.

This chapter covers the declaration form, parameter and return
semantics, and an important compilation fact: only the workflow body is
compiled to bytecode today. Read it carefully before relying on a
top-level `fn`.

---

## 5.1 Declaration

The surface form is:

```sol
fn name(p1: T1, p2: T2) <- RetType {
    # body statements
}
```

Parsed by `parse_function` in `sol/src/parser.rs`. Components:

- **`fn`** keyword (`Token::Fn`). The keyword is `fn`; there is no
  `function` keyword.
- **`name`** is an identifier. Anything else fails with a parse error
  such as `expected identifier, got ...`.
- **Parameter list** in `(` `)`. Zero or more `name: Type` pairs,
  comma separated. Every parameter requires a type annotation; the
  parser expects `name`, then `:`, then a type. A trailing comma after
  the last parameter is tolerated by the loop but is not idiomatic.
- **Optional return arrow** `<- RetType`. The arrow token is `<-`
  (`Token::Arrow`). There is no `->`; writing `->` lexes as two tokens
  (`-` then `>`) and fails to parse. The whole `<- RetType` clause is
  optional. Omit it when the function has no declared return type.
- **Body** is a brace delimited block. The braces are required.

```sol
fn add(a: int, b: int) <- int {
    return a + b;
}

fn announce() {
    print("ready");
}

fn noop() {}
```

All three are valid. `announce` and `noop` omit the `<- RetType` clause,
so they have no declared return type. `noop` has an empty body.

### Types in signatures

The valid type forms (from `parse_type`) are:

- The built in scalar types `bool`, `int`, `float`, `char`, `str`.
- Arrays, written PREFIX as `[]T`, for example `[]int` or `[][]float`.
- Any other identifier, treated as a named type (a struct or enum name).

There is no compile time validation that a named type actually exists,
and no validation that parameters or returns are used consistently. The
return type is recorded on the AST and is otherwise inert: nothing
checks that the body returns a value of that type.

### What you cannot put in a function declaration

| Construct | Reason |
|---|---|
| `function name(...)` | The keyword is `fn` only |
| `fn name(...) -> T` | The return arrow is `<-`, not `->` |
| Generic parameters (`fn f[T](...)`) | The grammar has no type parameter form |
| Default parameter values (`fn f(x: int = 1)`) | A parameter is exactly `name: Type` |
| `pub` / `export` / visibility modifiers | No such keywords exist |

---

## 5.2 How functions are compiled today

This is the single most important fact in this chapter.

The compiler (`sol/src/compiler.rs`, `Compiler::compile`) walks the
top-level items, but it only **records** function names in a set. It
then finds the one `workflow` item and compiles ONLY the workflow
body into a bytecode `Chunk`. The bodies of top-level `fn`
declarations are never emitted.

```sol
# This `fn` parses, and its NAME is registered, but its body is
# never compiled into the chunk.
fn helper(x: int) <- int {
    return x + 1;
}

workflow "demo" {
    print("hello");
}
```

### Calling a function by name

A call expression `name(args)` compiles to a `Call` instruction. At
runtime (`sol/src/vm.rs`), a `Call` first looks the name up in the
host registered native functions; if none matches, it falls through to
`exec_builtin`. The only names `exec_builtin` recognises are the VM
builtins `print`, `len`, `to_str`, and `type_name`. Any other name
produces a runtime string error:

```
function 'helper' not found
```

So in the canonical crate, a plain top-level `fn` is effectively
non-callable from a workflow unless the host has registered a native
function under the same name with `register_native`. Treat top-level
`fn` declarations as signatures that the host runtime may bind, not as
locally executable code.

### The callable surface that actually works

From inside a workflow body you can call:

- **VM builtins**: `print(...)`, `len(...)`, `to_str(...)`,
  `type_name(...)` (see chapter 13).
- **Host native functions** the embedding registered via
  `register_native`.
- **External Actions** through the capability forms in §5.4, which
  suspend the VM as a `RemoteCall` rather than running inline.

---

## 5.3 Returning

Two forms (`parse_stmt` in `sol/src/parser.rs`):

```sol
return;
return value;
```

`return;` parses as `Stmt::Return(None)`. `return value;` parses as
`Stmt::Return(Some(value))`. The trailing `;` is consumed when present.

At the bytecode level (`compile_stmt`):

- `return value;` compiles the value expression, then a `Return`
  instruction.
- `return;` pushes `Unit`, then a `Return` instruction.

In the VM, `Return` pops the top of the stack (or `Unit` if the stack
is empty) and reports it as `Completed(value)`. There is no compile
time check that a function returns a value of any declared type, and no
unreachable code analysis. A `return` simply ends execution of the
running chunk with that value.

### Falling off the end

A workflow (the only compiled body) that runs off the end without a
`return` does not error. When the program counter passes the final
instruction, the VM marks itself completed and reports
`Completed(top_of_stack)`, or `Completed(Unit)` if the stack is empty.
The compiler appends a `Halt` instruction at the end of the workflow
body, and `Halt` likewise completes with the top of stack or `Unit`.

So the practical rule is: the result of a run is whatever value happens
to be on top of the stack when execution ends, or `Unit`. To make the
result explicit, end with `return value;`.

---

## 5.4 Calling external Actions (capabilities)

A workflow reaches the outside world by issuing a capability call. Each
of these compiles to a VM `RemoteCall`: the VM suspends and hands the
host a capability string plus one params value, and the host resumes it
with `resolve_remote_call`.

There are three surface forms (see `compile_expr` in
`sol/src/compiler.rs`):

```sol
# 1. The `call` builtin: a capability string plus one params value.
call("discord.send", { channel: "ops", text: "hi" });

# 2. An imported module action: `module.func(args)`.
#    Requires `import module;` at the top level.
discord.send({ channel: "ops", text: "hi" });

# 3. A namespace / RPC call: `module::rpc(args)`.
discord::send({ channel: "ops", text: "hi" });
```

- The `call("cap", params)` form carries the capability string `cap`
  and a single params value (commonly a struct literal `{ ... }`).
- The `module.func(args)` form only becomes a remote call when
  `module` was imported; the capability string is `"module.func"`.
- The `module::rpc(args)` form produces the capability string
  `"module::rpc"` and passes a single params value.

Capability analysis (`sol/src/analysis.rs`) collects exactly these
strings; see chapter 12 for how a host resolves them.

---

## 5.5 Recursion and ordering

Because only the workflow body is compiled and top-level `fn` bodies
are not emitted, there is no in language recursion to describe in the
canonical crate. Declaration order among top-level items does not affect
compilation: the compiler scans all items first to record imports and
function names, then compiles the single workflow. A workflow can be
declared before or after the functions and imports it references.

---

## 5.6 The workflow entry point

A SOL program has exactly one `workflow`, and it is the unit that runs.
Its name is a string literal:

```sol
workflow "process_order" {
    # body statements run top to bottom
    return 0;
}
```

If a program contains no `workflow`, compilation fails with the string
error `no workflow found in program` (surfaced through the editor bridge
as `E_NO_WORKFLOW`). The workflow body, not a function named `start`, is
the entry point.

---

## 5.7 Common mistakes

| Pattern | What happens |
|---|---|
| `fn add(a: int, b: int) -> int { ... }` | Parse error: the arrow is `<-`, not `->` |
| `function add(...) { ... }` | Parse error: the keyword is `fn` |
| `fn start { ... }` | Parse error: `(` is required after the name |
| Calling a top-level `fn` from a workflow | Runtime error `function '<name>' not found`, unless the host registered a native of that name |
| Relying on a declared return type | Inert: no compile time return checking exists |
| Expecting overloading or default params | Not supported by the grammar |

---

## 5.8 Sources cited in this chapter

- `sol/src/parser.rs` — `parse_function`, `parse_params`, `parse_type`,
  `parse_stmt` (`return`)
- `sol/src/ast.rs` — `FunctionDecl`, `Param`, `Type`, `Stmt::Return`
- `sol/src/compiler.rs` — `compile` (workflow only), `compile_stmt`
  (`Return`), `compile_expr` (`Call`, `WorkflowCall`, `NamespaceCall`)
- `sol/src/vm.rs` — `Call` handling, `exec_builtin`, `Return`/`Halt`
- `sol/src/analysis.rs` — capability extraction
- `compiler-wasm/src/lib.rs` — `E_NO_WORKFLOW` for a missing workflow
