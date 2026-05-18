# SolFlow — Phase A Demo Vertical Slice

> Production-grade visual IDE for the **SOL language** — Phase A vertical slice.
>
> Not a demo gimmick: a real working slice of the product with a deliberately
> limited language subset and a **temporary** TypeScript exporter that will be
> replaced by a Rust/WASM compiler bridge in Phase B.

## What this is

A Vue 3 + Vue Flow + Pinia + CodeMirror app that lets users:

1. **Manual Build** — drag nodes onto a canvas, wire control + data edges,
   configure node parameters, and watch the live SOL source preview update.
2. **Import workflow JSON** — load a previously-saved `SolGraph` JSON.
3. **Export `.sol`** — download the canonical SOL source emitted from the
   current graph.
4. **Load sample workflows** — open a real, editable workflow loaded from
   data (NOT hardcoded UI).

Supported SOL features:

- multiple functions per file (tab bar)
- structs + struct literals + field access + field assignment
- enums + variant access
- arrays (literal, index read, index write)
- `while` and `for x in array` loops
- `if` / `if-else`
- `let`, assignment, `print`, `return`, user-defined function calls
- arithmetic / comparison / logical operators
- imports as a declarative side-panel (not callable in Phase A — SOL itself
  doesn't resolve them yet)

## Stack

- **Vue 3** + Composition API + TypeScript (strict)
- **Vue Flow** for the canvas
- **Pinia** for state
- **CodeMirror 6** for the source preview
- **Vite 5** for dev / build
- **nanoid** for stable node/edge IDs

## Run it

```bash
cd solflow-demo
pnpm install
pnpm dev           # http://localhost:5173
```

Build for production:

```bash
pnpm build
pnpm preview
```

Typecheck only (no emit):

```bash
pnpm typecheck
```

## Project structure

```
solflow-demo/
├── src/
│   ├── main.ts                       Vue app bootstrap
│   ├── App.vue                       three-pane shell
│   ├── components/                   UI components
│   │   ├── Toolbar.vue
│   │   ├── FunctionTabs.vue
│   │   ├── Sidebar.vue               tab host: Palette / Types / Imports
│   │   ├── NodePalette.vue
│   │   ├── TypesPanel.vue            struct + enum editor
│   │   ├── ImportsPanel.vue          declarative imports
│   │   ├── Canvas.vue                Vue Flow wrapper
│   │   ├── SolNode.vue               single component renders ALL node kinds
│   │   ├── Inspector.vue             per-kind property editor
│   │   ├── SourcePreview.vue         live CodeMirror SOL view
│   │   └── DiagnosticsDrawer.vue
│   ├── graph/                        graph model + helpers
│   │   ├── schema.ts                 SolGraph + Node + Edge + Type types
│   │   ├── kinds.ts                  per-kind palette/port metadata
│   │   ├── factory.ts                createNode() helpers
│   │   ├── scope.ts                  in-scope variable walking
│   │   └── validate.ts               client-side validation
│   ├── emit/                         **TEMPORARY** Graph→SOL exporter
│   │   ├── emit.ts                   walks graph, emits SOL string
│   │   └── README.md                 marks file as temporary; Phase B replaces
│   ├── stores/
│   │   ├── graph.store.ts            Pinia: workflow state, undo, autosave
│   │   └── ui.store.ts               Pinia: panel state
│   ├── samples/                      sample workflows as DATA
│   │   ├── hello.json                jjsi-style
│   │   ├── monitor.json              jj_comp-style
│   │   ├── orchestration.json        s1-style
│   │   └── payments.json             s2-style
│   └── styles/
│       ├── tokens.css                design tokens (dark theme)
│       └── theme.css                 component theming
├── index.html
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## Phase A shortcuts (intentionally temporary)

Every file below is marked as Phase A; Phase B will replace each with the
real Rust/WASM compiler bridge. None of these are "fake" — they all work
correctly, they're just simpler than the production code path.

| Shortcut | What it does | Replacement |
|---|---|---|
| `src/emit/emit.ts` | TypeScript walker that converts SolGraph → SOL source. | Phase B: WASM call to `emit_sol(graph_to_ast(graph))`. |
| `src/graph/validate.ts` | Client-side port-type + missing-input + branch-termination checks. | Phase B: WASM call to `analyze_ast`. |
| One-way `Graph → SOL` only | No `.sol` import (no parser yet). | Phase B: WASM `parse_sol` lets us import any `.sol`. |
| `localStorage` autosave + JSON download for persistence | No server. | Phase B: workflows persisted in Postgres via SolFlow server. |
| No "Run" button | Can't execute SOL from the demo. | Phase B: ship to SOL controller over HTTP/RPC. |
| Single user, no auth | Browser tab is the session. | Phase B: real auth. |
| No Sol Man (AI assistant) | Phase 6+. | Phase B+. |

## Phase A → Phase B migration

The seams that make replacement clean:

- `src/emit/emit.ts` — same TS signature in Phase B; impl delegates to WASM.
- `src/graph/validate.ts` — same shape; impl calls `analyze_ast`.
- `src/graph/schema.ts` — strict subset of Phase B's `SolGraph`; forward-compat.
- All node components — unchanged in Phase B; just more kinds added.

See `reference/SOL_VISUAL_EDITOR_ANALYSIS.md` for the full Phase B plan.

## License

TBD — see `reference/WORKFLOW_PLATFORM_BLUEPRINT.md` §21 open decisions.
