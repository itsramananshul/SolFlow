# Examples Catalogue

> **Status:** Substantive. A catalogue of canonical SOL programs
> that parse, compile, and run through the `openprem-sol-v2` crate
> (the `sol/` crate) and the editor's wasm bridge
> (`compiler-wasm/src/lib.rs`). Every snippet below is valid
> canonical SOL: `fn` with the `<-` return arrow, `#` comments,
> `[]T` arrays, `;`-separated struct fields and enum variants,
> `workflow "name" { }` for the runnable unit.

## How to read this catalogue

Each entry lists:

- **Role** — positive (idiomatic program that parses, compiles,
  and runs) or negative (intentional error that the bridge reports
  with one of its five codes).
- **Demonstrates** — the language features the example exercises.
- **Bridge result** — for negatives, the diagnostic the bridge
  emits (`E_PARSE` / `E_CODEGEN` / `E_NO_WORKFLOW` / `E_RUNTIME` /
  `ICE0001`). There is no type-checker and no numeric error-code
  system; see chapter 18 §18.6.

The editor ships five built-in graph samples (`src/samples/`,
`SAMPLES` in `src/samples/index.ts`) that emit canonical SOL of
the shapes shown here: `hello`, `monitor`, `orchestration`,
`payments`, `enterprise`. The canonical formatter that defines the
exact emitted shape is `sol/src/format.rs`, exercised by
`sol/tests/format_roundtrip.rs`.

---

## Positive examples

### Minimal workflow

- **Role:** Positive — smallest runnable unit.
- **Demonstrates:** `workflow` declaration, `let` with type
  annotation, arithmetic, the `print` builtin.

```sol
workflow "main" {
    let x: int = 2 + 3 * 4;
    print(x);
}
```

The runnable unit is the `workflow`; `run_source_json` looks for
the first `workflow` declaration and executes its body. With no
`workflow`, the bridge returns `E_NO_WORKFLOW`.

### Helper function with the `<-` return arrow

- **Role:** Positive — function + call.
- **Demonstrates:** `fn name(params) <- RetType`, parameter
  passing, `return value;`, calling a helper from the workflow.

```sol
fn sub_func(x: int) <- int {
    return x * 2;
}

workflow "main" {
    let y: int = sub_func(9);
    print(y);
}
```

The return type uses `<-`, not `->`. Omitting `<- RetType`
declares a function with no declared return type.

### Struct + helper + workflow (the `hello` sample)

- **Role:** Positive — struct definition, struct literal, field
  access through a parameter.
- **Demonstrates:** `struct` with `;`-separated fields, named
  struct literal, member access (`p.name`), a void helper, a
  bool-returning helper with an early `return`.

```sol
fn is_high(val: int, limit: int) <- bool {
    if (val > limit) {
        return true;
    }
    return false;
}

struct Person {
    name: str;
    age: int;
}

fn print_person(p: Person) {
    print(p.name);
    print(p.age);
}

workflow "hello" {
    let p: Person = Person { name: "evan", age: 19 };
    print_person(p);
}
```

`if` requires parentheses around its condition. Struct fields are
separated by `;`. A struct literal uses commas between its
field assignments.

### Enum + comparison + nested branches (the `orchestration` sample)

- **Role:** Positive — small orchestration.
- **Demonstrates:** `import`, `enum` with `;`-separated variants,
  `Enum::Variant`, nested `if`/`else`, an external capability call.

```sol
import slack;

enum Health {
    Up;
    Degraded;
    Pending;
}

workflow "orchestration" {
    let status: Health = Health::Up;
    if (status == Health::Up) {
        call("alert.send", { msg: "all good", level: 1 });
    } else {
        print("needs attention");
    }
}
```

Enum variants are `;`-separated. Choose distinct first characters
for the variants: the bytecode dispatches each variant by
`(first_char as i128) % 10`, so two variants sharing a first
character collide at runtime (the editor warns with
`enum-first-char-collision`). A `call("cap.name", params)` carries
a single params value (here a struct literal) and becomes a remote
call the host resolves.

### Loops over an array (the `monitor` sample)

- **Role:** Positive — iteration and accumulation.
- **Demonstrates:** array literal, `for-in` (no parentheses),
  `while` (parentheses required), assignment, the `len` builtin.

```sol
workflow "monitor" {
    let readings: []int = [3, 1, 4, 1, 5];
    let total: int = 0;
    for r in readings {
        total = total + r;
    }
    let n: int = len(readings);
    let i: int = 0;
    while (i < n) {
        print(readings[i]);
        i = i + 1;
    }
    print(total);
}
```

Arrays are prefix `[]T`. `for x in xs { }` takes no parentheses;
`while (c) { }` requires them. `len` is one of the four builtins
(`print`, `len`, `to_str`, `type_name`).

### Imported capability + emit (the `payments` sample shape)

- **Role:** Positive — payment-style dispatch.
- **Demonstrates:** `import "name" from module;`, an imported
  module call `module.func(args)`, `emit "event";`.

```sol
import "charge" from stripe;

workflow "payments" {
    let amount: float = 42.5;
    if (amount > 0.0) {
        stripe.charge({ cents: amount });
        emit "payment.charged";
    } else {
        emit "payment.skipped";
    }
}
```

`import "charge" from stripe;` brings in a single named capability.
An imported call `stripe.charge(args)` and a `call("...", ...)`
both become remote calls. `emit "name";` takes a string-literal
event name.

### Anonymous structs, indexing, unary ops

- **Role:** Positive — expression coverage.
- **Demonstrates:** mixed-type array literal, index read, unary
  negation and logical not, anonymous struct literal.

```sol
workflow "exprs" {
    let a: []int = [1, 2, 3];
    let b: int = a[0];
    let c: int = -a[0];
    let d: bool = !true;
    let s = { id: "r", t: 1.5 };
    print(b);
}
```

`{ id: "r", t: 1.5 }` is an anonymous struct literal (no name).
Unary `-` and `!` are valid prefix operators. When a `let` omits
its type annotation, annotate it where the intended type matters,
since the parser records `bool` by default for an unannotated
binding.

---

## Negative examples

These are intentional errors. The bridge reports each with one of
its five codes; none use a `E0xxx` / `T90xx` scheme.

### Empty initializer in `let`

- **Role:** Negative — parse error.
- **Triggers:** `let x: int = ;` — no initializer expression.
- **Bridge result:** `E_PARSE` (severity Error, phase Parser). The
  parser's plain-string message describes the unexpected token.

```sol
workflow "bad" {
    let x: int = ;
}
```

### Wrong return arrow

- **Role:** Negative — parse error.
- **Triggers:** writing `->` instead of `<-`. The lexer tokenizes
  `->` as `Minus` then `Gt`, which the parser cannot accept in
  return-type position.
- **Bridge result:** `E_PARSE`.

```sol
fn double(x: int) -> int {
    return x * 2;
}

workflow "bad" {
    print(double(3));
}
```

The fix is `fn double(x: int) <- int { … }`.

### No workflow declaration

- **Role:** Negative — run-time bridge error.
- **Triggers:** a program with functions but no `workflow`.
- **Bridge result:** `E_NO_WORKFLOW` (severity Error, phase
  Analyzer) from `run_source_json`, which needs a `workflow` to
  execute.

```sol
fn helper() <- int {
    return 1;
}
```

### Integer division by zero

- **Role:** Negative — runtime error.
- **Triggers:** `1 / 0` evaluated at runtime. There is no
  compile-time type checker; the failure surfaces only when the VM
  runs.
- **Bridge result:** `E_RUNTIME` (severity Warning, phase Runtime).
  The VM returns `Failed(string)` and the bridge reports it.

```sol
workflow "bad" {
    let x: int = 1 / 0;
    print(x);
}
```

### External capability blocked in the browser sim

- **Role:** Negative — unresolved remote call in the simulator.
- **Triggers:** a `call("cap", params)` (or imported module call)
  that the in-browser run cannot fulfil. The VM yields a
  `RemoteCall`; the sim cannot resolve it and stops.
- **Bridge result:** the run reports `ExtCallBlocked
  { function_name, url }`.

```sol
workflow "remote" {
    call("warehouse.ship", { order_id: 42 });
}
```

In a hosted run the controller resolves the capability and resumes
execution; only the unhosted browser sim treats it as blocked.

---

## Notes on canonical syntax (quick reference)

| Construct | Canonical form |
|---|---|
| Function | `fn name(p: T) <- Ret { … }` (the `<- Ret` is optional) |
| Runnable unit | `workflow "name" { … }` |
| Return type arrow | `<-` (never `->`) |
| Comment | `# to end of line` (no block comments, no `//`) |
| Array type | `[]T` (prefix), e.g. `[]int`, `[][]float` |
| Struct | `struct S { f: T; g: U; }` (fields `;`-separated) |
| Enum | `enum E { A; B; }` (variants `;`-separated) |
| Variable | `let name: Type = value;` |
| Branch / loop | `if (c) { } else { }`, `while (c) { }`, `for x in xs { }` |
| Capability call | `call("m.f", params)` |
| Imported call | `m.f(args)` |
| Namespace call | `m::rpc(args)` |
| Enum variant | `Enum::Variant` |
| Emit | `emit "event";` |
| Builtins | `print`, `len`, `to_str`, `type_name` (the complete set) |

---

## Sources cited in this catalogue

- `sol/src/lexer.rs` — tokens (the `<-` Arrow; `#` comments; the
  22 keywords)
- `sol/src/parser.rs`, `sol/src/ast.rs` — grammar and AST node set
- `sol/src/format.rs` — the canonical pretty-printer that defines
  emitted shape
- `sol/tests/format_roundtrip.rs` — round-trip coverage of the
  language used to ground the positive examples
- `sol/src/vm.rs` — the four builtins, arithmetic and division-by-zero
  behavior, remote-call yielding
- `compiler-wasm/src/lib.rs` — the wasm bridge and its five
  diagnostic codes
- `src/samples/index.ts` and `src/samples/*` — the editor's five
  built-in graph samples
