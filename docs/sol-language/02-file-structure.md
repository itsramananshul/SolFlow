# 02 — File Structure

This chapter describes what a `.sol` file looks like at the top level,
what can appear there, how execution is selected, and how the lexer
handles comments and whitespace.

## What this chapter answers

- What does the top level of a `.sol` file look like?
- What can appear at the top level, and is there a required order?
- How is the unit of execution selected?
- What does the lexer treat as comments and whitespace?

## 2.1 The top-level grammar

A SOL file is a flat sequence of top level items. The parser loops over
`parse_top_level` (`sol/src/parser.rs`) and dispatches on the leading
keyword. The five item forms are exactly:

| First token | Item |
|---|---|
| `import` | import declaration |
| `fn` | function declaration |
| `struct` | struct declaration |
| `enum` | enum declaration |
| `workflow` | workflow declaration |

There are no nested modules and no package header. Anything else at the
top level is a parse error (a plain string message; there are no error
codes).

```sol
import "send" from discord;

struct Message {
    channel: str;
    body: str;
}

enum Priority {
    Low;
    High;
}

fn build(text: str) <- Message {
    return Message { channel: "general", body: text };
}

workflow "notify" {
    let m: Message = build("ship it");
    call("discord.send", m);
}
```

## 2.2 The unit of execution

SOL has no `start` entry function. The runnable unit is a `workflow`. Each
`workflow "name" { ... }` declares an independently executable unit
identified by its string literal name. The host selects a workflow by name
and runs it through `WorkflowExecutor` (`sol/src/workflow.rs`). A file may
contain multiple workflows.

If the bridge is asked to run a source with no workflow, it reports the
`E_NO_WORKFLOW` diagnostic (`compiler-wasm/src/lib.rs`).

## 2.3 Ordering

The top level is order free in the sense that the parser accepts the five
item forms in any order. Functions, structs, enums, and workflows can be
declared in whatever sequence the source uses; the compiler resolves names
when it lowers each workflow.

## 2.4 Comments and whitespace

Comments start with `#` and run to the end of the line
(`sol/src/lexer.rs`). There are no block comments and no `//` line
comments.

```sol
# this is a comment
workflow "demo" {
    print("hi");   # trailing comment
}
```

Whitespace (spaces, tabs, newlines, carriage returns) separates tokens and
is otherwise insignificant; the language is brace delimited, not
indentation sensitive. The lexer tracks no line or column information, so
errors do not carry spans today.

Note that comments live only in the source text. The AST has no comment
nodes, so the formatter (`sol/src/format.rs`) drops comments on a format
round trip.

## 2.5 Imports and external Actions

`import` declarations name external modules whose Actions a workflow can
call. Two forms exist:

```sol
import discord;              # import the whole module
import "send" from discord;  # import a named Action from a module
```

Importing a module does not pull in code at parse time. It records the
module name so that calls such as `discord.send(params)` or
`call("discord.send", params)` can be resolved by the host at runtime as
`RemoteCall`s. What an Action means to the host is covered in the imports
and controllers chapter.

## Sources

- `sol/src/parser.rs` — `parse_top_level`, `parse_import`,
  `parse_function`, `parse_struct`, `parse_enum`, `parse_workflow`
- `sol/src/lexer.rs` — comment and whitespace handling, keyword set
- `sol/src/ast.rs` — top level item shapes
- `sol/src/workflow.rs` — the workflow executor
- `compiler-wasm/src/lib.rs` — `E_NO_WORKFLOW` when no workflow is present
