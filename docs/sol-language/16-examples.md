# 16 — Examples

> **Status:** Substantive (commit 4). Annotated walkthroughs of
> canonical sample programs. Each walkthrough cross-references the
> chapter(s) that explain the rules a given construct relies on.

This chapter is the guided tour. The lookup index of every
fixture lives in [`EXAMPLES.md`](./EXAMPLES.md).

---

## 16.1 `retest.sol` — minimal viable program

```sol
function sub_func(x: int) -> int {
    return x * 2;
}

function start() -> int {
    let y: int = sub_func(9);
    print(y);
}
```

### Annotations

- **Line 1.** Function declaration; one `int` parameter; `int`
  return. Chapter 05 §5.1.
- **Line 2.** `return x * 2;` — `*` is multiplicative, binds tighter
  than additive (chapter 08 §8.6); the result type matches the
  parameter type (chapter 04 §4.2.1).
- **Line 5.** Entry function `start` (chapter 05 §5.6).
- **Line 6.** `let y: int = sub_func(9);` — declared type, call
  expression as initializer. Forward declarations are unnecessary
  here because the analyzer's two-pass design registers all
  top-level functions before any body is walked (chapter 05 §5.5).
- **Line 7.** `print(y);` — one argument; dispatched to `PrintInt`
  at the bytecode (chapter 13 §13.1).
- **Missing.** This `start` has no `return;`. The function is
  declared `-> int` but the analyzer doesn't enforce the return
  path (chapter 05 §5.1). The top-of-stack value at function exit
  is undefined; idiomatic SOL would add `return 0;`.

---

## 16.2 `jjsi.sol` — struct + helper + start

```sol
function is_high(val: int, limit: int) -> bool {
    if val > limit {
        return true;
    }
    return false;
}

struct Person {
    name: str,
    age: int,
}

function print_person(p: Person) {
    print(p.name);
    print(p.age);
}

function start() -> int {
    let p: Person = Person {
        name: "evan",
        age: 19,
    };
    print_person(p);
    return 0;
}
```

### Annotations

- **Lines 1–6.** A pure helper. Note the `if val > limit { return
  true; } return false;` shape — there is no `else` because the
  early `return` makes the trailing `return` unreachable when
  the condition is true.
- **Lines 8–11.** Struct declaration; field types are
  primitives. Field-order is name-keyed (chapter 09 §9.1).
- **Lines 13–16.** Function consumes a `Person` by reference
  (chapter 14 §14.6). Two `print` calls — one per value, since
  only the first argument of a `print` is emitted (chapter 13
  §13.1).
- **Lines 19–22.** Struct literal with field-order matching the
  declaration. The literal could equally read `Person { age: 19,
  name: "evan" }` — fields are by name (chapter 09 §9.2).
- **Line 24.** `return 0;` — idiomatic end of `start`.

---

## 16.3 `s1.sol` — small orchestration

```sol
import EdgeRouter.SecurityControl.AuthApp.ValidateToken.Expiration as TokenTimeout;
import GlobalRouter.InventoryControl.WarehouseApp.GetStock.Level as StockLevel;

enum AppHealth {
    Offline,
    Initializing,
    Stable = 200,
    Overloaded = 503,
}

struct ProcessNode {
    id: int,
    threshold: float,
    tag: char,
    service_name: str,
    is_active: bool,
    metrics: [4]int,
}

function start_service(name: str) {
    print("started service:");
    print(name);
}
function stop_service(name: str) {
    print("stopped service:");
    print(name);
}

function verify_capacity(node: ProcessNode, current: float) -> AppHealth {
    if current > node.threshold {
        return AppHealth::Overloaded;
    } else {
        if node.is_active {
            return AppHealth::Stable;
        } else {
            return AppHealth::Initializing;
        }
    }
}

function orchestrate_service(request_id: int) -> int {
    let limit: float = 90.5;
    let identity: char = 'S';
    let label: str = "Inventory_Orchestrator";
    let data_history: [4]int = [10, 22, 15, 30];

    let current_node: ProcessNode = ProcessNode {
        id: request_id,
        threshold: limit,
        tag: identity,
        service_name: label,
        is_active: true,
        metrics: data_history,
    };

    let status: AppHealth = verify_capacity(current_node, 85.2);

    if status == AppHealth::Stable {
        start_service(current_node.service_name);
        return 1;
    } else {
        if status == AppHealth::Overloaded {
            stop_service(current_node.service_name);
            return 0;
        } else {
            return 2;
        }
    }
}

function inc(x: int) -> int {
    return x + 1;
}

function start() {
    print(orchestrate_service(0));
}
```

### Annotations

- **Lines 1–2.** `import … as …;` syntax (chapter 12 §12.3). At
  the analyzer level these only bind `TokenTimeout` and
  `StockLevel` as `Void` variables. They serve as comments today.
- **Lines 4–9.** Enum with mixed implicit and explicit values. The
  parser-level iota would map `Offline → 0, Initializing → 1,
  Stable → 200, Overloaded → 503`. **At runtime, the bytecode
  uses `first_char % 10` instead** (chapter 10 §10.5) — so
  `Offline → 79 % 10 = 9`, `Initializing → 73 % 10 = 3`,
  `Stable → 83 % 10 = 3`, `Overloaded → 79 % 10 = 9`. Multiple
  pairs of variants collide at runtime; the `if status ==
  AppHealth::Stable` and `if status == AppHealth::Overloaded`
  checks therefore don't behave as the source reads. **This is a
  good illustration of why T9002 is a real-world bug, not a
  theoretical one.**
- **Lines 11–18.** Struct with a fixed-size `[4]int` field. Note
  the bytecode treats `[4]int` and `[]int` interchangeably for
  type-equality purposes (chapter 04 §4.6).
- **Lines 20–27.** Two void-returning helper functions; each makes
  two `print` calls. Idiomatic — one `print` per value.
- **Lines 29–39.** Nested `if`/`else` inside a function that
  returns an enum. The two-level nesting is the language's
  workaround for the absence of `match`.
- **Lines 41–67.** Composing struct literal, call, and conditional
  return. The `let current_node: ProcessNode = ProcessNode { … }`
  pattern is the canonical way to build a typed record before
  threading it through helpers.

### Caveat for runtime behavior

Because of T9002 (enum-variant hash collision) and T9003 (print
first-arg-only), this program's *observable behavior* differs
from what the source reads:

- Comparisons against `AppHealth::Stable` / `AppHealth::Overloaded`
  may match unintended variants.
- The `print("started service:"); print(name);` pattern works as
  expected — that's two separate statements.

Treat `s1.sol` as a structural example, not a behavioral
specification.

---

## 16.4 `test_control.sol` — control-flow exhaustive

See the full source in `reference/sol files/test_control.sol`. This
fixture is a regression harness rather than a single program;
the relevant patterns are:

- `if (cond) { return X; }` followed by `return Y;` — the
  early-return pattern (chapter 07 §7.4).
- `while i < N { … i = i + 1; }` — counter loop (chapter 07 §7.2).
- `for item in [list] { … }` and `for item in empty_array { … }`
  — the iteration variable inherits the element type (chapter 11
  §11.5).
- `if ((true || false) && !false) { … }` — combined logical ops,
  non-short-circuiting (chapter 08 §8.3).

Use this fixture as a "what shape does my control flow want?"
reference; pattern-match against the test name that most closely
describes your scenario.

---

## 16.5 `test_struct.sol` — struct exhaustive

See the full source in `reference/sol files/test_struct.sol`. Most
useful patterns:

- `struct Empty {}` — empty struct (`test_empty_struct`).
- `struct Nested { p: Point, label: str }` — struct-in-struct
  (`test_struct_in_struct`).
- `Point { y: 99, x: 11 }` — field-order doesn't matter
  (`test_field_order`).
- `p.x = 100;` — field mutation (`test_mutate_field`).
- `function test_struct_in_func(p: Point) -> int { return p.x + p.y; }`
  — struct pass-through (`test_pass_struct`). Mutation
  visibility through the parameter is reference-semantics by
  default (chapter 14 §14.6).

---

## 16.6 Reading the larger fixtures

Two larger fixtures — `gemini_long.sol` and `largemini.sol` —
exercise the broader surface end-to-end. They are useful as
*self-tests* after a documentation change: anything in those
files that you can't trace back to a chapter rule is either a
documentation gap or a fixture quirk worth flagging.

When reading either:

1. Start at `start` — the conventional entry. Walk every call it
   makes.
2. For each helper, identify the chapter that explains its core
   construct (call → chapter 05, branch → chapter 07, struct
   literal → chapter 09, etc.).
3. Cross-check enum and `print` behavior against T9002 / T9003 in
   the error reference. These two real-world bugs distort the
   observable behavior of any program that uses them.

---

## 16.7 Sources cited

- Fixtures: `retest.sol`, `jjsi.sol`, `s1.sol`,
  `test_control.sol`, `test_struct.sol`, `gemini_long.sol`,
  `largemini.sol`
- Cross-references: chapters 04 – 14 throughout
- [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) — T9002, T9003
