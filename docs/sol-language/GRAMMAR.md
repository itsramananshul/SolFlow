# SOL Grammar

> **Status:** §1 (lexical), §2 (top-level declarations), §3
> (statements) and §5 (type syntax) — substantive (commit 2).
> §4 (expressions and operators) lands in commit 3.

This file is the EBNF-style grammar derived from the SOL parser.
Source citations point at the line ranges of the matching
production in the canonical compiler crate.

## Conventions

- `UPPER_CASE` names are lexical tokens defined in §1.
- `lower_case` names are syntactic productions defined in §2 – §5.
- `'x'` is a literal terminal.
- `{ x }` is zero-or-more `x`.
- `[ x ]` is optional `x`.
- `( a | b )` is one of `a` or `b`.
- A trailing `;` in the EBNF corresponds to a literal source-level
  `;`; trailing whitespace in productions is insignificant.

Where the parser is more permissive than what idiomatic SOL uses
(e.g. trailing commas, empty fields), the grammar reflects the
parser exactly; the manual chapters call out where idioms are
narrower.

---

## §1 Lexical structure

### Whitespace

```
WS               =  ? Unicode is_whitespace() character ?  |  '_'
COMMENT_LINE     =  '//' { ? any character except newline ? } newline
COMMENT_BLOCK    =  '/*' { ? any character ? } '*/'
TRIVIA           =  WS  |  COMMENT_LINE  |  COMMENT_BLOCK
```

Trivia is consumed and discarded by the lexer (`lexer.rs:305–331`).
The literal `_` is included in the trivia set; see chapter 03 §3.1
for the practical consequences (no digit separators; underscored
identifiers can lose their leading `_`).

### Identifiers

```
IDENT_START      =  ? Unicode is_alphabetic() character ?
IDENT_CONT       =  IDENT_START  |  digit  |  '_'
IDENT            =  IDENT_START { IDENT_CONT }
```

After lexing, an `IDENT` that matches one of the keywords below is
re-tagged as the keyword token (`lexer.rs:341–356`).

### Keywords

```
KW_EXT       =  'ext'
KW_FOR       =  'for'
KW_IN        =  'in'
KW_AS        =  'as'
KW_FUNCTION  =  'function'
KW_IF        =  'if'
KW_ELSE      =  'else'
KW_IMPORT    =  'import'
KW_WHILE     =  'while'
KW_STRUCT    =  'struct'
KW_ENUM      =  'enum'
KW_LET       =  'let'
KW_RETURN    =  'return'
KW_TRUE      =  'true'
KW_FALSE     =  'false'
```

There are **fifteen** keywords. No `export`, no `match`, no
`break`, no `continue`, no `pub`, no `const`.

### Literals

```
INTEGER          =  digit { digit }                              (* `[0-9]+` decimal only *)
FLOAT            =  digit { digit } '.' digit { digit }
STRING           =  '"' { ? any source character except '"' ? } '"'
CHAR             =  '\'' ? any single source character ? '\''
BOOL             =  KW_TRUE  |  KW_FALSE
```

Notes:

- `INTEGER` is parsed as `i128` then truncated to `i64` at runtime
  (`lexer.rs:383`, `vm.rs:143–146`).
- `FLOAT` requires digits on both sides of the dot — `1.` is two
  tokens (`INTEGER`, `Dot`), not a float (`lexer.rs:370–375`).
- `STRING` has **no escape sequences**; backslashes are stored
  literally (`lexer.rs:224–233`).
- `CHAR` is the next single source character after `'`, then the
  lexer advances past whatever follows it; supply exactly one
  character (`lexer.rs:218–223`).

### Operators and punctuation

```
LPAREN '('   RPAREN ')'   LSQUARE '['   RSQUARE ']'   LCURLY '{'   RCURLY '}'

DOT '.'   COMMA ','   COLON ':'   COLONCOLON '::'   SEMI ';'   ARROW '->'

EQ '='   BANG '!'   BANG_EQ '!='   EQ_EQ '=='
LT '<'   LT_EQ '<='   GT '>'   GT_EQ '>='
PLUS '+'   DASH '-'   STAR '*'   SLASH '/'
AMP '&'   AMPAMP '&&'   PIPE '|'   PIPEPIPE '||'
LSHIFT '<<'   RSHIFT '>>'   CARET '^'   TILDE '~'
```

The lexer is maximal-munch for two-character tokens
(`==`, `!=`, `<=`, `>=`, `<<`, `>>`, `&&`, `||`, `->`, `::`).
Source: `lexer.rs:238–296`.

There is no `%` operator. There is no `**` operator. There is no
ternary `?` / `:`.

---

## §2 Top-level declarations

```
file             =  { decl }
decl             =  ext_function_decl
                 |  function_decl
                 |  var_decl
                 |  struct_decl
                 |  enum_decl
                 |  import_stmt

ext_function_decl =
    KW_EXT KW_FUNCTION IDENT '(' [ param_list ] ')' [ '->' type ] ';'

function_decl     =
    KW_FUNCTION IDENT '(' [ param_list ] ')' [ '->' type ] block

param_list        =  param { ',' param }
param             =  IDENT ':' type

var_decl          =  KW_LET IDENT ':' type [ '=' expr ] ';'

struct_decl       =  KW_STRUCT IDENT '{' [ field_list ] '}'
field_list        =  field { ',' field } [ ',' ]
field             =  IDENT ':' type

enum_decl         =  KW_ENUM IDENT '{' [ variant_list ] '}'
variant_list      =  variant { ',' variant } [ ',' ]
variant           =  IDENT [ '=' INTEGER ]

import_stmt       =  KW_IMPORT IDENT { '.' IDENT } [ KW_AS IDENT ] ';'
```

Source: `parser.rs:177–194` for the dispatcher, and the per-form
productions cited under each construct in chapter 03.

The parser's field-list and variant-list loops terminate when the
next token is not `,`, which means *one* trailing item may legally
appear without a comma — and a trailing comma is also accepted.
See `parser.rs:498–513` (struct) and `parser.rs:529–554` (enum).

---

## §3 Statements

```
block            =  '{' { stmt } '}'

stmt             =  for_stmt
                 |  if_stmt
                 |  import_stmt
                 |  while_stmt
                 |  var_decl
                 |  return_stmt
                 |  block
                 |  expr_stmt

for_stmt         =  KW_FOR IDENT KW_IN expr block         (* struct literals disabled in expr *)
if_stmt          =  KW_IF expr block [ KW_ELSE block ]    (* struct literals disabled in expr *)
while_stmt       =  KW_WHILE expr block                   (* struct literals disabled in expr *)
return_stmt      =  KW_RETURN [ expr ] ';'

expr_stmt        =  expr ';'
```

Source: `parser.rs:347–360` for `block`, `:361–382` for `stmt`,
`:383–404` (`for`), `:405–423` (`if`), `:425–438` (`while`),
`:475–486` (`return`).

The "struct literals disabled in expr" annotation refers to the
`can_struct` flag the parser flips off before parsing the
condition / iterable of a `for` / `if` / `while`. The flag is
re-enabled inside parentheses, so wrap in `( … )` to use a struct
literal in those positions.

`if` parses *one* `block` after `else`, but because `block` falls
through to `stmt` when the next token is not `{`, the canonical
`else if cond { … }` chain works:

```
KW_ELSE  →  block  →  stmt  →  if_stmt
```

with no special-cased `else if` production.

---

## §4 Expressions and operators

The expression chain is a fourteen-level Pratt-style cascade
(`parser.rs:584–595`). Each production calls the next-tighter one
and then loops applying any operators that belong at this
precedence. Operators *at the same level* associate as marked.

### Precedence table (lowest → highest)

| Level | Production | Operators | Assoc | Result type |
|---|---|---|---|---|
| 1 | `assignment` | `=` | right | type of RHS |
| 2 | `logic_or` | `\|\|` | left | `bool` |
| 3 | `logic_and` | `&&` | left | `bool` |
| 4 | `bitwise_or` | `\|` | left | `int` |
| 5 | `bitwise_xor` | `^` | left | `int` |
| 6 | `bitwise_and` | `&` | left | `int` |
| 7 | `equality` | `==` `!=` | left | `bool` |
| 8 | `relational` | `<` `<=` `>` `>=` | left | `bool` |
| 9 | `shift` | `<<` `>>` | left | `int` |
| 10 | `additive` | `+` `-` | left | operand type |
| 11 | `multiplicative` | `*` `/` | left | operand type |
| 12 | `unary` | `!` `-` `~` (prefix) | right | operand type |
| 13 | `postfix` | `.` `[ ]` | left | depends |
| 14 | `primary` | literals, identifier, `(…)`, calls, struct literal, enum variant, array literal | — | — |

### Expression productions

```
expr           =  assignment

assignment     =  logic_or  [ '='  assignment ]
logic_or       =  logic_and    { '||'  logic_and }
logic_and      =  bitwise_or   { '&&'  bitwise_or }
bitwise_or     =  bitwise_xor  { '|'   bitwise_xor }
bitwise_xor    =  bitwise_and  { '^'   bitwise_and }
bitwise_and    =  equality     { '&'   equality }
equality       =  relational   { ('==' | '!=')   relational }
relational     =  shift        { ('<' | '<=' | '>' | '>=')  shift }
shift          =  additive     { ('<<' | '>>')   additive }
additive       =  multiplicative { ('+' | '-')   multiplicative }
multiplicative =  unary        { ('*' | '/')   unary }

unary          =  ( '!' | '-' | '~' ) unary
               |  postfix

postfix        =  primary { '.' IDENT  |  '[' expr ']' }

primary        =  INTEGER
               |  FLOAT
               |  STRING
               |  CHAR
               |  BOOL
               |  IDENT '(' [ expr_list ] ')'                    (* function call *)
               |  IDENT '{' [ field_init_list ] '}'              (* struct literal — see can_struct below *)
               |  IDENT '::' IDENT                               (* enum variant *)
               |  IDENT                                          (* bare identifier reference *)
               |  '(' expr ')'                                   (* grouping *)
               |  '[' [ expr_list ] ']'                          (* array literal *)

expr_list           =  expr { ',' expr }
field_init_list     =  IDENT ':' expr { ',' IDENT ':' expr }
```

### The `can_struct` flag (parser state)

Struct literals share a leading `IDENT '{' …` shape with the body
block of `if` / `while` / `for-in`. To resolve the ambiguity the
parser carries a `can_struct: bool` flag (`parser.rs:131, 394–397,
408–411, 428–431`), defaulting to true. Before parsing the condition
of `if` / `while` / `for-in`, the parser sets it to false; the
struct-literal rule in `primary` checks it before consuming the
`{`. The flag is reset to true inside parentheses (`parser.rs:714–716`).

Effect, expressed as a guard on the production:

```
primary  →  IDENT '{' field_init_list '}'   only when can_struct == true
```

In source-code terms: to use a struct literal as a condition, wrap
it in parentheses:

```sol
if (Point { x: 0, y: 0 }) { … }
```

### Operator absences (for completeness)

The grammar above is exhaustive. The following constructs do *not*
appear in the parser:

- `%` (modulo)
- `**` (exponentiation)
- `?:` (ternary)
- `?.` (safe access)
- `??` (nullish coalescing)
- `..` / `..=` (ranges)
- `=>` (closure / fat arrow)
- `in` outside of `for-in` headers
- `as` outside of `import` aliases

Each of these is rejected at lex/parse time.

---

## §5 Type syntax

```
type             =  primitive_type
                 |  ident_type
                 |  array_type
                 |  tuple_type

primitive_type   =  'int'  |  'float'  |  'str'  |  'char'  |  'bool'    (* spelled as IDENT, matched in parse_type *)
ident_type       =  IDENT                                                (* anything else *)

array_type       =  '[' [ INTEGER ] ']' type

tuple_type       =  '(' [ type { ',' type } ] ')'
```

Source: `parser.rs:196–248`.

Notes:

- The primitives are recognized by string-matching the identifier
  inside `parse_type` — they are *not* keywords at the lexer level.
  Don't name a variable `int` (it'll be tokenized as `Ident("int")`
  and treated as an identifier in expression position) — but the
  type position will always interpret the name as the primitive.
- `array_type` size is an `INTEGER` literal only; the parser
  refuses anything else with: `only integers can be used to specify
  an array size`. Omitting the size produces an unsized array
  (`[]T`).
- `tuple_type` is parser-accepted but no value form exists; see
  chapter 04 §4.4.

---

## Cross-references

- Explanatory prose: chapters 03, 04, 05, 06.
- Normative rules and rationale: [`SPEC.md`](./SPEC.md).
- Conformance fixtures: every positive `.sol` test file
  (chapter 16).
- Diagnostics: [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).
