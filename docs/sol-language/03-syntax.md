# 03 — Syntax Reference

This chapter is the broad surface syntax reference for SOL: trivia, token
kinds, top level items, statements, and expression forms. Operator
precedence and the normative grammar in EBNF form live in
[`GRAMMAR.md`](./GRAMMAR.md). Every fact here is checked against
`sol/src/lexer.rs` and `sol/src/parser.rs`.

SOL has no type checker and no semantic analysis phase. The lexer and
parser produce string errors (`Result<_, String>`); there are no error
codes. "Invalid example" below means "the parser returns an error
message"; the exact wording is an implementation detail and may change.

---

## 3.1 Trivia — whitespace and comments

### Whitespace

Whitespace (spaces, tabs, newlines, carriage returns) separates tokens and
is otherwise discarded by the lexer (`sol/src/lexer.rs`). There is no
significant indentation; the language is brace delimited.

### Comments

Comments begin with `#` and run to the end of the line.

```sol
# runs to end of line
let x: int = 5;   # trailing comment
```

There are no block comments and no `//` line comments. The `#` form is the
only comment syntax (`sol/src/lexer.rs`).

### Identifiers

```
[A-Za-z_][A-Za-z0-9_]*
```

An identifier is ASCII alphanumeric plus `_`. It may start with `_` but not
with a digit. Unicode identifiers are not supported. Underscores are
ordinary identifier characters; `order_id` and `_tmp` are both valid names.
There are no numeric digit separators.

---

## 3.2 Lexical tokens

The exact lexer output is enumerated in [`GRAMMAR.md`](./GRAMMAR.md) §1;
this section is the human summary.

### Keywords (22 total)

```
bool  int  float  char  str
let  if  else  while  for  in  return
fn  workflow  emit  call
struct  enum  import  from
```

That is twenty two keywords (`sol/src/lexer.rs`). `true` and `false` are
bool literals, not keywords. There is no `function`, no `ext`, no `export`,
no `match`, no `break`, no `continue`, no `as`.

The five primitive type names (`bool int float char str`) are real
keyword tokens, so they cannot be used as identifiers.

### Integer literal

```
[0-9]+
```

Decimal only. No binary, octal, or hex prefixes; no digit separators.
Stored as `i64`.

### Float literal

```
[0-9]+\.[0-9]+
```

A digit must appear on both sides of the `.`. `1.` and `.5` are not
floats; a leading dot terminates the integer at the dot
(`sol/src/lexer.rs`). Stored as `f64`.

### String literal

```
"..."
```

Delimited by double quotes. The lexer processes these escapes: `\n`, `\t`,
`\r`, `\\`, `\"`, `\'`. Any other backslash sequence passes through
literally (`sol/src/lexer.rs`).

### Character literal

```
'X'
```

Exactly one source character between single quotes. There is NO escape
processing for char literals; the lexer reads one character then the
closing quote.

### Boolean literal

```
true   false
```

These are bool literal tokens, not keywords.

### Operators and punctuation

```
+  -  *  /
==  !=  <  >  <=  >=
&&  ||  !
=        (assignment)
<-       (return-type arrow)
.        (member access)
::       (double colon)
,  :  ;  (  )  {  }  [  ]
```

Source: `sol/src/lexer.rs`. The return type arrow is `<-`, written as the
`Arrow` token. There is no `->`; writing `->` lexes as two tokens
(`Minus`, then `Gt`) and fails to parse. There is no `%`, no `**`, no
bitwise `& | ^ ~`, no shift `<< >>`, and no ternary `? :`.

---

## 3.3 Top-level items

The file level parser is a loop over `parse_top_level`
(`sol/src/parser.rs`). It dispatches on the leading keyword and admits
exactly these forms:

| First token | Item |
|---|---|
| `import` | import declaration |
| `fn` | function declaration |
| `struct` | struct declaration |
| `enum` | enum declaration |
| `workflow` | workflow declaration |

Anything else is a parse error.

### Function declaration

```sol
fn name(p1: T1, p2: T2) <- RetType {
    # body
}
```

The `<- RetType` clause is optional; omit it for no declared return type
(`sol/src/parser.rs`). Parameters are `name: Type`, comma separated. The
parameter list may be empty.

*Valid:*

```sol
fn add(a: int, b: int) <- int {
    return a + b;
}

fn announce() {
    print("ready");
}
```

### Struct declaration

```sol
struct Name {
    field_a: T1;
    field_b: T2;
}
```

Fields are `name: Type` pairs, each terminated with a semicolon
(`sol/src/parser.rs`). Note that struct fields use `;` separators, not
commas; this is different from struct literal construction (which uses
commas). An empty struct body is accepted.

### Enum declaration

```sol
enum Name {
    Variant1;
    Variant2;
}
```

Each variant is an identifier terminated with a semicolon
(`sol/src/parser.rs`). Variants carry no explicit value in the syntax.

> Runtime note: the canonical bytecode dispatches each enum variant by
> `(first_char as i128) % 10`, so two variants whose first characters
> share a residue mod 10 can compare equal at runtime. The editor flags
> this as the `enum-first-char-collision` warning
> (`src/graph/validate.ts`).

### Workflow declaration

```sol
workflow "name" {
    # body
}
```

The workflow name is a string literal. A workflow body is a block of
statements. A file may declare more than one workflow.

### Import declaration

```sol
import module;              # import a module
import "name" from module;  # import a named Action from a module
```

A module is a bare identifier; the named form takes a string literal name
(`sol/src/parser.rs`). Imports name external Action providers that the host
resolves at runtime; no module resolution happens at parse time.

---

## 3.4 Statements

Inside a function or workflow body, `parse_stmt` (`sol/src/parser.rs`)
dispatches on the leading token:

| Leading token | Statement |
|---|---|
| `let` | local variable declaration |
| `if` | if / else |
| `while` | while loop |
| `for` | for-in loop |
| `return` | return |
| `emit` | emit event |
| anything else | expression or assignment statement |

A bare `{` in statement position is an error; blocks appear only as the
bodies of `if`, `while`, `for`, `fn`, and `workflow`.

### Variable declaration

```sol
let name: Type = value;   # with type annotation
let name = value;          # without annotation
```

The type annotation is optional. If it is omitted the AST records `bool`
by default (`sol/src/parser.rs`), so annotate the type when it matters. An
initializer is required after `=`. The trailing `;` is optional but
recommended.

```sol
let amount: int = 100;
let name = "ada";
```

### Assignment

```sol
target = value;
```

`target` is an identifier, a member access `a.field`, or an index `a[i]`
(`sol/src/parser.rs`). Assignment is a statement, not an expression; it
does not nest inside other expressions.

```sol
counter = counter + 1;
point.x = 0;
items[0] = "first";
```

### if / else

```sol
if (cond) {
    # ...
} else {
    # ...
}
```

Parentheses around the condition are required. The `else` block is
optional. An `else if` chain is written by nesting:

```sol
if (a) {
    print("a");
} else {
    if (b) {
        print("b");
    }
}
```

### while

```sol
while (cond) {
    # ...
}
```

Parentheses around the condition are required.

### for-in

```sol
for item in iterable {
    # ...
}
```

No parentheses. The iterable is typically an array; `item` is bound to
each element in turn.

### return

```sol
return;          # no value
return value;    # with value
```

### emit

```sol
emit "event_name";
```

The event is a string literal.

### Expression statement

```sol
print("hello");
add(1, 2);
```

Any expression followed by an optional `;`.

---

## 3.5 Expressions

The full precedence chain is in [`GRAMMAR.md`](./GRAMMAR.md) §4. This
section catalogues the expression forms.

### Literals

- Integer: `0`, `42`, `100`
- Float: `1.5`, `3.14`
- String: `"hello"`
- Char: `'a'`
- Bool: `true`, `false`
- Array: `[1, 2, 3]`

### Identifier reference

```
name
```

### Function call

```
f(arg1, arg2)
```

Arguments are comma separated expressions. The callee is any expression
that resolves to something callable: a declared `fn`, a builtin
(`print`, `len`, `to_str`, `type_name`), or an imported module action.

### Member access (postfix)

```
expr.field
```

Reads a struct field, or names an imported module action as in
`module.func(args)`.

### Index access (postfix)

```
expr[index]
```

Indexes an array.

### Array literal

```
[expr1, expr2, expr3]
```

Comma separated expressions inside square brackets. Elements should share
a type; the VM expects homogeneous arrays.

### Struct literal

```
Name { field_a: expr, field_b: expr }
```

A named struct literal. Fields inside the literal are comma separated
(this differs from the struct declaration, where fields are `;`
separated). An anonymous struct literal omits the name:

```sol
let p = { x: 0, y: 0 };
```

The parser peeks past the `{` to tell a struct literal from a block, so
struct literals work in most positions without extra parentheses
(`sol/src/parser.rs`).

### Enum variant

```
EnumName::Variant
```

### Namespace / RPC call (postfix `::`)

```
expr::name(args)
```

A `::` followed by a name and a parenthesized argument list is a namespace
call. At runtime it becomes a `RemoteCall` with capability `"expr::name"`
(`sol/src/parser.rs`, `sol/src/vm.rs`).

### Capability call

```
call("module.action", params)
```

`call` is a keyword form, not an ordinary function. It takes a capability
string and a single params value (commonly a struct literal). At runtime
it becomes a `RemoteCall` with the given capability string.

```sol
call("discord.send", { channel: "general", body: "hi" });
```

### Parenthesized expression

```
(expr)
```

A grouping construct.

### Unary expressions

```
-expr      # numeric negation
!expr      # logical not
```

These are the only two unary operators (`sol/src/parser.rs`). There is no
bitwise complement.

### Binary expressions

Binary operators, lowest to highest precedence: `||`, `&&`, the comparison
operators (`== != < > <= >=`, non associative), `+ -`, then `* /`. See
[`GRAMMAR.md`](./GRAMMAR.md) §4 for the full chain.

---

## 3.6 What the parser does not accept

Constructs that look plausible but are not in the language:

| Construct | Why it fails |
|---|---|
| `fn f() -> int { }` | The return arrow is `<-`, not `->`; `->` lexes as `-` then `>` |
| `// comment` | The only comment syntax is `#` |
| `function f() { }` | The function keyword is `fn`; there is no `function` keyword |
| `ext` / `export` | No such keywords |
| `match expr { }` | No `match` keyword |
| `break;` / `continue;` | No such keywords |
| C-style `for (init; cond; step)` | Only `for item in iterable` exists |
| `cond ? a : b` ternary | No conditional operator |
| `expr % expr` | No `%` operator |
| `a & b`, `a << b`, `~a` | No bitwise or shift operators |
| `1_000` digit separators | No digit separators |
| Hex / octal / binary integer literals | Lexer accepts only `[0-9]+` |
| `T[]` postfix array type | Array types are prefix: `[]T` |

---

## 3.7 Sources cited in this chapter

- `sol/src/lexer.rs` — tokens, keywords, comments, literals, the `<-`
  arrow
- `sol/src/parser.rs` — top level items, statements, expressions,
  precedence chain
- `sol/src/ast.rs` — AST node shapes
- `sol/src/vm.rs` — `RemoteCall` semantics for `call`, `module.func`, and
  `::` calls
- `src/graph/validate.ts` — the `enum-first-char-collision` editor warning
