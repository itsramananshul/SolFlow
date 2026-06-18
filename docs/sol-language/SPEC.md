# SOL Language Specification (Normative)

> **Status:** Normative. This is the minimum a second implementation
> would need to honor in order to be called SOL compatible. It
> describes the canonical language as implemented by the
> `openprem-sol-v2` crate (the `sol/` source tree). The reference
> manual carries the prose; this file states the rules.
>
> **Source of truth:** `sol/src/lexer.rs`, `sol/src/parser.rs`,
> `sol/src/ast.rs`, `sol/src/compiler.rs`, `sol/src/instruction.rs`,
> `sol/src/value.rs`, `sol/src/vm.rs`, `sol/src/workflow.rs`,
> `sol/src/analysis.rs`, `sol/src/format.rs`, the editor bridge
> `compiler-wasm/src/lib.rs`, and the editor `src/compiler/types.ts`
> and `src/graph/validate.ts`.

---

## 1. Conventions

- Normative terms (**must**, **must not**, **shall**, **should**)
  match RFC 2119 usage.
- Lexical productions are spelled in `UPPER_CASE`. Syntactic
  productions are spelled in `lower_case`.
- There is no separate semantic analysis or type checking phase. The
  pipeline is source to `Lexer` to `Parser` (AST) to `Compiler`
  (bytecode `Chunk`) to `Vm` (stack VM). Every fallible step returns
  `Result<_, String>`; failures are plain string messages with no
  error codes and no source spans.

---

## 2. Lexical structure

### 2.1 Source encoding

Source text is processed as a sequence of Rust `char` units (Unicode
scalar values). UTF-8 is the standard on-disk encoding. Identifiers
are restricted to the ASCII subset (§2.4); Unicode identifiers are
not supported.

### 2.2 Trivia and comments

Whitespace is trivia and is discarded between tokens. There is
exactly one comment form:

- Line comments: `#` through end of line.

There are no block comments and no `//` comments. A `//` lexes as two
`Slash` tokens and will not be treated as a comment.

### 2.3 Keywords (terminal)

The keyword set is exactly twenty two:

```
bool  int  float  char  str  let  if  else  while  for  in
return  fn  workflow  emit  call  struct  enum  import  from
```

(`true` and `false` are bool literals, not keywords.) A second
implementation **must not** reserve any other identifier.

The function keyword is `fn`. There is no `function` keyword.

### 2.4 Identifiers

```
IDENT       = IDENT_START { IDENT_CONT }
IDENT_START = ASCII letter | '_'
IDENT_CONT  = ASCII letter | digit | '_'
```

An identifier may start with `_` but not with a digit. An `IDENT`
whose lexeme matches a keyword in §2.3 is re-tagged as that keyword.

### 2.5 Literals

```
INTEGER = digit { digit }                          -- stored as i64
FLOAT   = digit { digit } '.' digit { digit }      -- stored as f64
STRING  = '"' { char | escape } '"'
CHAR    = '\'' single-char '\''
BOOL    = 'true' | 'false'
```

- Integer literals are stored as `i64`.
- Float literals **must** have at least one digit on each side of the
  `.`. A leading-dot form such as `.5` is **not** a float.
- String literals recognize the escapes `\n \t \r \\ \" \'`. Any
  other backslash sequence passes through literally.
- Char literals hold exactly one character; no escape processing is
  performed inside a char literal.

The lexer never hard errors. Unterminated strings or chars and
malformed numbers fall back silently rather than raising. There is no
line, column, or span tracking.

### 2.6 Operators and punctuation

```
+  -  *  /                  -- arithmetic
==  !=  <  >  <=  >=        -- comparison
&&  ||  !                   -- logical
=                          -- assignment
<-                         -- return-type arrow (the Arrow token)
.                          -- member access
::                         -- double colon (enum variant / namespace call)
,  :  ;  (  )  {  }  [  ]  -- punctuation
```

The return-type arrow is `<-`. There is **no** `->` token: writing
`->` lexes as `Minus` followed by `Gt` and fails to parse. A second
implementation **must** apply maximal munch so that `<-`, `==`,
`!=`, `<=`, `>=`, `&&`, `||`, and `::` are each a single token.

---

## 3. Syntax: top-level

```
file = { top_level }
top_level = import_decl
          | fn_decl
          | struct_decl
          | enum_decl
          | workflow_decl
```

A second implementation **must** accept all five forms.

### 3.1 Imports

```
import_decl = 'import' module ';'
            | 'import' STRING 'from' module ';'
```

`module` is an identifier (optionally dotted as a member path). An
import binds a module name that later `module.func(args)` calls can
reference (§6).

### 3.2 Functions

```
fn_decl = 'fn' IDENT '(' [ params ] ')' [ '<-' type ] block
params  = param { ',' param }
param   = IDENT ':' type
```

The `<- type` return annotation is optional; omit it for a function
with no declared return type. Parameters are comma separated. The
return arrow is `<-`, never `->`.

### 3.3 Structs

```
struct_decl = 'struct' IDENT '{' { field } '}'
field       = IDENT ':' type ';'
```

Struct fields are semicolon terminated.

### 3.4 Enums

```
enum_decl = 'enum' IDENT '{' { variant } '}'
variant   = IDENT ';'
```

Enum variants are semicolon terminated. A variant is a bare name; it
carries no payload and no explicit integer assignment in the surface
syntax.

### 3.5 Workflows

```
workflow_decl = 'workflow' STRING block
```

The workflow name is a string literal. A workflow body is a block of
statements (§5). The workflow is the executable entry point: the
editor bridge runs the first workflow declared in a file (§9).

---

## 4. Types

### 4.1 Type forms

```
type           = primitive_type | array_type | named_type
primitive_type = 'bool' | 'int' | 'float' | 'char' | 'str'
array_type     = '[]' type                  -- PREFIX, e.g. []int, [][]float
named_type     = IDENT                       -- a struct or enum name
```

Arrays use a **prefix** bracket form: `[]int`, `[][]float`. There is
no sized-array form and no postfix `[N]` form.

### 4.2 Type annotations are not statically enforced

Type annotations on parameters, `let` bindings, struct fields, and
return positions are **parsed and recorded** but are **not checked**.
There is no type checker and no semantic analysis pass. A mismatched
type does not fail at compile time; it surfaces at runtime as a
`Failed(String)` value when an operation cannot be applied to the
actual runtime value (§7). A second implementation that adds static
type checking remains conformant only if it still accepts every
program the canonical crate accepts.

### 4.3 Runtime value forms

At runtime the VM manipulates these `Value` variants
(`sol/src/value.rs`):

```
Bool  Int(i64)  Float(f64)  Char  Str  Array  Struct(map)
Enum(name, variant)  Unit  Module(name)  RemoteRef{ id, owner }
```

- `int` is a 64-bit signed integer.
- `float` is IEEE-754 binary64.
- `str` is a Unicode string; equality is content based.
- `char` is a single character value.
- `Array` is an ordered list of values with no enforced element type.
- `Struct` is a name keyed map of field values.
- `Enum` is a `(type name, variant name)` pair.
- `Unit` is the result of a statement or a function with no returned
  value.
- `Module` is a bound import name.
- `RemoteRef` is an opaque handle to a host resolved resource.

### 4.4 The default-bool hazard for `let`

When a `let` binding omits its type annotation, the AST records the
type as `bool` by default (`sol/src/parser.rs`). This default has no
effect on runtime evaluation (annotations are not enforced), but
tools that inspect the AST will read the annotation as `bool`.
Programs **should** annotate every `let` to avoid surprising AST
consumers.

---

## 5. Statements

```
stmt = let_stmt | assign_stmt | if_stmt | while_stmt | for_stmt
     | return_stmt | emit_stmt | expr_stmt

let_stmt    = 'let' IDENT [ ':' type ] '=' expr [ ';' ]
assign_stmt = target '=' expr ';'
target      = IDENT | expr '.' IDENT | expr '[' expr ']'
if_stmt     = 'if' '(' expr ')' block [ 'else' block ]
while_stmt  = 'while' '(' expr ')' block
for_stmt    = 'for' IDENT 'in' expr block
return_stmt = 'return' [ expr ] ';'
emit_stmt   = 'emit' STRING ';'
expr_stmt   = expr ';'
block       = '{' { stmt } '}'
```

- `let`: the type annotation is optional (§4.4); the trailing `;` is
  optional.
- `if` and `while` **require** parentheses around the condition.
- `for ... in ...` takes **no** parentheses; the iterable is
  typically an array.
- `emit "name";` emits a named event. The event name is a string
  literal.
- An assignment target is an identifier, a field access `a.field`, or
  an index `a[i]`.

---

## 6. Expressions

Precedence from lowest to highest:

| Level | Operators / forms | Associativity |
|---|---|---|
| 1 | `\|\|` | left |
| 2 | `&&` | left |
| 3 | `==` `!=` `<` `>` `<=` `>=` | non-associative |
| 4 | `+` `-` | left |
| 5 | `*` `/` | left |
| 6 | `-` `!` (unary prefix) | right |
| 7 | postfix (`.field`, `[i]`, `::`, call) | left |
| 8 | primary | — |

### 6.1 Postfix forms

- `a.field` member access.
- `a[i]` index.
- `Enum::Variant` enum variant value.
- `expr::name(args)` namespace / RPC style call. This becomes a
  RemoteCall with the capability string `"expr::name"` (§7.3).
- `callee(args)` ordinary call.

### 6.2 Primary forms

- Literals: int, float, bool, char, str.
- Array literal `[a, b, c]`.
- Struct literal `Name { f: v, g: w }`, or anonymous `{ f: v }`.
  Struct-literal fields are comma separated.
- Grouping `( expr )`.
- `call("cap.name", params)` capability call (§7.3). It carries a
  single params value, commonly a struct literal.
- Identifiers.

### 6.3 Operator semantics (runtime)

Operator results are determined at runtime, not by a static type
checker:

- `int op int` yields `int`. Mixing `int` and `float` coerces both to
  `float`. `+` applied to two strings concatenates them.
- Division by zero is a runtime error for both `int` and `float`.
- Comparison operators yield `bool`.
- `&&`, `||`, `!` operate on truthy values: a `Bool`, or an `Int`
  where nonzero is true and zero is false. Applying a logical
  operator to any other value is a runtime error.
- Applying an operator to operands the VM cannot combine produces a
  `Failed(String)` at runtime.

Argument evaluation order is left to right (the compiler pushes
arguments in source order).

---

## 7. Runtime behavior

### 7.1 Execution model

A SOL workflow compiles to a bytecode `Chunk` and executes on a
single threaded stack VM (`sol/src/vm.rs`). The VM holds a value
stack and a flat locals array. `WorkflowExecutor`
(`sol/src/workflow.rs`) ties parse, compile, and run together for one
workflow.

### 7.2 Stepping and the StepResult

`Vm::step(budget)` advances execution. `budget` is a **statement
budget**: it counts `StmtBoundary` crossings, not raw instructions.
Each call returns a `StepResult`:

```
StepResult = Completed(Value)
           | Yielded(steps: u64)
           | RemoteCall { capability: String, params: Value }
           | Failed(String)
```

- `Completed(v)` means the workflow finished with value `v`.
- `Yielded(n)` means the statement budget was exhausted after `n`
  statements; the host may step again.
- `RemoteCall` means the workflow reached an external Action.
- `Failed(msg)` means a runtime error occurred; `msg` is a plain
  string.

### 7.3 External Actions and capabilities

Three call forms become a `RemoteCall`:

- `call("m.f", params)` with capability string `"m.f"`.
- imported `m.f(args)` with capability string `"m.f"`.
- `m::rpc(args)` with capability string `"m::rpc"`.

Each carries a single params value. The host resolves the capability
and resumes the workflow via
`WorkflowExecutor::resolve_remote_call(capability, result)`. The on
the wire transport for resolving a capability is not part of the
language; a host **may** choose any transport.

### 7.4 Built-in functions

The complete set of VM built-ins (`sol/src/vm.rs`) is four:

- `print(...)`: variadic. It space joins its arguments, appends a
  newline, writes the line to the captured output buffer, and returns
  `Unit`. All arguments are printed, not just the first.
- `len(str | array) <- int`: length of a string or array.
- `to_str(any) <- str`: string form of any value.
- `type_name(any) <- str`: returns one of `"bool"`, `"int"`,
  `"float"`, `"char"`, `"str"`, `"array"`, `"struct"`, `"enum"`,
  `"unit"`, `"module"`, `"remote_ref"`.

No other identifier is a built-in. Additional host helpers are
supplied by the host through `Vm::register_native`. The `crypto`
module (ed25519 sign and verify plus sha512, `sol/src/crypto.rs`) is
exported by the crate but is **not** a SOL built-in; a host must wrap
it via `register_native` to expose it.

### 7.5 Errors

Runtime errors are plain strings carried in `StepResult::Failed`.
There are no error codes, no spans, and no compile-time type
diagnostics in the core crate. Division by zero, indexing out of
range, applying an operator to incompatible values, and calling an
unknown built-in are all reported as string messages.

---

## 8. Capability analysis (advisory)

`sol/src/analysis.rs` provides static inspection helpers that do not
type check the program:

- `extract_capabilities(source) <- Result<Vec<String>, String>`
  gathers the capability strings a workflow can call, from
  `call("cap", ...)` string literals and imported `module.func(...)`
  calls.
- `analyze_workflow(source, name) <- Result<WorkflowAnalysis, String>`
  returns `workflow_name`, `call_graph` (entries of `{ module,
  capability }`), `imported_modules`, and `capabilities`.

These are advisory tools for host wiring; they are not a semantic
analysis or type-checking phase.

---

## 9. The editor bridge and diagnostics

The editor talks to the language through `compiler-wasm`, which wraps
the crate and returns a stable JSON envelope.

### 9.1 Exported bridge functions

`compiler-wasm/src/lib.rs` exports: `version`, `parse_source_json`,
`analyze_source_json`, `compile_source_json`, `compile_for_wire_json`,
`format_source_json`, and `run_source_json`. Each returns a JSON
`Envelope { ok, value, diagnostics }`. `run_source_json` adds a `run`
object (`return_value`, `output`, `steps`, `runtime_error`, `trace`,
and related fields). `run_source_json` runs the **first** workflow
declared in the source; if none exists it emits the `E_NO_WORKFLOW`
diagnostic.

### 9.2 The complete bridge diagnostic vocabulary

The bridge emits exactly five diagnostics
(`severity / phase / code`):

| Severity | Phase | Code |
|---|---|---|
| Error | Parser | `E_PARSE` |
| Error | Codegen | `E_CODEGEN` |
| Error | Analyzer | `E_NO_WORKFLOW` |
| Warning | Runtime | `E_RUNTIME` |
| Error | Internal | `ICE0001` |

There are **no** `E0xxx` or `T90xx` codes anywhere in the live
pipeline. The diagnostic JSON shape (`severity`, `phase`, `code`,
`message`, `span`, `related`, `help`) is the stable editor contract
in `src/compiler/types.ts`. Its `DiagnosticPhase` enumerates `Lexer |
Parser | Analyzer | Codegen | Runtime | Internal`; `Lexer` is
reserved and is not emitted today.

### 9.3 Browser simulator runtime errors

The browser simulation surfaces a narrow `RtErr` set: `ExtCallBlocked
{ function_name, url }` and `StepLimit { limit }`. The wider
`RuntimeError` / `RuntimeErrorView` union (DivByZero,
IndexOutOfBounds, StackUnderflow, ExtCallFailed, HeapShapeMismatch,
Cancelled, Timeout, ResourceLimit) is the wire shape shared with the
controller for exhaustive matching; the simulator does not emit most
of those variants.

---

## 10. Editor-side structural validation

`src/graph/validate.ts` runs structural checks on the visual graph.
These are editor checks, distinct from the compiler diagnostics in
§9, and use kebab-case codes:

```
no-entry  unnamed-function  enum-first-char-collision  missing-input
bad-inline-expression  unset-struct  unknown-struct  unset-field
unset-enum  unknown-enum  unset-variant  unset-call  unknown-call
unset-var  unresolved-var  type-mismatch
```

`enum-first-char-collision` reflects a real runtime hazard: the
canonical bytecode dispatches each enum variant by `(first_char as
i128) % 10`, so two variants whose first characters share a mod-10
residue compare equal at runtime even though the by-name simulator
runs them correctly. The editor surfaces this as a warning. A program
**should** ensure no two variants of one enum begin with characters
that collide under that residue.

---

## 11. Formatter

`format_source(src) <- Result<String, String>` (`sol/src/format.rs`)
reparses and pretty prints with four-space indentation. It emits the
`<-` return arrow, `workflow "name" { }`, and `if (c) { } else { }`.
Comments are not represented in the AST, so a format round trip drops
all comments.

---

## 12. Conformance

A SOL implementation is conformant iff:

- It accepts the canonical lexical, top-level, statement, and
  expression grammar of §§2 to 6, including the `<-` return arrow, the
  `#` comment form, the prefix `[]T` array form, and the twenty two
  keyword set.
- Its runtime model matches §7: a stepping stack VM, the `StepResult`
  contract, capability based RemoteCall, and the four built-ins
  `print`, `len`, `to_str`, `type_name`.
- It reports failures as string errors, with no compile-time type
  checker, and (at the bridge layer) emits only the five diagnostics
  of §9.2.

The reference implementation is the `openprem-sol-v2` crate
(`sol/src/*`); its source is the ground truth for any ambiguity this
spec leaves underspecified.
