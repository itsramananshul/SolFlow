# 14 — Runtime Semantics

> **Status:** Substantive (commit 4). Sourced from `bytecode.rs`
> (whole file — instruction emission), `vm.rs:1–476` (instruction
> execution, frame layout, heap layout), and the fixture
> `error_runtime.sol`.

This chapter answers: "what actually happens when a SOL program
runs?" The earlier chapters describe what the language *accepts*
syntactically and what the analyzer *checks* semantically; this
chapter is about evaluation order, side effects, heap behavior,
and the runtime errors a program can trap into.

---

## 14.1 The execution model in one paragraph

The compiler lowers each `.sol` file into a flat sequence of
stack-based bytecode instructions (`bytecode.rs`). The VM
(`vm.rs`) is a single-threaded interpreter that holds a value
stack, a call stack of frames, and a heap of compound values
(strings, structs, arrays). Each instruction reads from the
stack, may consult the heap, and pushes its result back. Control
flow is implemented with absolute `Jump` / `JumpFalse` ops that
target instruction offsets. Function calls push a new frame; the
function epilogue pops back. The VM runs in a tight loop until
`done` is set or the instruction pointer runs off the end of the
program.

---

## 14.2 The value stack and the heap

### Stack values are `u64`

Every value on the value stack is a `u64` (`vm.rs:18, 71–73`).
The interpretation of those 64 bits depends on the *type* the
compiler attached to the value — but the stack itself carries no
type tag. Per type:

| Type | What the 64 bits hold |
|---|---|
| `int` | `i64` (signed) — see §14.4 |
| `float` | IEEE-754 `f64` bit pattern (`f64::to_bits` / `from_bits`) |
| `bool` | `0` (false) or `1` (true) |
| `char` | `u32` Unicode scalar value, zero-extended |
| `str` | heap index into `self.heap` |
| struct | heap index |
| array | heap index |

### Heap layout

The heap is a `Vec<HeapObject>` (`vm.rs:8–11, 20`). The three
heap-object variants today:

```rust
enum HeapObject {
    String(String),
    Struct(Vec<u64>),
    Array(Vec<u64>),
}
```

Heap objects are referenced by *index*, not by pointer. A heap
index is a `u64` that fits on the stack. Two important
consequences:

1. **Struct and array values have reference semantics by default.**
   Passing a struct or array to a function passes the heap index;
   mutation through one binding is visible through the other.
   (Confirmed by `vm.rs:189–214, 522–537` plus the `Inst::Call`
   frame setup at `vm.rs:56–67`, which pushes argument values
   verbatim without deep-copy.)
2. **Heap entries are never freed today.** The `HeapObject` vector
   grows monotonically with allocations; there is no garbage
   collector. This is sustainable only because SOL programs are
   typically short-lived per session.

### Frames

A call frame is `{ return_ptr: usize, old_fp: usize }`
(`vm.rs:13–16`). The interpreter stores the instruction to return
to and the previous frame pointer. Locals live on the value stack
above the current `fp`; `Inst::LoadLocal(offset)` and
`Inst::StoreLocal(offset)` index relative to `fp`
(`vm.rs:118–131`).

---

## 14.3 Evaluation order

### Argument order

Function arguments are evaluated **left to right** and pushed onto
the stack in source order (`bytecode.rs:469–471, 461–463,
473–477`). The called frame's `fp` is set so that argument 0 is
at `fp + 0`, argument 1 is at `fp + 1`, and so on. **Side effects
in argument expressions occur in source order.**

### Operand order

Binary operators evaluate the left operand first, then the right
operand, then apply the op (`bytecode.rs:366–370`). The runtime
arithmetic helpers reflect this — they pop `b` (RHS) then `a` (LHS)
and apply `a OP b` (`vm.rs:143–146`, etc.).

### Short-circuiting

`&&` and `||` are **not short-circuiting** at the bytecode level
(`vm.rs:177–178`). Both operands are evaluated and pushed before
the `LogAnd` / `LogOr` op consumes them. Chapter 07 §7.5 has the
defensive nested-`if` pattern.

### Statement order

Statements run in source order; control-flow constructs change
the instruction pointer (`Jump` / `JumpFalse`) but the *emitted*
order of statements is exactly source order.

---

## 14.4 Numeric behavior

### Integer

The lexer accepts integer literals as `i128`; the VM truncates
them to `i64` when they reach an arithmetic instruction
(`vm.rs:143–146`):

```rust
let b = self.pop() as i64; let a = self.pop() as i64;
self.push((a + b) as u64);
```

Two consequences:

- **Overflow.** In release builds, Rust integer arithmetic wraps
  in two's complement. In debug builds, overflow panics. Programs
  shipped via a release build of the SOL compiler therefore wrap
  silently; if you need overflow detection you must implement it
  manually.
- **Truncation of large literals.** `9_223_372_036_854_775_808`
  (which is `i64::MAX + 1`) parses cleanly as `i128` and then
  truncates to `i64::MIN` at runtime. Avoid literals outside the
  `i64` range.

### Integer division by zero

`a / 0` where `a` and `0` are `i64` raises a runtime panic:

```
thread '...' panicked at 'attempt to divide by zero', src/sol/vm.rs:...
```

Fixture: `error_runtime.sol`. The panic is *not* caught by SOL —
it terminates the session. Defensive: check the denominator with
an explicit `if`.

### Float

Float arithmetic uses native `f64` ops (`vm.rs:156–166`). IEEE-754
semantics apply: `inf`, `-inf`, `NaN` are valid result values;
division by zero yields one of them and does **not** trap.

```sol
let x: float = 1.0 / 0.0;        # x is inf
let y: float = 0.0 / 0.0;        # y is NaN
```

`NaN` is unequal to itself; `x == x` is false if `x` is `NaN`.
Comparisons against `NaN` return false. This is standard IEEE
behavior; it is mentioned here because the language does **not**
provide an `is_nan` predicate.

---

## 14.5 Strings

Strings are heap-resident `HeapObject::String` values
(`vm.rs:7, 50–54, 109–112`). The stack carries a heap index. Two
literals always allocate two distinct heap entries:

```sol
let a: str = "hello";
let b: str = "hello";
# a and b refer to different heap entries; they are NOT pointer-equal
```

String **equality** is content-based (`bytecode.rs:683`, runtime
`EqStr` op). String **concatenation** is not reachable from source
(chapter 04 §4.2.4). String **indexing**, **slicing**, and
**length** are not available; expose any of these via `ext
function`.

---

## 14.6 Structs

Construction (`Inst::NewStruct(n)`) pops `n` field values from the
stack, packs them into a `Vec<u64>`, pushes a new heap entry, and
pushes the entry index (`vm.rs:189–196`).

Field access (`Inst::GetField(idx)`) pops the heap index, looks up
the `Vec<u64>`, and pushes the element at `idx`
(`vm.rs:198–214`). Field write (`Inst::SetField(idx)`) does the
mirror: it pops the heap index and the new value, and writes the
new value into the heap entry at `idx`.

### The alphabetical field order

The bytecode emitter sorts each struct's fields alphabetically by
name before recording the layout (`bytecode.rs:126–131`). All
subsequent emission — `NewStruct`, `GetField`, `SetField` — uses
the sorted layout's positions. Two important practical effects:

1. **Source declaration order does not affect runtime behavior.**
   `struct Point { y: int, x: int }` lays out as `{ x, y }`
   alphabetically.
2. **Missing fields are zero-filled.** When a struct literal omits
   a field, the emitter pushes `Inst::PushConst(ExprUndefined)` in
   that slot (`bytecode.rs:500`), which materializes as `0`. This
   is documented because the analyzer does not warn about missing
   fields (chapter 09 §9.2).

### Reference semantics

A struct value on the stack is a heap index. Passing a struct to
a function passes the index, not a copy. Writing to a struct's
field through one binding is visible through every other binding
that refers to the same heap entry. There is no syntax for an
explicit copy; if you need to duplicate a struct, manually
construct a new one.

---

## 14.7 Arrays

Construction (`Inst::NewArray`) pops the desired length from the
stack and pushes an empty heap-resident `Vec<u64>`. The emitter
then dups the heap index per element, pushes the element index
and value, and calls `SetElem` to fill each slot
(`bytecode.rs:522–532`). For a 3-element array literal that
emits the equivalent of:

```text
PushConst(3); NewArray;
Dup; PushConst(0); <compile el0>; SetElem; Pop;
Dup; PushConst(1); <compile el1>; SetElem; Pop;
Dup; PushConst(2); <compile el2>; SetElem; Pop;
```

(`bytecode.rs:522–531`). The final stack value is the heap index
of the array.

Index read (`GetElem`) and write (`SetElem`) are direct
`Vec<u64>` indexing operations at the runtime level; the VM does
*not* perform an explicit bounds check before indexing — out-of-
bounds reads or writes therefore depend on the underlying
`Vec::index` behavior, which panics on out-of-bounds in Rust.

The implicit `for-in` desugar (`bytecode.rs:272–328`) materializes
a synthetic length local using `Inst::ArrayLen` and walks the
array with a counter. The length op is not reachable directly
from source — the language has no `len(arr)` syntax (chapter 13).

### Reference semantics

Like structs, arrays are heap-resident and referenced by index.
Mutation visible through aliases.

---

## 14.8 Function calls and frames

### Local function call

`Inst::Call(addr, n)` (`vm.rs` runtime, paraphrased from
`bytecode.rs:467–472`):

1. Push the current `return_ptr` and `old_fp` as a new frame.
2. Set `fp = stack.len() - n` so locals start at the arguments.
3. Set `inst_ptr = addr` to enter the callee.

`Inst::Ret` and `Inst::RetVal` undo this (the former with no return
value, the latter pushing the return value back onto the caller's
stack).

Recursion is supported by this design (the stack grows per call,
the frame pointer is restored on return). There is no tail-call
elimination — deep recursion eventually stack-overflows.

### External call

`Inst::ExtCall(arg_types, ret_type)` consumes the URL and function
name pushed alongside the arguments, dispatches through the host's
transport (the SOL VM doesn't see the wire), and pushes the
return value back. The call is synchronous from the program's
viewpoint.

---

## 14.9 Runtime errors

The VM's panic surface — anything that aborts the session at
runtime — is:

| Cause | Source | Defense |
|---|---|---|
| Integer division by zero | `vm.rs:146` (`a / b` where `b == 0`) | check denominator with `if` |
| Stack underflow / overflow | `vm.rs:77` (`pop` on empty stack); also implicit in any push past `Vec` capacity | this is an internal-error class; correctly-typed programs should not trigger it. If you do, it likely indicates a compiler bug. |
| Array index out of bounds | `vm.rs` `GetElem` / `SetElem` via `Vec::index` | range-check before indexing |
| Heap index out of bounds | a malformed struct/array reference | should not occur in correctly-emitted programs |
| Mis-typed JSON in `rpc_*` | `vm.rs:339–476` panics paths | validate JSON shape before deserializing |
| `process::exit(1)` from the compiler proper | many `eprintln! + exit(1)` sites in `parser.rs` / `analyzer.rs` / `bytecode.rs` | compile-time, not runtime; but worth noting that a "compilation failure" actually terminates the host process today |

There is **no language-level error handling** — no `try` / `catch`,
no `Result<T,E>` type, no panic catching. A runtime error
terminates the session unrecoverably. To handle failures
gracefully, do the checking in SOL (`if denominator != 0`) or
delegate to an `ext function` that the host can surface as a
typed result.

---

## 14.10 Termination

A SOL session "finishes" in one of three ways:

1. **`start` returns.** The VM unwinds the call stack normally;
   the top-of-stack value at the moment of the final `Ret` is the
   session's exit value.
2. **The instruction pointer runs off the end of the program.**
   The VM sets `done = true` and returns the top-of-stack value if
   present, else `0` (`vm.rs:93–96`).
3. **A runtime panic occurs.** The Rust panic propagates out of
   the interpreter loop; the host sees a panic in its handler
   call.

Cases (1) and (2) are clean exits. Case (3) is not catchable from
SOL; the host must protect itself with whatever panic-recovery
strategy it uses for any other Rust subroutine.

---

## 14.11 Sources cited in this chapter

- `vm.rs:7–11` — `HeapObject`
- `vm.rs:13–16` — `Frame`
- `vm.rs:18–48` — `VM` shape + construction
- `vm.rs:56–86` — execution loop + `call_entry`
- `vm.rs:88–116` — instruction dispatch (push const, store/load local)
- `vm.rs:143–186` — arithmetic / comparison / logic / bitwise
- `vm.rs:189–214` — struct ops
- `vm.rs:225–305` — array ops + control-flow ops + call/ret
- `vm.rs:309–336` — `print*`
- `vm.rs:339–476` — `Serialize*` / `Deserialize*` / `ExtCall`
- `bytecode.rs:115–164` — codegen entry + autocall of `start`
- `bytecode.rs:126–131, 494–507` — struct alphabetical layout
- `bytecode.rs:272–328` — `for-in` desugar
- `bytecode.rs:467–481` — call emission
- `error_runtime.sol` — division-by-zero fixture
