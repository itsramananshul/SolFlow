# Connectors (internal/dev/test fallback)

> **Not the product provider model.** The canonical provider system is
> the OpenPrem SDK protocol: agents register via `POST /register` and
> SolFlow invokes them directly. See `OPENPREM_PROVIDERS.md`. The
> `SOLFLOW_CONNECTORS` registry described below is retained only as an
> internal/dev/test fallback (a simple `{module, function, params}` HTTP
> contract). External calls resolve the OpenPrem registry first, then
> fall back to `SOLFLOW_CONNECTORS`, then block.

**Phase C C.4 (shipped 2026-05-27).** Connectors are how the
SolFlow controller takes a SOL `ext function` call and turns it
into real external I/O. Browser-sim still blocks ExtCall (no
network in the browser by design); the controller dispatches it
through this framework instead.

## Providers: the live path (Phase 3)

The canonical run path (`controller/src/canonical_exec.rs`) resolves a
SOL `call("module.function", payload)` through a **provider registry**: a
map from module name to a connector base URL, read from the
`SOLFLOW_CONNECTORS` environment variable (a JSON object). A module of
`"*"` is a wildcard that catches every Action. This is the registry the
controller actually uses, and `GET /providers` lists it (the editor's
Controller Settings shows it as "Registered providers").

For each external Action the controller POSTs
`{ "module": <str>, "function": <str>, "params": <json> }` to the
module's URL and feeds the JSON response back into the workflow. A module
with no registered provider (and no `"*"` fallback) is **blocked** with a
clear, source mapped error naming the module and function; the failure
ties to the exact `call(...)` line via the execution trace. The per call
HTTP timeout is `SOLFLOW_CONNECTOR_TIMEOUT_MS` (default 30000).

### Run a capability workflow end to end (demo connector)

```sh
# 1. Start the bundled demo connector (functions: echo, add, greeting).
cargo run -p solflow_controller --bin demo-connector            # :8099

# 2. Start the controller with the demo module registered.
SOLFLOW_CONNECTORS='{"demo":"http://127.0.0.1:8099"}' \
  cargo run -p solflow_controller --bin solflow-controller      # :3939

# 3. Run a workflow that makes a real external call:
#      workflow "start" {
#        let sum: int = call("demo.add", { a: 20, b: 22 });
#        print(sum);   # prints 42
#        return sum;
#      }
#    Select Local Controller in the editor's Run modal, or POST it to
#    /workflows + /runs. The Trace tab shows extcall + extresult steps.
```

In Browser Simulation the same workflow blocks the call (no providers in
the browser) and the Trace tab shows the blocking error at the call site.

The rest of this document describes the richer `Connector` trait
framework (`controller/src/connector/`), which is available for typed,
policy aware connectors and is the path the `ext function` /
`connector://` model uses.

## Architecture

```
   SOL program                            Controller process
   ───────────                            ──────────────────
   ext function fetch(...) -> ...           VM (synchronous,
        at "connector://http?url=..."        canonical SOL)
        ↓                                        │
   compiles to                                   │ Inst::ExtCall
   Inst::ExtCall(arg_types, ret_type)            ↓
                                            ExtCallHandler
                                                 │ block_on
                                                 ↓
                                            ConnectorRegistry
                                                 │ lookup("http")
                                                 ↓
                                            HttpConnector::invoke
                                                 ↓
                                            real network I/O
                                            (timeout / retry / size cap)
                                                 ↓
                                            ConnectorOutcome ──→ VM stack
```

Three crisp boundaries:

| Layer | Crate / file | What lives here |
|---|---|---|
| Runtime hook | `solflow_runtime::extcall` | `ExtCallHandler` trait, `ExtCallType` (primitives), `ExtCallValue`, `ExtCallError` |
| Framework | `solflow_controller::connector` | `Connector` trait, `ConnectorRegistry`, `ConnectorInvocation`, `ConnectorOutcome`, `ConnectorError`, `parse_connector_url` |
| Reference impl | `solflow_controller::connector::http` | `HttpConnector` |

The VM stays browser-safe — it knows nothing about HTTP or
`connector://` URLs. The controller installs an `ExtCallHandler`
that bridges to the registry; without one, ExtCall returns
`RunError::ExtCallBlocked` as it has since C.1.

## URL grammar

```
connector://<name>?<key>=<value>(&<key>=<value>)*
```

Rules:

- `<name>` is the connector's registered name (matches its
  `ConnectorMeta::name`). Lookup is exact — case-sensitive.
- No path segments allowed (`connector://http/foo` is rejected).
  All configuration goes through query parameters so the wire
  format stays uniform.
- Values are URL-encoded; spaces become `+` or `%20`, `&` and
  `=` inside values must be `%26` / `%3D`.

Examples:

```
connector://http?url=https%3A%2F%2Fapi.example.com%2Fwidgets&method=GET
connector://http?url=https%3A%2F%2Fapi.example.com%2Fevents&method=POST&header.authorization=Bearer%20xyz
connector://slack?channel=alerts
```

Parser API: `solflow_controller::parse_connector_url(s) ->
Result<ParsedConnectorRef, ConnectorError>`.

## HTTP connector

The canonical reference. Speaks HTTP/1.1 + HTTP/2 over rustls
via reqwest. Registered by default in `LocalController::new()`;
override with `with_connector_registry()` if you need a
custom set (e.g. allowlist-restricted in production).

### URL parameters

| Param | Required | Default | Meaning |
|---|---|---|---|
| `url` | yes | — | Target HTTP(S) URL |
| `method` | no | `GET` | `GET` / `POST` / `PUT` / `PATCH` / `DELETE` / `HEAD` (or any RFC-7230 extension method token) |
| `timeout_ms` | no | policy | Per-call wall-clock budget override |
| `header.<name>` | no | — | Sets `<name>: <value>` request header. Repeatable. |
| `body_format` | no | `json` | `json` = serialize args as request body; `none` = send no body |

### Args → request

- **Methods with bodies** (POST/PUT/PATCH): `args` is JSON-encoded
  into the request body. `content-type: application/json` is set
  unless overridden via `header.content-type`.
- **Methods without bodies** (GET/DELETE/HEAD): if `args` is a
  JSON object, each top-level key becomes a query parameter
  appended to the URL. Non-object args (numbers, strings) are
  ignored for body-less methods.

### Response → value

- The body is read up to `policy.max_response_bytes` (default
  1 MiB). Oversize responses fail with `ResponseTooLarge`
  (caught at `content-length` if declared).
- `content-type: application/json*` or `text/json*` → body parsed
  as a JSON value.
- Everything else → body returned as a UTF-8 string (lossy on
  invalid bytes).
- `2xx` → success.
- `401` / `403` → `ConnectorError::AuthFailed` (distinct so the
  editor can prompt for credentials).
- Other `4xx` → `HttpStatus` (not retried — caller's fault).
- `5xx` → `HttpStatus`; eligible for retry per policy.

### Retry + timeout

- The overall budget is `policy.timeout_ms` (default 10s).
- `policy.retry_attempts` (default 0) controls how many *additional*
  attempts after the first failure. Total attempts = `1 + retry_attempts`.
- Backoff: `policy.backoff_base_ms * 2^attempt`, clamped so the
  next sleep can't exceed remaining budget.
- Only **transport** errors + **5xx** + **timeouts** are retried.
- All attempts failed → `RetryExhausted { attempts, last_error }`.

### Security

- `HttpConnector::with_allowlist(vec![UrlAllowEntry::...])` pins
  permitted hosts. Two match modes:
  - `UrlAllowEntry::exact("api.example.com")` — exact host match
  - `UrlAllowEntry::host_and_subdomains("example.com")` — also
    matches `*.example.com` (`api.example.com`, `a.b.example.com`)
- Without an allowlist, the connector permits any URL — fine
  for local development; production deployments should always
  set one.
- Allowlist check happens BEFORE any I/O — rejected URLs never
  touch the network.

## ExtCall error model

When a connector returns an error, the VM surfaces it as
`RunError::ExtCallFailed { connector, function_name, message }`
on the run record. The editor's RunModal renders this distinctly
from the browser-sim `ExtCallBlocked` variant.

The structured `ConnectorError` variants the connector returns:

| Kind | When | Editor render |
|---|---|---|
| `connector_not_found` | URL names a connector that isn't registered | "External call X via connector Y failed: connector not found" |
| `invalid_connector_url` | URL doesn't parse as `connector://…?…` | same shape |
| `missing_param` / `invalid_param` | required URL param missing or unparseable | same shape, message names the param |
| `timeout` | wall-clock budget exhausted | same shape with elapsed_ms / limit_ms |
| `auth_failed` | HTTP 401/403 | same shape |
| `http_status` | non-2xx (non-auth) | same shape with status + body excerpt |
| `dns_failure` | DNS didn't resolve | same shape with host |
| `network` | TCP refused / TLS handshake / unclassified transport | same shape |
| `retry_exhausted` | all retries failed | same shape with attempts + final cause |
| `payload_too_large` | request body exceeds limit | rare for HTTP today |
| `response_too_large` | response body exceeds policy cap | same shape |
| `url_not_allowed` | URL rejected by allowlist | same shape |
| `cancelled` | run cancelled (C.6) | reserved |
| `invalid_response` | body couldn't be decoded | same shape |

## Type bridging

The boundary supports only **primitive types** in C.4:

- `int` ↔ JSON number
- `float` ↔ JSON number
- `bool` ↔ JSON boolean
- `string` ↔ JSON string (any JSON value also accepted, stringified)
- `void` (return only)

Args become a positional JSON array: `[arg0, arg1, ...]`. The HTTP
connector forwards that as the body for POST/PUT/PATCH; for GET
the args-as-object → query-params rule above applies.

Compound types (struct, array) error cleanly with
`ExtCallError::Unsupported`. Adding compound marshalling is a
later-milestone concern; the trait shape is designed not to
change when it lands.

## End-to-end example

A workflow that calls a JSON HTTP endpoint:

```text
                  ┌──────────────────────────────────────────┐
                  │  ext function fetch_widget(id: int) -> int │
                  │      via "connector://http?url=...&method=POST"│
                  └──────────────────────────────────────────┘
                                       │
                                       ▼
  POST http://localhost:3939/workflows  ← submit compiled bytecode
                                       │
                                       ▼
  POST http://localhost:3939/runs     ← create_run, trigger Manual
                                       │
                                       ▼
                            controller spawns execute_run
                                       │
                                       ▼
                      VM hits Inst::ExtCall, calls handler
                                       │
                                       ▼
                          parse "connector://http?..."
                                       │
                                       ▼
                       HttpConnector.invoke(invocation)
                                       │
                                       ▼
                       POST https://example.com/...
                       (1 MiB cap, 10s timeout, retries off)
                                       │
                                       ▼
                       response JSON value pushed to VM stack
                                       │
                                       ▼
                              run completes; GET /runs/:id
                              returns the final value to the editor
```

End-to-end coverage:
`controller/src/local.rs::tests::ext_call_runs_through_http_connector_end_to_end`
hand-crafts bytecode that hits a wiremock server through this
exact path and verifies the response value reaches the run record.

## Failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| Run record shows `Failed` with `[controller] external call to ... failed: connector not found` | URL names a connector that isn't registered on this controller | check `GET /connectors`; add a registration via `LocalController::with_connector` |
| Same shape but `connector "http" returned HTTP 500` | upstream server failed; retries off by default | set `policy.retry_attempts = N` in the connector's default policy, OR override via `connector://http?...&...&retry_attempts=N` (when this URL param is wired in a future milestone) |
| `url_not_allowed` | host not in the connector's allowlist | adjust allowlist or use an allowed host |
| `response_too_large` | body exceeded 1 MiB cap | raise `max_response_bytes` (currently hardcoded to default in c75) |

## Adding a new connector

```rust
use solflow_controller::{Connector, ConnectorInvocation, ConnectorMeta,
                         ConnectorOutcome, ConnectorError, InvocationPolicy};

struct SlackConnector { /* … */ }

#[async_trait::async_trait]
impl Connector for SlackConnector {
    fn meta(&self) -> ConnectorMeta {
        ConnectorMeta {
            name: "slack".into(),
            description: "Slack chat connector".into(),
            version: "0.1.0".into(),
            default_policy: InvocationPolicy::default(),
        }
    }
    async fn invoke(
        &self,
        invocation: ConnectorInvocation,
    ) -> Result<ConnectorOutcome, ConnectorError> {
        // your implementation
        todo!()
    }
}

// Register at controller boot:
let controller = LocalController::new(persistence)
    .with_connector(Arc::new(SlackConnector::new(...)));
```

## Related docs

- [Local Controller](./CONTROLLER_LOCAL.md) — boot + connect
- [Phase C Roadmap](./PHASE_C_ROADMAP.md) — milestone status
- `controller/src/connector/mod.rs` — trait + registry source
- `controller/src/connector/http.rs` — HTTP connector source
- `runtime/src/extcall.rs` — VM hook surface
