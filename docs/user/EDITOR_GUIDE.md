# Editor Guide

The SolFlow editor has four main areas:

```
┌────────────────────────────────────────────────────────────────┐
│  Toolbar  · function tabs · run · sol man · help              │
├──────────────┬─────────────────────────────────┬───────────────┤
│              │                                 │               │
│   Sidebar    │           Canvas                │  Source       │
│              │   (graph editing area)          │  preview      │
│  - palette   │                                 │   (live SOL)  │
│  - types     │                                 │               │
│  - imports   │                                 │               │
│              │                                 │               │
├──────────────┴─────────────────────────────────┴───────────────┤
│  Diagnostics drawer (collapsible)                              │
└────────────────────────────────────────────────────────────────┘
```

## Toolbar

| Button | What |
|---|---|
| **Function tabs** | Switch between functions in the current workflow. Right-click a tab to rename / delete. |
| **Run** | Open the Run modal — compiles + executes via canonical SOL VM. |
| **Sol Man** | Open the LLM-assisted generation modal. Requires BYO API key. |
| **Help** | Quick keyboard shortcuts + about. |

## Sidebar

Three tabs:

- **Palette** — drag node kinds onto the canvas. Hover any
  palette item for a tooltip explaining what it does.
- **Types** — declare structs + enums for the current workflow.
- **Imports** — declare `import "path" as alias;` statements.

## Canvas

The graph editor. Built on Vue Flow.

| Action | How |
|---|---|
| Add a node | Drag from palette, OR right-click → context menu, OR `/` to open the search palette |
| Wire control flow | Drag from a node's dark right-side port to another node's left port |
| Wire data | Drag from a colored output port to a matching-color input port |
| Inline expression | Click a port's "expr" field in the Inspector and type SOL |
| Move nodes | Drag (multi-select with Shift+drag) |
| Delete | Select + Backspace, OR right-click → Delete |
| Frame a region | Add a Frame node (palette → Frame); resize via corner handles |
| Add a note | Add a Note node (palette → Note); free-text annotation |

## Source preview pane

Two modes:

**Read-only (default).** The pane lives-update with the
graph-derived SOL source. Click **Copy** to grab the source.

**Edit mode.** Click **Edit**. The buffer detaches from live
updates. While editing:

- The Compiler Diagnostic Panel below the editor shows
  real-time lexer / parser / analyzer / codegen errors. Click
  any diagnostic to scroll the cursor to its source range.
- **Import to graph** parses the buffer and replaces the visual
  graph. An Import Report modal shows what landed as Full /
  Partial / Source-only / Unsupported.
- **Reset to graph** discards the buffer and returns to live
  preview.
- **Download .sol** exports the buffer as a file.

Edit mode is **detached** by design — the buffer doesn't write
back to the graph until you explicitly import. See the [Sync model](../sol-language/SYNC_MODEL.md).

## Run modal

Opens via the toolbar **Run** button. Auto-runs each time it
opens.

Three tabs:

- **Output** — print lines, return value, runtime error (if any),
  with click-to-source / click-to-canvas links on errors
- **Trace** — every source range the VM executed, in order;
  each row clickable to focus source pane or canvas node
- **Generated SOL** — the canonical SOL source that was compiled
  + run

Status bar shows compile-stage state ("completed" / "runtime
error" / "compile failed") + VM step count.

External calls (`ext function ... at <url>`) are blocked in
browser simulation — the run reports a structured
`ExtCallBlocked` error rather than faking a network success.

## Diagnostics drawer

Collapsible drawer at the bottom of the canvas. Shows
graph-level validation issues:

- Missing required inputs on wired nodes
- Type mismatches on data edges
- Branch-termination warnings
- Enum first-character collisions (T9002)

Click a diagnostic to focus the offending node on the canvas.

The drawer's diagnostics are SEPARATE from the
compiler-diagnostic panel inside the source pane:

- Drawer: graph-shape validation (TS-side)
- Source-pane panel: canonical compiler diagnostics (WASM)

Both should be empty for a clean workflow.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `/` | Open node search palette |
| `Esc` | Close modal / popover |
| `Cmd+Z` / `Ctrl+Z` | Undo |
| `Cmd+Shift+Z` / `Ctrl+Y` | Redo |
| `Cmd+C` / `Ctrl+C` | Copy selected nodes |
| `Cmd+V` / `Ctrl+V` | Paste |
| `Backspace` / `Delete` | Delete selection |

(Vue Flow handles pan/zoom via mouse + trackpad.)

## What's NOT in the editor (yet)

- Inline node-level execution stepping (per-step canvas highlight tracking canonical execution)
- Click-to-source on analyzer diagnostics with per-leaf-expression
  precision — currently clicks land at the enclosing block span
- Multi-cursor source editing
- Workflow templates beyond the bundled samples

These are intentional non-goals for the current product surface.
The [`docs/sol-language/B_RELEASE_NOTES.md`](../sol-language/B_RELEASE_NOTES.md)
file documents what's deferred and why.
