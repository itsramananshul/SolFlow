# 22 — Cross-Layer Assumptions

> **Status:** Substantive (commit 10). Each layer of the SOL
> toolchain makes assumptions about the layer below it. When an
> assumption is violated — typically by a tool that bypasses the
> normal pipeline — the result is silent corruption, undefined
> behavior, or a deep stack panic that surfaces in code that had
> nothing to do with the original violation.
>
> This chapter catalogues every such assumption that is not
> defended against by explicit runtime checks. It is the reference
> a future "verified pipeline" or "bytecode verifier" would build
> against, and the safety guide for anyone writing a tool that
> sits anywhere in the pipeline.

The toolchain has six layers:

```
Source text
   ↓ (lexer)
Token stream
   ↓ (parser)
AST (Program = Vec<Ast>)
   ↓ (analyzer, pass 1 + pass 2)
AST + populated TypeTable arena (tt_arena)
   ↓ (bytecode emitter)
Instruction stream (Vec<Inst>) + function table
   ↓ (VM)
Observable behavior (stdout, host responses, exit code)
```

Plus three editor-side layers:

```
SolFlow graph (SolWorkflow)
   ↓ (validate.ts)
"Valid" graph
   ↓ (emit.ts)
SOL source text  →  feeds into the source pipeline above

[Sol Man spec]
   ↓ (applyGraph.ts: repairSpec + specToWorkflow)
SolFlow graph  →  feeds into the validator
```

This chapter lists what each producer guarantees and what each
consumer assumes. Mismatches are marked.

---

## 22.1 Lexer → Parser

### What the lexer guarantees

| Guarantee | Confidence | Source |
|---|---|---|
| Output is a `Vec<Token>` (possibly empty) | **Specified** | `lexer.rs:206–208` |
| Each token's variant matches its `TokenKind` (the `get_kind()` map is total) | **Current-impl** | `lexer.rs:122–183` — case-for-case match |
| `Integer(i128)` holds a parsed integer, possibly value `0` on parse failure | **Current-impl** | `lexer.rs:380–386` (`unwrap_or(0)`) |
| `Float(f64)` holds a parsed float, possibly value `0.0` on parse failure | **Current-impl** | `lexer.rs:380–386` (`unwrap_or(0.0)`) |
| `String(s)` holds the raw characters between the delimiting `"` — no escape interpretation | **Specified** | `lexer.rs:224–233` |
| `Char(c)` holds the *next* source character after `'`; the closing `'` is not validated | **Current-impl** | `lexer.rs:218–223` |
| `Ident(s)` holds an `is_alphabetic()`-started, alphanumeric+underscore identifier | **Specified** | `lexer.rs:333–339` |
| Keywords are returned as their dedicated `Token` variants, not as `Ident` | **Specified** | `lexer.rs:341–356` |
| Two-char operators are produced maximally (`==`, not `=` + `=`) | **Specified** | `lexer.rs:238–296` |
| The lexer exits the process if it sees an unrecognized character | **Current-impl** | `lexer.rs:298` |

### What the parser assumes

| Assumption | Defended? | What breaks if violated |
|---|---|---|
| `tokens` is non-empty at the start | No | `parser.rs:181` reads `self.tokens[self.index]` directly; an empty vec passes the index check but indexing panics |
| Tokens between expressions are syntactically reachable from a `primary()` entry | Partial — `primary()` has a catch-all (`parser.rs:740–744`) | Catch-all prints and exits; downstream code stops |
| `Integer(_)` and `Float(_)` token payloads are usable as-is | Yes | Parser just stores into AST nodes; no semantic check at parse |
| `Token::Eq` only ever appears in assignment / `let` initializer positions | No | Could appear anywhere; the precedence chain handles it at level 1 only |
| Identifiers in type position match primitive names exactly (`int`, `float`, etc.) | No | Anything else becomes `Type::Ident(name)` — T9009 |

**Cross-layer concern:** The parser is reasonably defensive
against malformed token streams (via `eat()` checking
`TokenKind`), but the **defense is via `process::exit(1)`**, not
via structured errors. Any future tool that consumes the lexer
in-process without expecting `exit(1)` to be a control-flow
operator must wrap the parser call in a panic boundary.

---

## 22.2 Parser → Analyzer

### What the parser guarantees

| Guarantee | Confidence | Source |
|---|---|---|
| Output is a `Program` (= `Vec<Ast>`) | **Specified** | `parser.rs:26, 177–179` |
| Every `Ast::DeclFunc.body` is `Box<Ast::Block { … }>` | **Specified** | `parser.rs:322, 347–360` |
| Every `Ast::Block.scope` is `usize::MAX` (placeholder) | **Current-impl** | `parser.rs:356` |
| Every `Ast::DeclFunc.scope` is `usize::MAX` (placeholder) | **Current-impl** | `parser.rs:324` |
| Every `Ast::ExprBinary.op` is one of the operator tokens permitted at the relevant precedence level | **Current-impl** | `parser.rs:560–583` |
| Every `Ast::DeclStruct.fields` is a populated `HashMap<String, Type>` (possibly empty) | **Specified** | `parser.rs:498–516` |
| Top-level declarations are exactly one of: `DeclFunc`, `DeclExtFunc`, `DeclVar`, `DeclStruct`, `DeclEnum`, `StmtImport` | **Specified** | `parser.rs:180–194` |
| The AST contains no `Ast::ExprUndefined` (this variant exists for VM use only) | **Current-impl** | No parser path emits it |

### What the analyzer assumes

| Assumption | Defended? | What breaks if violated |
|---|---|---|
| `scope: usize::MAX` will be replaced by the analyzer via `*scope = self.new_table()` | No — the analyzer mutates the AST in place | A second walk over the same AST sees the previously-assigned scope and reuses the wrong table |
| `ExprBinary.op` is a known operator token | Partial — explicit match arms cover the recognized ops, with a fallthrough `_ => panic!` (`analyzer.rs:300–302`) | Panic with `unsupported binary operator: <op>` |
| Function bodies always reach the analyzer with `Block` at the top (not a bare statement) | Yes — parser guarantees this | n/a |
| `DeclVar.value` is `Option<Box<Ast>>` and **may be ignored** | Yes — the analyzer ignores it (`analyzer.rs:138–141`) | Hides analyzer-side type checking; documented as a known hole (chapter 06 §6.1) |
| Forward references between top-level functions resolve via pass-1 registration | Yes — by design | n/a |

**Cross-layer concern:** The analyzer **mutates** the AST in
place (writing to `Block.scope`, `DeclFunc.scope`). Tools that
pass the same AST through two analyzer runs will see stale
scope IDs. Any caller that wants to re-analyze must clone the
program first.

**Subtle:** the parser's `usize::MAX` sentinel for unanalyzed
scopes is a numeric value, not an `Option`. The bytecode emitter
later checks `if scope_id < self.type_tables.len()` to gate
scope use — but this means a *bug* in the analyzer that left a
scope as `usize::MAX` is silently tolerated by the emitter (no
scope is opened; no symbols are visible). The analyzer's
correctness is load-bearing.

---

## 22.3 Analyzer → Bytecode emitter

### What the analyzer guarantees

| Guarantee | Confidence | Source |
|---|---|---|
| `tt_arena: Vec<TypeTable>` is populated with at least the globals (function signatures) | **Specified** | `analyzer.rs:67, 80–98` |
| Every `Ast::Block.scope` and `Ast::DeclFunc.scope` is a valid index into `tt_arena` (not `usize::MAX`) | **Specified** | `analyzer.rs:113, 152` |
| Names referenced in `ExprVar`, `ExprFuncCall`, `ExprMemAcc`, `ExprEnumVar`, `ExprArrAcc` resolve in scope | **Current-impl** — enforced by `get_entry().unwrap_or_else(exit)` | Without this, the codegen would emit `LoadLocal(0)` for an unresolved name — see T9014 mechanism |
| Operator types match at every `ExprBinary` (except `Token::Eq` whose LHS may be a varname/field/index) | **Specified** | `analyzer.rs:241–303` |
| Argument count and types match function signatures at every `ExprFuncCall` | **Specified** | `analyzer.rs:391–404` |
| `ExprStructInit` and `ExprArrayInit` reach the codegen **unchecked** (analyzer's `todo!()` fallthrough) | **Current-impl** | `analyzer.rs:499–500` |
| `let`-initializer types and function return-path correctness reach the codegen **unchecked** | **Current-impl** | `analyzer.rs:138–141` (let), `analyzer.rs:120–132` (return — commented out) |

### What the bytecode emitter assumes

| Assumption | Defended? | What breaks if violated |
|---|---|---|
| Every name in `ExprFuncCall` resolves to either a built-in, an `ext function`, or a `function` | Partial — falls back to `pending_calls` for unresolved names | If the name is never registered, the pending-call patch loop doesn't match; the placeholder `Inst::Call(0, count)` stays in the bytecode and the VM re-enters from instruction 0 (T9014-adjacent infinite loop) |
| Every `ExprVar` name is in either `locals` or `type_tables` | Yes — falls through `find_local_offset` to auto-create a slot defaulting to `Type::Integer` | T9014 (top-level let) is the prototype of "this silently does the wrong thing" |
| Every `ExprMemAcc.lhs` resolves to a `Type::Ident(struct_name)` whose layout is in `struct_layouts` | No | If lhs type is something else, `field_idx` defaults to 0, and the emit is `GetField(0)` — reads the first field's slot, semantically wrong |
| Every `ExprBinary` operands' types are equal and known | No | `emit_binary_op` matches on the inferred type via `infer_type`; unknown ops fall through to `_ => {}` (silent no-op — `bytecode.rs:670`) and emit *nothing*. The stack discipline downstream is then broken |
| `print`, `rpc_*` arg counts match the analyzer's checks | No — but the analyzer already enforces them | A bypassed analyzer + emitter would emit malformed `Inst::SerializeRequest` etc. |
| `Codegen.fn_returns` has every regular function's return type by the time it's needed for `display_type` / `infer_type` | **No** — see T9015 | Returns `Integer` by default; affects `print` dispatch and ext-call arg-type inference |
| Struct layouts (`struct_layouts`) are populated before any struct literal or field access emits | Yes — pre-pass | n/a |

**Cross-layer concern:** Several emit-time fallbacks
(`unwrap_or(Type::Integer)`, `unwrap_or(0)` for field index,
silent no-ops for unknown operators) mask analyzer bugs as
runtime symptoms in unrelated code. Any improvement to the
analyzer's coverage should be paired with stricter emitter
assertions to surface analyzer regressions directly.

**Subtle:** the emitter's `find_local_offset` looks up unknown
names in `type_tables` (linear scan) and creates a fresh slot.
This is **deliberately permissive** — it lets the emitter run
even when the analyzer has been bypassed — but it also means
the emitter cannot distinguish "analyzer ran but missed this
name" from "name doesn't exist". A stricter mode would refuse
to emit for unresolved names.

---

## 22.4 Bytecode emitter → VM

### What the emitter guarantees

| Guarantee | Confidence | Source |
|---|---|---|
| Output is a `Vec<Inst>` | **Specified** | `bytecode.rs:115–164` |
| `Inst::Jump(target)` and `Inst::JumpFalse(target)` target indices are < `insts.len()` | **Current-impl** | All emitted jumps point into the same function or program-end (no jumps off the end) |
| `Inst::Call(target, n)`'s `target` points at the first instruction of a function body (after the `Jump`-over) | **Current-impl** | `bytecode.rs:397–398, 467–472` |
| `Inst::Ret` / `Inst::RetVal` are reached exactly once per `Call` frame | **Current-impl** | Implicit `Ret` epilogue at every function's end guarantees this for well-emitted code |
| Stack discipline: each emitted instruction pushes / pops exactly what the per-op contract specifies | **Current-impl** | The emitter tracks this implicitly through statement-vs-expression classification |
| `Inst::NewStruct(n)` is preceded by exactly `n` value pushes | **Current-impl** | `bytecode.rs:494–507` walks the struct layout |
| `Inst::ExtCall(types, ret)`'s stack has args, name, URL in that order | **Current-impl** | `bytecode.rs:461–466` |
| `Inst::GetField(idx)` / `Inst::SetField(idx)` use the alphabetical-layout position | **Current-impl** | `bytecode.rs:126–131, 508–520` |
| `Inst::PushConst(ast)` only passes constant-AST variants the VM recognizes (`ExprInteger`, `ExprFloat`, `ExprChar`, `ExprString`, `ExprBool`, `ExprUndefined`) | **Current-impl** | `bytecode.rs:182–187, 500` |

### What the VM assumes

| Assumption | Defended? | What breaks if violated |
|---|---|---|
| `pop` is called with at least one stack value | Yes — explicit `expect("Runtime Error: Stack underflow")` (`vm.rs:77`) | Panic |
| `LoadLocal(offset)` indexes a valid stack position | No — direct `self.stack[idx]` (`vm.rs:120`) | Rust `Vec` panics on out-of-bounds → session aborts |
| `StoreLocal(offset)`'s offset is reasonable | Partially — the VM pushes `0` until the index is reachable (`vm.rs:127–129`), so any offset is "valid" in the sense of not panicking, but writes to absurd offsets balloon the stack with zeros |
| `Jump(target)` and `JumpFalse(target)` are valid program indices | No — direct `inst_ptr = target` (`vm.rs:264–272`) | Next `step()` reads `program[inst_ptr]`; panics if out of bounds |
| `Call(target, n)` has `n` argument values on the stack and `target` is a function entry | No | Sets `fp = stack.len() - n` (could underflow!) and jumps. Wrong arg count corrupts every subsequent `LoadLocal` |
| `Ret` / `RetVal` see a corresponding frame on the call stack | Partial — `if let Some(frame) = self.call_stack.pop()` (`vm.rs:284, 297`); the `else` branch finalizes the run | A missing frame ends the program prematurely |
| Heap indices in op operands point at the correct `HeapObject` variant | No — silent fall-through on the `if let` (T9010) | Stack underflow downstream |
| `PushConst(Ast::ExprX)` only receives `X` ∈ the supported set | Partial — the catch-all `panic!("Runtime Error: Invalid constant AST node passed to VM")` (`vm.rs:113`) | Panic |
| `Inst::Dup` is called on a non-empty stack | Yes — `expect("Runtime Error: Cannot DUP empty stack")` (`vm.rs:138`) | Panic |
| Integer arithmetic operands are `i64`-cast-safe (e.g. division RHS is not 0) | Partial — division is unchecked, others wrap silently | `IntDiv` by zero panics (E2001); overflow wraps in release / panics in debug |
| `PrintChar`'s operand is a valid Unicode scalar | Yes — `char::from_u32(...).unwrap_or(...)` would panic; the VM uses `unwrap_or(?)` via `if let Some(c)` (`vm.rs:322`) | The print silently does nothing if the value isn't a valid char |
| RPC `Inst::Serialize*` / `Deserialize*` operands match the declared shape | No | Panic with one of: `expects a string for …`, `failed to parse JSON in …`, etc. |
| `ExtCall`'s URL string is a valid HTTP/1.1 endpoint | No | `TcpStream::connect` panics with the error string |

**Cross-layer concern:** The VM is intentionally minimalist. Most
of its assumptions are "the emitter knows what it's doing". A
direct-bytecode-injection tool (debugger, hot-patcher, future
bytecode loader from disk) must replicate every emit-time
guarantee or trigger silent corruption.

The most dangerous category is the **silent no-op family** —
ops like `GetField`/`SetField`/`ConcatStr`/`EqStr` that match on
heap-object shape via `if let` and do nothing if the shape is
wrong. These produce no panic, no error, and break the stack
discipline of every subsequent instruction. T9010 covers them.

---

## 22.5 SolFlow validator → SolFlow emitter

### What the validator guarantees

| Guarantee | Confidence |
|---|---|
| Every required input port has either a wired data edge or a non-empty inline expression string | **Specified** (post commit `3aab8e0`) |
| Every `call` node's `functionId` resolves to a known function | **Specified** |
| Every `structLiteral` / `fieldAccess` / `fieldSet` node's `structName` resolves to a known struct | **Specified** |
| Every `enumVariant` node's `enumName` + `variantName` both resolve | **Specified** |
| Every `assign` node's `varName` is non-empty | **Specified** |
| The graph has either a `start` function or a `trigger` node (warning) | **Specified** |
| Data-edge endpoint types are compatible (warning on mismatch) | **Specified** |

### What the validator does NOT check (and the emitter assumes anyway)

| Unchecked property | What the emitter does anyway |
|---|---|
| Inline expression string is parseable as SOL (T9018) | Inserts verbatim |
| Inline expression string's type matches the port's declared type | Inserts verbatim |
| `if`/`while`/`for-in` conditions are `bool`-typed | Emits as-is; analyzer rejects later if mismatch |
| Literal node's `value` text matches `litType` (T9021) | Light formatting only |
| Struct literal supplies every declared field | Walks the layout; missing fields become `/* missing */` or fall through to default (depends on missing-input path) |
| Type annotations are valid SOL primitives (not `string`, `any`, etc.) | Emits whatever `typeLabel` produces — T9009 / T9019 |
| Function call's argument count matches signature | Validator's port system enforces structurally, but a stale graph (after the target function changed) can pass validation while violating arity |

**Cross-layer concern:** The emitter is positioned as "produce
SOL from a validated graph"; in practice, "validated" means
"structurally valid". Content validation — the SOL-side
semantic checks the canonical analyzer performs — is deferred
entirely to the compiler. A graph that validates and emits
successfully can still fail at SOL parse or analyze time.

---

## 22.6 Sol Man store → SolFlow validator

### What `applyGraph.ts` (`specToWorkflow` / `specToInsertSnapshot`) guarantees

| Guarantee |
|---|
| The returned `SolWorkflow` has the schema-required shape (one or more functions, structs/enums as declared) |
| The repair pass has rewritten every `call` node whose `callTarget` doesn't resolve to a known function into a `print` placeholder |
| Edges that would reference non-existent ports (because the call→print rewrite changed the available port set) are dropped |
| Warnings collected from the repair pass are returned alongside the workflow |

### What the validator assumes

| Assumption | Defended? | Notes |
|---|---|---|
| Every node has a `data` field with a recognized `kind` | Yes — TypeScript exhaustive switches catch unknown kinds | n/a |
| Every node's `ports` field reflects the current `data` (matches `rebuildPorts(data, ctx)`) | Yes — `applyGraph.ts` calls `createNode` which calls `rebuildPorts` | A direct-mutation tool that changes `data` without rebuilding ports would break the validator |
| `Inline expressions` map only contains string values | Yes — TypeScript types enforce | n/a |
| The `id-map` step preserves edge consistency | Yes — `applyGraph.ts` rejects edges whose endpoints don't resolve | n/a |

### What `applyGraph.ts` does NOT check (and the validator can't either)

| Unchecked property | Eventual symptom |
|---|---|
| The LLM's spec doesn't produce a top-level `let` (T9014) | Runtime panic when the user clicks Apply and runs the workflow |
| Enum variants in the spec don't share first characters (T9002) | Runtime mis-dispatch when the workflow runs |
| The spec's `print` nodes have only one argument (the editor's port shape enforces this structurally) | n/a (editor model prevents) |
| Inline expressions in the spec are well-formed SOL | Compile failure at SOL parse |

**Cross-layer concern:** Sol Man's store-side validation gate
(`hasErrors` check + `force` override) is the last safety net
before broken graphs reach the canvas. The repair pass catches
the most common failure mode (empty `call` nodes). All other
LLM-side dangers covered in chapter 19 §19.8 are the LLM's
responsibility to avoid — neither the repair pass nor the
validator catches them.

---

## 22.7 The bypass paths

Five known paths through the toolchain that bypass one or more
layers' guarantees:

1. **Compiler crate consumer that calls `init.rs::init_with_ext`
   directly**: bypasses no layer; uses the full pipeline.
2. **Sol Man with `force: true`**: bypasses the validator gate.
   The emitter still runs; the emitted SOL still goes through
   the canonical pipeline. End-result: parse/analyze errors at
   compile time rather than apply time.
3. **Hand-edited workflow JSON loaded via `graph.loadWorkflow`**:
   bypasses Sol Man's repair pass and `applyGraph`. The graph
   skips straight to the validator. Inline expressions and
   structural shape are validated; content is not.
4. **Direct AST construction (any future tool that builds
   `Vec<Ast>` programmatically)**: bypasses the lexer and
   parser. Must populate every field correctly — most critically
   `Block.scope` and `DeclFunc.scope` placeholders that the
   analyzer expects to be `usize::MAX`.
5. **Direct bytecode construction (future debugger, hot-patcher,
   bytecode loader)**: bypasses every layer. Must honor every
   stack-discipline invariant the VM assumes (§22.4). The
   smallest-blast-radius assertion: every `Call` must be
   matched by a `Ret`/`RetVal` that pops the corresponding
   frame.

Each bypass path widens the set of behaviors that can reach the
VM. A future "verified pipeline" should declare which bypasses
are supported and add type-tag-aware runtime checks for the
unsupported ones.

---

## 22.8 Implications for future tooling

A short list of design directions this assumption inventory
suggests:

- **A bytecode verifier pass** between codegen and execution. It
  would re-walk the `Vec<Inst>`, track an abstract stack-type
  state, and reject mis-matched ops *before* the VM panics.
  Would also catch hand-emitted bytecode bugs.
- **An analyzer regression check** that asserts every name
  referenced in the AST resolves. Today this is enforced by
  `exit(1)` paths but not by a single comprehensive check.
- **An emitter strictness mode** that refuses to silently
  default-substitute (no `Type::Integer` fallback in `infer_type`,
  no `0` field-index fallback in `GetField`, no auto-creation in
  `find_local_offset`). For production builds the lenient mode
  is fine; for development the strict mode would surface latent
  bugs.
- **A round-trip canonical-SOL parser** in the editor. Today
  the editor is producer-only (chapter 18 §18.5). A round-trip
  parser would let the editor consume a hand-written `.sol`,
  giving every cross-layer mismatch a place to surface — the
  editor would refuse to import SOL that produces a graph it
  cannot then re-emit identically.
- **Diagnostic-as-value** error returns (`Result<T,
  Vec<Diagnostic>>`) at every layer boundary. Today every error
  is `process::exit(1)` or `panic!`; converting them to values
  would let tools recover, batch errors, and present them to
  the user in a structured way.

These directions are out-of-scope for the current docs but worth
naming so the audit can guide the next compiler refactor.

---

## 22.9 Sources cited in this chapter

- `lexer.rs` (full file) — lexer guarantees
- `parser.rs` (full file) — parser AST guarantees + `usize::MAX`
  scope placeholders
- `analyzer.rs` (full file) — analyzer scope mutation +
  unchecked-paths
- `bytecode.rs` (full file) — emitter guarantees + auto-create
  fallbacks
- `vm.rs` (full file) — VM assumptions about emitter correctness
- `util.rs` — `type_eq` helper (used by analyzer)
- `src/graph/validate.ts` — editor validator
- `src/graph/factory.ts` — port construction (`rebuildPorts`
  invariant)
- `src/emit/emit.ts` — editor emitter
- `src/sol-man/applyGraph.ts` — repair pass + spec-to-graph
- `src/stores/sol-man.store.ts` — Sol Man store gate
- All T9xxx entries in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md)
- Cross-references: chapters 03, 04, 05, 06, 14, 18, 19, 20, 21
