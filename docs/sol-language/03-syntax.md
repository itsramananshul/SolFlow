# 03 — Syntax Reference

> **Status:** Substantive (commit 2). Cross-checked against `lexer.rs`
> (~390 lines) and `parser.rs` (~750 lines).

This chapter is the broad surface-syntax reference for SOL: every
trivia rule, every token kind, every top-level declaration, every
statement, every expression form. Operator precedence and the deep
semantics of each construct live in chapters 04 – 14; the normative
grammar in EBNF form lives in [`GRAMMAR.md`](./GRAMMAR.md).

For every construct the chapter shows:

- the form the parser accepts,
- a minimal valid example (a real fixture where one exists),
- a minimal invalid example with the diagnostic the compiler prints.

---

## 3.1 Trivia — whitespace, comments, and `_`

### Whitespace

Whitespace separates tokens. Anything `is_whitespace()` per
Rust's Unicode rules — ASCII space, tab, newline, carriage return,
form feed, plus the Unicode whitespace set — is consumed and
discarded by the lexer (`lexer.rs:307`). There is no significant
indentation; the language is brace-delimited.

### Line comments

```
// runs to end of line
```

Confirmed `(lexer.rs:311–317)`.

### Block comments

```
/* spans
   any number of lines */
```

Confirmed `(lexer.rs:319–328)`. Block comments are **not nestable** —
the lexer scans for the first `*/` after the opening `/*` and treats
intervening `/*` as ordinary characters.

### The underscore quirk (Confirmed)

The lexer treats a bare `_` as trivia, *outside* of identifiers and
numeric literals (`lexer.rs:307`). Two practical consequences:

```sol
let _x = 5;      // tokenizes as `let x = 5;` — the leading _ is eaten
let n = 1_000;   // tokenizes as `let n = 1; 000;` — TWO integers
```

Inside an identifier (after the first alphabetic character) `_` is
allowed as a name character (`lexer.rs:336`):

```sol
let order_id: int = 7;   // fine — `_` lies between alphanumerics
```

Rule of thumb: **never lead an identifier with `_` and never use
`_` as a numeric digit separator.** Use `snake_case` only between
two letters or digits.

---

## 3.2 Lexical tokens

The exact lexer output is enumerated in
[`GRAMMAR.md`](./GRAMMAR.md) §1; this section gives the human
summary.

### Keywords (15 total)

```
ext  for  in  as  function  if  else  import  while
struct  enum  let  return  true  false
```

Source: `lexer.rs:341–356`.

**There is no `export` keyword.** Any source written with
`export function …` is rejected by the parser as "unknown
declaration" because the top-level dispatcher (`parser.rs:182–193`)
admits exactly: `ext`, `function`, `let`, `struct`, `enum`,
`import`. This is the single most common cross-source surprise
— see [§3.3](#33-top-level-declarations).

### Identifiers

```
[A-Za-z][A-Za-z0-9_]*
```

The first character must be alphabetic (Unicode `is_alphabetic()`,
`lexer.rs:215`); subsequent characters may be alphanumeric or `_`.
There is no separate "raw identifier" form; the keywords above are
reserved unconditionally.

### Integer literal

```
[0-9]+
```

Decimal only. No binary / octal / hex prefixes. No digit separators
(see the underscore quirk in §3.1). Parsed as `i128` at lex time
(`lexer.rs:383`), then truncated to `i64` at runtime
(`vm.rs:143–146`). Values that overflow `i64` therefore *parse*
successfully but compute with truncated arithmetic — this is
documented in [chapter 14](./14-runtime-semantics.md).

### Float literal

```
[0-9]+\.[0-9]+
```

A digit must appear on both sides of the `.` (`lexer.rs:370–376`).
`1.` and `.5` are not floats; the lexer terminates the integer at
the dot in both cases.

### String literal

```
"..."
```

Delimited by double quotes. The lexer stores every character between
the quotes verbatim — **there are no escape sequences**
(`lexer.rs:224–233`). A literal newline inside the quotes becomes a
newline in the string. `\n`, `\t`, `\"`, `\\` are stored as the
two-character sequences they look like, not interpreted.

> **Uncertain:** whether the tooling surface ever intends to add
> escapes. Document the current state explicitly; flag any future
> change.

### Character literal

```
'X'
```

A single source character between two single quotes
(`lexer.rs:218–223`). The lexer skips exactly one character then
the closing quote — it does **not** check that the closing quote is
present, and it does **not** support escape sequences. `''` would
read whatever character happens to follow as the value.

### Boolean literal

```
true   false
```

Keywords, not identifiers.

### Operators and punctuation

Single-character punctuation:

```
( ) [ ] { } . , : ; = ! < > + - * / & | ^ ~
```

Two-character operators (lexer maximal-munch):

```
::  ->  ==  !=  <=  >=  <<  >>  &&  ||
```

Source: `lexer.rs:238–296`. The Pratt precedence table in
[chapter 08](./08-expressions.md) and [`GRAMMAR.md`](./GRAMMAR.md)
shows which of these are binary operators, which are unary, which
are postfix, and which are pure punctuation.

---

## 3.3 Top-level declarations

The file-level parser is one straight loop over `declaration()`
(`parser.rs:177–179`). It dispatches on the first keyword
(`parser.rs:180–194`) and admits exactly these forms:

| First token | Form |
|---|---|
| `ext` | external-function declaration |
| `function` | function declaration with body |
| `let` | top-level variable declaration |
| `struct` | struct declaration |
| `enum` | enum declaration |
| `import` | import statement |

Anything else `panic!`s with "unknown declaration".

### Function declaration

```sol
function name(p1: T1, p2: T2) -> R {
    // body
}
```

The return arrow and type are optional; omission means `Void`
(`parser.rs:315–320`). The parameter list may be empty; a trailing
comma is not accepted by the standard production (it terminates the
loop without a comma — `parser.rs:309–312`).

*Valid:*

```sol
function add(a: int, b: int) -> int {
    return a + b;
}

function announce() {
    print("ready");
}
```

*Invalid (missing brace):*

```sol
function start() -> int
    return 0;
```

Diagnostic: `expected `{` after for loop declaration` — actually, the
parser tries `block()`, which only opens a block on `{`; otherwise
it falls through to `statement()` which sees `return` and proceeds —
so this particular shape *parses*. The real invalid shape is a
missing right brace, which produces:

```
left curly brace is never closed
```

### External-function declaration

```sol
ext function name(p1: T1, …) -> R;
```

No body; **terminated with a semicolon** (`parser.rs:284`). The
host runtime supplies the implementation (see [chapter 12](./12-imports-and-controllers.md)).

### Variable declaration (top-level or local)

```sol
let name: T;          // declaration with no initializer (parser-accepted)
let name: T = expr;   // declaration with initializer
```

The initializer is optional at the parser level (`parser.rs:337`),
**but** the analyzer does not currently type-check the initializer
against the declared type (`analyzer.rs:138–141`). Anything you
write between `=` and `;` is parsed as an expression and stored in
the AST, but the analyzer never walks it. See
[chapter 06](./06-variables-and-scope.md) and
[chapter 15](./15-errors-and-diagnostics.md).

*Valid:*

```sol
let amount: int = 100;
let flag: bool;
```

*Invalid (empty initializer):*

```sol
let x: int = ;
```

Diagnostic chain: the parser advances past `=`, calls
`expression()`, which reaches `primary()` and sees `;` which is not
an expression-starting token. The compiler prints:

```
not an expressionable token: Semi
could not parse expression!
```

Fixture: `error_parse1.sol`.

### Struct declaration

```sol
struct Name {
    field_a: T,
    field_b: T,
}
```

Comma-separated fields. The trailing comma is optional; the parser
breaks the field loop when it sees anything other than `,` after a
field (`parser.rs:510–513`). Empty bodies are accepted (the loop
terminates on the closing brace).

**Field order is not preserved internally.** The parser stores
fields in a `HashMap` (`parser.rs:48`); the iteration order on
round-trip is unspecified. Code should refer to fields by name, not
by position.

### Enum declaration

```sol
enum Name {
    Variant,
    Variant = 5,
    NextOne,
}
```

Each variant carries an `isize` value, assigned by the rule:
"start at `0`, increment after each variant; if a variant has
`= N`, reset the counter to `N` and continue from there"
(`parser.rs:530–550`). Variant data is also `HashMap`-stored; the
same field-order caveat applies.

### Import statement

```sol
import path.segment.more;
import path.segment as alias;
```

A dotted path of identifiers (`parser.rs:440–474`). The alias
clause is optional. At the analyzer level the import only registers
the alias as a `Void`-typed name (`analyzer.rs:166–171`); no module
resolution happens today. **Treat `import` as parser-accepted but
semantically inert** until the analyzer wires it up.

### `let` at top level — **don't use**

`let` is a valid top-level declaration (it's in the dispatcher at
`parser.rs:185`), and the analyzer happily registers it in the
global type table — but the **runtime is broken for this case**.
The codegen's per-function reset of the local-slot counter
combined with the runtime's frame-pointer arithmetic means a
function-body read of a top-level binding either panics on
out-of-bounds stack access or silently returns unrelated stack
data.

Full mechanics in [chapter 06 §6.1](./06-variables-and-scope.md),
[chapter 20 §20.2](./20-implementation-notes.md), and
[`ERROR_REFERENCE.md#T9014`](./ERROR_REFERENCE.md#t9014--top-level-let-is-broken-reading-from-a-function-panics-at-runtime).
Idiomatic SOL puts every variable declaration inside a function
body.

---

## 3.4 Statements

Inside a function body, `statement()` (`parser.rs:361–382`)
dispatches on the leading token:

| Leading token | Statement |
|---|---|
| `for` | for-in loop |
| `if` | if / else |
| `import` | import (yes, in statement position too) |
| `while` | while loop |
| `let` | local variable declaration |
| `return` | return |
| `{` | nested block |
| anything else | expression statement (terminated by `;`) |

Each statement form is covered in the chapter that owns its
semantics:

- `let` — [chapter 06](./06-variables-and-scope.md)
- assignment (an expression statement using `=`) — [chapter 06](./06-variables-and-scope.md)
- `if` / `while` / `for` / `return` — [chapter 07](./07-control-flow.md)
- nested blocks — [chapter 06](./06-variables-and-scope.md)

### Expression statements

```sol
print("hello");
add(1, 2);
counter = counter + 1;
```

An expression statement is any expression followed by `;`. The
parser requires the semicolon (`parser.rs:373`). Diagnostic on
omission:

```
expected semicolon to follow exprstmt
```

Fixture: `error_parse2.sol`.

### Assignment is an expression

```sol
counter = counter + 1;
```

Assignment is parsed at the top of the expression precedence stack
(`parser.rs:584–585`), so it is *technically* an expression. In
practice it is only useful as a statement; its "value" is the
right-hand side, but no construct in the language reads that value.

---

## 3.5 Expressions

The full expression grammar — including operator precedence — is in
[chapter 08](./08-expressions.md) and [`GRAMMAR.md`](./GRAMMAR.md).
This section is a *catalogue* of expression forms.

### Literals

- Integer: `0`, `42`, `100`
- Float: `1.5`, `3.14`
- String: `"hello"`
- Char: `'a'`
- Bool: `true`, `false`

### Identifier reference

```
name
```

Resolves against the current lexical scope (chapter 06). Resolution
distinguishes variables from types — a struct or enum name is a
*type*, not an expression, and using one as a bare identifier in
expression position fails name resolution.

### Function call

```
f(arg1, arg2, arg3)
```

The arguments are expressions, comma-separated, with an optional
trailing comma (the parser doesn't accept a trailing comma — it
breaks the loop on anything other than `,`; `parser.rs:670–681`).
Calls work for declared functions, `ext` functions, the
special-cased `print`, and the RPC helpers documented in
[chapter 13](./13-builtins-and-stdlib.md).

### Field access (postfix)

```
expr.field
```

Left-associative postfix (`parser.rs:608–617`). Requires the
left-hand side to be a struct value; see chapter 09.

### Index access (postfix)

```
expr[index]
```

Left-associative postfix (`parser.rs:618–623`). Requires the
left-hand side to be an array (chapter 11). Indexes are integers
(see [chapter 11](./11-arrays.md) for the float-index quirk in the
analyzer).

### Struct literal

```
Name { field_a: expr, field_b: expr }
```

`Name` is an identifier that resolves to a struct type. The body is
field-name / expression pairs separated by commas
(`parser.rs:683–697`).

**Caveat:** struct literals are *only* parsed in expression
positions where the parser's `can_struct` flag is true. The flag is
forced to `false` inside `if`, `while`, and `for-in` conditions
(`parser.rs:394–397`, `408–411`, `428–431`) so that
`if cond { … }` is unambiguous. To use a struct literal in a
condition, wrap it in parentheses — the `(` re-enables struct
parsing (`parser.rs:714–716`).

```sol
if (Point { x: 0, y: 0 }) { … }   // OK
if Point { x: 0, y: 0 } { … }      // parses as `if Point { … }` (block as if body)
```

### Enum variant reference

```
EnumName::VariantName
```

Parsed only in primary position (`parser.rs:699–706`).

### Array literal

```
[expr1, expr2, expr3]
```

Comma-separated expressions inside square brackets
(`parser.rs:726–739`). All elements should share a type; the
analyzer presently does not enforce this for the literal itself
because `ExprArrayInit` falls through the analyzer's `todo!()`
catch (`analyzer.rs:500`), but the bytecode emitter and VM expect
homogeneous arrays.

### Parenthesized expression

```
(expr)
```

Acts as a grouping construct and *re-enables struct literals*
inside the parentheses regardless of the surrounding `can_struct`
state (`parser.rs:714–716`).

### Unary expressions

```
-expr      // numeric negation (int / float)
!expr      // logical not (and acts on int/float per the analyzer)
~expr      // bitwise complement (int only)
```

Parsed at the unary level (`parser.rs:596–604`). `!` is unusually
permissive: the analyzer accepts it on `bool`, `int`, *and* `float`
(`analyzer.rs:317–324`), where the integer and float cases are
treated as "is value zero". Prefer `!` for booleans only.

### Binary expressions

Every binary form is covered in [chapter 08](./08-expressions.md);
the table summary is in [`GRAMMAR.md`](./GRAMMAR.md). Per-operator
type rules are enforced by `analyzer.rs:241–303`.

---

## 3.6 What the parser **does not** accept

A short list of constructs that look like they should work but do
not, sourced from the absence of corresponding tokens / productions:

| Construct | Why it fails |
|---|---|
| `export function …` | No `export` keyword in the lexer; the top-level dispatcher rejects it as an unknown declaration |
| `break;` / `continue;` | No `break` / `continue` keywords in the lexer (`lexer.rs:341–356`). The analyzer keeps a `can_break` flag but no statement ever sets it |
| C-style `for (init; cond; step) { … }` | The parser's `for_stmt` admits only the `for name in expr { … }` form (`parser.rs:383–404`) |
| `match expr { … }` | No `match` keyword |
| `cond ? a : b` ternary | No conditional operator in the precedence table (`parser.rs:584–595`) |
| `expr % expr` | No `%` operator token in the lexer |
| `e.method(args)` | `.` is parsed as field access; the result is *not* callable because the lexer has no chained "call" production after a member access |
| `[1; N]` array repeat literal | Only comma-separated literals are accepted |
| Numeric digit separators (`1_000`) | `_` is consumed as trivia (§3.1) |
| Hex / octal / binary integer literals | Lexer accepts only `[0-9]+` |
| String escape sequences (`"\n"`) | The lexer stores raw characters; `\n` is the two characters `\` then `n` |

---

## 3.7 Sources cited in this chapter

- `lexer.rs:215` — identifier first-char rule
- `lexer.rs:218–223` — character literal
- `lexer.rs:224–233` — string literal
- `lexer.rs:238–296` — operator and punctuation table
- `lexer.rs:307` — whitespace + underscore handling
- `lexer.rs:311–328` — comments
- `lexer.rs:336` — identifier continuation rule
- `lexer.rs:341–356` — keyword table
- `lexer.rs:370–386` — numeric literal
- `parser.rs:177–194` — top-level declaration dispatcher
- `parser.rs:196–248` — type parser
- `parser.rs:250–287` — `ext function` declaration
- `parser.rs:289–325` — `function` declaration
- `parser.rs:326–345` — `let` declaration
- `parser.rs:347–360` — block / statement entry
- `parser.rs:361–382` — statement dispatcher
- `parser.rs:383–404` — `for` statement
- `parser.rs:405–438` — `if` / `while` statements
- `parser.rs:440–474` — `import` statement
- `parser.rs:475–486` — `return` statement
- `parser.rs:487–558` — struct / enum declarations
- `parser.rs:584–629` — expression precedence chain and postfix
- `parser.rs:630–751` — primary expression dispatcher
- `analyzer.rs:138–171` — what gets analyzed for `let` and `import`
- `analyzer.rs:317–324` — `!` analyzer rule
- `analyzer.rs:500` — `todo!` fallthrough for struct / array literals
- Fixtures: `error_parse1.sol`, `error_parse2.sol`, every positive
  fixture for the *valid* shapes above
