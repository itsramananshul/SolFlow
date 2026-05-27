# FAQ

## What is SolFlow?

A visual IDE for the SOL language. You build workflows on a
graph canvas (Vue Flow); SolFlow generates SOL source from the
graph and runs it through the canonical SOL VM compiled to
WebAssembly. Diagnostics, type-checking, and execution all use
the canonical Rust implementation — no JS reimplementation of
language semantics owns user-displayed output.

## What is SOL?

A small statically-typed orchestration language. Functions,
structs, enums, arrays, if/while/for, type-safe arithmetic,
external function calls. See [`docs/sol-language/01-overview.md`](../sol-language/01-overview.md).

## What can SolFlow do today?

| Capability | Status |
|---|---|
| Visual graph editor with 22 node kinds | ✅ |
| Live source preview (graph → SOL) | ✅ |
| SOL source editing with real-time compiler diagnostics | ✅ |
| Import .sol source → visual graph | ✅ |
| Canonical-VM execution in browser | ✅ |
| Execution trace with source + node mapping | ✅ |
| Sample workflows | ✅ |
| LLM-assisted graph generation (Sol Man) | ✅ (BYO key) |
| External call execution | ❌ Blocked in browser; documented as honest non-feature |
| Multi-user / deployment | ❌ Phase C territory |

## What can't SolFlow do?

**Browser sim refuses external calls.** When a workflow reaches
an `ext function ... at <url>` call, the canonical VM returns a
structured `ExtCallBlocked` error rather than faking a successful
HTTP roundtrip. The editor renders this clearly: "external call
to `fetch_x` at `https://api.example.com` is blocked. External
calls are not available in browser simulation — deploy to run
them for real."

This is by design. Future deployment infrastructure (Phase C)
will run workflows against real controllers.

## How does graph ↔ source sync work?

**Explicit-action only.** No live two-way binding. Four
sanctioned transfers:

| Direction | Trigger |
|---|---|
| Graph → source (preview) | Live — preview updates as you edit graph |
| Graph → source (edit mode) | Click "Edit" — buffer detaches from live updates |
| Source → graph (import) | Click "Import to graph" — replaces current graph |
| Source → graph (discard) | Click "Reset to graph" or close edit mode |

Full philosophy: [`SYNC_MODEL.md`](../sol-language/SYNC_MODEL.md).

## What gets lost in graph → source → graph round-trip?

- Comments (the graph doesn't model them)
- User-authored field order in structs (importer alphabetizes for determinism)
- Custom whitespace + parenthesization (emit is canonical)

Semantics + node structure DO survive. Full detail:
[`CANONICALIZATION.md`](../sol-language/CANONICALIZATION.md).

## My diagnostic says "Internal compiler error" — what now?

You hit a SolFlow bug, not a problem with your SOL source. Please
file an issue with the source that triggered it. The `Internal`
phase + `ICE_*` codes are reserved for bugs in SolFlow itself.

## Where do my workflows save?

`localStorage` in your browser. SolFlow has no backend; there's
nothing to sign in to and no server to send data to. Export your
workflow as `.sol` source (Source pane → Edit → Download .sol)
if you want a portable copy.

## Why are some import-report rows labeled "Source-only" or "Unsupported"?

Some SOL constructs don't have a visual graph representation
(e.g. `ext function` declarations, top-level lets that aren't
auto-wrappable). The importer surfaces every such decision
honestly rather than silently dropping the code. See
[`IMPORT_COMPATIBILITY.md`](../sol-language/IMPORT_COMPATIBILITY.md)
for the per-construct matrix.

## What's "Sol Man"?

An optional LLM-assisted workflow generator. You describe what
you want in plain English ("approve a refund > $100, notify
otherwise"); Sol Man asks an LLM to produce a starter graph.
You provide your own API key (OpenAI / OpenRouter / similar).
Keys stay in `localStorage`; SolFlow doesn't host a backend.

The generated graph goes through the same validators every other
workflow does — Sol Man can't bypass diagnostics or produce
unsafe constructs. Treat its output as a starting point, not
finished work.

## What runs where?

```
              ┌──────────────────────────────────────────────┐
              │       Your browser tab                       │
              │   ┌───────────────────────────────────────┐  │
   You type → │   │  Vue editor (main thread)             │  │
              │   │  - Vue Flow canvas                    │  │
              │   │  - CodeMirror source pane             │  │
              │   │  - RunModal                           │  │
              │   └─────────────────┬─────────────────────┘  │
              │                     │ postMessage             │
              │   ┌─────────────────▼─────────────────────┐  │
              │   │  Compiler worker                      │  │
              │   │  - parse_source_json (every keystroke)│  │
              │   │  - analyze_source_json (every keystroke)│ │
              │   └───────────────────────────────────────┘  │
              │                                              │
              │   ┌───────────────────────────────────────┐  │
              │   │  Main-thread WASM (explicit actions)  │  │
              │   │  - compile_source_json (Import button)│  │
              │   │  - run_source_json    (Run button)    │  │
              │   └───────────────────────────────────────┘  │
              └──────────────────────────────────────────────┘
```

No backend. Everything is one static-site deploy + browser WASM.

## Where does the SOL language come from?

SOL is the language SolFlow exists to make accessible. The
compiler + VM in this repo are vendored from a canonical
implementation — see [`compiler/UPSTREAM.md`](../../compiler/UPSTREAM.md)
and [`runtime/UPSTREAM.md`](../../runtime/UPSTREAM.md) for
provenance + the surgical edits made for browser use.
