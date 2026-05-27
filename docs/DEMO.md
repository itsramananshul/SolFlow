# SolFlow in 5 minutes — Demo walkthrough

A scripted walkthrough showing SolFlow's headline capabilities.
Use this when demoing the product live, recording a screencast,
or taking screenshots.

## Prerequisites

```bash
npm install
npm run dev          # http://localhost:5173
```

The dev server takes ~5 seconds to start. The browser tab loads
in another ~1 second; the WASM compiler bundle lazy-loads on
first compile call (typically <500ms one-time).

## The 5-minute story

### Minute 1 — "What is SolFlow?"

On first load, the **Welcome screen** appears. Read the tagline:

> "Visual orchestration IDE for SOL."

Point out the three primary cards: **Generate with AI**,
**Open a sample**, **Start blank**.

Click **"Open a sample"** (the middle card, which auto-loads
the most-impressive sample — the 40-node Enterprise workflow).

> **Screenshot slot #1**: `docs/screenshots/01-welcome.png`
> Capture the full welcome screen with the three cards visible.

### Minute 2 — "Visual graph editor"

The Enterprise workflow loads with ~40 nodes across 5 framed
regions. Talk through what's on screen:

- Left **sidebar** — palette of every node kind (drag onto canvas)
- Center **canvas** — graph editor (Vue Flow); pan/zoom freely
- Right **source preview** — the SOL source SolFlow generates
  from the graph, live-updated
- Bottom **diagnostics drawer** — graph-level validation issues
  (should be empty for this sample)

Drag a node around the canvas. The source preview doesn't change
(only the graph's structure affects emit, not node positions).

> **Screenshot slot #2**: `docs/screenshots/02-canvas.png`
> Capture the canvas mid-screen showing the Enterprise sample's
> framed regions + the source pane on the right.

### Minute 3 — "Live compiler diagnostics"

Click **Edit** on the source preview pane (top-right of the
source pane). The buffer detaches from live graph updates;
**Edit Mode** badge appears.

Type something invalid. For instance, change
`function start() -> int` to `function start -> int` (removing
the parens).

Within 250ms, the **Compiler Diagnostic Panel** at the bottom
of the source pane shows:

```
parser:  E0005  expected left parenthesis after function name
```

Click the diagnostic row. The CodeMirror cursor jumps to the
offending position.

> **Screenshot slot #3**: `docs/screenshots/03-diagnostic.png`
> Capture the source pane with the diagnostic panel showing,
> with the error visible.

**Key talking point:** That diagnostic came from the canonical
Rust SOL compiler, compiled to WebAssembly and running in your
browser. No fake JavaScript reimplementation of language
semantics.

### Minute 4 — "Canonical execution"

Discard the broken edit (click **Reset to graph**). The buffer
restores. Now click the **Run** button in the toolbar.

The **Run modal** opens. The canonical SOL VM (compiled to WASM,
in `runtime/`) executes the bytecode.

Three tabs:

- **Output** — print lines + return value
- **Trace** — every source range the VM executed, click any to
  jump to source / canvas
- **Generated SOL** — the canonical source that was compiled

Click the **Trace** tab. Show:
- The step count badge
- Each step's `line:col` clickable link
- The "→ canvas" button on steps with graph mapping

Click a `line N:C` link → jumps to the Generated SOL tab. Click
a `→ canvas` button → switches function + focuses the node.

> **Screenshot slot #4**: `docs/screenshots/04-run-trace.png`
> Capture the Run modal's Trace tab populated with step rows.

**Key talking point:** This trace came from the canonical SOL
VM — same compiler/runtime SOL would use in production. The
animated canvas playback (if you see nodes light up) uses a
separate approximate JS interpreter for visualization only;
the displayed text output is authoritative.

### Minute 5 — "Source → graph import"

Close the run modal (Esc). Click **Edit** again. Open
`docs/sol-language/16-examples.md` in a new tab and copy a
small example. Paste it into the editor, replacing the existing
source.

Click **Import to graph**. The canvas updates with the
reconstructed nodes. The **Import Report modal** opens showing:

- Per-function classification (Full / Partial / Source-only /
  Unsupported)
- Per-statement counts
- Notices for any degraded constructs

> **Screenshot slot #5**: `docs/screenshots/05-import-report.png`
> Capture the import report modal with a multi-function summary
> table visible.

**Key talking point:** The importer is honest. Every degradation
surfaces as a notice; unsupported syntax is preserved as a
placeholder node carrying the original SOL text inline. Nothing
is silently dropped.

## End beats

Wrap with:

> "What you just saw runs entirely in your browser. The
> SolFlow editor is a Vue app; the compiler + runtime are
> canonical Rust compiled to WebAssembly — no backend, no
> auth, no server-side workflow store. Open source, MIT
> licensed."

## Recording tips

- Use 1280×800 or 1440×900 browser window for cleaner screenshots
- Run with `npm run dev` (not the production build) so you can
  see live-reload during edits if you make changes mid-demo
- Press `?` to surface the help modal with keyboard shortcuts +
  docs links — useful if the audience asks "how do I do X"
- The presentation mode (`P` key) hides editor chrome for a
  cleaner-feel demo
