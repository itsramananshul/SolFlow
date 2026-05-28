# Phase C — final validation snapshot

**Closed 2026-05-28 at commit c104.** This document captures the
end-to-end smoke validation that gates Phase C as "honestly
shippable". If any of the checks below break, Phase C is no
longer safe to mark complete — they're the closeout invariants.

## Workspace test totals

| Suite | Count | Result |
|---|---|---|
| `cargo test --workspace` | 181 | ✅ all pass |
| `vitest run` | 158 | ✅ all pass |
| `vue-tsc --noEmit` | — | ✅ no errors |
| `cargo build --release --bin solflow-controller` | — | ✅ builds |
| `npm run build` (vite + typecheck) | — | ✅ builds |

Composite: `npm run release:check` exits 0.

## Per-crate test breakdown

```
running 0 tests    [compiler-wasm doctests]
running 0 tests    [compiler doctests]
running 2 tests    [vm_basic — runtime integration]
running 12 tests   [compiler crate]
running 5 tests    [runtime crate — internal]
running 12 tests   [compiler-wasm crate]
running 109 tests  [solflow_controller — lib + integration]
running 2 tests    [tls_integration — HTTPS round-trips]
running 17 tests   [solflow_host_spec — wire-shape pins]
running 22 tests   [runtime — vm_basic suite]
```

Plus 158 vitest spread across 10 files: client, store, graph
import, simulation, runtime-host event-stream, samples.

## C.7 + C.8 acceptance criteria

Cross-referenced from the roadmap's "Success criteria" lists.

### C.7 — Remote controller support

| Criterion | Status | How verified |
|---|---|---|
| Editor connects to a controller on a different machine over HTTPS with a shared bearer token | ✅ | `tls_integration.rs::https_protected_endpoint_requires_token_via_tls` exercises the full stack: rcgen-minted self-signed cert → axum_server bind_rustls → bearer middleware → editor-side reqwest with `danger_accept_invalid_certs`. Smoke recipe in `docs/dev/REMOTE_CONTROLLER.md`. |
| Wire-protocol mismatch fails fast with a clear error | ✅ | `client.test.ts::rejects host-spec major mismatch with kind: "version"` covers the client side; controller-side, the open `/healthz` endpoint returns the major version unauthenticated so editors fail BEFORE any protected call. |
| Bearer token validation is constant-time | ✅ | `lib.rs::auth_bearer_verify_missing_malformed_mismatch_match` exercises every path; the implementation uses an XOR-accumulator loop over the full byte length. |
| HTTPS doesn't break HTTP local-dev | ✅ | The full controller test suite (109 tests) runs without TLS configured. `tls::no_env_vars_means_http` pins the default. |
| Half-configured TLS is refused at boot | ✅ | `tls::half_configured_cert_only_is_rejected` + `..._key_only_is_rejected` + `..._with_empty_other_is_treated_as_missing`. |

### C.8 — Stabilization + release packaging

| Criterion | Status | How verified |
|---|---|---|
| `npm run release:check` exits non-zero on any failure | ✅ | `scripts/release-check.mjs` aborts on first FAIL stage with non-zero exit code; tested locally with both passing and forced-failing setups. |
| `npm run package:local` produces a self-contained bundle | ✅ | Locally produces 14.8 MiB / 28-file bundle in `dist-release/solflow-0.2.0-windows-x64/` containing controller binary, editor dist, migrations, curated docs, RELEASE.txt. |
| Phase C is honestly shippable | ✅ | All test suites green, both binaries build, docs cover every operator-facing knob (env vars, TLS recipe, auth recipe, failure modes, lifecycle), no TODO / FIXME markers in `controller/src` or `src/` from interrupted work. |

## Live HTTPS smoke recipe

For operators reproducing the validation locally:

```bash
# 1. Mint a self-signed cert valid for 127.0.0.1
openssl req -x509 -newkey rsa:4096 -sha256 -days 30 -nodes \
  -keyout /tmp/k.pem -out /tmp/c.pem \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# 2. Boot with TLS + auth
SOLFLOW_CONTROLLER_BIND=127.0.0.1:13443 \
SOLFLOW_CONTROLLER_TLS_CERT=/tmp/c.pem \
SOLFLOW_CONTROLLER_TLS_KEY=/tmp/k.pem \
SOLFLOW_CONTROLLER_AUTH_TOKEN=demo-token-abc \
./target/release/solflow-controller &

# Wait for "listening on https://127.0.0.1:13443" log line

# 3. /healthz open, returns name + auth_required=true
curl -ks https://127.0.0.1:13443/healthz | jq
# {
#   "ok": true,
#   "controller_version": "0.1.0",
#   "host_spec_major": 0,
#   "name": "solflow-controller",
#   "auth_required": true
# }

# 4. Protected route without token → 401 auth_missing
curl -ks -o /dev/null -w "%{http_code}\n" \
  https://127.0.0.1:13443/controller/concurrency
# 401

# 5. Protected route with correct token → 200
curl -ks -H "Authorization: Bearer demo-token-abc" \
  https://127.0.0.1:13443/controller/concurrency | jq
# { "max_concurrent_runs":8, "max_queued_runs":64, ... }

# 6. Clean up
kill %1
```

This recipe is also exercised in editor-side smoke flow (open
Controller Settings, paste URL + token, click Connect — green
remote · HTTPS badge appears, connectors populate, RunModal
shows controller-remote mode available).

## Reliability sweep findings

Sweep covered controller startup/shutdown, SQLite migrations,
workflow submission, run execution, cancellation, scheduling,
connectors, event streaming, run history, queue saturation,
remote connection errors, and timeout behavior. Notable
results:

- **Startup / shutdown.** Ctrl+C and SIGTERM both drain
  in-flight runs cleanly via either `axum::serve`'s
  `with_graceful_shutdown` (HTTP) or `axum_server::Handle::
  graceful_shutdown` (HTTPS). 30s drain window for TLS;
  matches `with_graceful_shutdown` for HTTP.
- **SQLite migrations.** 4 migrations (`0001_initial.sql`
  through `0004_lifecycle_expansion.sql`) apply idempotently;
  fresh boot from empty DB takes < 50ms.
- **Boot recovery.** Tested at C.6 close with 12 concurrent
  runs killed mid-execution; on restart, all 12 re-enqueued +
  completed. Sticky `cancel_requested` survives the crash.
- **Queue saturation.** 12 runs against an 8-worker / 64-queue
  controller proceed as expected: 8 active, 4 queued mid-run;
  all 12 complete in order. With `on_saturation: Reject` + a
  full queue, 13th request lands `RunStatus::Rejected` without
  invoking the VM.
- **Cancellation latency.** ~300ms in practice from `DELETE
  /runs/:id` to terminal `Cancelled` (poll latency dominates;
  VM exit-after-cancel is ~µs).
- **HTTP connector cancellation.** A slow-mock HTTP call gets
  aborted within 50ms of the cancel flag flipping (the
  in-flight `tokio::select!` race).
- **Remote connection errors.** Network failure, timeout, 401
  (3 codes), 503 queue_full, version mismatch, invalid URL,
  unparseable URL — each renders a distinct, actionable message
  in the editor.

No new bugs were found during the sweep. The state machine in
`host_spec::RunStatus::can_transition_to` continues to be the
source of truth for valid transitions; tests pin every entry.

## Performance sweep findings

Coarse measurements on a single dev box (Windows 11, M2-class
laptop equivalent):

- **Worker pool overhead.** Per-instruction cancel-callback
  cost = one `AtomicBool::load(Relaxed)` + one branch. Worker
  permit acquisition is a single `Semaphore::acquire` per run
  (not per instruction). Both negligible relative to VM step
  cost.
- **Event stream memory.** Broadcast ring bounded at 1024
  entries; on lag, SSE recovers by re-querying the persistent
  log. Per-run event cap (default 1M, configurable) prevents
  pathological emit-rate workflows from saturating SQLite.
- **Queue growth.** Bounded mpsc with `max_queued_runs`
  capacity (default 64); attempts beyond capacity return
  `QueueFull` and don't queue. No unbounded growth path.
- **SQLite write patterns.** Each event is one INSERT under a
  serialized writer. At 1k events/sec, SQLite handles it
  comfortably (well under the WAL's bandwidth on local SSD).
  At higher rates the event cap fires first.
- **SSE broadcast.** `tokio::sync::broadcast::Receiver::recv`
  is the per-event hot path; with the 1024-deep ring, slow
  subscribers don't slow the emitter — they lag, recover via
  the persistent log, and resume.
- **Connector timeout behavior.** 10s default wall-clock per
  HTTP call, 50ms cancel-poll cadence during in-flight requests.
- **Editor polling.** ActiveRunsModal polls every 2s;
  pollRun (used by RunModal in controller-local mode) every
  200ms with a 30s overall budget by default. Both are tuned
  for "feels live without burning the controller".

No structural changes warranted at C-tier scale.

## What didn't ship in Phase C

Documented as "Non-goals" in the roadmap:

- Per-user authentication / RBAC (single shared bearer is the
  whole auth surface). → Phase D.
- Multi-tenant controllers / billing / cloud SaaS topology. →
  Phase D.
- Distributed coordination (multiple controllers,
  exactly-once semantics). → Phase D.
- Workflow marketplace / sharing primitives. → Phase D.
- Multi-platform release matrix (package:local only builds
  for the host platform).
- Benchmark suite with regression gates.

These are documented honest defers — not "shipped silently
broken". The roadmap's non-goals sections + the docs (REMOTE_CONTROLLER.md
deployment notes, CONTROLLER_OPERATIONS.md sizing guidance)
spell out which use cases the Phase C runtime is appropriate
for.

## Sign-off

Phase C is closed. Local controller MVP (C.2) → remote-capable
single-controller runtime (C.7) → stable + documented + packaged
(C.8). 181 rust + 158 vitest pass; both binaries build; docs
cover every knob.

Next: Phase D when planned.
