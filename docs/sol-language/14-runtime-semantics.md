# 14 — Runtime Semantics

> **Status:** Canonical. Sourced from the bytecode stack VM in
> `sol/src/vm.rs` (`Vm`, `step`, `exec_instruction`, `Value`),
> `sol/src/value.rs`, and `sol/src/instruction.rs`.

This chapter answers: what actually happens when a SOL workflow runs?
The canonical execution path is the bytecode stack VM in `sol/src/vm.rs`.
The crate also contains a tree-walking interpreter, but it is
`#[deprecated]`; the VM is the canonical runtime and the only one the
editor bridge drives.

---

## 14.1 The execution model in one paragraph

`Compiler::compile` lowers a parsed `Program` into a flat `Chunk` of
stack instructions. The `Vm` holds a value stack, an indexed array of
locals, a constant pool, and an optional map of host-registered native
functions. Each instruction pops operands from the stack and pushes its
result. Control flow uses absolute `Jump` / `JumpIfFalse` ops that set
the program counter directly. There are no call frames for SOL functions
in the canonical chunk model; execution proceeds until a `Return` /
`Halt`, until the program counter runs off the end, or until the VM
pauses on an external Action.

---

## 14.2 Values

The VM's `Value` enum (`sol/src/value.rs`) is a tagged union; the stack
carries fully-typed values, not raw words:

| Variant | Holds |
|---|---|
| `Bool` | a boolean |
| `Int(i64)` | a 64-bit signed integer |
| `Float(f64)` | an IEEE-754 double |
| `Char` | a Unicode scalar |
| `Str` | a string |
| `Array` | a `Vec<Value>` |
| `Struct` | a string-keyed `HashMap<String, Value>` |
| `Enum(name, variant)` | an enum name and variant name |
| `Unit` | the unit value (e.g. the result of `print`) |
| `Module(name)` | a module reference (from an `import`) |
| `RemoteRef { id, owner }` | a handle to a host-owned remote value |

Arrays, structs, and enums are ordinary owned values in the VM model.

---

## 14.3 Stepping and the statement budget

The host drives the VM with `Vm::step(budget)` (`sol/src/vm.rs`). The
`budget` is a **statement budget**: it counts how many `StmtBoundary`
instructions the VM may cross in one step call, not how many raw
instructions it runs. Each `step` returns a `StepResult`:

```rust
pub enum StepResult {
    Completed(Value),                              // workflow finished
    Yielded(u64),                                  // budget exhausted; call step again
    RemoteCall { capability: String, params: Value }, // external Action; host resolves
    Failed(String),                                // runtime error (plain string)
}
```

- `Completed(v)` — the workflow returned (or ran off the end). `v` is the
  final value.
- `Yielded(n)` — the budget was spent after `n` statement crossings;
  call `step` again to continue.
- `RemoteCall { capability, params }` — the VM paused on an external
  Action (chapter 12). The host resolves it with `resolve_remote_call`
  and calls `step` again.
- `Failed(msg)` — a runtime error occurred. `msg` is a plain string;
  there is no error code and no span (chapter 15).

This cooperative stepping is what lets the editor run a workflow
incrementally and lets a host interleave external Actions.

---

## 14.4 Arithmetic and coercion

Binary arithmetic (`bin_op` in `sol/src/vm.rs`) follows these rules:

- `int op int` produces an `int`.
- Mixing `int` and `float` coerces the int to float and produces a
  `float`.
- `float op float` produces a `float`.
- `+` on two strings concatenates them (`"a" + "b"` is `"ab"`). This is
  the only string operation built into arithmetic.
- Any other operand combination is a runtime error.

```sol
let a = 3 + 4;          # 7        (int)
let b = 3 + 4.0;        # 7.0      (float, int coerced)
let s = "id=" + "42";   # "id=42"  (string concat)
```

### Division by zero

Integer and float division both check for a zero divisor and raise a
runtime error `division by zero` (`Instruction::Div` in `sol/src/vm.rs`).
There is no IEEE infinity result for `/ 0` in this VM; it always errors.
Guard the divisor with an explicit `if` when it might be zero.

### Comparisons

The comparison ops (`cmp_op`) accept int/int, float/float, and mixed
int/float (coercing). Other combinations error. Comparison results are
`Bool`.

---

## 14.5 Truthiness

A condition (`JumpIfFalse`) accepts only two value kinds:

- `Bool` — used directly.
- `Int` — truthy when nonzero, falsy when zero.

Any other value used as a condition is a runtime error
(`cannot use <value> as condition`). `while` and `if` conditions follow
this rule.

---

## 14.6 Strings, arrays, and structs

- **Strings** support length via `len` (byte length) and concatenation
  via `+`. Indexing, slicing, and search are not in the language.
- **Arrays** are constructed by `MakeArray`, indexed with `Index`
  (`a[i]`), and measured with `len`. Indexing out of bounds is a runtime
  error (`index <i> out of bounds`).
- **Structs** are string-keyed maps. `MakeStruct` builds them from a
  struct literal; `MemberAccess` reads a field (`s.field`); `StoreField`
  writes one (`s.field = v`). Reading an absent field is a runtime error
  (`field '<name>' not found`).

```sol
workflow "demo" {
    let xs = [10, 20, 30];
    print(len(xs), xs[1]);   # "3 20"
    let p = { x: 1, y: 2 };
    print(p.x);              # "1"
}
```

---

## 14.7 Enums and the first-character dispatch hazard

An enum value is `Enum(enum_name, variant_name)`. The canonical bytecode
dispatches each enum variant by `(first_char as i128) % 10` of the
variant name. Two variants whose first characters share the same mod-10
residue therefore compare equal at runtime, even when the by-name
simulator runs them correctly. For example, `Active` and `Aborted` both
start with `A` and collide.

Give every variant of an enum a distinct first character to stay clear of
this hazard. The editor validator surfaces it as the
`enum-first-char-collision` warning (chapter 15, `src/graph/validate.ts`).

---

## 14.8 External Actions during a run

When the VM reaches a `call("m.f", p)`, an imported `m.f(args)`, or a
namespace `m::rpc(args)`, it pauses with `StepResult::RemoteCall`
(chapter 12). The host resolves it via `resolve_remote_call(capability,
result)` and resumes by calling `step` again; the resolved result is
pushed onto the stack as the call's value. The VM arranges to skip the
next statement boundary on resume so the budget is not double-counted.

---

## 14.9 Runtime errors

There is **no** in-language error handling: no `try` / `catch`, no
`Result` type, no recovery. A runtime fault surfaces as
`StepResult::Failed(string)` (or, for a few VM-internal faults, as the
`Err(String)` arm of `step`). Representative messages from
`sol/src/vm.rs`:

| Cause | Message |
|---|---|
| Division by zero | `division by zero` |
| Array index out of bounds | `index <i> out of bounds` |
| Missing struct field | `field '<name>' not found` |
| Unknown variable | `variable '<name>' not found` |
| Unknown function | `function '<name>' not found` |
| Non-condition truthiness | `cannot use <value> as condition` |
| Type-incompatible arithmetic | `cannot add/subtract/... <a> and <b>` |

All of these are plain strings. There are no `E0xxx` / `T90xx` codes and
no source spans at the language level. To fail gracefully, validate in
SOL (a defensive `if`) before performing the risky operation.

---

## 14.10 Termination

A run ends in one of three ways:

1. **The workflow returns** (a `Return` / `Halt` instruction). `step`
   returns `Completed(value)`.
2. **The program counter runs off the end.** The VM marks itself
   completed and returns `Completed` with the top-of-stack value (or
   `Unit`).
3. **A runtime error occurs.** `step` returns `Failed(message)` (or the
   `Err` arm). The host decides how to surface it; there is no
   in-language catch.

---

## 14.11 Sources cited in this chapter

- `sol/src/vm.rs` — `Vm`, `step`, `exec_instruction`, `exec_builtin`,
  `bin_op`, `cmp_op`, `StepResult`, `resolve_remote_call`
- `sol/src/value.rs` — the `Value` enum
- `sol/src/instruction.rs` — the `Instruction` / `Chunk` model
- `src/graph/validate.ts` — the editor's `enum-first-char-collision`
  warning
