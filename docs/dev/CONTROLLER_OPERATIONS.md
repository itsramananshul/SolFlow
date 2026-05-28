# Controller operations

**Phase C C.8 stabilization doc (2026-05-28).** Operational
reference for running `solflow-controller` as a long-lived
service — full env-var table, startup log format, lifecycle of a
single request, and the failure-mode catalog operators need to
keep on hand.

For onboarding flows ("how do I get this running") see
[CONTROLLER_LOCAL.md](./CONTROLLER_LOCAL.md) (local-dev) and
[REMOTE_CONTROLLER.md](./REMOTE_CONTROLLER.md) (TLS + remote).

## Environment variable reference

Every controller knob is an env var. No CLI flags, no config
file. The set is intentionally small.

| Variable | Default | Type | Effect |
|---|---|---|---|
| `SOLFLOW_CONTROLLER_BIND` | `127.0.0.1:3939` | `host:port` | Bind address. Use `0.0.0.0:port` to accept from any interface (only when TLS + auth are also set). |
| `SOLFLOW_CONTROLLER_DB` | `./solflow.db` | path | SQLite database path. Created on first boot. Persists run history, schedules, run_events, sticky cancel bits. |
| `SOLFLOW_CONTROLLER_STEP_LIMIT` | `10_000_000` | int | Per-run VM step cap. Run lands `Failed { StepLimit }` when exceeded. |
| `SOLFLOW_CONTROLLER_TIMEOUT_SECS` | `600` | seconds | Per-run wall-clock cap. Run lands `TimedOut` when exceeded. Phase C.6. |
| `SOLFLOW_CONTROLLER_MAX_OUTPUT_LINES` | `100_000` | int | Per-run `print()` line cap. `Failed { ResourceLimit { resource: "output_lines" } }` when exceeded. |
| `SOLFLOW_CONTROLLER_MAX_EVENTS_PER_RUN` | `1_000_000` | int | Per-run `RunEvent` cap. One terminal `ResourceLimit { resource: "events" }` marker is emitted; subsequent events drop silently. Phase C.6. |
| `SOLFLOW_CONTROLLER_AUTH_TOKEN` | unset | string | Bearer token. Unset / empty = no auth. Non-empty = required on every protected endpoint. Phase C.7. |
| `SOLFLOW_CONTROLLER_TLS_CERT` | unset | path | PEM-encoded certificate chain. Required with `_TLS_KEY` for HTTPS. Phase C.7. |
| `SOLFLOW_CONTROLLER_TLS_KEY` | unset | path | PEM-encoded private key. Must match `_TLS_CERT`. Phase C.7. |
| `RUST_LOG` | `info,tower_http=info,sqlx=warn` | tracing filter | Standard `tracing-subscriber` filter. `RUST_LOG=debug` for verbose. |

### Defaults at a glance

Run the controller with no env vars at all and you get:

- HTTP on `127.0.0.1:3939` (loopback-only)
- SQLite in `./solflow.db`
- 10M steps / 600s / 100k print lines / 1M events per run
- No auth (any local client can submit + run workflows)
- 8 concurrent runs / 64-deep queue / Queue saturation policy
  (these are `ConcurrencyPolicy::default()` constants, not env
  vars — change via `LocalController::with_concurrency_policy`
  at compile time in a custom binary, or accept the defaults)

This is the local-dev shape. To safely expose externally you
**must** at minimum set `SOLFLOW_CONTROLLER_AUTH_TOKEN` and
either tunnel through HTTPS at the reverse proxy or set
`_TLS_CERT` + `_TLS_KEY`.

## Startup log format

A clean boot looks like this:

```
INFO solflow_controller: starting solflow-controller bind=127.0.0.1:3939 db_path=./solflow.db
INFO solflow_controller: run policy step_limit=10000000 wall_clock_secs=600
INFO solflow_controller: auth: disabled (set SOLFLOW_CONTROLLER_AUTH_TOKEN to enable)
INFO solflow_controller: transport: HTTP (set SOLFLOW_CONTROLLER_TLS_CERT + _TLS_KEY for HTTPS)
INFO solflow_controller: listening on http://127.0.0.1:3939
```

With TLS + auth:

```
INFO solflow_controller: starting solflow-controller bind=0.0.0.0:3939 db_path=./solflow.db
INFO solflow_controller: run policy step_limit=10000000 wall_clock_secs=600
INFO solflow_controller: auth: bearer-token required on protected endpoints
INFO solflow_controller: TLS enabled — loading cert + key cert=/etc/solflow/cert.pem key=/etc/solflow/key.pem
INFO solflow_controller: transport: HTTPS (rustls)
INFO solflow_controller: listening on https://0.0.0.0:3939
```

With TLS but no auth (the binary flags this as a footgun):

```
WARN solflow_controller: HTTPS is enabled but bearer-token auth is NOT — anyone who reaches this endpoint can submit + execute workflows. Set SOLFLOW_CONTROLLER_AUTH_TOKEN before exposing to a network you don't fully control.
```

If boot recovery picks up runs that were mid-execution at the
previous shutdown:

```
INFO solflow_controller: boot recovery re-enqueued 3 runs
```

These re-execute at-least-once; workflow side-effects may fire
twice. See [RUN_LIFECYCLE.md → Boot recovery](./RUN_LIFECYCLE.md#boot-recovery).

## Lifecycle of a single request

End-to-end path of a `POST /runs` from the editor:

```
   editor
     │  POST /runs  Authorization: Bearer xxx
     │
     ▼
  axum router
     │
     ▼
  ┌─────────────────────────────────────────┐
  │  layer: TraceLayer    (tracing span)    │
  │  layer: CorsLayer     (CORS + OPTIONS)  │
  │  layer: require_bearer_token            │  ← C.7 c98
  │   • OPTIONS or AuthConfig::Disabled →   │
  │     pass through                         │
  │   • else: verify(Authorization) →       │
  │     constant-time compare; on Err send  │
  │     401 + structured code               │
  └─────────────────────────────────────────┘
     │
     ▼
  axum handler `post_runs`
     │  LocalController::create_run(...)
     ▼
  RunManager::enqueue(...)
     │  • check sticky cancel_requested (boot-recovered runs)
     │  • bounded mpsc.try_send → Accepted | Rejected | QueueFull
     │  • persist row with status=Queued; emit Queued event
     ▼
   202 Accepted  RunCreated { run_id, status }   (or 503 queue_full)
     │
     │  (response returns to client immediately)
     │
   ─── dispatch loop (separate task) ───
     ▼
  dispatcher
     │  • acquire semaphore permit (max_concurrent_runs)
     │  • re-check cancel; promote Queued→Starting
     │  • spawn worker
     ▼
  worker
     │  • emit Starting + Started events
     │  • execute_run → spawn_blocking → VM
     │  • VM polls cancel_callback between every instruction
     │     (OR of user_cancel + timeout_flag)
     │  • Print / ExtCall / runtime errors → EventSink → SSE
     │  • VM returns Ok → Succeeded
     │     VM returns Cancelled + cancel_flag set → Cancelled
     │     VM returns Cancelled + timeout_flag set → TimedOut
     │     VM returns runtime error → Failed
     │  • persist terminal row; emit terminal event; release permit
     ▼
  RunManager::reconcile_post_execution
     │  • if user_cancel was set, promote terminal → Cancelled
     │    (user intent wins over VM termination cause)
```

The full state machine + every transition is in
[RUN_LIFECYCLE.md](./RUN_LIFECYCLE.md). The state machine source
of truth is `host_spec::RunStatus::can_transition_to`.

## Health-probing a controller

`GET /healthz` is **always open** (no auth check) so editors can
fingerprint + capability-probe a controller before sending
credentials. Use it for liveness checks, version compat
verification, and "is auth required" probes.

```bash
curl -s https://controller.example/healthz | jq
{
  "ok": true,
  "controller_version": "0.1.0",
  "host_spec_major": 0,
  "name": "solflow-controller",
  "auth_required": true
}
```

`name` + `auth_required` are absent on pre-C.7 controllers (they
were added in c97). The editor treats absent fields as "unknown
name" + "no auth" respectively, so it can still talk to older
controllers — but with conservative UX defaults.

## Operational dashboards / readouts

Two endpoints intended for operators (also exposed in the editor's
ActiveRunsModal):

- `GET /runs/active` — array of in-flight runs. Each entry:
  `{ run_id, workflow_id, dispatched_at }`. In-memory snapshot
  from `RunManager`; does NOT include Queued runs.
- `GET /controller/concurrency` — saturation snapshot:
  `{ max_concurrent_runs, max_queued_runs, active_runs,
     queued_runs, saturation_policy }`.

Both require auth when configured. Polling cadence in the editor
is 2s by default; for monitoring tools tune to whatever your
backend prefers.

## Common failure modes

| Symptom in logs | Diagnosis | Resolution |
|---|---|---|
| `TLS cert/key load failed (cert=..., key=...): permission denied` | Process can't read the cert/key files | `chmod 600 *.pem; chown <user>:<group> *.pem` |
| `TLS cert/key load failed (...): no certificates found` | File doesn't contain PEM-encoded certs | Re-run `openssl x509 -in cert.pem -text` — empty/wrong file? Convert from DER via `openssl x509 -inform DER`. |
| `TLS misconfigured: SOLFLOW_CONTROLLER_TLS_KEY is set but SOLFLOW_CONTROLLER_TLS_CERT is not` | Half-configured TLS | Set both env vars or neither. |
| Repeated `WARN ... boot recovery failed: ...` lines | Persistence is unhealthy | Check disk space + file permissions on `_DB`. SQLite corruption is unrecoverable in this version; restore from backup. |
| `boot recovery re-enqueued N runs` (N > 0) | Recovery is working; runs are about to re-execute | Expected on every restart if you had runs in flight. Verify your workflows are idempotent — at-least-once is documented. |
| 401 floods in access logs from one IP | Token rotated, that client didn't pick up the change | Re-issue the new token to that user / service. |
| 503 `queue_full` floods | Sustained throughput > `max_concurrent_runs` × inverse VM duration | Increase `max_concurrent_runs` (rebuild from a custom main with `with_concurrency_policy`), OR accept backpressure (clients should treat 503 as "retry shortly"). |

## Resource sizing rules of thumb

These are coarse — measure your own workloads, but for first
deployment:

| Workload shape | Recommended config |
|---|---|
| Local dev, one user, mostly browser-sim | All defaults |
| Solo remote dev, ≤ 10 concurrent runs | Defaults + auth + TLS |
| Team of ~10, mostly HTTP-ExtCall workflows, bursty | Step limit 50M, wall-clock 1800s, max_events 10M; consider higher `max_concurrent_runs` via a custom main |
| CI integration (`POST /events/...`) firing many runs | Watch 503 rate; raise `max_queued_runs` to 256+ via custom main, OR add a CI-side retry-on-503 loop |

## Backups

The SQLite database is the only persistent state. A `cp` while
the controller is running is unsafe (sqlite3 may be mid-write).
Two safe approaches:

1. **`sqlite3 .backup`** — works against a running controller:
   ```bash
   sqlite3 /var/lib/solflow/db.sqlite ".backup /backups/db-$(date +%F).sqlite"
   ```
2. **Stop the controller briefly + `cp`** — fine for low-traffic
   deployments; copy completes in ms-to-seconds depending on
   DB size.

Restore is a `cp` back to the configured path; the controller's
`recover_runs()` rebuilds in-memory state from there.

## Related docs

- [Local controller](./CONTROLLER_LOCAL.md) — local-dev intro
- [Remote controller](./REMOTE_CONTROLLER.md) — TLS + auth setup
- [Run lifecycle](./RUN_LIFECYCLE.md) — the state machine inside
- [Scheduling](./SCHEDULING.md) — timer + webhook triggers
- [Connectors](./CONNECTORS.md) — ExtCall + HTTP reference
- [Events](./EVENTS.md) — SSE protocol + run_events table
- [Phase C roadmap](./PHASE_C_ROADMAP.md) — the milestone plan
