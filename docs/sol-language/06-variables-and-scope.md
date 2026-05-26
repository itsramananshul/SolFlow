# 06 — Variables and Scope

> **Status:** Substantive (commit 2). Cross-checked against
> `parser.rs:326–345` (`let`), `parser.rs:584–585` (assignment
> precedence), `analyzer.rs:37–60, 138–171, 219–240` (scope and
> assignment handling).

SOL has one kind of variable: a typed binding introduced by `let`,
written to by assignment, and resolved by name from the enclosing
lexical scope. There is no `const` keyword; there is no
distinction between mutable and immutable bindings; every `let` is
freely assignable.

This chapter covers how bindings are introduced, how they are
written to, what is visible where, what happens when a name is
re-used in the same scope, and what happens when a name is
referenced that doesn't exist.

---

## 6.1 Declaration

```sol
let name: T;          // declaration only (parser accepts; rare in practice)
let name: T = expr;   // declaration plus initializer
```

Parsed at `parser.rs:326–345`. The initializer is **optional at the
parser level** — the `=` token starts the initializer, and the
parser only requires an expression if `=` is present.

**Significant subtlety:** the analyzer does not walk the
initializer expression (`analyzer.rs:138–141`):

```rust
Ast::DeclVar { name, kind, .. } => {
    self.add_entry(name.to_owned(), Symbol::Variable { kind: Box::new(kind.clone()) });
    Some(kind.clone())
}
```

The `..` ignores the AST node's `value` field. Two consequences:

1. **The declared type and the initializer's type are not checked
   against each other.**
   ```sol
   let x: int = "this is a string";   // analyzer accepts
   ```
   The bytecode emitter is what ultimately decides whether the
   program compiles; the analyzer does not catch the mismatch.
2. **An undefined name *inside* an initializer is not caught at
   the `let`.**
   ```sol
   let x: int = undefined_var;        // analyzer accepts the let
   print(x);                          // x reads as 0 / garbage
   ```
   The bad name only surfaces if a later expression reads `x` in a
   walked context, or if the bytecode emitter walks initializers
   (it does, but its diagnostics are less structured).

Treat both above as known holes in the analyzer; they are queued
for fix in `SOL_CRATE_IDE_READINESS_PLAN.md` §1, blocker #18.

### Examples

*Valid:*

```sol
let amount: int = 100;
let label: str = "ready";
let p: Point = Point { x: 1, y: 2 };
```

*Parser-accepted, semantically uninitialized:*

```sol
let flag: bool;       // legal to parse; flag has whatever 0-bits mean as bool
```

Avoid the uninitialized form in production code.

*Parse error — empty initializer:*

```sol
let x: int = ;
```

The parser sees `=`, advances, calls `expression()`, hits `;`, and
prints:

```
not an expressionable token: Semi
could not parse expression!
```

Fixture: `error_parse1.sol`.

*Parse error — missing semicolon:*

```sol
let x: int = 5
return x;
```

The parser parses the expression `5`, then expects `;`:

```
expected semicolon at the end of a variable declaration
```

Fixture: `error_parse2.sol`.

---

## 6.2 Assignment

```sol
name = expr;
s.field = expr;
a[i] = expr;
```

Assignment is parsed at the top of the expression precedence chain
(`parser.rs:584–585`) using `right_rec`, which makes `=` *right
associative* — so `a = b = 5;` parses as `a = (b = 5);`. In
practice the analyzer doesn't allow `b = 5` to be the right-hand
side of another assignment, because assignment returns the
right-hand side's type, but reading off the resulting value isn't
useful in SOL. Treat chained assignment as a parser quirk; prefer
two separate statements.

### Plain assignment to a variable

The analyzer's `ExprBinary { op: Token::Eq, … }` branch
(`analyzer.rs:291–297`) requires the LHS and RHS to have matching
types:

```sol
let count: int = 0;
count = count + 1;       // OK
count = "string";        // analyzer: cannot assign mismatched types
```

If the LHS is an identifier that isn't in scope:

```sol
foo = 5;                 // analyzer: cannot assign mismatched types: ...
                         // because the type-check of the LHS fails first
                         // with "variable `foo` could not be found in the current scope"
```

### Assignment to a struct field

```sol
s.field = expr;
```

The LHS parses as `ExprMemAcc`; the analyzer's existing path
type-checks the access and then the assignment binary-op rule
applies. Demonstrated by `test_struct.sol`.

### Assignment to an array element

```sol
a[i] = expr;
```

The LHS parses as `ExprArrAcc`; the same applies. Demonstrated by
`test_array.sol`.

### What is not assignable

| LHS | Reason it doesn't work |
|---|---|
| A function name | The analyzer registers functions as `Symbol::Variable { kind: Function {…} }`; assigning to one trips type-mismatch |
| A literal (`5 = x;`) | Parser-accepted but produces an invalid AST shape that the analyzer rejects |
| An `enum` variant | Variant references are `ExprEnumVar`, which has no assignment path |

---

## 6.3 Lexical scope

SOL's scope is lexical and block-structured. The unit of scope is
the block (`Ast::Block`), which the analyzer creates a new
`TypeTable` for on entry and pops on exit (`analyzer.rs:150–165`).

### What introduces a new scope

| Construct | New scope? |
|---|---|
| Function body | Yes (the function's outer scope; parameters are added here before the body is walked) |
| Top-level program | One global scope holds every top-level decl |
| `{ … }` block | Yes — each braced block creates a fresh `TypeTable` **unless the block is empty** (the analyzer's `Ast::Block` handler short-circuits for `stmts.len() == 0` and returns `Type::Void` without opening a scope) |
| `if` body, `else` body | Yes — each is a block |
| `while` body | Yes |
| `for-in` body | Yes for the body; **the iteration variable is *not* in the body's scope** — see §6.5 |
| `import` statement (top-level or inside function) | No new scope, but **the alias is added to the current scope as a `Void`-typed local** — see chapter 12 §12.3 |
| Inline expression | No |

A consequence of the empty-block case: `function noop() {}` and
`{ { { { } } } }` cost nothing at the analyzer level — no scope
is opened, no `TypeTable` is allocated. The bytecode emitter
still emits an `Inst::Ret` (for the empty function body) or
nothing (for the empty `{}` blocks), and runtime behavior is
identical to a no-op.

### What is visible

A name binding is visible from the point of its declaration
forward, in the block it was declared in and in every nested block
within that block, until the block ends. After the block ends, the
binding goes out of scope.

```sol
function start() -> int {
    let a: int = 1;
    if (a == 1) {
        let b: int = 2;
        print(a);      // OK — a is in scope
        print(b);      // OK — b is in this block
    }
    print(b);          // analyzer: variable `b` could not be found
    return a;
}
```

`test_scope.sol` is the canonical fixture for this rule.

### Use before declaration

```sol
function start() -> int {
    print(x);           // analyzer: variable `x` could not be found in the current scope
    let x: int = 5;
    return x;
}
```

The analyzer resolves `print`'s argument expression in the scope as
it was *at that point* in the walk. Forward references work only at
the **top level** (for functions, structs, enums — pre-registered
in pass 1; see chapter 05); they do not work for local `let`s.

Fixture: `error_semantic1.sol` (which uses `undefined_var` in a
`return`).

---

## 6.4 Shadowing — forbidden in the same scope

SOL **does not allow** re-declaring a name in the *same* scope:

```sol
function start() -> int {
    let x: int = 5;
    let x: int = 10;     // analyzer: redefinition of `x`
    return x;
}
```

The `add_entry` helper rejects duplicates at the current top of the
scope stack (`analyzer.rs:50–53`). Fixture: `error_semantic2.sol`.

Shadowing across nested scopes is **allowed** — an inner block may
declare a name that already exists in an outer scope, and the inner
name takes precedence inside the inner block:

```sol
let n: int = 1;
{
    let n: int = 2;
    print(n);           // 2
}
print(n);               // 1
```

The analyzer's name lookup (`get_entry`, `analyzer.rs:57–60`)
walks the scope stack from innermost outward and returns the first
match.

---

## 6.5 The `for-in` iteration variable

```sol
for x in array {
    print(x);
}
```

The iteration variable `x` is added to the **for-statement's
enclosing scope** (`analyzer.rs:211`), not to the loop body's
scope. The body block then opens its own nested scope on top.

The practical consequence: **`x` is still in scope after the loop
ends.**

```sol
function start() -> int {
    let xs: []int = [1, 2, 3];
    for x in xs {
        print(x);
    }
    return x;            // analyzer accepts — `x` is still in scope here
}
```

This is a known quirk. Two reasonable defenses:

1. Wrap the loop in an extra block so the iteration variable is
   scoped tightly:
   ```sol
   {
       for x in xs { print(x); }
   }
   // x is out of scope here
   ```
2. Choose iteration-variable names that don't collide with later
   logic.

---

## 6.6 Function parameters

Parameters are bound in the function body's outermost scope before
the body is walked (`analyzer.rs:113–116`):

```sol
function greet(name: str) -> int {
    print(name);
    return 0;
}
```

Re-`let`ting a parameter name in the function's top-level body
trips the duplicate-name rule, same as any other shadowing in the
same scope:

```sol
function greet(name: str) -> int {
    let name: str = "anon";        // redefinition of `name`
    return 0;
}
```

Re-`let`ting inside a nested block is fine; that's just regular
shadowing across scopes.

---

## 6.7 Mutability

There is **no immutability marker**. Every `let` is mutable. Every
`let` may be reassigned to with `=` provided the new value has a
matching type. If you want a constant, the convention is naming —
SCREAMING_SNAKE_CASE — and not reassigning it. The compiler does
not enforce this.

---

## 6.8 Common diagnostics

| Diagnostic | Cause | Fixture |
|---|---|---|
| `variable `<name>` could not be found in the current scope` | Read of a name not in scope | `error_semantic1.sol` (via `return undefined_var;`) |
| `error: redefinition of `<name>`` | Re-`let` in the same scope, or parameter shadowed by a top-level body `let` | `error_semantic2.sol` |
| `cannot assign mismatched types: …` | `name = expr;` where types don't match | n/a |
| `expected semicolon at the end of a variable declaration` | Missing `;` on a `let` | `error_parse2.sol` |
| `not an expressionable token: Semi` (then `could not parse expression!`) | Empty initializer (`let x: int = ;`) | `error_parse1.sol` |

Every entry is repeated with bad / fixed examples in
[`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md).

---

## 6.9 Sources cited in this chapter

- `parser.rs:326–345` — `let` declaration
- `parser.rs:584–585` — assignment precedence
- `analyzer.rs:37–60` — TypeTable management and duplicate-name rule
- `analyzer.rs:138–141` — `let` analyzer entry (notably skips `value`)
- `analyzer.rs:150–165` — block scope entry/exit
- `analyzer.rs:201–217` — `for-in` iteration variable binding
- `analyzer.rs:219–240` — assignment / `Eq` op type-check
- `analyzer.rs:291–297` — assignment binary-op rule
- `analyzer.rs:483–498` — variable reference resolution
- Fixtures: `test_scope.sol`, `error_semantic1.sol`,
  `error_semantic2.sol`, `error_parse1.sol`, `error_parse2.sol`,
  `test_array.sol`, `test_struct.sol`
