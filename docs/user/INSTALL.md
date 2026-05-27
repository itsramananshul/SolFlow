# Install

SolFlow runs entirely in your browser via WebAssembly. The
canonical SOL compiler + VM are compiled to WASM and shipped with
the editor; you don't need a Rust toolchain to use SolFlow.

You DO need Node.js to run the dev server.

## Prerequisites

| Tool | Version | Why |
|---|---|---|
| **Node.js** | ≥ 20 | Vite dev server, build pipeline, vitest |
| **npm** | bundled with Node | package install |

That's it for **users**. Contributors who want to modify the
Rust compiler need extra tooling — see [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Setup

```bash
git clone <repo-url>
cd SolFlow
npm install
```

`npm install` will pull editor dependencies (~150 packages,
mostly Vue / CodeMirror / Vue Flow). Pre-compiled WASM bundles
ship in `compiler-wasm/pkg/` and `compiler-wasm/pkg-node/`; no
Rust toolchain needed.

## First run

```bash
npm run dev
```

Open http://localhost:5173. The editor loads with an empty
canvas; the **Welcome** card offers three starting points:

1. **Open a sample** — pre-built workflows showcasing different
   features
2. **Generate with Sol Man** — describe what you want in plain
   English; an LLM produces a starter workflow (requires API key)
3. **Read the Quickstart** → [`QUICKSTART.md`](./QUICKSTART.md)

## Verify your setup

```bash
npm run check
```

Runs typecheck + 69 TypeScript tests + 49 Rust workspace tests.
All green means everything's wired correctly.

## Production build

```bash
npm run build           # vue-tsc + vite build
npm run preview         # serve dist/ locally to test
```

The build output in `dist/` is a static site — drop it on any
static-file host (Vercel, Netlify, S3, GitHub Pages). No server
runtime required.

## Troubleshooting

**`npm run dev` complains about missing `compiler-wasm/pkg`**
The pre-compiled WASM bundles are committed under
`compiler-wasm/pkg/` and `compiler-wasm/pkg-node/`. If they're
missing (e.g. from a `git clean -fdx`), rebuild them — see
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) for the toolchain.

**WASM fails to load in the browser**
Open the dev tools console. The most common cause is the wasm
file is being served with the wrong Content-Type. Vite handles
this automatically; if you're serving the production build from
a static host, ensure `.wasm` files get
`Content-Type: application/wasm`.

**Browser shows "internal compiler error"**
Diagnostics with severity `Internal` are bugs in SolFlow itself,
not problems with your SOL source. Please file an issue with the
source that triggered it.

**Sol Man (LLM generation) asks for a key**
Sol Man calls an external LLM. You provide your own key in the
Sol Man modal. Currently supported: OpenAI-compatible APIs +
OpenRouter. Keys are stored in `localStorage` only; they're
never sent to a SolFlow-hosted backend (there isn't one).
