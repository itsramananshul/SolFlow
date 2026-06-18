# 21 — Behavior Classification

> **Status:** Rewritten against the canonical `openprem-sol-v2` crate
> (the `sol/` directory) and the `compiler-wasm` bridge. This chapter no
> longer uses the deleted compiler's `T9xxx` / `E0xxx` badge scheme. It is
> an honest catalogue of the canonical language's real, observable
> behaviors and quirks, each grounded in `sol/src/*`,
> `compiler-wasm/src/lib.rs`, or the editor `src/*`.

The earlier chapters describe **what** the language does. This chapter
catalogues the **quirks** — the behaviors that surprise people, with the
exact reason each one happens and how to write around it.

The single thing to keep in mind throughout: the canonical pipeline is
`Lexer -> Parser -> Compiler -> Vm` with **no type-checking phase**.
Every error is a plain `String`. Anything a static type-checker would
normally reject either fails at runtime or does not fail at all.

---

## 21.1 No compile-time type checking

There is no analyzer or type-checker. The compiler lowers the workflow
body to bytecode without validating value types, and the VM discovers
mismatches only when an instruction actually runs.

```sol
workflow "type_mismatch" {
    let n = 1 + "two";   # parses, compiles, fails at RUNTIME
    print(n);
}
```

`Compiler::compile` emits `PushInt(1)`, `PushStr`, `Add` and is happy.
At runtime `bin_op` (`sol/src/vm.rs:507`) sees `Int` and `Str`, matches
no arithmetic rule, and returns `StepResult::Failed("cannot add 1 and
two")`. The bridge surfaces that string as a `Warning / Runtime /
E_RUNTIME` diagnostic (`compiler-wasm/src/lib.rs:162`). There is no code,
no span, no "expected int, found str" — just the message.

**Write around it:** keep operand types consistent; do not rely on a
type-checker to catch mistakes before a run.

---

## 21.2 `let` without a type annotation records `bool`

The parser's `let` rule (`sol/src/parser.rs:241`) only reads a type when
a `:` follows the name. With no annotation it records `Type::Bool` by
default:

```rust
let type_ = if matches!(self.peek(), Some(Token::Colon)) {
    self.next_token();
    self.parse_type()?
} else {
    Type::Bool                       // default
};
```

Because nothing type-checks the initializer (§21.1), this default is
mostly cosmetic at runtime — `let n = 5;` still stores an `Int`, since the
VM stores whatever the initializer evaluates to. But the AST node
(visible to the editor and to anything that reads `Stmt::Let.type_`)
claims `bool`. **Annotate every `let`** (`let n: int = 5;`) so the
recorded type matches reality.

---

## 21.3 The enum-variant first-character collision hazard

The canonical bytecode dispatches each enum variant by
`(first_char as i128) % 10`. Two variants whose first characters share a
mod-10 residue compare equal at runtime even though a by-name simulator
runs them correctly. `Status::Active` and `Status::Aborted` both hash to
`'A' % 10`.

The editor surfaces this as a **warning**, code
`enum-first-char-collision`, in `src/graph/validate.ts:99`. It buckets
each enum's variants by `name.charCodeAt(0) % 10` and warns when any
bucket holds more than one variant:

```ts
const code = v.name.charCodeAt(0) % 10;   // src/graph/validate.ts:86
```

Because the in-browser simulator implements the intended by-name
semantics, the collision is invisible during editor testing and only
bites in production. **Write around it:** give every variant of an enum a
distinct first character (the orchestration and payments samples in
`src/samples/*` do exactly this on purpose).

---

## 21.4 Truthiness is `Bool` or nonzero `Int` only

`JumpIfFalse` (`sol/src/vm.rs:374`) accepts only two value kinds as a
condition:

```rust
let truthy = match &cond {
    Value::Bool(b) => *b,
    Value::Int(n)  => *n != 0,
    _ => return Err(format!("cannot use {} as condition", cond)),
};
```

So `if (3)` runs the `then` branch (3 is nonzero), `if (0)` does not, and
`if ("")` / `if (some_struct)` is a runtime error, not a falsy value.
Conditions feed `if`, `while`, and the synthetic comparison the `for`
desugar emits. **Write around it:** use real boolean expressions in
conditions.

---

## 21.5 int / float coercion in arithmetic

`bin_op` (`sol/src/vm.rs:507`) and `cmp_op` (`:527`) coerce a mixed int /
float pair to float:

```sol
workflow "coerce" {
    let x: float = 1 + 2.5;    # Int + Float coerces to Float(3.5)
    print(x);
}
```

int with int yields int; float with float yields float; either mixed pair
promotes the int operand to float and yields a float. The same rule
applies to comparisons. There is no implicit string-to-number coercion
and no number-to-string coercion (except via the `+` rule below and the
`to_str` builtin).

---

## 21.6 `+` concatenates two strings

`Add` is the only operator with a string case (`sol/src/vm.rs:520`):

```rust
(Value::Str(a), Value::Str(b)) if label == "add" => Value::Str(format!("{}{}", a, b)),
```

So `"foo" + "bar"` is `"foobar"`. There is no `Str + Int` rule —
`"n=" + 5` is a runtime error (`cannot add n= and 5`). Convert first:
`"n=" + to_str(5)`.

```sol
workflow "concat" {
    let greeting: str = "hello, " + "world";
    print(greeting);
}
```

---

## 21.7 Division by zero is a runtime error

`Div` (`sol/src/vm.rs:321`) checks the divisor for every numeric
combination and returns `"division by zero"` rather than producing `inf`
or panicking — for ints and floats alike:

```sol
workflow "divzero" {
    let bad: int = 1 / 0;   # StepResult::Failed("division by zero")
    print(bad);
}
```

The bridge reports it as `Warning / Runtime / E_RUNTIME`. The run stops at
that statement.

---

## 21.8 Indexed assignment `a[i] = x` is rejected by codegen

You can read `a[i]` (the `Index` instruction), but you cannot assign
through it. `compile_stmt`'s assignment arm (`sol/src/compiler.rs:81`)
rejects an index target outright:

```rust
Target::Index(_, _) => {
    return Err("index assignment not supported".into());
}
```

So `a[0] = 5;` fails to compile and surfaces as `Error / Codegen /
E_CODEGEN`. Field assignment (`obj.field = x`) *is* supported
(`sol/src/compiler.rs:69`, via `StoreField` then a store back to the root
local). **Write around it:** rebuild the array or assign to a struct
field instead.

---

## 21.9 Top-level statements other than declarations do not parse

`parse_top_level` (`sol/src/parser.rs:84`) accepts only `fn`, `struct`,
`enum`, `workflow`, and `import`:

```rust
match self.peek() {
    Some(Token::Fn)       => …,
    Some(Token::Struct)   => …,
    Some(Token::Enum)     => …,
    Some(Token::Workflow) => …,
    Some(Token::Import)   => …,
    Some(t) => Err(format!("unexpected top-level token {:?}", t)),
    None => Err("unexpected EOF".into()),
}
```

A bare `let x = 1;` or `print("hi");` at file scope is a parse error
(`E_PARSE`). Statements live only inside a `workflow`, `fn`, or block
body. Every runnable program needs a `workflow "name" { … }`.

---

## 21.10 The formatter drops comments

The AST has no comment nodes — the lexer discards `#` comments while
tokenizing (`sol/src/lexer.rs:260`). `format_source` reparses and
pretty-prints the AST, so a format round-trip **loses every comment**.
This is expected, not a bug; the formatter is AST-faithful, not
source-faithful.

---

## 21.11 Only the workflow body is compiled; plain `fn` calls fail at runtime

The compiler emits **only the body of the workflow**
(`sol/src/compiler.rs:40`). Top-level `fn` declarations are recorded by
name but their bodies are never lowered into bytecode. A call to a plain
`fn` from the workflow body becomes `Instruction::Call(name_idx, argc)`,
which the VM dispatches against the native table first and the builtin
table second (`sol/src/vm.rs:126`). A user `fn` is in neither, so it falls
through `exec_builtin`'s catch-all:

```rust
_ => Err(format!("function '{}' not found", name)),   // sol/src/vm.rs:503
```

```sol
fn helper() <- int { return 7; }      # body never compiled

workflow "calls_helper" {
    let v = helper();                 # runtime: "function 'helper' not found"
    print(v);
}
```

The only names that resolve from a workflow body are the builtins
(`print`, `len`, `to_str`, `type_name`), host-registered natives
(`WorkflowExecutor::register_native`, `sol/src/workflow.rs:155`), and
external capabilities (`call("…")`, imported `module.func(...)`, or
`module::rpc(...)`, which are not function calls at all but `WorkflowCall`
/ `ModuleCall` parked for the host). **Write around it:** inline logic
into the workflow body, or register a host native, instead of relying on
top-level user functions.

---

## 21.12 Builtins, and what they accept

`exec_builtin` (`sol/src/vm.rs:455`) defines the entire builtin surface:

| Builtin | Signature | Behavior |
|---|---|---|
| `print(...)` | variadic `<- unit` | space-joins all args, appends a newline, writes to the capture buffer |
| `len(x)` | `str \| array <- int` | element/byte count; errors on other types |
| `to_str(x)` | `any <- str` | the value's `Display` form |
| `type_name(x)` | `any <- str` | one of `bool`, `int`, `float`, `char`, `str`, `array`, `struct`, `enum`, `unit`, `module`, `remote_ref` |

`crypto` (ed25519 / sha512, `sol/src/crypto.rs`) is exported by the crate
but is **not** a builtin; a host must expose it through `register_native`
before SOL code can use it.

---

## 21.13 The five bridge diagnostics and the editor checks

The only diagnostics that exist are the five the `compiler-wasm` bridge
emits and the kebab-case structural checks the editor runs on the graph.
There is no `E0xxx` / `T90xx` scheme.

Bridge codes (`compiler-wasm/src/lib.rs`):

| Severity | Phase | Code | When |
|---|---|---|---|
| Error | Parser | `E_PARSE` | parse failure |
| Error | Codegen | `E_CODEGEN` | compile failure (includes "no workflow", "index assignment not supported") |
| Error | Analyzer | `E_NO_WORKFLOW` | run requested with no workflow |
| Warning | Runtime | `E_RUNTIME` | a statement failed at runtime |
| Error | Internal | `ICE0001` | a caught panic |

Editor structural checks (`src/graph/validate.ts`): `no-entry`,
`unnamed-function`, `enum-first-char-collision`, `missing-input`,
`bad-inline-expression`, `unset-struct`, `unknown-struct`, `unset-field`,
`unset-enum`, `unknown-enum`, `unset-variant`, `unset-call`,
`unknown-call`, `unset-var`, `unresolved-var`, `type-mismatch`. These run
on the graph before SOL is emitted; they are not produced by the
compiler.

---

## 21.14 Quick reference: what to rely on, what to avoid

**Rely on:**

- The pipeline shape (`Lexer -> Parser -> Compiler -> Vm`) and the
  statement-budget step model (yield/resume across remote calls).
- `int`/`float` coercion in arithmetic and comparison.
- `+` concatenating two strings.
- `len`, `to_str`, `type_name`, `print` as the builtin surface.
- Division by zero, indexed assignment, and missing-workflow all failing
  with a clear (if codeless) error.

**Avoid relying on:**

- Any type error being caught before a run (none are).
- An un-annotated `let` recording the initializer's real type (it records
  `bool`).
- Enum variants that share a first character being distinguishable at
  runtime (they collide under the mod-10 dispatch; heed the editor
  warning).
- A plain top-level `fn` call resolving (it does not, unless a host
  native shadows the name).
- Comments surviving a format round-trip (they do not).
- Top-level statements other than declarations parsing (they do not).

---

## 21.15 Sources cited in this chapter

- `sol/src/parser.rs` — `let` bool default, top-level dispatch
- `sol/src/compiler.rs` — workflow-body-only codegen, index-assignment
  rejection, field assignment, capability lowering
- `sol/src/vm.rs` — truthiness, arithmetic/coercion, string `+`,
  division by zero, builtins, plain-call dispatch
- `sol/src/lexer.rs` — `#` comments discarded
- `sol/src/format.rs` — comment-dropping round-trip
- `sol/src/crypto.rs` — host-only `Keypair`
- `sol/src/workflow.rs` — `register_native`
- `compiler-wasm/src/lib.rs` — the five bridge diagnostic codes and the
  `RtErr` runtime shapes
- `src/graph/validate.ts` — `enum-first-char-collision` and the other
  editor structural checks
