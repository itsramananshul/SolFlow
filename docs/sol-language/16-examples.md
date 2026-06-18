# 16 — Examples

> **Status:** Substantive. Annotated walkthroughs of canonical SOL
> programs. Every example is valid, runnable canonical SOL: a
> `workflow "name" { ... }` is the executable entry point, comments
> use `#`, functions use `fn` with the `<-` return arrow, and arrays
> use the prefix `[]T` form.

This chapter is the guided tour. Each example is written against the
canonical `openprem-sol-v2` crate (`sol/src/*`). The editor bridge
(`compiler-wasm/src/lib.rs`) runs the first `workflow` it finds.

---

## 16.1 Minimal viable program

```sol
fn double(x: int) <- int {
    return x * 2;
}

workflow "minimal" {
    let y: int = double(9);
    print(y);
}
```

### Annotations

- **`fn double(x: int) <- int`** declares a function with one `int`
  parameter and an `int` return annotation. The return arrow is `<-`,
  not `->`. The annotation is recorded but not statically enforced
  (chapter 04); a mismatch would surface at runtime as a string error.
- **`return x * 2;`** uses `*`, which binds tighter than `+`
  (chapter 08).
- **`workflow "minimal" { ... }`** is the executable entry point. Its
  name is a string literal.
- **`let y: int = double(9);`** binds a `let` with an explicit type
  annotation and a call expression initializer. Annotating the type
  is recommended: an omitted annotation defaults to `bool` in the AST
  (chapter 06).
- **`print(y);`** writes `y` and a newline to the captured output
  buffer. `print` is one of the four VM built-ins (chapter 13).

---

## 16.2 Struct plus helper plus workflow

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

fn describe(p: Person) {
    print(p.name);
    print(p.age);
}

workflow "describe-person" {
    let p: Person = Person {
        name: "evan",
        age: 19,
    };
    describe(p);
}
```

### Annotations

- **`is_high`** is a pure helper. The `if (val > limit) { return
  true; } return false;` shape needs no `else`: when the condition is
  true the early `return` runs, otherwise control falls through to
  `return false;`. The condition is parenthesized, as `if` requires.
- **`struct Person { name: str; age: int; }`** declares a struct.
  Struct fields are semicolon terminated.
- **`describe`** has no return annotation; it returns `Unit`. It
  consumes a `Person` and makes two `print` calls.
- **`Person { name: "evan", age: 19 }`** is a struct literal. Literal
  fields are comma separated and matched by name, so `Person { age:
  19, name: "evan" }` is equivalent (chapter 09).

---

## 16.3 Enums, control flow, and a small orchestration

```sol
enum AppHealth {
    Offline;
    Booting;
    Stable;
    Throttled;
}

struct ProcessNode {
    id: int;
    threshold: float;
    tag: char;
    service_name: str;
    is_active: bool;
    metrics: []int;
}

fn start_service(name: str) {
    print("started service:");
    print(name);
}

fn stop_service(name: str) {
    print("stopped service:");
    print(name);
}

fn verify_capacity(node: ProcessNode, current: float) <- AppHealth {
    if (current > node.threshold) {
        return AppHealth::Throttled;
    } else {
        if (node.is_active) {
            return AppHealth::Stable;
        } else {
            return AppHealth::Booting;
        }
    }
}

fn orchestrate(request_id: int) <- int {
    let limit: float = 90.5;
    let identity: char = 'S';
    let label: str = "inventory_orchestrator";
    let data_history: []int = [10, 22, 15, 30];

    let current_node: ProcessNode = ProcessNode {
        id: request_id,
        threshold: limit,
        tag: identity,
        service_name: label,
        is_active: true,
        metrics: data_history,
    };

    let status: AppHealth = verify_capacity(current_node, 85.2);

    if (status == AppHealth::Stable) {
        start_service(current_node.service_name);
        return 1;
    } else {
        if (status == AppHealth::Throttled) {
            stop_service(current_node.service_name);
            return 0;
        } else {
            return 2;
        }
    }
}

workflow "orchestrate-service" {
    print(orchestrate(0));
}
```

### Annotations

- **`enum AppHealth { ... }`** declares an enum. Variants are
  semicolon terminated and carry no payload. The variants here begin
  with distinct first characters (`O`, `B`, `S`, `T`). This matters:
  the canonical bytecode dispatches each variant by `(first_char as
  i128) % 10`, so two variants whose first characters share a mod-10
  residue compare equal at runtime (chapter 10). Choosing distinct
  leading characters avoids that hazard.
- **`metrics: []int`** uses the prefix array form. Array types are
  written `[]T`, never with a postfix size.
- **`verify_capacity`** returns an enum value through nested
  `if`/`else`. Nesting is the canonical workaround for the absence of
  a `match` construct.
- **`status == AppHealth::Stable`** compares an enum value to a
  variant. Because the variant first characters are distinct, the
  comparisons behave as written.
- **`workflow "orchestrate-service"`** drives the whole program and
  prints the integer result of `orchestrate(0)`.

---

## 16.4 Control-flow patterns

```sol
workflow "control-flow" {
    # early-return style lives inside helpers; here we show loops.
    let total: int = 0;
    let i: int = 0;
    while (i < 5) {
        total = total + i;
        i = i + 1;
    }
    print(total);

    let nums: []int = [3, 7, 11];
    for n in nums {
        print(n);
    }

    if ((true || false) && !false) {
        print("logic ok");
    }
}
```

### Annotations

- **`while (i < 5) { ... }`** is the counter loop. `while` requires
  parentheses around the condition; the body increments the counter.
- **`for n in nums { ... }`** iterates an array. `for ... in` takes no
  parentheses.
- **`if ((true || false) && !false) { ... }`** combines logical
  operators. The outer parentheses are the `if` condition delimiter;
  the inner parentheses group the disjunction. Logical operators work
  on `Bool` values, or on `Int` where nonzero is true (chapter 08).

---

## 16.5 Calling external Actions

```sol
import inventory;

workflow "check-stock" {
    # capability-string form: becomes a RemoteCall with capability "warehouse.get_stock"
    let level: int = call("warehouse.get_stock", { sku: "A-100" });
    print(level);

    # imported-module form: becomes a RemoteCall with capability "inventory.reserve"
    let ok: bool = inventory.reserve({ sku: "A-100", qty: 2 });
    print(ok);
}
```

### Annotations

- **`call("warehouse.get_stock", { sku: "A-100" })`** is the
  capability-string call form. It carries a single params value
  (commonly a struct literal) and becomes a `RemoteCall` with the
  capability string `"warehouse.get_stock"`. The host resolves it and
  resumes the workflow with the result (chapter 12).
- **`inventory.reserve({ ... })`** is the imported-module call form.
  After `import inventory;`, a `module.func(args)` call becomes a
  `RemoteCall` with the capability string `"inventory.reserve"`.
- Both forms yield a `RemoteCall` from `Vm::step`; the workflow pauses
  until the host calls `resolve_remote_call`.

---

## 16.6 Emitting events

```sol
workflow "emit-events" {
    let amount: int = 1500;
    if (amount > 1000) {
        emit "high_value_order";
    } else {
        emit "standard_order";
    }
}
```

### Annotations

- **`emit "high_value_order";`** emits a named event. The event name
  is a string literal. `emit` is a statement, not an expression.

---

## 16.7 Sources cited

- Canonical crate: `sol/src/lexer.rs`, `sol/src/parser.rs`,
  `sol/src/ast.rs`, `sol/src/compiler.rs`, `sol/src/vm.rs`,
  `sol/src/value.rs`, `sol/src/workflow.rs`.
- Editor bridge: `compiler-wasm/src/lib.rs`.
- Cross references: chapters 04 to 14 throughout.
