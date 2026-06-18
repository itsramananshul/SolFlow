# SOL Grammar

This file is the EBNF-style grammar of the canonical SOL parser
(`sol/src/parser.rs`), with the lexical layer from `sol/src/lexer.rs`.
Source citations name the matching module; the lexer and parser track no
line or column information, so no line ranges are given.

## Conventions

- `UPPER_CASE` names are lexical tokens defined in §1.
- `lower_case` names are syntactic productions defined in §2 to §5.
- `'x'` is a literal terminal.
- `{ x }` is zero-or-more `x`.
- `[ x ]` is optional `x`.
- `( a | b )` is one of `a` or `b`.

Where the parser is more permissive than idiomatic SOL, the grammar
reflects the parser.

---

## §1 Lexical structure

### Whitespace and comments

```
WS       =  ? space | tab | newline | carriage-return ?
COMMENT  =  '#' { ? any character except newline ? } newline
TRIVIA   =  WS  |  COMMENT
```

Trivia is consumed and discarded by the lexer (`sol/src/lexer.rs`). The
only comment syntax is `#` to end of line. There are no block comments and
no `//` comments.

### Identifiers

```
IDENT_START  =  'A'..'Z'  |  'a'..'z'  |  '_'
IDENT_CONT   =  IDENT_START  |  '0'..'9'
IDENT        =  IDENT_START { IDENT_CONT }
```

ASCII only; may start with `_`, never with a digit. An `IDENT` that matches
a keyword below is re-tagged as that keyword token.

### Keywords (22)

```
KW_BOOL 'bool'   KW_INT 'int'   KW_FLOAT 'float'   KW_CHAR 'char'   KW_STR 'str'

KW_LET 'let'   KW_IF 'if'   KW_ELSE 'else'   KW_WHILE 'while'
KW_FOR 'for'   KW_IN 'in'   KW_RETURN 'return'

KW_FN 'fn'   KW_WORKFLOW 'workflow'   KW_EMIT 'emit'   KW_CALL 'call'

KW_STRUCT 'struct'   KW_ENUM 'enum'   KW_IMPORT 'import'   KW_FROM 'from'
```

There are twenty two keywords (`sol/src/lexer.rs`). `true` and `false` are
bool literal tokens, not keywords. There is no `function`, `ext`, `export`,
`match`, `break`, `continue`, `as`, `pub`, or `const`.

### Literals

```
INTEGER  =  digit { digit }                              (* decimal only, stored i64 *)
FLOAT    =  digit { digit } '.' digit { digit }          (* digits required both sides *)
STRING   =  '"' { string_char } '"'
CHAR     =  '\'' ? exactly one source character ? '\''
BOOL     =  'true'  |  'false'
```

Notes:

- `INTEGER` is decimal only; no digit separators, no radix prefixes.
- `FLOAT` requires digits on both sides of the dot; `1.` and `.5` are not
  floats.
- `STRING` processes the escapes `\n \t \r \\ \" \'`; any other backslash
  sequence passes through literally.
- `CHAR` performs no escape processing; supply exactly one character.

### Operators and punctuation

```
LPAREN '('   RPAREN ')'   LBRACKET '['   RBRACKET ']'   LBRACE '{'   RBRACE '}'

DOT '.'   COMMA ','   COLON ':'   DOUBLECOLON '::'   SEMICOLON ';'   ARROW '<-'

ASSIGN '='   NOT '!'   NE '!='   EQ '=='
LT '<'   LE '<='   GT '>'   GE '>='
PLUS '+'   MINUS '-'   STAR '*'   SLASH '/'
AND '&&'   OR '||'
```

The return-type arrow is `<-` (`ARROW`). There is no `->`. There are no
bitwise operators (`& | ^ ~`), no shift operators (`<< >>`), no `%`, no
`**`, and no ternary `? :`. Source: `sol/src/lexer.rs`.

---

## §2 Top-level items

```
program     =  { top_level }

top_level   =  import_decl
            |  function_decl
            |  struct_decl
            |  enum_decl
            |  workflow_decl

import_decl   =  KW_IMPORT IDENT ';'
              |  KW_IMPORT STRING KW_FROM IDENT ';'

function_decl =  KW_FN IDENT '(' [ param_list ] ')' [ ARROW type ] block

param_list    =  param { ',' param }
param         =  IDENT ':' type

struct_decl   =  KW_STRUCT IDENT '{' { field } '}'
field         =  IDENT ':' type ';'

enum_decl     =  KW_ENUM IDENT '{' { variant } '}'
variant       =  IDENT ';'

workflow_decl =  KW_WORKFLOW STRING block
```

Source: `sol/src/parser.rs` (`parse_top_level`, `parse_import`,
`parse_function`, `parse_struct`, `parse_enum`, `parse_workflow`).

Struct fields and enum variants are each terminated with a `;`. The `<-`
return type on a function is optional; omitting it means no declared return
type.

---

## §3 Statements

```
block       =  '{' { stmt } '}'

stmt        =  let_stmt
            |  if_stmt
            |  while_stmt
            |  for_stmt
            |  return_stmt
            |  emit_stmt
            |  assign_or_expr_stmt

let_stmt    =  KW_LET IDENT [ ':' type ] '=' expr [ ';' ]

if_stmt     =  KW_IF '(' expr ')' block [ KW_ELSE block ]
while_stmt  =  KW_WHILE '(' expr ')' block
for_stmt    =  KW_FOR IDENT KW_IN expr block

return_stmt =  KW_RETURN [ expr ] [ ';' ]
emit_stmt   =  KW_EMIT STRING [ ';' ]

assign_or_expr_stmt
            =  expr '=' expr [ ';' ]      (* assignment; LHS must be a valid target *)
            |  expr [ ';' ]               (* expression statement *)
```

Source: `sol/src/parser.rs` (`parse_stmt`, `parse_block`).

Notes:

- `if` and `while` require parentheses around the condition. `for-in` has
  no parentheses.
- The type annotation on `let` is optional; when omitted the AST defaults
  the type to `bool`. The initializer after `=` is required.
- The trailing `;` is optional on `let`, `return`, `emit`, and expression
  statements (the parser consumes it if present).
- An assignment target must reduce to an identifier, a member access
  (`a.field`), or an index (`a[i]`); other left-hand sides are an error.
- An `else if` chain is written by nesting an `if_stmt` inside the `else`
  block; there is no dedicated `else if` production.

---

## §4 Expressions and operators

The expression parser is a precedence-climbing cascade
(`sol/src/parser.rs`). Each level calls the next-tighter one.

### Precedence (lowest to highest)

| Level | Production | Operators | Assoc |
|---|---|---|---|
| 1 | `logic_or` | `\|\|` | left |
| 2 | `logic_and` | `&&` | left |
| 3 | `comparison` | `==` `!=` `<` `>` `<=` `>=` | non-associative |
| 4 | `additive` | `+` `-` | left |
| 5 | `multiplicative` | `*` `/` | left |
| 6 | `unary` | `-` `!` (prefix) | right |
| 7 | `postfix` | `.` `[ ]` `::` `( )` | left |
| 8 | `primary` | literals, identifier, `(...)`, array/struct literals, `call(...)` | — |

The comparison level is non-associative: the parser reads at most one
comparison operator per `comparison` production, so `a < b < c` does not
parse as a chain.

### Expression productions

```
expr           =  logic_or

logic_or       =  logic_and    { '||'  logic_and }
logic_and      =  comparison   { '&&'  comparison }
comparison     =  additive     [ ('==' | '!=' | '<' | '>' | '<=' | '>=')  additive ]
additive       =  multiplicative { ('+' | '-')  multiplicative }
multiplicative =  unary        { ('*' | '/')  unary }

unary          =  ( '-' | '!' ) unary
               |  postfix

postfix        =  primary { postfix_op }
postfix_op     =  '.' IDENT                          (* member access *)
               |  '[' expr ']'                       (* index *)
               |  '::' IDENT '(' [ arg_list ] ')'    (* namespace / RPC call *)
               |  '::' IDENT                          (* enum variant; LHS must be a bare IDENT *)
               |  '(' [ arg_list ] ')'               (* call *)

primary        =  INTEGER
               |  FLOAT
               |  BOOL
               |  CHAR
               |  STRING
               |  '(' expr ')'                                   (* grouping *)
               |  '[' [ arg_list ] ']'                           (* array literal *)
               |  KW_CALL '(' expr ',' expr ')'                  (* capability call *)
               |  IDENT '{' [ field_init_list ] '}'              (* named struct literal *)
               |  IDENT                                          (* bare identifier *)
               |  '{' [ field_init_list ] '}'                    (* anonymous struct literal *)

arg_list        =  expr { ',' expr }
field_init_list =  field_init { ',' field_init }
field_init      =  ( IDENT | STRING ) ':' expr
```

Source: `sol/src/parser.rs` (`parse_expr`, `parse_or`, `parse_and`,
`parse_comparison`, `parse_term`, `parse_factor`, `parse_unary`,
`parse_postfix`, `parse_primary`).

### Notes on disambiguation

- `IDENT '{' ... '}'` (struct literal) versus a following block: the
  parser peeks past the `{`. It treats the braces as a struct literal when
  the next token is `}` (empty struct), or an `IDENT`/`STRING` followed by
  `:`. Otherwise the `IDENT` is a bare identifier and the `{` starts a
  separate block (for example a loop body).
- `expr '::' IDENT '(' ... ')'` is a namespace call (`Expr::NamespaceCall`);
  at runtime it produces a `RemoteCall` with capability `"expr::IDENT"`.
- `IDENT '::' IDENT` with no following `(` is an enum variant
  (`Expr::EnumVariant`); the left side must be a bare identifier.
- `call("module.action", params)` is the capability-call form. It takes a
  capability expression and exactly one params expression; at runtime it
  produces a `RemoteCall`.

### Operator absences

The following do not appear in the parser and are rejected at lex or parse
time:

- `->` (the return arrow is `<-`)
- `%` (modulo)
- `**` (exponentiation)
- `& | ^ ~` (bitwise)
- `<< >>` (shift)
- `?:` (ternary), `?.`, `??`
- `..` / `..=` (ranges)
- `=>` (fat arrow)
- assignment inside an expression (`=` is statement-level only)

---

## §5 Type syntax

```
type            =  primitive_type
                |  array_type
                |  named_type

primitive_type  =  KW_BOOL | KW_INT | KW_FLOAT | KW_CHAR | KW_STR
array_type      =  '[' ']' type            (* prefix; nests, e.g. [][]float *)
named_type      =  IDENT                    (* a struct or enum name *)
```

Source: `sol/src/parser.rs` (`parse_type`).

Notes:

- Array types are prefix: `[]int`, `[][]float`. There is no sized-array
  syntax and no postfix `T[]` form.
- The five primitives are real keyword tokens, so they cannot be used as
  identifiers.
- A `named_type` is any identifier; it names a `struct` or `enum`. Name
  resolution happens when a workflow is compiled, not in the parser.

---

## Cross-references

- Surface syntax prose: chapter 03.
- File structure and top level items: chapter 02.
- Diagnostics: the crate returns string errors; the `compiler-wasm` bridge
  emits `E_PARSE`, `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001`
  (`compiler-wasm/src/lib.rs`); editor structural checks use kebab-case
  codes (`src/graph/validate.ts`).
