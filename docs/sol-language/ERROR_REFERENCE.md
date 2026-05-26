# Error Reference

> **Status:** Substantive (commit 4). Catalogue of every diagnostic
> the SOL compiler and runtime currently emit, plus the tool-side
> mismatches the documentation team has identified.

The narrative companion to this catalogue is [chapter 15](./15-errors-and-diagnostics.md).

## Notation

- **`E0xxx`** — parse errors (lexer / parser).
- **`E1xxx`** — semantic errors (analyzer).
- **`E2xxx`** — runtime errors (VM).
- **`T9xxx`** — tool-side mismatches; not part of the language.

Codes are **provisional**. The current compiler does not emit
numerical codes; the names exist so consumers (SolFlow, Sol Man)
have a stable handle on each diagnostic until the compiler adopts
a compatible scheme.

Each entry shows:

- *Severity / Category* — error/warning/note + parse/semantic/runtime/tool.
- *Where it fires* — source citation in the compiler.
- *Cause* — the rule that was violated.
- *Bad example* — minimal source that triggers it.
- *Diagnostic shape* — the text the compiler prints today.
- *Fix* — the minimal change that resolves it.
- *Fixture / Related chapter* — links into the manual.

---

## Parse errors

### E0001 — Empty initializer in `let`

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:339, 749` — `expression()` is called
after `=` and hits `Semi`, which is not a valid expression start.

**Cause:** A `let` declaration uses `=` but supplies no expression.

**Bad example:**

```sol
let x: int = ;
```

**Diagnostic:**

```
not an expressionable token: Semi
could not parse expression!
```

**Fix:** Supply an initializer (`let x: int = 0;`) or drop the
`=` entirely (`let x: int;`).

**Fixture:** `error_parse1.sol`
**Related chapter:** [06 §6.1](./06-variables-and-scope.md)

---

### E0002 — Missing semicolon on a statement

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:342` (for `let`), `parser.rs:373`
(for expression statements), `parser.rs:483` (for `return`),
`parser.rs:284` (for `ext function`).

**Cause:** A statement form expects a terminating `;` and finds
something else.

**Bad example:**

```sol
let x: int = 5
return x;
```

**Diagnostic:**

```
expected semicolon at the end of a variable declaration
```

(or `expected semicolon to follow exprstmt`, `expected semicolon
at the end of a return statement`, `expected semicolon after ext
function declaration`).

**Fix:** Add the `;`.

**Fixture:** `error_parse2.sol`
**Related chapter:** [03 §3.4](./03-syntax.md)

---

### E0003 — Unknown declaration at top level

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:189–192` — top-level dispatcher
hits an unexpected first token.

**Cause:** The first token of what should be a declaration is not
one of `ext`, `function`, `let`, `struct`, `enum`, `import`.

**Bad example:**

```sol
export function foo() {}
```

**Diagnostic:**

```
unknown declaration: Ident("export")
```

**Fix:** Drop the `export` (SOL has no such keyword; every
top-level function is implicitly visible to the host). Or use
one of the legal top-level forms.

**Fixture:** none in repo
**Related chapter:** [03 §3.3, 03 §3.6](./03-syntax.md), [05](./05-functions.md), [12](./12-imports-and-controllers.md)

---

### E0004 — Missing `function` after `ext`

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:252` — `eat(TokenKind::Func, …)`.

**Cause:** `ext` is followed by something other than `function`.

**Bad example:**

```sol
ext fn foo() -> int;
```

**Diagnostic:**

```
expected `function` keyword after `ext`
```

**Fix:** Use `function`, not `fn` or another keyword.

**Related chapter:** [05 §5.4](./05-functions.md)

---

### E0005 — Missing brace, closed brace, or bracket

**Severity:** error · **Category:** parse
**Where it fires:** Multiple — `parser.rs:221, 239, 296, 313, 355, 399, 414, 433, 496, 514, 527, 555, 621, 681, 697, 723, 736`. Each call to `eat(...)` for a structural token raises one of these.

**Cause:** A missing `(`, `)`, `[`, `]`, `{`, or `}`.

**Diagnostic (varies):**

```
left curly brace is never closed
expected `)` after tuple type
expected right parenthesis after parameter list
expected left parenthesis after function name
expected `]` to close array index
expected `{` after if statement declaration
expected `}` to close struct declaration
expected `]` to close an array initializer
...
```

**Fix:** Balance the brackets / braces / parentheses.

---

### E0006 — Array size must be an integer

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:215`.

**Cause:** The size in an `[N]T` array type is not an integer
literal.

**Bad example:**

```sol
let arr: [n]int = [1, 2, 3];     // n is an identifier, not an integer literal
```

**Diagnostic:**

```
only integers can be used to specify an array size
```

**Fix:** Use a literal (`[3]int`) or omit the size (`[]int`).

**Related chapter:** [11 §11.1](./11-arrays.md)

---

### E0007 — Invalid type in type position

**Severity:** error · **Category:** parse
**Where it fires:** `parser.rs:245`.

**Cause:** A type position holds a token the parser can't begin a
type with (not an identifier, not `[`, not `(`).

**Diagnostic:**

```
`<TOKEN>` is not valid in a type specifier
```

**Fix:** Use a valid type form per chapter 04.

---

### E0008 — Unrecognized character (lexer)

**Severity:** error · **Category:** parse
**Where it fires:** `lexer.rs:298`.

**Cause:** A source character does not start any valid token. (The
lexer otherwise tolerates almost everything via the trivia /
identifier / number paths.)

**Diagnostic:**

```
unrecognized character: '<C>'
```

**Fix:** Remove the offending character.

---

## Semantic errors

### E1001 — Variable not in scope

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:485–488` (read), `analyzer.rs:222–224` (assign).

**Cause:** An identifier in expression position resolves to no
visible binding.

**Bad example:**

```sol
function start() -> int {
    let x: int = 5;
    return undefined_var;
}
```

**Diagnostic:**

```
variable `undefined_var` could not be found in the current scope
```

**Fix:** Declare the variable (`let undefined_var: int = …;`) or
correct the spelling.

**Fixture:** `error_semantic1.sol`
**Related chapter:** [06 §6.3](./06-variables-and-scope.md)

---

### E1002 — Redefinition of name (variable / parameter / function / struct / enum)

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:50–53`.

**Cause:** A second binding in the same scope tries to use a name
that's already bound there.

**Bad example (duplicate `let`):**

```sol
function start() -> int {
    let x: int = 5;
    let x: int = 10;
    return x;
}
```

**Bad example (duplicate `function`):**

```sol
function foo() -> int { return 5; }
function foo() -> int { return 10; }
```

**Diagnostic:**

```
error: redefinition of `<name>`
```

**Fix:** Rename one of the duplicates. (Shadowing across **nested**
scopes is fine; see chapter 06 §6.4.)

**Fixtures:** `error_semantic2.sol`, `error_semantic3.sol`
**Related chapters:** [05 §5.1](./05-functions.md), [06 §6.4](./06-variables-and-scope.md)

---

### E1003 — Wrong condition type in `if` / `while`

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:174–176` (if), `analyzer.rs:190–192` (while).

**Cause:** The condition expression is not of type `bool`.

**Bad example:**

```sol
if 5 { print("nope"); }
```

**Diagnostic:**

```
condition of if statement must be of type `bool`, got Integer
```

(The same text — including "if statement" — also fires for
`while`; this is a known imprecision in the message text.)

**Fix:** Convert to a boolean comparison (`if 5 != 0 { … }`).

**Related chapter:** [07 §7.1, 07 §7.2](./07-control-flow.md)

---

### E1004 — `for-in` iterable must be an array

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:202–209`.

**Cause:** The expression after `in` does not have an array type.

**Bad example:**

```sol
for x in 5 { print(x); }
```

**Diagnostic:**

```
array in which for loop is iterating over must have the known type `Array`
```

**Fix:** Pass an array — e.g. `for x in [5] { … }`.

**Related chapter:** [07 §7.3](./07-control-flow.md)

---

### E1005 — Illegal return (outside any function body)

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:469–472`.

**Cause:** A `return` appears outside any `function` body.

**Bad example:**

```sol
return 5;
function start() {}
```

**Diagnostic:**

```
illegal return statement
```

**Fix:** Move the `return` inside a function body.

---

### E1006 — Arithmetic on mismatched types

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:247–251`.

**Cause:** `+`, `-`, `*`, `/` between two operands of different
types.

**Bad example:**

```sol
let n: int = 1;
let f: float = 1.5;
let r: int = n + f;
```

**Diagnostic:**

```
mismatched types in arithmetic: Integer + Float
```

**Fix:** Convert one operand explicitly (via an `ext function`,
since SOL has no built-in casts).

**Related chapter:** [04 §4.2.1, 04 §4.2.2](./04-types.md)

---

### E1007 — Arithmetic on a non-numeric type

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:255–258`.

**Cause:** `+`/`-`/`*`/`/` on operands that match but are neither
`int` nor `float` (e.g. `bool`, `str`, `char`, struct, enum).

**Diagnostic:**

```
arithmetic operation Plus not supported for type Bool
```

**Fix:** Use the right operator for the type, or restructure the
program.

**Related chapter:** [04](./04-types.md)

---

### E1008 — Cannot compare mismatched types

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:265–270`.

**Cause:** `==`, `!=`, `<`, `<=`, `>`, `>=` between two operands of
different types.

**Diagnostic:**

```
cannot compare mismatched types: Integer < Float
```

**Fix:** Convert one side or change the comparison.

---

### E1009 — Logical op requires booleans

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:274–279`.

**Cause:** `&&` or `||` with at least one non-`bool` operand.

**Diagnostic:**

```
logical operation AmpAmp requires boolean operands
```

**Fix:** Convert each operand to `bool` via a comparison.

---

### E1010 — Bitwise op requires integers

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:283–288`.

**Cause:** `&`, `|`, `^`, `<<`, `>>` with at least one non-`int`
operand.

**Diagnostic:**

```
bitwise operation Caret requires integer operands
```

**Fix:** Use `int` operands.

---

### E1011 — Cannot negate a non-number

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:309–314`.

**Cause:** Unary `-` on something other than `int` or `float`.

**Diagnostic:**

```
cannot negate a non number type: <expr>(<TYPE>)
```

---

### E1012 — `!` on the wrong type

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:317–323`.

**Cause:** Unary `!` on a type other than `bool`, `int`, or `float`.
The acceptance of `int`/`float` is unusually permissive; idiomatic
SOL uses `!` only on `bool`.

**Diagnostic:**

```
can't not this type: <expr>(<TYPE>)
```

---

### E1013 — `~` requires an integer

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:325–331`.

**Cause:** Unary `~` on a non-`int` operand.

**Diagnostic:**

```
cannot bitwise invert a non integer type
```

---

### E1014 — Cannot assign mismatched types

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:291–296`.

**Cause:** `target = expr` where `target` and `expr` have
different types.

**Diagnostic:**

```
cannot assign mismatched types: Integer = String
<lhs-debug-print> = <rhs-debug-print>
```

---

### E1015 — Call to undefined function

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:376–379`.

**Cause:** Call site refers to a name not registered as a function
(neither `function` nor `ext function` nor a built-in).

**Diagnostic:**

```
attempting to make a function call on an undefined name `<name>`
```

**Fix:** Declare the function or `ext function` (chapter 12 §12.1).

---

### E1016 — Call on a non-function name

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:384–387`.

**Cause:** The name resolves but is not a function (e.g. a struct
name being "called").

**Diagnostic:**

```
attempting to make a function call on a non-function type: `<name>`
```

---

### E1017 — Wrong argument count

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:391–394`.

**Diagnostic:**

```
function `<name>` expects <N> arguments but received <M>
```

---

### E1018 — Wrong argument type

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:398–402`.

**Diagnostic:**

```
function `<name>` expected <T> in position <i> but was passed <S>
```

---

### E1019 — Field access on a non-struct

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:411–413`.

**Bad example:**

```sol
let a: []int = [1, 2, 3];
print(a.length);            // arrays aren't structs; .length doesn't exist
```

**Diagnostic:**

```
<TYPE> is not a struct with members
```

---

### E1020 — Unknown struct in scope

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:417–419` (also fires for enum
lookup in `analyzer.rs:438–441`; the text reuses "struct").

**Diagnostic:**

```
could not find struct `<NAME>` in scope
```

---

### E1021 — Name resolved but not a struct

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:421–424`.

**Diagnostic:**

```
`<NAME>` is not a struct
```

---

### E1022 — Field not on struct

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:426–429`.

**Diagnostic:**

```
`<STRUCT>` has no member `<FIELD>`
```

---

### E1023 — Name resolved but not an enum

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:442–445`.

**Diagnostic:**

```
`<NAME>` is not an enum
```

---

### E1024 — Variant not in enum

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:447–451`.

**Diagnostic:**

```
`<NAME>` has no variant `<VAR>`
```

---

### E1025 — Array index of wrong type

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:459–461`.

**Diagnostic:**

```
Type Error: Array index must be an integer or float
```

(The acceptance of `float` is almost certainly a typo; treat array
indexes as `int`.)

---

### E1026 — Indexing into a non-array

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:463–465`.

**Diagnostic:**

```
Type Error: Cannot index into a non-array type
```

---

### E1027 — `rpc_request` wrong shape

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:346–363`.

**Causes:** Argument count not equal to 2; first arg not `str`;
second arg not an array.

**Diagnostics:**

```
rpc_request expects 2 arguments, got <N>
rpc_request: first argument must be str
rpc_request: second argument must be an array
```

---

### E1028 — `rpc_response` wrong arity

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:367–372`.

**Diagnostic:**

```
rpc_response expects 1 argument, got <N>
```

---

### E1029 — Unset assignment target

**Severity:** error · **Category:** semantic
**Where it fires:** `analyzer.rs:222–224, 235–238`.

**Note:** The error text `variable <name> is assigned to before
initialization` fires both when the LHS doesn't exist *and* when
types don't match. The "before initialization" wording can be
misleading; in many cases the real cause is a type mismatch
(E1014). Read the surrounding context before trusting the
literal text.

---

### E1030 — `<token>` not a valid statement start

**Severity:** error · **Category:** semantic / parse boundary
**Where it fires:** `parser.rs:377–379` — statement dispatch
fallthrough when neither a keyword nor an expression starts the
position.

**Diagnostic:**

```
identifier `<token>` is not the start of any known statement
```

---

## Runtime errors

### E2001 — Integer division by zero

**Severity:** error · **Category:** runtime
**Where it fires:** `vm.rs:146` (`a / b` with `b == 0` on `i64`).

**Cause:** A `/` operator on `int` operands evaluated with the RHS
equal to zero.

**Bad example:**

```sol
function start() -> int {
    return 1 / 0;
}
```

**Diagnostic:**

```
thread '...' panicked at 'attempt to divide by zero', src/sol/vm.rs:146:...
```

**Fix:** Check the denominator with `if` before dividing.

**Fixture:** `error_runtime.sol`
**Related chapter:** [14 §14.4](./14-runtime-semantics.md)

---

### E2002 — Array index out of bounds

**Severity:** error · **Category:** runtime
**Where it fires:** `vm.rs` `GetElem` / `SetElem` via
`Vec::index`.

**Diagnostic:**

```
thread '...' panicked at 'index out of bounds: the len is <N> but the index is <M>', ...
```

**Fix:** Range-check before indexing. Maintain a parallel length
variable, or expose `len(arr)` via an `ext function`.

---

### E2003 — Stack underflow

**Severity:** error · **Category:** runtime
**Where it fires:** `vm.rs:77` (`pop` on empty stack).

**Diagnostic:**

```
Runtime Error: Stack underflow
```

This indicates a compiler bug — correctly-emitted bytecode should
never underflow. Report against the compiler, not the program.

---

## Tool-side mismatches

### T9001 — Editor emits `// @trigger` annotations

**Category:** tool — SolFlow ↔ SOL canonical

**Description:** SolFlow's emitter writes lines like
`// @trigger webhook event="order.received"` ahead of the
function header. The SOL parser tolerates these as line comments
(`lexer.rs:311–317`). They are not part of canonical SOL.

**Where it lives:** `src/emit/emit.ts:emitTriggerComment`.

**Recommendation:** Treat the annotation as editor metadata, not
language. Round-tripping a SOL file by hand will drop the
annotation unless the tool preserves it separately.

---

### T9002 — Enum variant values are a character hash, not the iota

**Category:** tool — compiler-internal mismatch

**Description:** The parser computes per-variant iota values
(starting at 0 or the most recent `= N`), but the bytecode emitter
ignores those and uses `first_char % 10` instead
(`bytecode.rs:538–541`). At runtime, two variants that share a
first character collide; two enum variant `= N` annotations are
silently ignored.

**Where it lives:** `bytecode.rs:538–541`.

**Recommendation:** Until fixed in the compiler, make every variant
within an enum start with a *different first character*; do not
rely on `= N` annotations; do not assume variants of different
enums have different runtime values.

**Related chapter:** [10 §10.5](./10-enums.md)

---

### T9003 — `print(a, b, …)` only prints the first argument

**Category:** tool — compiler-internal mismatch

**Description:** The analyzer accepts any number of arguments of
any types for `print` (`analyzer.rs:340–345`). The bytecode emitter
compiles only `args[0]` and dispatches a single `Print*` op based
on its type (`bytecode.rs:425`). Arguments after the first are
silently dropped.

**Where it lives:** `bytecode.rs:423–432`.

**Recommendation:** Use one argument per `print` call. To print
multiple values, emit multiple statements.

**Related chapter:** [13 §13.1](./13-builtins-and-stdlib.md)

---

### T9004 — Compile failure of an `ext function` without endpoint

**Category:** tool — host configuration

**Description:** When a SOL source declares an `ext function` that
isn't in the host's flattened `[ext]`-to-URL map, the bytecode
emitter exits with `no endpoint configured for ext function <name>`
(`bytecode.rs:457–460`). This is **intentional** — better to fail
at compile time than at runtime — but it appears as a compiler
error message rather than a configuration-layer message.

**Recommendation:** When you see this, the fix is in the host's
configuration, not in the SOL source. Verify the function name
appears in `[ext]` and the node it belongs to appears in `[nodes]`.

**Related chapter:** [12 §12.4](./12-imports-and-controllers.md)

---

### T9005 — `ConcatStr` exists at the bytecode level but is unreachable from source

**Category:** tool — compiler internal feature gap

**Description:** The bytecode has an `Inst::ConcatStr` op
(`bytecode.rs:682`), but the analyzer rejects `str + str`
(`analyzer.rs:247–258` requires `int` or `float` operands). So
the bytecode op is dead code from the user's perspective.

**Recommendation:** Until the analyzer accepts `str + str`,
concatenate via an `ext function`.

**Related chapter:** [04 §4.2.4](./04-types.md)

---

### T9006 — `TypeMismatch::ArraySize` is computed but never surfaced

**Category:** tool — diagnostic-quality gap

**Description:** The `type_eq` helper distinguishes
`TypeMismatch::ArraySize` (sizes differ but inner type matches)
from `TypeMismatch::Inequal` (generic mismatch). Every analyzer
call site collapses both via `.is_err()` into the same generic
diagnostic `cannot ... mismatched types`. The user therefore
can't tell from the message that the size was the specific
problem.

**Where it lives:** `util.rs:1–14`; all `analyzer.rs` call sites
that consult `type_eq`.

**Recommendation:** A future analyzer should match on the
`TypeMismatch` variant and emit a distinct "array size mismatch:
`[3]int` vs `[5]int`" message for the size case.

**Related chapter:** [04 §4.6](./04-types.md)

---

### T9007 — Tuple type equality truncates to the shorter length

**Category:** tool — compiler bug (latent)

**Description:** `util.rs:17–23` compares tuple types with
`types_lhs.iter().zip(types_rhs).any(|(l, r)| type_eq(l, r).is_err())`.
`zip` truncates to the shorter iterator, so a difference in
arity is silently ignored — `(int, int)` and `(int, int, int)`
compare equal.

**Where it lives:** `util.rs:17–23`.

**Recommendation:** Add a `types_lhs.len() == types_rhs.len()`
guard.

**Latency:** Tuple types have no surface value form today; this
bug is currently unreachable from user code but would become live
the moment tuple literals land. Logged proactively.

**Related chapter:** [04 §4.4, 04 §4.6](./04-types.md)

---

### T9008 — Function type equality ignores return types

**Category:** tool — compiler bug (latent)

**Description:** `util.rs:24–32` compares function types only by
matching the `Void`-ness flags of the two return types. If both
returns are `Void` or both are non-`Void`, the actual return types
are not compared. So `function() -> int` and `function() -> str`
are considered equal because both have non-`Void` returns and
zero params.

**Where it lives:** `util.rs:24–32`. Also affected: the tuple
zip-truncation (T9007) applies to params, so `function(int, int)`
and `function(int, int, int)` would compare equal.

**Recommendation:** Replace the void-flag dance with a direct
`type_eq(*ret_lhs, *ret_rhs)` call, and add a param-arity guard.

**Latency:** First-class function values don't exist in SOL
today; this bug is unreachable from user code but would become
live if function types were ever exchanged between sites.

**Related chapter:** [04 §4.6](./04-types.md)

---

### T9009 — Unknown primitive name silently treated as nominal type

**Category:** tool — analyzer gap

**Description:** `parser.rs:198–209` recognizes `int, float, str,
char, bool` as primitive type names in type position; anything
else becomes `Type::Ident(name)` — a nominal struct/enum
reference. The analyzer **does not** check at the declaration
site that the named type actually exists; the check only happens
at use sites that walk into the type (e.g. field access).

In practice: a typo like `string` for `str` is silently accepted
in any struct field, function parameter, or `let` annotation.
The fixture `largemini.sol` uses `name: string` and `name:
string` in `struct Person`, which the bytecode emitter happily
processes because it doesn't validate struct field types.

**Where it lives:** `parser.rs:198–209`, `analyzer.rs:138–145`
(`let` and `struct` decl paths skip value/field-type checks).

**Recommendation:** Either special-case the common misspellings
(`string` → `str`) with a helpful diagnostic, or run a full
struct/enum resolution pass before analyzing function bodies.

**Related chapter:** [04 §4.1](./04-types.md), [09 §9.1](./09-structs.md)

---

### T9010 — Several VM ops silently no-op on type mismatch

**Category:** tool — runtime soft-fail

**Description:** Multiple VM instruction handlers do an
`if let HeapObject::X(...) = ...` match and silently fall through
when the heap reference is the wrong shape:

- `Inst::GetField`, `Inst::SetField` (`vm.rs:198–212`) — expect
  `HeapObject::Struct`
- `Inst::GetElem`, `Inst::SetElem`, `Inst::NewArray` reads,
  `Inst::ArrayLen` (`vm.rs:220–243`) — expect
  `HeapObject::Array`
- `Inst::ConcatStr`, `Inst::EqStr` (`vm.rs:245–261`) — expect
  `HeapObject::String`

When the match fails, **the expected push does not happen**, and
subsequent instructions pop from a stack that's shorter than they
expect, surfacing as `Runtime Error: Stack underflow` or wrong-
value behavior later in the program.

**Where it lives:** `vm.rs` per-op handlers above.

**Recommendation:** Replace the silent fall-through with an
explicit `panic!("Runtime Error: <op> expected <kind>")`. This
fails loudly at the actual mistake instead of corrupting the
stack and failing somewhere downstream.

**Latency:** Should not arise in well-emitted bytecode. Can
arise from compiler bugs, hand-written bytecode, or future
features that introduce new `HeapObject` variants.

**Related chapter:** [14 §14.6, 14 §14.7](./14-runtime-semantics.md)

---

### T9011 — Void function `Ret` leaves `0` on the caller's stack

**Category:** tool — runtime behavior worth knowing

**Description:** `Inst::Ret` (`vm.rs:283–293`) unwinds the call
frame and pushes `0` onto the caller's stack. The emitter
appends `Inst::Ret` at the end of every function body
(`bytecode.rs:414`), so:

- A function declared without `-> T` (Void) always ends with
  `Ret`. Callers see `0` pushed.
- Even a `Void` function compiles to "leaves 0 on the caller's
  stack".
- The analyzer treats `Void` as a separate type with no surface
  spelling — but at the VM level, a Void function call is
  indistinguishable from "returns the integer 0".

For `start` returning via a bare `return;` or by falling off the
end, the host sees `0`. This is usually the desired exit code,
but a `start` that finishes with a non-zero top-of-stack value
might end up returning whatever was left there by the prior
instruction.

**Where it lives:** `vm.rs:283–293`, `bytecode.rs:414`.

**Recommendation:** Behavior is consistent and predictable;
just be aware. When designing a host that inspects `start`'s
return value, treat Void-returning entries as returning `0`.

**Related chapter:** [05 §5.3, 05 §5.6](./05-functions.md), [14 §14.10](./14-runtime-semantics.md)

---

### T9012 — `ExtCall` transport: hand-rolled HTTP/1.1, no HTTPS, no timeout

**Category:** tool — runtime limitation

**Description:** The VM's `Inst::ExtCall` (`vm.rs:469–579`) opens
a fresh TCP connection per call and writes a hand-formatted
HTTP/1.1 request. The relevant limitations:

- **HTTP only.** The runtime strips a leading `http://` and
  assumes the rest is `host[:port]/path`. URLs starting with
  `https://` are parsed incorrectly and either fail to connect or
  hit the wrong destination.
- **No timeout.** A hung endpoint hangs the SOL session.
- **No HTTP status check.** A non-2xx response with a JSON body
  is treated identically to a success.
- **Default values on missing/wrong response shape.** A response
  whose `data` field is missing or of the wrong JSON type
  produces the declared return type's default value (0, 0.0,
  false, "?", or stringified JSON) — *silently*.
- **Panics on connect / write / JSON failures.** These propagate
  out as uncaught Rust panics and terminate the session.

**Where it lives:** `vm.rs:469–579`.

**Recommendation:** Defensive SOL programs that call `ext`
functions should: (1) check return values for default-equivalent
sentinels when correctness matters; (2) factor any long-running
or potentially-failing call behind a host-supplied `ext function`
that the host can wrap in its own timeout/retry policy.

**Related chapter:** [12 §12.4](./12-imports-and-controllers.md), [14 §14.9](./14-runtime-semantics.md)

---

### T9013 — Bare expression statements emit code that is immediately popped

**Category:** tool — language minor

**Description:** Statements like `100 + 200;` and `f();` (call of
a non-void function used as a statement) are parser-accepted
expression statements (chapter 03 §3.4). The bytecode emitter
compiles them, then immediately emits `Inst::Pop` to discard the
result (`bytecode.rs:218–223`). The expression's side effects
happen, but the value is discarded.

**Where it lives:** `bytecode.rs:166–177, 218–223`.

**Recommendation:** Don't write expression statements whose only
purpose is to compute a value. Either assign to a `let` or omit.
For function calls whose return value you don't care about, the
pattern is fine — the implicit `Pop` is exactly what you want.

The fixture `largemini.sol::blockIsolation` uses `100 + 200;` as
a deliberate no-op inside an isolating block, which is a fine
illustration of the pattern.

**Related chapter:** [03 §3.4](./03-syntax.md)

---

## Maintenance

- New diagnostics are added in this file *first*, then linked from
  the relevant chapter and from the audit's open-questions table.
- Each entry must cite a source location and at least one fixture
  or example.
- When the compiler adopts numeric codes, this file is the
  reconciliation point — the provisional `E0xxx` / `E1xxx` /
  `E2xxx` codes will either be renumbered to match upstream or
  rewritten as aliases for the upstream scheme.
