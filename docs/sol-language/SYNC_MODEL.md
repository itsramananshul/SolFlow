# Source ↔ Graph Sync — explicit-action model (B.9 early)

Status as of B.7 c22 (2026-05-27). Not yet implemented as
bidirectional live sync — this document records the architectural
**philosophy** we'll build on.

## The rule

> **Synchronization between source and graph is always an explicit
> user action, never a watcher.**

There is no "live two-way binding" between the SOL source editor
and the visual graph. Every transfer between the two
representations crosses a clearly-named boundary that the user
triggered.

The reason: source and graph are **not isomorphic**. The graph
loses comment text, expression parenthesization choices, and
whitespace. The source loses node positions, frame groupings,
notes, and visual layout. Pretending they synchronize bidirectionally
in real-time produces silent data loss in both directions.

Honest UX > magical UX.

## The four sanctioned transfers

| Direction | Trigger | Behavior |
|---|---|---|
| **Graph → Source (preview)** | Live, watched | The SourcePreview pane re-emits SOL whenever the graph changes. This is read-only display, not a sync — the source is *derived* from the graph and the graph is canonical. |
| **Graph → Source (edit)** | User clicks "Edit" | Source pane snapshots the current emit and enters a **detached** buffer. Subsequent graph changes do NOT touch the buffer. Source is now editable, and the graph and source are explicitly out of sync until one direction wins. |
| **Source → Graph (import)** | User clicks "Import to graph" | Parses the buffer, walks AST, builds a fresh workflow, and **replaces** the current graph. Surfaces an `ImportReport` describing every classification decision. Edit mode exits; the graph is canonical again. |
| **Source → Graph (cancel edit)** | User clicks "Reset to graph" or "Done editing" without importing | The buffer is discarded. The graph stays as-is; live preview resumes. |

There is intentionally **no "merge" path** in the matrix above.
Merging an edited buffer back into an existing graph is a
non-problem when the importer always replaces — the user picks the
direction, and the loser is explicit about what it lost.

## Conflict resolution

There are no conflicts in this model. Each transfer is a one-way
overwrite at a user-named moment.

What the user used to call a "conflict" — "I edited source AND I
changed the graph" — becomes a choice:

- Import → graph loses, source wins.
- Reset → source loses, graph wins.

Either way, the loser is destroyed at a clearly-named click. No
silent partial merges.

## What "detached" means

When the user is in edit mode and the buffer differs from the
last graph-derived emit, the source is **detached**. This state
is rendered explicitly:

- An amber banner above the editor saying "Editing in detached mode."
- The detached-vs-not state drives the banner intensity (deeper
  amber when the buffer has diverged from the emit).
- Live compiler diagnostics still run on the buffer (the WASM
  compiler doesn't care about graph state; it parses what it's
  given).
- The user can copy the buffer, download it, import it, reset it,
  or do nothing. Closing edit mode without importing or resetting
  is fine — the buffer simply gets discarded.

## What this model is NOT

| Anti-feature | Why we don't do it |
|---|---|
| Live two-way binding | Lossy in both directions; impossible to do without inventing semantics. |
| AST-diff merge | Diffing two parser ASTs and reconciling node positions / frames / notes is exponential work for marginal benefit. |
| Watching the buffer to update the graph as you type | Garbage-in-garbage-out: every intermediate keystroke produces a half-parsed AST. Users don't want that grafted onto their canvas. |
| Watching the graph to "highlight unsaved source changes" | The graph IS canonical when not editing. There's no such state. |

## What changes when round-trip is improved

Round-trip can get better — see `IMPORT_COMPATIBILITY.md` for the
known asymmetries — but the **sync model doesn't change**. Even a
perfect round-trip wouldn't justify hiding the import/reset
decision from the user.

The eventual ideal: a user edits source, hits "Import to graph",
sees an empty `ImportReport` (everything imported as `full`),
clicks Done, and the cycle is invisible. The buttons are still
there, the action is still explicit, but the cost approaches zero
for the happy path.

## What's pending

- **Source-span attachment** on imported nodes (`meta.sourceSpan`).
  Lets the editor highlight the source range that produced a
  node when you click it on the canvas. Compiler-side prerequisite:
  the lexer + parser need to attach spans at every emit site
  (`compiler/REMAINING_PANICS.md` tracks this).
- **Click-to-source / click-to-graph** navigation between panes.
  Cheap once spans land.
- **`fieldSet` / `indexSet` import** (B.8). Lifts the importer's
  `unsupported` rating on complex assignments.
- **Smarter ordered structures** — `IndexMap` for struct fields
  to make user-authored field order round-trip safe.

The sync model above accommodates all of these without changing
shape.
