# Screenshots

Placeholder directory for product screenshots referenced by
[`../DEMO.md`](../DEMO.md) and the project README.

Screenshots aren't committed yet — they need to be captured
from a running editor. This README catalogs what each slot
should show so anyone with the dev server running can refresh
them.

## Slots

| File | What to capture | Window size | Where in the editor |
|---|---|---|---|
| `01-welcome.png` | Full welcome screen with three primary cards (Generate / Sample / Blank) + sample-prompt chips visible | 1440×900 | First load, or click brand mark → "Show welcome" |
| `02-canvas.png` | Enterprise sample loaded; canvas mid-screen showing framed regions + source pane on the right | 1440×900 | After clicking "Open a sample" from the welcome screen |
| `03-diagnostic.png` | Source pane in edit mode with a parse error in the Compiler Diagnostic Panel visible | 1280×800 | In edit mode, intentionally type invalid syntax |
| `04-run-trace.png` | Run modal's Trace tab populated with step rows showing line:col + canvas links | 1280×800 | Click Run, switch to Trace tab |
| `05-import-report.png` | Import Report modal with multi-function summary table | 1280×800 | Edit source → Import to graph (using a multi-function sample's SOL) |

## How to capture

```bash
npm run dev
```

Open http://localhost:5173, set browser zoom to 100%, resize
window to the size noted in the table above. Use OS-native
screenshot tools:

- **Windows**: Win+Shift+S → save as PNG
- **macOS**: Cmd+Shift+4 → drag selection → automatic PNG to Desktop
- **Linux**: gnome-screenshot -a → save

Drop captured PNGs into this directory matching the filenames
in the table.

## Marketing / docs reuse

Once captured, these screenshots can be referenced from:

- The product README's "What you can do" section (currently
  text-only)
- The user docs (Quickstart, FAQ)
- External writeups, social posts, demo decks

Keep them DARK-THEMED — the editor's design tokens use the
dark theme by default; light-theme captures would feel
inconsistent with the in-app experience.

## What NOT to capture

- Sol Man with a real API key visible in screenshots — keys
  go in localStorage and shouldn't leak in marketing material.
  If demoing Sol Man, use a dummy key field or crop it out.
- The Diagnostics Drawer if it shows test-flavored or in-progress
  workflows (use one of the curated samples instead).
- Browser dev tools open (close them with F12 first).
