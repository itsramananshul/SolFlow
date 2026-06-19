"""SolFlow compatibility fixture: a real OpenPrem SDK central-warehouse agent.

The upstream example `examples/supply-chain` declares the
`central-warehouse.inventory` / `central-warehouse.purchase` capabilities only
in its controller TOML; it ships NO provider implementation. This fixture
supplies one so `supply-chain/check-inventory.sol` can run end to end on the
SolFlow Local Controller.

This is NOT an unchanged upstream agent. It is a SolFlow-maintained
compatibility fixture. It is, however, a genuine OpenPrem SDK provider: it uses
the upstream `openprem.Application` SDK, registers via `POST /register`, and is
invoked with the upstream request shape, exactly like the real example agents.
It does not use SolFlow's `SOLFLOW_CONNECTORS` format.

The behavior is inferred from `check-inventory.sol` and `ctrl-east.toml`:
  - `central-warehouse.inventory({})` returns the current stock as an int.
  - `central-warehouse.purchase({shop, brick_type, count})` adds `count` units
    and returns a human-readable confirmation string.

Usage:
    PYTHONPATH=.../sdk/python python central_warehouse_fixture.py \
        [controller_url=http://127.0.0.1:3939] [port=9210] [initial_stock=50]
"""
import sys

from openprem import Application

CONTROLLER = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:3939"
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 9210
# Default below 100 so the workflow's `if (inv < 100)` purchase branch runs.
STOCK = {"value": int(sys.argv[3]) if len(sys.argv) > 3 else 50}

app = Application(
    name="central-warehouse",
    controller=CONTROLLER,
    listen=("0.0.0.0", PORT),
)


@app.capability("inventory")
def inventory(params=None):
    print(f"  [central-warehouse] INVENTORY -> {STOCK['value']}", flush=True)
    return STOCK["value"]


@app.capability("purchase")
def purchase(params=None):
    p = params if isinstance(params, dict) else {}
    shop = p.get("shop", "unknown")
    brick = p.get("brick_type", "brick")
    count = int(p.get("count", 0))
    STOCK["value"] += count
    msg = f"Purchased {count} {brick} bricks for {shop}; stock now {STOCK['value']}"
    print(f"  [central-warehouse] PURCHASE -> {msg}", flush=True)
    return msg


if __name__ == "__main__":
    app.run()
