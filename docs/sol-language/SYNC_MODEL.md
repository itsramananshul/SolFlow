# Source and Graph Sync — explicit-action model

This document records the architecture for moving between the SOL
source editor and the visual graph.

## The rule

> **Synchronization between source and graph is always an explicit
> user action, never a watcher.**

There is no "live two-way binding" between the SOL source editor and
the visual graph. Every transfer between the two representations
crosses a clearly-named boundary that the user triggered.

The reason: source and graph are **not isomorphic**. The graph loses
comment text, expression parenthesization choices, and whitespace
(comments cannot survive at all — the canonical AST in
`sol/src/ast.rs` has no comment nodes). The source loses node
positions, frame groupings, notes, and visual layout. Pretending they
synchronize bidirectionally in real time produces silent data loss in
both directions.

Honest UX beats magical UX.

## The four sanctioned transfers

| Direction | Trigger | Behavior |
|---|---|---|
| **Graph to Source (preview)** | Live, watched | The SourcePreview pane re-emits SOL via `src/emit/emit.ts` whenever the graph changes. This is read-only display, not a sync — the source is *derived* from the graph and the graph is canonical. |
| **Graph to Source (edit)** | User clicks "Edit" | Source pane snapshots the current emit and enters a **detached** buffer. Subsequent graph changes do NOT touch the buffer. Source is now editable, and the graph and source are explicitly out of sync until one direction wins. |
| **Source to Graph (import)** | User clicks "Import to graph" | Parses the buffer through `compiler-wasm` (`parse_source_json`), walks the resulting `Program` AST in `src/graph/import/importer.ts`, builds a fresh workflow, and **replaces** the current graph. Surfaces an `ImportReport` describing every classification decision. Edit mode exits; the graph is canonical again. |
| **Source to Graph (cancel edit)** | User clicks "Reset to graph" or "Done editing" without importing | The buffer is discarded. The graph stays as-is; live preview resumes. |

There is intentionally **no "merge" path** in the matrix above.
Merging an edited buffer back into an existing graph is a non-problem
when the importer always replaces — the user picks the direction, and
the loser is explicit about what it lost.

## Conflict resolution

There are no conflicts in this model. Each transfer is a one-way
overwrite at a user-named moment.

What the user might call a "conflict" — "I edited source AND I changed
the graph" — becomes a choice:

- Import to graph: graph loses, source wins.
- Reset: source loses, graph wins.

Either way, the loser is destroyed at a clearly-named click. No silent
partial merges.

## What "detached" means

When the user is in edit mode and the buffer differs from the last
graph-derived emit, the source is **detached**. This state is rendered
explicitly:

- An amber banner above the editor saying "Editing in detached mode."
- The detached-versus-not state drives the banner intensity (deeper
  amber when the buffer has diverged from the emit).
- Live compiler diagnostics still run on the buffer. The bridge
  (`compiler-wasm/src/lib.rs`) parses, compiles, and optionally runs
  exactly the text it is handed; it has no notion of graph state. The
  diagnostics it can return are the five bridge codes (`E_PARSE`,
  `E_CODEGEN`, `E_NO_WORKFLOW`, `E_RUNTIME`, `ICE0001`); there is no
  type-checker, so type mismatches do not surface here.
- The user can copy the buffer, download it, import it, reset it, or
  do nothing. Closing edit mode without importing or resetting is fine
  — the buffer simply gets discarded.

## What this model is NOT

| Anti-feature | Why we do not do it |
|---|---|
| Live two-way binding | Lossy in both directions; impossible without inventing semantics. |
| AST-diff merge | Diffing two parser ASTs and reconciling node positions, frames, and notes is exponential work for marginal benefit. |
| Watching the buffer to update the graph as you type | Garbage in, garbage out: every intermediate keystroke produces a half-parsed AST. Users do not want that grafted onto their canvas. |
| Watching the graph to "highlight unsaved source changes" | The graph IS canonical when not editing. There is no such state. |

## What changes when round-trip is improved

Round-trip can get better — see `IMPORT_COMPATIBILITY.md` for the
known asymmetries — but the **sync model does not change**. Even a
perfect round-trip would not justify hiding the import/reset decision
from the user.

The eventual ideal: a user edits source, hits "Import to graph", sees
an empty `ImportReport` (everything imported as `full`), clicks Done,
and the cycle is invisible. The buttons are still there, the action is
still explicit, but the cost approaches zero for the happy path.

## What is pending

- **Source-span attachment** on imported nodes. The wire protocol
  already carries an `instruction_spans` sidecar
  (`src/runtime-host/types.ts`, `compile_for_wire_json` in
  `compiler-wasm/src/lib.rs`) so the controller can source-map runtime
  errors. The editor side can use the same spans to highlight the
  source range that produced a node when you click it on the canvas.
  The crate's lexer does not yet track spans (`sol/src/lexer.rs` has
  no line/column tracking), so node-level spans are coarse today.
- **Click-to-source / click-to-graph** navigation between panes.
  Cheap once finer spans land.
- **Assignment-statement import** — the canonical parser does not
  currently parse assignment statements (`parse_stmt` in
  `sol/src/parser.rs` produces only `Let / If / While / For / Return /
  Emit / Expr`), so `assign` / `fieldSet` / `indexSet` nodes are never
  produced from real source. The importer's handlers exist for
  forward-compatibility but are unreachable from today's parser.

The sync model above accommodates all of these without changing shape.
