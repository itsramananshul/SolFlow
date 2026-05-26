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

## 20.13 Function declaration emit — inline with `Jump`-over

`bytecode.rs:393–422` shows the per-function code emission
pattern. Functions are emitted *inline* in the main instruction
stream, with an `Inst::Jump(...)` placeholder that the codegen
patches to skip past the function body during normal top-level
execution.

```text
<top-level code so far>
Jump(end_of_foo)         // placeholder; patched after body emit
foo_entry:                // recorded in self.functions["foo"]
  <foo body>
  Ret
end_of_foo:
<more top-level code>
```

Calls into `foo` use `Inst::Call(foo_entry, n)`, jumping into
the inlined body. After the body's `Ret`, the next-instruction
pointer points at whatever comes after the `Ret` — which is the
patched `Jump` target (end_of_foo). Execution flows back through
the caller normally.

Two consequences worth knowing:

1. **Per-function locals reset.** Each `DeclFunc` emits
   `self.locals.clear(); self.next_slot = 0;` at the start
   (`bytecode.rs:401–402`). This is what makes top-level `let`
   broken (§20.14 below) — the per-function reset throws away
   the codegen's record of any top-level binding.
2. **`active_scopes` is dead infrastructure.** The
   `push(scope_from(scope_id))` / `pop()` calls at
   `bytecode.rs:409–411, 416–418` populate a `Vec<Scope>` that
   nothing else in the codegen reads. The field exists, gets
   maintained, and serves no purpose.

---

## 20.14 Top-level `let` — bytecode-level walk

The combination of three implementation details makes top-level
`let` unsafe to use. None of the three is individually surprising;
the interaction is.

1. **Top-level code emits at `fp = 0`.** Before the implicit
   `Call(start_addr, 0)` runs (`bytecode.rs:159–161`), there is
   no frame on the call stack, the VM's `fp` is `0`, and any
   top-level `StoreLocal(0)` writes to `stack[0]`.
2. **The implicit `Call` sets `fp = stack.len() - arg_count`.**
   For `start` with no arguments, `arg_count = 0`, so `fp` becomes
   the *current top of the stack* — past any top-level state.
3. **The codegen's `find_local_offset` auto-creates a fresh local
   when a name isn't already registered in `self.locals`**
   (`bytecode.rs:559–578`). When a function body reads a
   top-level name, the codegen registers it as a *new* local at
   the function's `next_slot = 0`, and emits `LoadLocal(0)`. At
   runtime that's `stack[fp + 0]` — past the top-level data.

Concrete walk for `let g: int = 42; function start() -> int { return g; }`:

```text
Codegen emit (in order):
  PushConst(42)          // top-level let g
  StoreLocal(0)          // codegen.locals = {"g": (0, Integer)}; next_slot = 1
  Jump(end_of_start)     // placeholder
  start_entry:
    // DeclFunc reset: locals.clear(); next_slot = 0
    // body emit:
    LoadLocal(0)         // find_local_offset("g") not in locals →
                         //   walk type_tables → found Integer → register at slot 0
    RetVal
    Ret                  // implicit
  end_of_start:
  Call(start_entry, 0)   // appended

VM execution:
  fp = 0, stack = []
  → PushConst(42): stack = [42]
  → StoreLocal(0): pops 42, writes to stack[0]. stack = [42], fp = 0.
  → Jump to Call(start_entry, 0).
  → Call: push frame {return_ptr=end, old_fp=0}; fp = stack.len() - 0 = 1; goto start_entry.
  → LoadLocal(0): idx = fp + 0 = 1. stack.len() == 1 → PANIC: index out of bounds.
```

If the stack happens to have additional values pushed before the
`LoadLocal` runs (e.g. from intervening expression evaluation),
the read may return garbage instead of panicking — which is worse,
because it silently corrupts subsequent computation.

Logged as **T9014** in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md). Recommendation:
don't write top-level `let`s.

---

## 20.15 Forward function calls and `display_type` falls back to `Integer`

The codegen's `fn_returns` map is populated piecewise:

- Built-in RPC return types are inserted at the start of
  `gen_bcode` (`bytecode.rs:117–121`).
- `DeclExtFunc` return types are inserted in pass 1
  (`bytecode.rs:133–136`).
- *Regular* `DeclFunc` return types are inserted in pass 2 *at
  the moment the function decl is emitted*
  (`bytecode.rs:397–399`).

Pass 2 walks the program in source order. So when a call site
emits *before* its target function's decl is emitted, the
codegen's `fn_returns.get(name)` returns `None`. The
`infer_type` helper used by `display_type` (`bytecode.rs:627–629`)
silently falls back to `Type::Integer`:

```rust
Ast::ExprFuncCall { name, .. } => {
    self.fn_returns.get(name).cloned().unwrap_or(Type::Integer)
}
```

Practical impact: `print(forward_call())` where `forward_call`
returns `str` dispatches via `Inst::PrintInt` instead of
`Inst::PrintString`. The heap index of the string gets printed
as a decimal number rather than its content.

Logged as **T9015**. Defense: declare each function *before*
its first call site, or avoid using forward-called functions
inside `print` where the return type isn't `int`.

---

## 20.16 Built-in name dispatch precedence

`bytecode.rs:423–481` shows the call-site dispatch order:

1. If the name is exactly `"print"` and `args` is non-empty →
   `print` dispatch.
2. Otherwise if the name is exactly `"rpc_request"` →
   `SerializeRequest`.
3. Otherwise if the name is exactly `"rpc_response"` →
   `SerializeResponse`.
4. Otherwise if the name is exactly `"rpc_name"` →
   `DeserializeRequestName`.
5. Otherwise if the name is exactly `"rpc_args"` →
   `DeserializeRequestArgs`.
6. Otherwise if the name is exactly `"rpc_data"` →
   `DeserializeResponseData`.
7. Otherwise if `self.ext_functions.contains(name)` → `ExtCall`.
8. Otherwise if `self.functions.get(name)` resolves → local
   `Call(addr, n)`.
9. Otherwise → placeholder `Call(0, n)` + `pending_calls.push`.

The first six checks are string-equality against built-in names
and **happen before the `ext_functions` and local-function
checks**. A user-declared `ext function rpc_request(...) -> ...;`
is silently shadowed; the bytecode emitter sees the call name as
the built-in and emits `Inst::SerializeRequest`, ignoring the
user's host endpoint binding.

Logged as **T9016**. Defense: don't reuse any of `print`,
`rpc_request`, `rpc_response`, `rpc_name`, `rpc_args`, `rpc_data`
as an `ext function` or local function name.

---

## 20.17 CLI parser edge cases

The standalone SOL compiler's CLI (`cli.rs`) supports two flag
forms:

- `--long-flag` — toggled true if mentioned (e.g. `--debug-tokens`,
  `--debug-ast`)
- `-short-option value` — takes a value as the next argument

The implementation has two latent panics worth knowing:

- **Empty argv element.** `arg.chars().nth(0).unwrap()`
  (`cli.rs:20`) panics on `""`.
- **Single-character argv element.** `arg.chars().nth(1).unwrap()`
  (`cli.rs:21`) panics on a bare `-`.

The OS rarely passes empty entries; a bare `-` is uncommon as a
SOL CLI argument. The panics are unlikely in practice but worth
noting for tools that wrap the binary. Logged as **T9017**.

The known debug flags the binary recognizes
(`src/sol/main.rs:18, 24`):

- `--debug-tokens` — dump the lexer's token stream to stderr.
- `--debug-ast` — dump the parser's `Program` (Vec<Ast>) to
  stderr.

The binary's exit code is the VM's `run()` return value: zero
means success, non-zero is treated as test failure
(`src/sol/main.rs:37–40`). The compiler exits `1` if the program
returns non-zero. So the conventional `return 0;` at the end of
`start` matters: it's the success exit code.

---

## 20.18 VM assumptions about emitter correctness

The VM is a deliberately thin interpreter. Many of its handlers
assume the bytecode was emitted correctly and do not defend
against malformed sequences. Worth knowing because a future tool
that emits bytecode directly (bypassing the codegen) must honor
these assumptions or trigger silent corruption.

| Assumption | Where | What happens if violated |
|---|---|---|
| `pop` is called with at least one value on the stack | `vm.rs:77` | `Runtime Error: Stack underflow` panic |
| `LoadLocal(offset)` / `StoreLocal(offset)` reference valid stack positions | `vm.rs:118–131` | LoadLocal panics on out-of-bounds; StoreLocal grows the stack with `0`-fill (so writes silently succeed for any offset, however large) |
| Heap indices point at the expected `HeapObject` variant | `vm.rs:189–214, 220–243, 245–261` | The `if let HeapObject::X = ...` silently no-ops — see T9010 |
| `Call(addr, n)`'s `addr` points at a function body's first instruction | `vm.rs:274–281` | The VM jumps; if `addr` doesn't point at function code, the next op runs as bytecode wherever the data happens to look like an op |
| `Jump(target)` / `JumpFalse(target)` point inside the program | `vm.rs:264–272` | `inst_ptr` is set; the next `step()` reads `program[inst_ptr]`, which `panic!`s if out of bounds |
| `Ret` is reached exactly once per frame | `vm.rs:283–293` | Otherwise the call stack and value stack diverge; subsequent `Ret`s read the wrong frame |
| `RetVal` is preceded by exactly one pushed return value | `vm.rs:295–306` | Otherwise the wrong value is returned to the caller |
| `print` ops' top-of-stack value matches the declared type | `vm.rs:309–336` | `PrintChar` constructs a char from arbitrary bits and may produce `?`; `PrintString` indexes into the heap and could panic or silently print whatever object is at that index |

These are not bugs; they are the trade-off the VM makes for
simplicity. A future "verified bytecode" mode would add type
tags to stack slots and a verifier pass between codegen and
execution.

---

## 20.19 Compile-time and runtime panic paths summary

Consolidated list of every site that calls `process::exit(1)`
or `panic!`/`unreachable!()` in the compiler crate. A future
diagnostic-refactor would convert each of these into a structured
error result.

### Compile-time (exit before running any code)

- `lexer.rs:298` — unrecognized character
- `parser.rs:152–154, 191–192, 215, 245, 254, 263, 292, 301, 387, 446, 454, 464, 491, 501, 522, 532, 540, 613, 703, 741, 748` — every parse-error site (`eat()`, `panic!`, and explicit `eprintln + exit`)
- `analyzer.rs:51` — duplicate symbol
- `analyzer.rs:175, 191, 203, 207` — type-check errors for control flow
- `analyzer.rs:222, 229, 236, 249, 256, 267, 276, 285, 293, 311, 319, 327, 334, 348, 353, 360, 368, 377, 385, 392, 400, 412, 418, 422, 428, 439, 444, 449, 470, 486, 493` — type-check errors for expressions
- `analyzer.rs:460, 465` — array index / index-on-non-array
- `bytecode.rs:363, 458` — invalid assignment LHS; missing ext endpoint
- `bytecode.rs:113` — invalid constant AST node passed to `PushConst`

### Runtime (panic during VM execution)

- `vm.rs:77` — stack underflow
- `vm.rs:113` — invalid constant AST node (defensive; should be unreachable for correctly-emitted bytecode)
- `vm.rs:138` — `Dup` on empty stack
- `vm.rs:146` — integer division by zero (via Rust `i64` arithmetic)
- `vm.rs:345, 350, 358` — `rpc_request` runtime type mismatches
- `vm.rs:394` — `rpc_response` expected a string
- `vm.rs:423–430, 440–448, 455–465` — `rpc_*` JSON parse / field-missing
- `vm.rs:475, 479, 491, 524, 544` — `ExtCall` URL/name type, connect, JSON parse failures
- Implicit `Vec` out-of-bounds — array index, struct field, heap index — anywhere the VM does `vec[i]` without explicit bounds check

The compile-time list is long. The runtime list is short and
mostly defensive. Both lists are queued for refactor in
`SOL_CRATE_IDE_READINESS_PLAN.md` §1 blocker #2 (errors-as-values).

---

## 20.20 Sources cited in this chapter

- `init.rs:14–32` — pipeline composition
- `bytecode.rs:62–67, 559–578` — `Codegen` locals table and the
  `find_local_offset` quirk
- `bytecode.rs:117–139` — pre-registration of RPC builtins, ext
  functions, struct layouts
- `bytecode.rs:142–177, 218–223` — `is_expression_node`
  classification and the implicit `Pop`
- `bytecode.rs:151–157, 478–481` — `pending_calls` fixup
- `bytecode.rs:272–328` — `for-in` desugar
- `bytecode.rs:393–422` — function declaration emission with
  `Jump`-over (§20.13)
- `bytecode.rs:401–402` — per-function reset of locals/next_slot
- `bytecode.rs:423–481` — call-site dispatch precedence (§20.16)
- `bytecode.rs:423–453, 634–654` — `print` dispatch and
  `display_type`
- `bytecode.rs:494–520` — struct construction / field load/store
- `bytecode.rs:522–532` — array literal codegen
- `bytecode.rs:627–629` — `infer_type` fallback to `Integer`
  (§20.15)
- `vm.rs:118–131, 274–306` — call frame mechanics
- `vm.rs:189–214, 220–243, 245–261` — struct/array/string runtime
  ops (silent-no-op behavior — T9010)
- `vm.rs:283–293` — `Ret` push-0 behavior (T9011)
- `vm.rs:339–476` — RPC serialization layout
- `vm.rs:469–579` — `ExtCall` transport (T9012)
- `util.rs:1–42` — `type_eq` helper (T9006 / T9007 / T9008)
- `cli.rs:19–22` — CLI argv panics (T9017)
- `src/sol/main.rs:12–44` — compiler binary entry point + debug
  flags
- `parser.rs:198–209` — primitive type recognition (T9009)
- Fixtures: `largemini.sol` (uses `string` as a type name —
  §20.5; asserts content-equality of strings via `eqString`)
