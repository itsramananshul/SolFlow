# `src/emit/` — Graph → SOL Exporter

> **PHASE A — TEMPORARY IMPLEMENTATION.**
>
> This module exists because the Rust/WASM compiler bridge isn't built yet.
> See `reference/SOL_CRATE_IDE_READINESS_PLAN.md` §3.7 and §6 step 2.7 for
> the replacement plan.

## What this does

`emit(workflow: SolWorkflow): { source: string; warnings: string[] }`

Walks the workflow graph and produces canonical SOL source. One-way only —
Phase A does not parse `.sol` back into a graph; that requires the Rust
parser via WASM.

## Why TypeScript

The Phase B plan replaces this entire file with a WASM call:

```ts
import { graph_to_ast, emit_sol } from '@solflow/sol-wasm';

export function emit(graph: SolWorkflow) {
  const ast = graph_to_ast(graph);
  const source = emit_sol(ast);
  return { source, warnings: [] };
}
```

Until that WASM module exists (Phase B), this hand-rolled TypeScript
walker stands in. The public signature stays the same in both phases, so
`SourcePreview.vue` and the export-`.sol` button don't change when the
swap happens.

## Conventions the emitter follows

- 2-space indent.
- 4 trailing-semicolon statements: `let`, `assign`, `print`, `return`,
  `fieldSet`, `indexSet`, `call`.
- Parens always around binary expressions: `(a + b)`. Sidesteps
  precedence-emit headaches. Phase B's Rust pretty-printer can be
  smarter about omitting redundant parens.
- Parens around `if` / `while` conditions to match the canonical SOL
  test corpus style.
- Empty function body emits as `function name() {\n}`.
- Missing required inputs emit as `/* missing */` placeholders, and a
  warning is collected — never crashes.
