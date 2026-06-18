# 13 — Built-ins and Standard Library

> **Status:** Canonical. Sourced from `sol/src/vm.rs`
> (`exec_builtin`) and `sol/src/crypto.rs`.

SOL's standard library is intentionally tiny. The VM recognizes exactly
**four** built-in functions, dispatched by name in `Vm::exec_builtin`
(`sol/src/vm.rs`). There is no `import std`; these names are always
available. Everything else a workflow needs comes from external Actions
the host resolves (chapter 12) or from host-registered native helpers.

This chapter enumerates the complete built-in set and what the language
deliberately does not ship.

---

## 13.1 The complete built-in set

| Name | Signature | Behavior |
|---|---|---|
| `print` | `print(...)` variadic, returns Unit | Space-joins all arguments (using each value's display form), appends a single trailing newline, and writes the line to the captured output buffer. |
| `len` | `len(str \| array) <- int` | Length of a string (byte length) or array (element count). Errors on any other value. |
| `to_str` | `to_str(any) <- str` | The display form of any value as a string. |
| `type_name` | `type_name(any) <- str` | The runtime type name of any value. |

That is the entire built-in set. Names not in this table are not
built-ins; a `Call` to an unknown name (and no host-registered native of
that name) fails at runtime with `function '<name>' not found`.

### `print(...)`

`print` is variadic. It joins its arguments with single spaces, appends
one `\n`, and pushes the result into the VM's thread-local output buffer
(the host drains it with `take_output()` after a run; the browser sim
splits it into a `string[]`). It returns `Unit`.

```sol
workflow "demo" {
    let count = 3;
    print("count:", count);   # buffer receives "count: 3\n"
}
```

Unlike many languages, `print` here accepts any number of arguments and
prints all of them on one line. There is no no-newline variant and no
format-string built-in.

### `len(x) <- int`

```sol
let n = len("hello");       # 5
let m = len([1, 2, 3]);     # 3
```

`len` of a string returns its byte length; `len` of an array returns its
element count. Applying `len` to any other value is a runtime error.

### `to_str(x) <- str`

Returns the display string of any value. Useful for building messages:

```sol
let label = "id=" + to_str(42);   # "id=42"
```

(String `+` concatenation is what joins the two strings here; see
chapter 14.)

### `type_name(x) <- str`

Returns the runtime type tag of a value as one of:
`"bool"`, `"int"`, `"float"`, `"char"`, `"str"`, `"array"`, `"struct"`,
`"enum"`, `"unit"`, `"module"`, `"remote_ref"`.

```sol
let t = type_name([1, 2]);   # "array"
```

---

## 13.2 Crypto is exported, not a built-in

The crate ships ed25519 signing/verification and a sha512 digest in
`sol/src/crypto.rs` (`Keypair::sign`, `verify`, `sha512_digest`). These
are **Rust** functions exported by the crate. They are **not** SOL
built-ins and are not reachable by name from SOL source. A host that
wants to expose them must wrap them and register them with
`Vm::register_native(name, func)` (chapter 12 §12.4). Until a host does
so, calling, for example, `sha512(...)` from SOL fails with
`function 'sha512' not found`.

---

## 13.3 What the language does not ship

Outside the four built-ins above, the standard library is empty. Common
requests that are not in the language:

| Asked-for | Status |
|---|---|
| Math (`sqrt`, `abs`, `min`, `max`, `floor`, …) | Not built in. Expose via a host native or an external Action. |
| String slicing / search / case | Not built in. `len` and `+` (concat) are the only string operations; anything else goes through the host. |
| I/O beyond `print` | Not built in. Use an external Action. |
| Time / date | Not built in. Use an external Action. |
| Hashing / cryptography | Exported in `sol/src/crypto.rs` but not a built-in; host must register it as a native (§13.2). |
| JSON parsing in user code | Not built in. Build params as struct literals and let the host (de)serialize. |
| Throw / try / catch | Not in the language. A runtime error becomes `StepResult::Failed(string)` (chapter 15); there is no in-language recovery. |

The small surface is intentional. SOL is a thin orchestration layer;
anything beyond the four built-ins belongs in the host, reached as an
external Action or a registered native.

---

## 13.4 Sources cited in this chapter

- `sol/src/vm.rs` — `Vm::exec_builtin` (`print`, `len`, `to_str`,
  `type_name`), `register_native`, `take_output`
- `sol/src/crypto.rs` — exported `Keypair::sign`, `verify`,
  `sha512_digest` (host-wrappable, not built-ins)
