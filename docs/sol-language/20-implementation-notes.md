# 20 — Implementation Notes

> **Status:** Substantive (commit 7). Cross-checked against the
> full bodies of `bytecode.rs`, `vm.rs`, and `util.rs`. This
> chapter consolidates lower-level findings that span multiple
> language constructs and don't belong in any single chapter
> above. It is the place to look when "the docs say X but I'm
> seeing Y at runtime" and the discrepancy is rooted in how the
> bytecode emitter or VM happens to be wired.

The previous nineteen chapters are the *language reference*. This
chapter is the *current implementation's deep diagnostics*.
Everything here is dated and **may change**; verify against the
compiler source if the behavior matters.

---

## 20.1 The codegen pipeline in one paragraph

`init.rs:14–32` shows the full pipeline:

```text
source text  →  Lexer.tokens()        // lexer.rs
             →  Parser::run()         // parser.rs  →  Program (Vec<Ast>)
             →  Analyzer::run()       // analyzer.rs  (mutates Program, populates tt_arena)
             →  Codegen::gen_bcode()  // bytecode.rs  →  Vec<Inst>
             →  VM::from(bytecode)    // vm.rs
```

Each stage exits the process on the first error (`process::exit(1)`
or `panic!`) and emits a stderr line; recovery doesn't exist. The
audit lists structured error returns as blocker #2 in
`SOL_CRATE_IDE_READINESS_PLAN.md` §1.

---

## 20.2 Local-variable slots and `find_local_offset` quirk

The codegen tracks locals in `Codegen.locals: HashMap<String, (slot,
Type)>` and `next_slot: usize` (`bytecode.rs:62–67`). Slots are
indices into the value stack relative to the current function's
frame pointer. Each new `let` or parameter gets the next slot
and increments `next_slot`.

### Block-scope discipline

When the emitter enters a `Block`
(`bytecode.rs:211–232`), it saves `next_slot`, walks the
statements, then on exit:

```rust
self.locals.retain(|_, (slot, _)| *slot < saved_next);
self.next_slot = saved_next;
```

Locals declared inside the block are dropped from the map; the
stack rewinds for subsequent declarations. Note the **stack
itself does not shrink** at this point — the slots above
`saved_next` are still live on the value stack from earlier in
the chain. The cleanup happens implicitly when the function
returns and `Ret` truncates the stack to the saved `fp`
(`vm.rs:285`).

### `find_local_offset` auto-creates missing locals

The helper at `bytecode.rs:559–578` does something unexpected:
when it's asked for the offset of a name that isn't already in
the locals map, it walks every type table in
`Codegen.type_tables`, searches for a matching symbol, and
**creates a new local with the inferred type** plus increments
`next_slot`. The name is then registered in `locals`.

```rust
fn find_local_offset(&mut self, name: &str) -> isize {
    if let Some((slot, _)) = self.locals.get(name) { return *slot as isize; }
    let mut resolved_type = Type::Integer;
    for table in &self.type_tables {
        for (sym_name, sym) in table {
            if sym_name == name { /* found — set resolved_type */ }
        }
    }
    let slot = self.next_slot;
    self.locals.insert(name.to_string(), (slot, resolved_type));
    self.next_slot += 1;
    slot as isize
}
```

Consequences:

- A reference to a name that wasn't declared in the current
  function (e.g. a global `let` at the top level, or a stale
  binding) silently materializes as a fresh local at runtime.
- The "fresh local" doesn't hold the same value as the original
  binding — it gets whatever happens to be on the stack at that
  slot, which is undefined.
- The fallback resolved-type is `Type::Integer`. A `let` that
  reads a `str`-typed name from an outer scope therefore types as
  `int` and may produce nonsense at runtime.

This is a layered-on-top consequence of the analyzer not catching
the same shape (chapter 06 §6.3 explains the analyzer side).
Don't rely on it; treat any use of a name not visibly `let`-bound
in the current function as undefined behavior at the codegen
level.

---

## 20.3 The `is_expression_node` heuristic and implicit Pop

`bytecode.rs:166–177` lists every AST node the emitter considers
to produce a value:

```rust
fn is_expression_node(&self, node: &Ast) -> bool {
    matches!(node,
        Ast::ExprBinary { .. } | Ast::ExprUnary { .. } |
        Ast::ExprFuncCall { .. } | Ast::ExprMemAcc { .. } |
        Ast::ExprArrAcc { .. } | Ast::ExprInteger(_) |
        Ast::ExprFloat(_) | Ast::ExprString(_) |
        Ast::ExprChar(_) | Ast::ExprBool(_) |
        Ast::ExprVar(_) | Ast::ExprStructInit { .. } |
        Ast::ExprArrayInit { .. } | Ast::ExprEnumVar { .. } |
        Ast::ExprReturn { .. } | Ast::ExprUndefined
    )
}
```

At the top level of `gen_bcode` (`bytecode.rs:142–149`) and inside
`Block` (`bytecode.rs:218–223`), every statement classified as an
expression node gets an implicit `Inst::Pop` appended. This is
how `f();` works: the call leaves a return value on the stack, the
implicit Pop discards it.

### Three subtle consequences

1. **`Ast::ExprReturn` is classified as an expression node.** This
   is odd — `return` is a control-flow terminator, not a value.
   The `RetVal` op pops the return value and the call frame; if
   the surrounding code then tries to `Pop` again, it pops
   whatever was below. In practice the post-Ret instructions in
   the same chain are unreachable, so the spurious Pop is dead
   code and never executes.

2. **`Ast::ExprStructInit` is classified as an expression node.**
   So a bare `Point { x: 1, y: 2 };` statement at the body level
   constructs the struct *and immediately discards the heap
   reference*. The struct value is leaked on the heap (chapter
   14 §14.2). Don't construct structs as bare statements.

3. **`Ast::ExprArrayInit`** similarly — a bare `[1, 2, 3];` leaks
   the array.

These three cases are minor but worth knowing if you're reading
emitted bytecode and wondering where the extra Pop or leaked heap
entry came from.

---

## 20.4 Forward function calls — the `pending_calls` fixup

The emitter walks `Program` in source order. When it encounters
a call site for a function whose body hasn't been emitted yet, it
emits `Inst::Call(0, count)` with a placeholder target address
and records the index in `pending_calls` (`bytecode.rs:478–481`):

```rust
let inst_idx = insts.len();
insts.push(Inst::Call(0, count));
self.pending_calls.push((inst_idx, name));
```

After the whole program is emitted, a fixup loop
(`bytecode.rs:151–157`) walks `pending_calls` and patches each
placeholder:

```rust
for (inst_idx, name) in &self.pending_calls {
    if let Some(&addr) = self.functions.get(name) {
        if let Inst::Call(_, count) = insts[*inst_idx] {
            insts[*inst_idx] = Inst::Call(addr, count);
        }
    }
}
```

**If a call site references a name that's never registered as a
function**, the fixup loop's `if let Some(&addr)` simply doesn't
match — the placeholder `Inst::Call(0, count)` remains. At
runtime, `Inst::Call(0, count)` (`vm.rs:274–281`) pushes a frame
and sets `inst_ptr = 0`. The VM then begins re-executing the
program from instruction 0. **Effectively an infinite loop / re-
entry.** The analyzer should catch this earlier
(`analyzer.rs:376–379` rejects calls to undefined names), so this
shouldn't be reachable from valid source; document for tooling
that might bypass the analyzer.

---

## 20.5 The `string` quirk: unknown type names silently become nominal

`parser.rs:198–209` recognizes exactly `int, float, str, char,
bool` as primitive type names. Anything else in a type position
becomes `Type::Ident(name)` — a nominal struct/enum reference:

```rust
let ty = match ptype.as_str() {
    "int"   => Some(Type::Integer),
    "float" => Some(Type::Float),
    "str"   => Some(Type::String),
    "char"  => Some(Type::Char),
    "bool"  => Some(Type::Bool),
    _ => Some(Type::Ident(ptype))
};
```

The analyzer doesn't validate at the declaration site that the
named type exists; the validation happens only when something
walks into the type (chapter 09 §9.3 — field access requires the
LHS's struct name to resolve).

Fixture evidence: `largemini.sol` declares
`struct Person { name: string, age: int }` — `string` here is
*not* the primitive `str`; it's `Type::Ident("string")`, a
non-existent struct reference. The program runs because the
bytecode emitter doesn't type-check field values when building
struct literals (`bytecode.rs:494–507`), only positionally fills
the layout. Reading `p.name` then succeeds because `Person`
resolves, the `name` field resolves, and the recorded result type
is `Type::Ident("string")`. The value is a heap-string reference.
`print(p.name)` works because `print` dispatches by argument type
at codegen — `display_type` for an `ExprMemAcc` falls through to
`Type::Integer` (the catch-all at `bytecode.rs:630`), so
**`print` uses `Inst::PrintInt` to print a heap-string-index as a
number**. The fixture's expected output therefore prints heap
indices, not strings.

Don't write `string`. Use `str`. Logged as T9009.

---

## 20.6 `print` dispatch — the `display_type` helper

`bytecode.rs:634–654` has the `display_type` helper that picks
which `Print*` op to emit:

```rust
fn display_type(&self, node: &Ast) -> Type {
    match node {
        Ast::ExprBinary { op, .. } => match op.get_kind() {
            EqEq | BangEq | MoreThan | LessThan | MoreEq | LessEq
            | PipePipe | AmpAmp => Type::Integer,
            _ => self.infer_type(node),
        }
        Ast::ExprUnary { op, .. } => match op.get_kind() {
            Bang => Type::Integer,
            _ => self.infer_type(node),
        }
        _ => self.infer_type(node),
    }
}
```

The special-case here: any comparison or logical-op result is
displayed as `Integer` — so `print(5 == 5)` prints `1`, not
`true`. This matches the bytecode (`bool` is 0/1) but is worth
knowing for output formatting.

For other expressions the helper delegates to `infer_type`. Per
§20.5 above, `infer_type` falls back to `Type::Integer` for any
node it doesn't recognize — including `ExprMemAcc` against a
mis-typed struct, `ExprArrAcc` whose array's element type can't
be inferred, and any binary-op result whose inferred operand
type isn't `Float` or another recognized case.

---

## 20.7 Struct field layout — alphabetical, fixed at the decl

The emitter's first pass (`bytecode.rs:123–139`) records the
layout of every struct as an alphabetically-sorted
`Vec<(String, Type)>`:

```rust
sorted_fields.sort_by(|a, b| a.0.cmp(&b.0));
self.struct_layouts.insert(name.clone(), sorted_fields);
```

Subsequent emission uses the same sorted layout. `ExprStructInit`
walks the layout *in order*, looks up the matching field name in
the literal, and pushes either the literal's value or
`Inst::PushConst(ExprUndefined)` if missing
(`bytecode.rs:494–507`).

Three properties hold:

1. **Source-declaration order does not affect runtime behavior.**
   `struct Point { y: int, x: int }` and `struct Point { x: int,
   y: int }` produce identical bytecode and identical runtime
   values.
2. **Missing fields in a literal become `0` at runtime.** The
   `ExprUndefined` constant materializes as `0` in `vm.rs:108`.
3. **Field access (`p.x`) uses the layout's alphabetical
   position.** `bytecode.rs:512–520` looks up the field name in
   `struct_layouts` and emits `Inst::GetField(pos)`.

Combined: as long as the emitter and the access code are the
same compile, alphabetical order is consistent. Any tool that
constructs a struct value out-of-band (e.g. by emitting bytecode
directly) must use the same sort.

---

## 20.8 Array construction — the `Dup; Push i; <value>; SetElem; Pop` loop

`bytecode.rs:522–532` shows the pattern for an N-element array
literal:

```rust
insts.push(Inst::PushConst(Ast::ExprInteger(values.len() as i128)));
insts.push(Inst::NewArray);
for (i, val) in values.into_iter().enumerate() {
    insts.push(Inst::Dup);
    insts.push(Inst::PushConst(Ast::ExprInteger(i as i128)));
    self.compile(insts, val);
    insts.push(Inst::SetElem);
    insts.push(Inst::Pop);
}
```

`NewArray` pushes the heap index of an empty Vec. For each
element: `Dup` the heap index, push the element index, compile
the value, `SetElem` stores it (and pushes the value), `Pop`
discards the pushed value. The final stack value is the heap
index of the array.

The `SetElem` op pushes the stored value (`vm.rs:242`), so the
explicit `Pop` is necessary to keep stack discipline. This is
the pattern to mirror if you're writing an array-construction
helper that bypasses the AST.

---

## 20.9 The `for-in` desugar

`bytecode.rs:272–328` desugars `for elem in arr { body }` into
the equivalent of:

```text
__for_arr_N    = arr                       // store the array
__for_len_N    = ArrayLen(__for_arr_N)     // store the length
__for_idx_N    = 0                          // counter

loop_start:
  if __for_idx_N >= __for_len_N goto loop_end
  elem = __for_arr_N[__for_idx_N]
  body
  __for_idx_N = __for_idx_N + 1
  goto loop_start

loop_end:
```

Three implementation notes:

1. **The `__for_arr_N` / `__for_idx_N` / `__for_len_N` slots are
   per-loop**, with `N` from a monotonically-increasing
   `for_loop_counter`. Nested loops therefore don't collide.
2. **The iteration variable is bound *outside* the loop body's
   block** in the codegen (the `get_or_create_local(&elem_name,
   ...)` call happens before the body is compiled). After the
   for-statement exits, the iteration variable's slot is reaped
   by the block-scope cleanup (§20.2), but the *name* may still
   resolve to a slot during compilation — explaining the
   "iteration variable leaks into enclosing scope" behavior at
   the analyzer level (chapter 06 §6.5).
3. **`Inst::ArrayLen` is reachable only via this desugar**.
   There is no surface syntax that emits it directly (chapter 11
   §11.7).

---

## 20.10 Function call frames — `fp` arithmetic

Two key invariants:

1. **Arguments occupy the lowest slots of the new frame.** `Call`
   computes `fp = stack.len() - arg_count` (`vm.rs:279`), so arg
   0 is at `fp + 0`, arg 1 at `fp + 1`, etc. The `StoreLocal` /
   `LoadLocal` ops use offsets relative to `fp`.
2. **Locals grow the stack from `fp` upward.** When the body
   declares a `let`, the codegen emits a `StoreLocal(offset)`
   that uses `next_slot` (which is the next free slot above the
   parameters); the value sits at `fp + offset`.

On return:

- `Inst::Ret` (`vm.rs:283–293`): truncates the stack to `fp`
  (discarding all locals and intermediate values), restores the
  caller's `fp`, jumps to `return_ptr`, and pushes `0`.
- `Inst::RetVal` (`vm.rs:295–306`): pops the return value first,
  then truncates, restores, and pushes the return value.

This is why even a "void" function leaves something on the
caller's stack — see T9011 in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 20.11 RPC serialization layout

The on-the-wire shapes the runtime emits and parses (`vm.rs:339–476`):

### `rpc_request(name, args)` produces

```json
{ "type": "request", "name": "<name>", "args": [ … encoded args … ] }
```

Arg encoding by element type:
- `str` → JSON string
- `int` → JSON number (i64)
- `float` → JSON number (`from_f64`, or `0.0` for un-encodable)
- `bool` → JSON bool
- `char` → JSON string of one character
- anything else → Debug-formatted string fallback

### `rpc_response(value)` produces

```json
{ "type": "response", "data": <encoded-value> }
```

`data` is encoded per the value's declared type with the same
rules as `rpc_request`, except `str` arguments are passed through
`serde_json::from_str` and re-emitted (so a JSON-shaped string
becomes parsed JSON, not a string literal).

### `rpc_name(msg)` / `rpc_args(msg)` / `rpc_data(msg)` consume

JSON shaped like the above. `rpc_name` reads `msg["name"]`;
`rpc_args` reads `msg["args"]` (returned as a JSON string for
later parsing); `rpc_data` reads `msg["data"]` (also stringified).

A malformed message panics — `serde_json::from_str` returns
`Result`, and the runtime calls `.expect("Runtime Error: failed
to parse JSON in <op>")`. There is no recoverable error path.

### `Inst::ExtCall` request

Built identically to `rpc_request` (`vm.rs:509–514`). The
runtime POSTs the JSON body to the configured endpoint URL and
reads back JSON shaped like `rpc_response`. Return-type
coercion follows the table in chapter 12 §12.4.

---

## 20.12 What this implementation lacks

A consolidated checklist of *missing* capabilities, distinct
from documented bugs. Each item is worth knowing because a
future SOL feature might land in this slot.

| Capability | State | Where it would live |
|---|---|---|
| Pattern matching (`match expr { … }`) | Absent (no `match` keyword) | New keyword + parser production + analyzer + bytecode dispatch |
| `break` / `continue` | Absent (no keywords; the analyzer has a dead `can_break` flag) | Lexer + parser + bytecode loop-context tracking |
| First-class functions / closures | Absent | `Type::Function` is internal-only; no value form |
| String interpolation | Absent | New literal form |
| String escape sequences | Absent | Lexer string-literal path |
| Numeric digit separators | Absent (`_` is trivia) | Lexer |
| Hex / binary / octal integer literals | Absent | Lexer |
| `%` (modulo) operator | Absent | Lexer + parser precedence chain |
| Ternary `?:` | Absent | New precedence level |
| Range expressions (`0..N`) | Absent | New operator token + production |
| Nullable / optional types | Absent | New `Type::Optional` + analyzer handling |
| `try` / `catch` / `Result`-style error handling | Absent | Substantial language addition |
| Module system | Absent (`import` is parsed but inert) | Multi-file compilation pipeline |
| Generics / parametric polymorphism | Absent | Type system extension |
| Trait / interface dispatch | Absent | Type system extension |
| Tail-call optimization | Absent | VM optimization |
| Garbage collection | Absent (heap grows monotonically) | Runtime |
| Bounds-checked array access (explicit, with recoverable error) | Absent (panics) | Runtime + new diagnostic surface |
| Source spans on diagnostics | Absent | Audit blocker #3 |
| Multiple errors per compile | Absent (exits on first) | Audit blocker #2 |
| HTTPS for `ExtCall` | Absent (HTTP only) | Runtime |
| `ExtCall` timeouts | Absent | Runtime |

This list is the "future roadmap" expressed as gaps; consult the
audit (`reference/SOL_CRATE_IDE_READINESS_PLAN.md`) for the
prioritization.

---

## 20.13 Sources cited in this chapter

- `init.rs:14–32` — pipeline composition
- `bytecode.rs:62–67, 559–578` — `Codegen` locals table and the
  `find_local_offset` quirk
- `bytecode.rs:123–139, 494–520` — struct layout and field op
  emission
- `bytecode.rs:142–177, 218–223` — `is_expression_node`
  classification and the implicit `Pop`
- `bytecode.rs:151–157, 478–481` — `pending_calls` fixup
- `bytecode.rs:272–328` — `for-in` desugar
- `bytecode.rs:423–453, 634–654` — `print` dispatch and
  `display_type`
- `bytecode.rs:522–532` — array literal codegen
- `vm.rs:118–131, 274–306` — call frame mechanics
- `vm.rs:189–214, 220–243, 245–261` — struct/array/string runtime
  ops (silent-no-op behavior — T9010)
- `vm.rs:283–293` — `Ret` push-0 behavior (T9011)
- `vm.rs:339–476` — RPC serialization layout
- `vm.rs:469–579` — `ExtCall` transport (T9012)
- `util.rs:1–42` — `type_eq` helper (T9006 / T9007 / T9008)
- `parser.rs:198–209` — primitive type recognition (T9009)
- Fixture: `largemini.sol` (uses `string` as a type name — §20.5)
