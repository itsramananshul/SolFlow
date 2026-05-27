# Running SolFlow with a local controller

**Phase C C.2 (shipped 2026-05-27).** This guide walks through
running the editor against a real local SolFlow controller — the
"controller-local" execution mode introduced in C.2.

## What you get

When the editor is connected to a controller:

- **Workflow submission survives editor reload** — the controller
  stores compiled workflows in SQLite, so reopening a previous
  run via the Reopen button works even after both the editor and
  the controller have restarted.
- **Step-limit + wall-clock-timeout enforced by the controller**,
  not the browser. Default 10M steps + 600s wall-clock; override
  per-controller via env vars (see below).
- **Same canonical SOL VM**. The controller runs the same Rust
  bytecode interpreter the WASM bundle does — output, return
  values, and structured runtime errors are bit-identical to
  browser-sim for the same source.

What it **doesn't** give you yet (deferred to later C milestones):

| Capability | Lands in |
|---|---|
| Real external HTTP calls (ExtCall) | C.4 |
| Live event stream + execution-trace from controller | C.5 |
| Run cancellation (DELETE /runs/:id returns 501) | C.6 |
| Schedules / cron triggers / webhook ingestion | C.3 |
| Remote (TLS) controller mode | C.7 |

## One-time setup

1. **Build the controller binary** (release recommended after the
   first run; debug is fine while iterating):

   ```bash
   cargo build -p solflow_controller --release
   ```

2. **Build the WASM bundle** if you haven't already (needed by the
   editor for compile-for-wire):

   ```bash
   npm run build:wasm
   ```

## Boot the controller

```bash
# default config: 127.0.0.1:3939, ./solflow.db
./target/release/solflow-controller
```

Or with explicit configuration:

```bash
SOLFLOW_CONTROLLER_BIND=127.0.0.1:3939 \
SOLFLOW_CONTROLLER_DB=./.local/solflow.db \
SOLFLOW_CONTROLLER_STEP_LIMIT=10000000 \
SOLFLOW_CONTROLLER_TIMEOUT_SECS=600 \
RUST_LOG=info \
./target/release/solflow-controller
```

| Env var | Default | What it does |
|---|---|---|
| `SOLFLOW_CONTROLLER_BIND` | `127.0.0.1:3939` | bind address |
| `SOLFLOW_CONTROLLER_DB` | `./solflow.db` | SQLite path (parent dir must exist) |
| `SOLFLOW_CONTROLLER_STEP_LIMIT` | `10000000` | per-run VM step cap |
| `SOLFLOW_CONTROLLER_TIMEOUT_SECS` | `600` | per-run wall-clock cap |
| `RUST_LOG` | `info,tower_http=info,sqlx=warn` | tracing filter |

Healthcheck:

```bash
curl http://127.0.0.1:3939/healthz
# {"ok":true,"controller_version":"0.1.0","host_spec_major":0}
```

Stop with ctrl-c — in-flight requests drain before exit.

## Connect from the editor

1. `npm run dev` (or your usual editor URL)
2. Open **Toolbar → ⋯ → Controller Settings**.
3. Type the controller URL (e.g. `http://127.0.0.1:3939`) and
   click **Connect**.
4. The status dot turns green and the controller version /
   host-spec major appear in the connection panel.
5. Open **Run**. The header now shows a **Browser sim /
   Controller-local** toggle. Pick controller-local — the run
   submits to the controller, polls until it's done, and
   displays the same output / return value / runtime errors.

The editor remembers your controller URL and reconnects silently
on the next reload.

## Verify run-history persistence

1. Run a workflow in controller-local mode. Note its `run_id` in
   the Output pane meta footer.
2. Stop the controller (ctrl-c).
3. Restart the controller with the **same** `SOLFLOW_CONTROLLER_DB`
   path.
4. In the editor, open the Run modal, expand "Recent runs", click
   **Reopen** on the historic run. The full record (output,
   return value, status) comes right back.

## HTTP API quick reference

C.2 endpoints (all return JSON; non-2xx returns
`{ "error": { "code": "...", "message": "..." } }`):

| Method | Path | Body | Returns |
|---|---|---|---|
| `GET` | `/healthz` | — | `Health` |
| `POST` | `/workflows` | `WorkflowSubmission` | `WorkflowSubmissionResponse` |
| `POST` | `/runs` | `RunRequest` | `202 RunCreated` |
| `GET` | `/runs/:id` | — | `RunRecord` |
| `GET` | `/workflows/:id/runs?status=&limit=` | — | `RunRecord[]` |
| `DELETE` | `/runs/:id` | — | `501 NotImplemented` (lands in C.6) |

Schema is the `solflow_host_spec` crate; the TS mirror lives at
`src/runtime-host/types.ts`. The `enum_tag_format_uses_kind_field`
Rust test pins how discriminated unions are tagged on the wire.

## Troubleshooting

**"Not connected — controller unreachable"** when clicking
Connect:
- Confirm the controller is running (`curl http://localhost:3939/healthz`).
- Confirm the URL has `http://` (not `localhost:3939` alone).
- If it's another machine, confirm `SOLFLOW_CONTROLLER_BIND` is
  `0.0.0.0:3939`, not the default `127.0.0.1`.

**"Host-spec version mismatch"**:
- The controller is compiled against a different `host-spec` major
  than your editor's WASM bundle. Rebuild both: `cargo build -p
  solflow_controller --release && npm run build:wasm`.

**"Controller error (HTTP 400 / bytecode_invalid)" on
controller-local run**:
- Usually means the editor's compiled bytecode is empty. Confirm
  the source compiles cleanly (Browser-sim mode shows compile
  errors more visibly).

**Run says "Failed" but no structured error**:
- C.2 captures the failure reason in the run record's output but
  doesn't yet stream structured errors. The full error event log
  lands in C.5. For now, check the controller's stderr — it logs
  every failed run via `tracing::error!`.

**`unable to open database file`** on boot:
- The `SOLFLOW_CONTROLLER_DB` parent directory doesn't exist or
  isn't writable. Create the dir or pass a path you own.

## Related docs

- [Phase C architecture](./PHASE_C_ARCHITECTURE.md) — the canonical
  design the C.2 implementation realizes
- [Phase C roadmap](./PHASE_C_ROADMAP.md) — milestone timeline +
  shipped status
- `host-spec/src/lib.rs` — wire-protocol source of truth
- `controller/src/lib.rs` — controller trait + error surface
