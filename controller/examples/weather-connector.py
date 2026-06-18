#!/usr/bin/env python3
"""
Sample SolFlow connector endpoint.

The controller calls this for every external Action whose module is
registered in SOLFLOW_CONNECTORS. It POSTs JSON of the form:

    { "function": "<rpc name>", "params": { ...call args... } }

and uses the JSON body you return as the SOL return value of the call.
Edit the `respond` function so the fields match what your workflow reads
off the result (e.g. if your workflow does `r.temp_c`, return `temp_c`).

Run:  python3 weather-connector.py          # listens on 127.0.0.1:8088
"""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json

HOST, PORT = "127.0.0.1", 8088


def respond(function: str, params: dict) -> dict:
    # Route by the called function name. Return real data here; this is
    # a demo source you control, not a fabricated value inside the engine.
    if function == "read":
        return {"temp_c": 5, "wind_kph": 30, "alert": True}
    return {"ok": True}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        try:
            req = json.loads(self.rfile.read(length) or b"{}")
        except Exception:
            req = {}
        body = json.dumps(respond(req.get("function", ""), req.get("params", {}))).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        print("call:", self.path, flush=True)


if __name__ == "__main__":
    print(f"connector endpoint listening on http://{HOST}:{PORT}")
    HTTPServer((HOST, PORT), Handler).serve_forever()
