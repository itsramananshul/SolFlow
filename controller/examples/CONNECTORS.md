# Controller connectors (external Actions)

The controller runs canonical SOL itself. When a workflow calls an
external Action (an import-qualified call such as `weather_station.read({...})`),
the controller resolves the module to an HTTP endpoint and calls it for real.

## Register endpoints

Set `SOLFLOW_CONNECTORS` to a JSON object mapping each module name to its
base URL before starting the controller:

```
# PowerShell
$env:SOLFLOW_CONNECTORS = '{"weather_station":"http://127.0.0.1:8088"}'
./target/release/solflow-controller
```

A module with no registered endpoint stays honestly blocked
(`ExtCallBlocked`), exactly like the browser sim.

## Endpoint contract

For each Action the controller sends:

```
POST <base-url>
Content-Type: application/json

{ "function": "<rpc name>", "params": { ...call args... } }
```

Your endpoint returns a JSON body. That body becomes the SOL return value
of the call: a JSON object becomes a struct (so `r.temp_c` reads the
`temp_c` field), a number becomes an int or float, a string becomes a
string, an array becomes an array.

`controller/examples/weather-connector.py` is a runnable sample. Edit its
`respond` function so the returned fields match what your workflow reads.
