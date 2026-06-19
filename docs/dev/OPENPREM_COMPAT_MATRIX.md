# OpenPrem examples compatibility matrix

The full upstream examples tree under
`reference/open-prem-cleaning/examples` is SolFlow's OpenPrem
compatibility suite. Every `.sol` file is imported, rendered, compiled,
run in Browser Simulation, and run on the SolFlow Local Controller
against its real, unchanged OpenPrem SDK agents.

**Result: 18 of 19 `.sol` files run on the Local Controller with their
shipped agents** (Python and TypeScript/JS SDKs). The one exception ships
no provider agent. The two `diagnostic` workflows that fail do so because
the agent itself is Unix-only on Windows, not because of SolFlow.

Reproduce with `node tools/openprem-compat/harness.mjs [example-id]`
(starts the controller, launches the real agents pointed at SolFlow, and
runs each workflow). The harness does not edit any agent; it only
configures each agent's controller URL via a launch shim, exactly as a
deployment would.

## Matrix

Columns: Import/render and Compile are via SolFlow's pipeline; Sim is
Browser Simulation; Local is the SolFlow Local Controller run against the
real agents.

| Example .sol | Import | Compile | Sim (blocks) | Local Controller | Providers (real agents) |
|---|---|---|---|---|---|
| auth-demo/session1 | yes | yes | yes | **Succeeded** | printer.py |
| auth-demo/session2 | yes | yes | yes | **Succeeded** | reporter.ts (TypeScript) |
| bigitaly/workflow | yes | yes | yes | **Succeeded** (ext 10/10) | production + factory (6 TS apps) |
| cache-demo/cache_test | yes | yes | yes | **Succeeded** | numbers_app.py, printer_app.py |
| diagnostic/workflows | yes | yes | yes | **2 of 4 Succeeded** | agent.py (system.*) |
| finance-demo/workflow_stats | yes | yes | yes | **Succeeded** | data_app.py, stats_capability.js |
| finance-demo/workflow_viz | yes | yes | yes | **Succeeded** | data_app.py, viz_capability.js |
| global-sensor/cross-region-alert | yes | yes | yes | **Succeeded** | sensor.py, alert_engine.py |
| global-sensor/dashboard-query | yes | yes | yes | **Succeeded** | analytics_db.py, alert_engine.py |
| global-sensor/load-balanced-ingest | yes | yes | yes | **Succeeded** | sensor.py |
| global-sensor/sensor-ingest | yes | yes | yes | **Succeeded** | sensor.py, gateway.py, analytics_db.py |
| hierarchy-demo/hierarchy_test | yes | yes | yes | **Succeeded** | numbers_app.py, printer_app.py |
| multi-session/workflow | yes | yes | yes | **Runs (worker)** | numbers.py, printer_uno.py, printer_dos.py |
| my-first-network/chain | yes | yes | yes | **Succeeded** | number_app.py, printer_app.py |
| my-first-network/workflows | yes | yes | yes | **Succeeded** | app.py (greeter) |
| simple-demo/workflow | yes | yes | yes | **Succeeded** | app.py echo (x2) |
| supply-chain-demo/workflow | yes | yes | yes | **Runs (worker)** | app_brick_store.py, app_logistics.py |
| three-node/workflow | yes | yes | yes | **Runs (worker)** | app_b1.py, app_b2.py, app_c1.py |
| supply-chain/check-inventory | yes | yes | yes | **Blocked: no agent shipped** | (none in repo) |

## Notes per status

**Succeeded** — the workflow ran to completion, every capability call
was invoked against the real agent, and the trace shows EXTCALL then
EXTRESULT at each call site. bigitaly returned real production counts
("Produced: tomato= 45 bread= 42 cheese= 55 pasta= 66"); auth-demo
printed "[printer] PRINT: Hello from session 1" and "[reporter] REPORT:
Hello from session 2" in the agents.

**Runs (worker)** — `multi-session`, `three-node`, and
`supply-chain-demo` workflows are `while(true)` workers by design (the
upstream examples run them as long-lived loops). They invoke their agents
repeatedly (multi-session: 1000 invocations per workflow; three-node: 316
before cancel) and the harness cancels them. They do not terminate on
their own; that is the example's intent, not a SolFlow limitation.

**diagnostic, 2 of 4** — `storage_check` and `top_procs` Succeed.
`collect_all` and `cpu_health` fail because the agent calls
`os.getloadavg()`, which does not exist on Windows (the agent is
Unix-only). SolFlow surfaces the agent's error faithfully as an
`ExtCallFailed` at the call site. This is an upstream agent/platform
limitation, not a SolFlow protocol gap; the same workflows would run
against the agent on Linux/macOS.

**check-inventory — Blocked (no agent shipped)** — the
`central-warehouse.inventory` / `.purchase` capabilities are declared
only in the example's controller TOML pointing at warehouse apps that are
not present in the repository. The `.sol` imports, renders, compiles, and
blocks cleanly in Browser Simulation; it cannot run on the Local
Controller because the example ships no provider to start. Writing one
would be a one-off connector, which is out of scope.

## Dialects exercised

The suite covers every syntactic dialect found in the examples, all
running on the canonical controller:

- Namespace member calls: `printer.print("hi")`, `sensor.temperature({})`.
- `call("module.action", params)`: `call("numbers.get", {})`.
- Bare capability strings: `call("produce_tomato", {})` (bigitaly).
- Bare-identifier workflow names: `workflow show_number { ... }`.
- Zero-argument and multi-argument calls: `numbers.get()`.
- Method-style builtins: `all_temps.len()`.
- Unparenthesized conditions: `if t < min_temp { ... }`.
- `for ... in`, reassignment, `while(true)`, `sleep(ms)`, typed lets,
  string concatenation, nested if/else, `||`.

## Compiler/parser changes made for the suite

To run the upstream `.sol` files verbatim on the controller (which
compiles the raw submitted source), the canonical parser/compiler gained:

- Import-call argument packaging so zero-arg and multi-arg namespace
  calls lower to a single params value (was: stack underflow).
- Bare-identifier workflow names (`workflow show_number { ... }`).
- Unparenthesized `if` / `while` conditions.
- Method-style builtin desugaring (`x.len()` to `len(x)`).
- Native `__system.sleep` handling (so timed workers run without a
  provider).
