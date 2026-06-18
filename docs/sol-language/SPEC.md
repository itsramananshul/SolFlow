# SOL Language Specification (Normative)

> **Status:** Substantive (commit 6). Terse normative spec — the
> minimum a second implementation would need to honor in order to
> be called *SOL-compatible*. The reference manual carries the
> prose; this file states the rules.
>
> **Snapshot date:** 2026-05-26 (matches the compiler audit).

---

## 1. Conventions

- *Normative* terms — **must**, **must not**, **shall**,
  **should** — match RFC 2119 usage.
- Lexical productions are spelled in `UPPER_CASE` and defined in
  [§2](#2-lexical-structure). Syntactic productions are spelled in
  `lower_case` and defined in [§3](#3-syntax) and [§5](#5-statements).
- The complete EBNF lives in [`GRAMMAR.md`](./GRAMMAR.md); this
  file refers by production name.
- Where the current compiler **disagrees** with this spec
  (compiler bug, implementation gap), the disagreement is
  explicitly noted and cross-referenced to a `T9xxx` entry in
  [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 2. Lexical structure

### 2.1 Source encoding

Source text **must** be Unicode. The lexer iterates over
`char` units (Rust `char` = Unicode scalar value). No specific
file encoding is mandated by the language; in practice UTF-8 is
standard.

### 2.2 Trivia

Whitespace (`is_whitespace()`) and the bare character `_` outside
identifiers and numeric literals are trivia; the lexer discards
them between tokens. Two comment forms are recognized:

- Line comments: `//` through end of line.
- Block comments: `/*` through the next `*/`. Block comments
  **must not** be nested.

### 2.3 Keywords (terminal)

The set of keywords is **exactly** fifteen:

```
ext  for  in  as  function  if  else  import  while
struct  enum  let  return  true  false
```

A second implementation **must not** treat any other identifier as
a reserved word.

### 2.4 Identifiers

```
IDENT = IDENT_START { IDENT_CONT }
IDENT_START = ? Unicode is_alphabetic() ?
IDENT_CONT  = IDENT_START | digit | '_'
```

After lexing, an `IDENT` whose lexeme matches a keyword is
re-tagged as the keyword token.

### 2.5 Literals

```
INTEGER = digit { digit }
FLOAT   = digit { digit } '.' digit { digit }
STRING  = '"' { not '"' } '"'                 -- escape sequences NOT recognized
CHAR    = '\'' ( any single character ) '\''
BOOL    = 'true' | 'false'
```

Integer literals **must** parse as `i128`; values that exceed
`i64` truncate at runtime arithmetic (§7.4). Float literals **must**
have a digit on each side of the `.`. String literals **must not**
recognize backslash escapes (`\n`, `\t`, `\"`, `\\` are stored
literally).

### 2.6 Operators and punctuation

Single-character: `( ) [ ] { } . , : ; = ! < > + - * / & | ^ ~`
Two-character (lexer maximal-munch):
`::  ->  ==  !=  <=  >=  <<  >>  &&  ||`

A second implementation **must** apply maximal-munch — `<<` is one
token, not two `<`s; `->` is one token, not `-` followed by `>`.

---

## 3. Syntax

The full grammar lives in [`GRAMMAR.md`](./GRAMMAR.md). The
top-level shape:

```
file = { decl }
decl = ext_function_decl
     | function_decl
     | var_decl
     | struct_decl
     | enum_decl
     | import_stmt
```

A second implementation **must** accept all six top-level forms.
Anything else at the top level **must** be rejected.

The full EBNF of each `decl` is in `GRAMMAR.md` §2.

---

## 4. Types

### 4.1 Type forms

```
type = primitive_type | ident_type | array_type | tuple_type

primitive_type = 'int' | 'float' | 'str' | 'char' | 'bool'
ident_type     = IDENT                            -- struct or enum reference
array_type     = '[' [ INTEGER ] ']' type
tuple_type     = '(' [ type { ',' type } ] ')'   -- parser-accepted; no value form
```

Primitives are recognized by **string-matching the identifier in
type position** (`parser.rs:198–209`). They are not keywords at
the lexer level. A second implementation **may** keep the same
scheme or promote them to keywords; either is conformant.

### 4.2 Primitive semantics

- `int`: a 64-bit signed integer at runtime. Literals parse as
  128-bit; values exceeding 64-bit signed range **may** truncate
  silently.
- `float`: IEEE-754 binary64.
- `bool`: `true` or `false`; stored as `1` or `0` respectively.
- `str`: a Unicode string. Stored on the runtime heap; equality
  is content-based.
- `char`: a single Unicode scalar.

### 4.3 Composite semantics

- Array: heap-resident, reference-passed. Element type is uniform.
- Struct: heap-resident, reference-passed. Field order is **name-
  keyed**; the runtime layout is implementation-defined.
- Enum: a finite set of named variants. Variant values are
  implementation-defined. The current compiler uses a first-
  character hash (T9002) — this is a bug. A conformant
  implementation **should** use the parser's iota algorithm
  (`parser.rs:530–550`): start at 0; explicit `= N` resets the
  counter; each variant gets the current counter, then the
  counter increments.
- Tuple: parser-accepted as a type form; **no value form exists**
  in the surface syntax. A conformant implementation **may** add
  a tuple value form; until then, tuple types are inert.

### 4.4 Type equality

Two types are equal iff:

- Both are the same primitive variant.
- Both are arrays with element types that are equal. **Array
  sizes are not compared** — `[3]int` and `[5]int` are equal for
  assignment purposes.
- Both are `ident` types with the same name.

There is **no coercion**. There is **no implicit conversion**.

### 4.5 The `Void` type

`Void` is the result type of any statement and the implicit return
type of any function declared without `-> T`. `Void` **has no
surface spelling**.

---

## 5. Statements

```
stmt = for_stmt | if_stmt | import_stmt | while_stmt
     | var_decl | return_stmt | block | expr_stmt

for_stmt    = 'for' IDENT 'in' expr block      -- struct literals disabled in expr
if_stmt     = 'if' expr block [ 'else' block ] -- struct literals disabled in expr
while_stmt  = 'while' expr block               -- struct literals disabled in expr
return_stmt = 'return' [ expr ] ';'
expr_stmt   = expr ';'
block       = '{' { stmt } '}'
```

A conformant implementation **must** disable struct-literal
parsing inside `if` / `while` / `for-in` conditions, and **must**
re-enable it inside `(…)` groupings. Without this rule, the
grammar is ambiguous between `if Name { … }` as a struct literal
and `if Name { … }` as the start of an if-body block.

A `return` statement **must** be rejected outside a function body.

---

## 6. Expressions

The full precedence chain is in [`GRAMMAR.md`](./GRAMMAR.md) §4.
Summary (lowest precedence at top):

| Level | Operators | Associativity |
|---|---|---|
| 1 | `=` | right |
| 2 | `\|\|` | left |
| 3 | `&&` | left |
| 4 | `\|` | left |
| 5 | `^` | left |
| 6 | `&` | left |
| 7 | `==` `!=` | left |
| 8 | `<` `<=` `>` `>=` | left |
| 9 | `<<` `>>` | left |
| 10 | `+` `-` | left |
| 11 | `*` `/` | left |
| 12 | `!` `-` `~` (prefix) | right |
| 13 | `.` `[ ]` (postfix) | left |
| 14 | primary | — |

A conformant implementation **must** honor this precedence and
associativity. Subtraction at level 10 is left-associative:
`10 - 5 - 2 == 3`.

### 6.1 Operator type rules

| Op | LHS type | RHS type | Result |
|---|---|---|---|
| `+` `-` `*` `/` | `int` or `float` | same as LHS | same |
| `==` `!=` | any T | same T | `bool` |
| `<` `<=` `>` `>=` | any T | same T | `bool` |
| `&&` `\|\|` | `bool` | `bool` | `bool` |
| `&` `\|` `^` `<<` `>>` | `int` | `int` | `int` |
| `=` | T | T | T |
| `-` (unary) | `int` or `float` | — | same |
| `!` (unary) | `bool` | — | `bool` |
| `~` (unary) | `int` | — | `int` |
| `.` | struct | — | field type |
| `[ ]` | array | `int` | element type |

A conformant implementation **must** enforce these at compile time
(at the analyzer stage, before code generation).

### 6.2 Short-circuiting

This spec **does not require** short-circuiting for `&&` and
`||`. The current compiler evaluates both operands before
applying the op; a conformant implementation **may** short-circuit,
but **must** document the choice. A program that depends on
short-circuiting **should not** rely on operator chaining; use
nested `if` instead.

### 6.3 Argument evaluation order

Function arguments **must** be evaluated left-to-right. The
current compiler honors this via stack-push order (chapter 14
§14.3).

---

## 7. Runtime behavior

### 7.1 Execution model

A SOL source compiles to a stack-based bytecode and executes on a
single-threaded VM. The VM holds a value stack, a call stack of
frames, and a heap of compound values (strings, structs, arrays).
A conformant implementation **may** use a different model;
observable behavior must match.

### 7.2 Memory and references

- `int`, `float`, `bool`, `char` are value-typed; their bits sit
  on the value stack.
- `str`, struct, array are reference-typed at runtime; the stack
  carries an opaque reference (the current compiler uses a heap
  index). Mutation through one binding is visible through any
  other binding that refers to the same reference.

A conformant implementation **may** copy-on-pass instead of share-
on-pass, **provided** it does so consistently and documents the
choice.

### 7.3 Function frames

`Inst::Call` and `Inst::Ret` constitute the call/return
discipline. Tail-call optimization is **not required**. Deep
recursion **may** stack-overflow.

### 7.4 Numeric behavior

- Integer overflow **is not specified**. The current compiler
  wraps in release builds; panics in debug builds. A conformant
  implementation **may** wrap, panic, or saturate; **must**
  document the choice.
- Integer division by zero **must** terminate the session (panic
  or equivalent). The current compiler panics via Rust's `i64`
  arithmetic.
- Float division by zero **must** follow IEEE-754 (produces
  ±inf or NaN, does not trap).

### 7.5 Side-effect ordering

- `print` calls **must** observe source order. Output **must** be
  line-terminated.
- Function arguments are evaluated left-to-right (§6.3); their
  side effects therefore occur in source order.
- The order of side effects inside `&&` / `||` operands depends
  on whether the implementation short-circuits (§6.2).

---

## 8. Built-ins

A conformant implementation **must** provide the identifier `print`
in the global scope with this contract: "accept one argument of
any type and write it to a host-supplied output, followed by a
newline." The current compiler additionally accepts (but silently
ignores) extra arguments — see T9003.

A conformant implementation **may** provide the RPC helpers
(`rpc_request`, `rpc_response`, `rpc_name`, `rpc_args`,
`rpc_data`) as documented in [chapter 13](./13-builtins-and-stdlib.md).
These are not mandated by the spec; programs that use them are
tied to the current compiler.

No other built-ins are part of the language. Math, string,
collection, and I/O helpers **must** be supplied as `ext function`
declarations or built into a host-defined runtime.

---

## 9. The host runtime interface

The language commits to two pieces of behavior that any host
implementation must honor:

1. **`ext fn name(params) -> T;`** declares a function whose
   implementation the host supplies. The bytecode emitter
   resolves each `ext` name to a host-provided endpoint at
   compile time; an `ext` declaration without a configured
   endpoint **must** fail at compile time, not at runtime
   (current implementation: `bytecode.rs:457–460` exits with
   "no endpoint configured for ext function `<name>`").
2. **The conventional entry function is `start`.** A host **may**
   invoke other top-level functions, but the default startup path
   in any conformant compiler **must** be a call to `start` (if
   it exists), passing no arguments. The current compiler's
   bytecode appends `Inst::Call(start_addr, 0)` at the end of
   compilation.

The on-the-wire format used by the host to dispatch an `ext`
call is **not** part of the language. Any host implementation
**may** choose its own transport.

---

## 10. Compile-time errors and diagnostics

A conformant implementation **must** reject any source that
violates a rule in §§2 – 6 with a clear diagnostic. The current
compiler exits the process on the first error; a conformant
implementation **may** continue past the first error if it can
recover. Diagnostics **should** carry source locations
(byte offsets, line/column pairs, or both). The current compiler
does **not** carry source locations — see
`SOL_CRATE_IDE_READINESS_PLAN.md` §1 blocker #3.

---

## 11. Diagnostic shape (target)

The target diagnostic shape (when source spans land) is:

```
{
  severity:   Error | Warning | Note,
  code:       DiagnosticCode,           -- E0001 / E1004 / E2001 / T9002 etc.
  message:    String,
  span:       Span (byte_offset start..end),
  related:    [ { span, message } ]     -- "previous definition here" etc.
}
```

A conformant implementation **should** emit structured
diagnostics in this shape. The provisional code scheme is
documented in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 12. Open questions

Tracked centrally in [`00-source-audit.md` §6](./00-source-audit.md#6-open-questions);
items still open after commit 6:

- *No structural items remain unresolved.* The audit's eight
  original open questions plus four added during the
  documentation pass have all been resolved as of commit 4. Any
  future open question lands in the audit's table and is
  cross-linked here.

---

## 13. Versioning

This spec carries a snapshot date in the header. When the
compiler changes a rule, the matching rule in this spec changes
in lockstep; the previous behavior is footnoted, never silently
removed. A conformant implementation **should** target the most
recent published snapshot.

---

## 14. Conformance

A SOL implementation is *conformant* iff:

- It accepts every fixture from the positive-fixture corpus
  ([`EXAMPLES.md`](./EXAMPLES.md)) without error.
- It rejects every fixture from the negative-fixture corpus with
  a diagnostic of the matching category (parse / semantic /
  runtime).
- Its `print` behavior, `start`-entry convention, and `ext
  function` resolution rule follow §§7 – 9.
- Its operator precedence and type rules follow §§6.1 – 6.3.

A reference implementation lives in the canonical compiler crate;
its source is the ground truth for ambiguities the spec leaves
underspecified.
