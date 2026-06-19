# OpenPrem providers (the canonical provider system)

SolFlow is the visual IDE, runtime, and debugger for OpenPrem-style
workflows. Its Local Controller plays the role of **one OpenPrem
controller**: real upstream OpenPrem SDK agents (Python, TypeScript,
Rust, and the other language SDKs) register with it and SolFlow invokes
them directly, using the upstream controller-to-agent wire contract.

This is the primary provider path. The older `SOLFLOW_CONNECTORS`
registry (see `CONNECTORS.md`) remains only as an internal/dev/test
fallback and is no longer the product provider model.

## How it works

```
  OpenPrem SDK agent                SolFlow Local Controller
  (printer.py, reporter.ts, ...)    (127.0.0.1:3939)

  app.run()  ──POST /register──▶   stores name.action + bare action
                                    ── ▶ endpoint URL

  workflow:  printer.print("hi")
             │  capability "printer.print"
             ▼
            resolve(module, func) ─▶ agent endpoint
             │
             ▼
  ◀──POST <endpoint>/ ──  { "...params", "capability": "printer.print" }
   returns JSON  ──────────────────▶  fed back into the workflow
```

### Registration: `POST /register`

Agents POST the standard OpenPrem registration body (identical across
every language SDK):

```json
{ "name": "printer",
  "actions": [{ "name": "print" }],
  "endpoint": "http://127.0.0.1:9301",
  "endpoints": { "http": { "url": "http://127.0.0.1:9301" } },
  "public_key": "<base64 ed25519>" }
```

SolFlow stores each action under both `name.action` (`printer.print`)
and the bare `action` (`print`), matching the upstream controller's
double-keying. Registered agents appear in `GET /providers` and in the
editor's Controller Settings ("Registered providers"), tagged
`OpenPrem agent` with their action list.

### Invocation

When a workflow makes an external call, SolFlow resolves the capability
against the registry and POSTs to the agent's endpoint root with the
params flattened and the capability merged in, exactly as upstream's
`invoke_local_action`:

```text
object params -> { ...params, "capability": "<cap>" }
scalar params -> { "capability": "<cap>", "params": <scalar> }
```

The capability forms a workflow can use all resolve here:

| Workflow syntax | Capability | Resolves via |
|---|---|---|
| `printer.print("hi")` | `printer.print` | `name.action` map |
| `call("numbers.get", {})` | `numbers.get` | `name.action` map |
| `call("produce_tomato", {})` | `produce_tomato` | bare `action` map |
| `sensor.temperature({})` | `sensor.temperature` | dotted-action map |

### Auth (local/dev mode is unauthenticated)

Upstream Python and Rust agents only enforce Ed25519 request signatures
once they receive a `controller_public_key` at registration. SolFlow
**deliberately omits** that field from the `/register` response, so real
agents register and run unauthenticated. This is the documented
local/dev mode. Signing is future work; until then, run agents and the
controller on a trusted local network (the default `127.0.0.1` bind).

## Running an OpenPrem agent with SolFlow

1. Start the controller (auth off by default):

   ```sh
   cargo run -p solflow_controller --bin solflow-controller   # 127.0.0.1:3939
   ```

2. Start an unchanged upstream agent pointed at SolFlow. For the Python
   SDK, set the controller URL via the `Application(controller=...)`
   argument (the only change is configuration, not agent code):

   ```sh
   # printer.py with controller="http://127.0.0.1:3939"
   PYTHONPATH=.../sdk/python python printer.py
   ```

3. Inspect registrations:

   ```sh
   curl http://127.0.0.1:3939/providers
   # [{ "module": "printer", "url": "http://127.0.0.1:9301",
   #    "actions": ["print"], "kind": "openprem" }]
   ```

4. Submit a workflow (`POST /workflows` then `POST /runs`), or run it
   from the SolFlow editor against the Local Controller target. The
   `printer.print(...)` call is invoked against the real agent; the
   trace shows `EXTCALL` then `EXTRESULT` at the call's source line.

## Behavior guarantees

- **Browser Simulation** still blocks every external call clearly
  (`ExtCallBlocked`) — there is no network in the browser by design.
- **Missing provider** fails with a clear, source-mapped error naming
  the capability and how to fix it (start the agent and register it).
- **Agent error envelope** (`{"error": "..."}`) is surfaced as a
  provider failure (`ExtCallFailed`) tied to the call site, an honest
  improvement over the upstream controller, which passes it through.
- **Trace** records `EXTCALL` and `EXTRESULT` with source spans, and
  source-line highlighting works on failure.

## Compatibility with the upstream examples

The full upstream examples tree under
`reference/open-prem-cleaning/examples` is a compatibility suite. See
`OPENPREM_COMPAT_MATRIX.md` for the per-file status (import, render,
compile, Browser Simulation, Local Controller, providers, and the exact
blocker for anything that cannot run with its shipped agents).
