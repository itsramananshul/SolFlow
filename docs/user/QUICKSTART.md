# Quickstart

Build your first workflow in 5 minutes. Assumes the dev server
is running ([Install →](./INSTALL.md)).

## 1. Open a sample workflow

On first load, the canvas is empty. From the **Welcome** card,
click **"Open a sample"** and pick **Hello** — the simplest
example. The canvas populates with three nodes:

```
[ trigger: manual ] → [ print "hello world" ] → [ return 0 ]
```

The **Source preview** pane on the right shows the equivalent
SOL source, live-updated:

```sol
function start() -> int {
    print("hello world");
    return 0;
}
```

The source and the graph are two views of the same workflow.

## 2. Run it

Click the **Run** button (top toolbar). The Run modal opens
showing canonical execution output:

```
Output:
  hello world
return: 0
```

That output came from the canonical SOL VM compiled to WASM —
the same compiler/runtime SOL uses in production. The "approximate
JS interpreter" animations on the canvas are visualization only;
the displayed output is authoritative.

## 3. Edit the source directly

Click **Edit** on the source preview pane. Try changing
`"hello world"` to `"hello, " + name`. The compiler diagnostic
panel below the editor shows a real-time error:

```
[E1015] analyzer: attempting to make a function call on an
undefined name `name`
```

This is the canonical Rust analyzer running in your browser via
WASM — no fake JS-side reimplementation.

Fix the error: add `let name: str = "alice";` before the print.
Click **Import to graph**. The canvas updates with the new
`let` node, wired into the flow. An **Import Report** modal
shows what landed as Full / Partial / Source-only.

## 4. See the execution trace

Click **Run** again. The Run modal opens; switch to the **Trace**
tab. Each row shows a source range the VM executed:

```
#1  line 2:5    let name: str = "alice";
#2  line 3:5    print(("hello, " + name));
#3  line 4:5    return 0;
```

Click a `line N:C` link → jumps to the Generated SOL tab and
scrolls to that line. Click `→ canvas` (when present) → focuses
the corresponding node on the canvas.

That's the loop:

```
edit  →  see diagnostics live  →  import to graph  →  run  →  trace
```

## 5. Build something from scratch

From the welcome screen, click **New workflow**. Drag node kinds
from the left palette onto the canvas. Common starting moves:

- Drop a `trigger` (manual or timer) as the entry point
- Drop a `let` to bind a value
- Drop a `print` to see output
- Drop a `return` to terminate
- Wire control flow with the dark "next" port (right side of
  most nodes)
- Wire data via colored data ports (per type)

The source preview updates live as you build. When you've got
something that compiles, click **Run** to see canonical output.

## What's next

- **[Editor Guide](./EDITOR_GUIDE.md)** — every panel + every keyboard shortcut
- **[FAQ](./FAQ.md)** — common questions
- **[SOL Language Overview](../sol-language/01-overview.md)** — the language SolFlow generates + runs
- **[Sample workflows](../../src/samples/)** — open each from the Welcome card to see real patterns
