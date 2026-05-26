# 13 — Built-ins and Standard Library

> **Status:** Substantive (commit 4). Sourced from
> `analyzer.rs:70–79, 340–373`, `bytecode.rs:115–122, 423–453`,
> `vm.rs:309–336, 339–476`.

SOL's standard library is intentionally tiny — six identifier names
are bound at runtime by the compiler itself, not by the host:
`print` plus five RPC helpers. Everything else the program needs
must come from `ext function` declarations supplied by the host
(chapter 12).

This chapter enumerates every name the compiler treats specially,
the signature it expects, and the runtime behavior it produces.

---

## 13.1 `print`

```sol
print(value)
```

Prints `value` to stdout and a newline. The dispatch on `value`'s
runtime type is done by the bytecode emitter
(`bytecode.rs:423–432`):

| Argument type | Bytecode op | Output format |
|---|---|---|
| `int` | `PrintInt` | decimal i64 |
| `bool` | `PrintInt` | `1` for true, `0` for false |
| `float` | `PrintFloat` | Rust `f64` default formatter |
| `char` | `PrintChar` | the character itself |
| `str` | `PrintString` | the string contents |
| any other | `PrintInt` | the underlying `u64` slot |

### Important: only the first argument is printed

The analyzer accepts any number of arguments of any types
(`analyzer.rs:340–345`), but the bytecode emitter compiles **only
`args[0]`** (`bytecode.rs:425`). A call written `print(a, b, c)`
runs the side effect of evaluating `a, b, c` (well — only `a` per
the emitter; `b` and `c` are dropped silently), then emits one
print of `a`.

**Use one argument per `print` call.** To print multiple values,
emit multiple statements:

```sol
print("count:");
print(count);
```

This is a known compiler discrepancy between the analyzer's
permissive shape and the emitter's restrictive emission. It is
logged in [`ERROR_REFERENCE.md`](./ERROR_REFERENCE.md) as `T9003`.

### Newline behavior

The VM's print ops use Rust's `println!` (`vm.rs:309–336`) — each
call prints a trailing newline. There is no built-in form that
prints without a newline. There is no formatted-print built-in; if
you need composition, do it in the host via an `ext function`.

### `print` is `Void`

`print` returns `Void` in the analyzer's view, but at the bytecode
level each `Print*` op pushes `0` onto the stack
(`vm.rs:309–336`) — this keeps stack-discipline simple. The
pushed `0` is popped by the surrounding statement-expression
handling and is not visible to the program.

---

## 13.2 RPC helpers

Five built-in identifiers wrap a small serialization protocol the
compiler ships with. They are registered in the analyzer's global
scope before any source is checked (`analyzer.rs:70–79`):

| Name | Signature | Bytecode op |
|---|---|---|
| `rpc_request` | `(name: str, args: array) -> str` | `SerializeRequest(elem_type)` |
| `rpc_response` | `(value: T) -> str` | `SerializeResponse(value_type)` |
| `rpc_name` | `(msg: str) -> str` | `DeserializeRequestName` |
| `rpc_args` | `(msg: str) -> str` | `DeserializeRequestArgs` |
| `rpc_data` | `(msg: str) -> str` | `DeserializeResponseData` |

### `rpc_request(name, args)`

Builds a JSON message of the form

```json
{ "type": "request", "name": "<name>", "args": [ … ] }
```

(`vm.rs:339–383`). Returns the JSON as a `str`. The second
argument's array element type drives how each element is encoded —
strings become JSON strings; numbers become JSON numbers; chars
become single-character strings; bools become JSON bools. Anything
else falls back to a Debug-formatted string.

### `rpc_response(value)`

Builds a JSON message of the form

```json
{ "type": "response", "data": <value-as-json> }
```

Returns the JSON as a `str`. The serialization follows the same
type → JSON mapping as `rpc_request`.

### `rpc_name(msg)`, `rpc_args(msg)`, `rpc_data(msg)`

Parse a JSON message back. `rpc_name` returns the request's name
field; `rpc_args` returns the args field as a JSON string;
`rpc_data` returns the response's data field as a JSON string.

### When to use the RPC helpers

These helpers exist to let SOL programs participate in a simple
request/response wire protocol without going through `ext function`
plumbing. In practice the more common pattern is to declare
`ext function` calls that the host already knows how to dispatch;
the RPC helpers are useful when you want to handcraft a message
inside SOL itself.

---

## 13.3 What the language does **not** ship

SOL's standard library outside of the six identifiers above is
empty. The following are common ask-fors that simply are not
available in the language:

| Asked-for | Status |
|---|---|
| `len(array)` / `array.length` | Not exposed at the source level. The bytecode has an `ArrayLen` op but it is only emitted as part of the implicit `for-in` desugar (`bytecode.rs:272–328`); there is no syntax that reaches it. Expose array lengths via the host (`ext function len(a: []T) -> int;`) if you need them. |
| String slicing / length / concat | Not exposed. `bytecode.rs:681–689` has `ConcatStr` and `EqStr` ops, but `ConcatStr` cannot be reached because the analyzer rejects `str + str` (chapter 04 §4.2.4). `EqStr` is reachable via `str == str`. Anything more complex must go through `ext function`. |
| Math functions (`sqrt`, `abs`, `min`, `max`, `floor`, …) | Not in the language. Expose via `ext function`. |
| I/O beyond `print` | Not in the language. Use `ext function`. |
| Time / date | Not in the language. Use `ext function`. |
| Hashing / cryptography | Not in the language. Use `ext function`. |
| JSON parsing in user code | `rpc_*` helpers can build / parse a *specific* request/response envelope; arbitrary JSON parsing is not in the language. |
| Throw / try / catch | Not in the language. Errors that originate in the program become runtime panics; errors that originate in `ext function` calls are the host's responsibility to surface (typically as the return value or via a host-defined error channel). |

The discipline of a small standard library is intentional. SOL is
designed to be a thin orchestration layer; anything beyond that
belongs in the host.

---

## 13.4 Sources cited in this chapter

- `analyzer.rs:70–79` — RPC builtin signatures
- `analyzer.rs:340–373` — `print` / `rpc_*` analyzer paths
- `bytecode.rs:115–122` — built-in return-type registration
- `bytecode.rs:423–453` — built-in dispatch at codegen
- `bytecode.rs:272–328` — `for-in` desugar (uses `ArrayLen`)
- `bytecode.rs:681–689` — `ConcatStr` / `EqStr` codegen
- `vm.rs:309–336` — `print*` runtime
- `vm.rs:339–476` — `Serialize*` / `Deserialize*` runtime
- Fixtures: `s1.sol`, `s2.sol`, `retest.sol`, `largemini.sol`
