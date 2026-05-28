# Running SolFlow with a remote controller

**Phase C C.7 (shipped 2026-05-28).** This guide explains how to
run `solflow-controller` on one machine and connect an editor on
another. It builds on [CONTROLLER_LOCAL.md](./CONTROLLER_LOCAL.md);
read that first if you haven't tried a local setup.

## What C.7 added

| Feature | Before C.7 | After C.7 |
|---|---|---|
| Transport | HTTP only (localhost) | HTTP **or** HTTPS, locally or remotely |
| Auth | None (any client could submit workflows) | Optional shared bearer token, enforced on every protected endpoint |
| Version compat | Editor probed `host_spec_major` on connect | Same, plus `Health.name` fingerprint + `Health.auth_required` capability probe |
| Editor UX | URL field only | URL + token field, transport badge, unsafe-HTTP warning, version + auth status surfaced |

What C.7 deliberately did **not** add:

- Per-user identity / RBAC — single shared token is the whole
  auth surface. Multi-tenant identity lands in Phase D.
- Token rotation flows / refresh — restart the controller with a
  new token; editor re-prompts for it.
- Cluster / multi-controller coordination — also Phase D.

## Architecture

```
            ┌─────────────────────┐
            │   SolFlow editor    │  (your laptop / browser)
            │  ─ URL: https://X   │
            │  ─ token: ******    │
            └──────────┬──────────┘
                       │  TLS (rustls)
                       │  Authorization: Bearer …
                       ▼
            ┌─────────────────────┐
            │ solflow-controller  │  (remote box / cloud VM)
            │  ─ axum + rustls    │
            │  ─ SQLite (./db)    │
            │  ─ HTTP connector   │
            └─────────────────────┘
```

`/healthz` is always open so editors can fingerprint + capability-
probe a controller before sending credentials. Every other
endpoint requires the bearer token when `AuthConfig::Bearer` is
configured.

## Quickstart

### 1. Mint a token

A long random string. 32+ bytes from `/dev/urandom`:

```bash
# any of these work — pick one matching your shell
openssl rand -hex 32
head -c 32 /dev/urandom | xxd -p -c 64
python3 -c 'import secrets; print(secrets.token_hex(32))'
```

Store it somewhere your operator can hand it to editor users.
Not in source control.

### 2. Mint a TLS cert

For real public deployments, use a real CA (Let's Encrypt, your
org's internal PKI). For staging / lab use, a self-signed cert
is fine and the editor will accept it as long as your browser /
OS does.

```bash
# Self-signed for a single hostname (replace 'controller.lab'):
openssl req -x509 -newkey rsa:4096 -sha256 \
  -days 365 -nodes \
  -keyout key.pem \
  -out cert.pem \
  -subj "/CN=controller.lab" \
  -addext "subjectAltName=DNS:controller.lab"
```

The controller accepts standard PEM-encoded cert chains + a
matching PKCS#8 or RSA private key, both as separate files.

### 3. Boot with TLS + auth

```bash
SOLFLOW_CONTROLLER_BIND=0.0.0.0:3939 \
SOLFLOW_CONTROLLER_DB=/var/lib/solflow/db.sqlite \
SOLFLOW_CONTROLLER_TLS_CERT=/etc/solflow/cert.pem \
SOLFLOW_CONTROLLER_TLS_KEY=/etc/solflow/key.pem \
SOLFLOW_CONTROLLER_AUTH_TOKEN="$(cat /etc/solflow/token)" \
./solflow-controller
```

You'll see startup logs like:

```
INFO solflow_controller: starting solflow-controller bind=0.0.0.0:3939 db_path=/var/lib/solflow/db.sqlite
INFO solflow_controller: run policy step_limit=10000000 wall_clock_secs=600
INFO solflow_controller: auth: bearer-token required on protected endpoints
INFO solflow_controller: transport: HTTPS (rustls)
INFO solflow_controller: listening on https://0.0.0.0:3939
```

The controller refuses to start if exactly one of the two TLS
env vars is set (half-configured TLS is more dangerous than
none).

### 4. Connect from the editor

In the editor: Toolbar → Controller Settings (server icon) →
paste the URL (`https://controller.lab:3939`) + token. The
modal:

- shows a green **remote · HTTPS** badge once the URL parses
- shows a red **remote · HTTP ⚠** banner if you typed `http://`
  to a non-loopback host (cleartext = leaks token + bytecode)
- on Connect, calls `/healthz` first to fingerprint the
  controller, then `/connectors` to populate the available list
- surfaces auth + version mismatches with code-specific
  guidance, not just "HTTP 401"

## URL classification

The editor's `classifyControllerUrl(url)` (Phase C c99) drives the
transport badge + warnings. The full table:

| Input URL | `kind` | Editor renders |
|---|---|---|
| `http://localhost:3939` | `local` | `local · HTTP` badge (blue). No warnings. |
| `http://127.0.0.1:3939` | `local` | same |
| `http://[::1]:3939` | `local` | same |
| `https://localhost:3939` | `loopback_https` | `local · HTTPS` (green). Probably overkill for loopback but fine. |
| `https://controller.lab` | `https_remote` | `remote · HTTPS` (green). Recommended remote setup. |
| `http://controller.lab` | `unsafe_remote` | `remote · HTTP ⚠` (red) + full warning banner |
| `controller.lab:3939` | `invalid` (no_scheme) | URL field gets a red inline error |
| `file:///tmp/x` | `invalid` (bad_scheme) | same |

URLs are **never** silently upgraded from `http://` to `https://`
— the URL is the user's typed intent. If the user genuinely wants
HTTPS they can type it; the warning teaches them why.

## Auth failure modes

When the controller rejects a request, it returns `401
Unauthorized` with a structured JSON body. The editor maps each
case to specific guidance in the modal:

| `code` in response | Editor banner |
|---|---|
| `auth_missing` | "The controller requires a bearer token. Paste one into the Authentication field above and re-try." |
| `auth_mismatch` | "The token you sent doesn't match the controller's. Re-check the token from your operator and re-try." |
| `auth_malformed` | "Your token header is malformed. Make sure the value is just the token — the client adds the 'Bearer ' prefix automatically." |
| `unauthorized` (fallback) | "The controller refused your credentials. Re-check your token, then re-try." |

Setting a new token clears the stale `error{auth}` state so the
user can retry immediately.

## Environment variables

The controller reads everything from env vars (no CLI flags yet).
Full reference is in [CONTROLLER_OPERATIONS.md](./CONTROLLER_OPERATIONS.md).
Quick summary of the C.7 additions:

| Var | Default | Meaning |
|---|---|---|
| `SOLFLOW_CONTROLLER_TLS_CERT` | unset | PEM cert path. Both this + key must be set for HTTPS. |
| `SOLFLOW_CONTROLLER_TLS_KEY` | unset | PEM key path. |
| `SOLFLOW_CONTROLLER_AUTH_TOKEN` | unset | Bearer token. Empty/unset = no auth; non-empty = required on every protected endpoint. |

Pre-C.7 env vars (`_BIND`, `_DB`, `_STEP_LIMIT`,
`_TIMEOUT_SECS`, `_MAX_OUTPUT_LINES`,
`_MAX_EVENTS_PER_RUN`) still work unchanged.

## Smoke testing remote mode

The shortest end-to-end smoke that exercises the full C.7 stack:

```bash
# Terminal 1 — controller with TLS + auth, on the loopback for
# this smoke (no real DNS needed)
openssl req -x509 -newkey rsa:4096 -sha256 -days 30 -nodes \
  -keyout /tmp/k.pem -out /tmp/c.pem \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

SOLFLOW_CONTROLLER_BIND=127.0.0.1:13443 \
SOLFLOW_CONTROLLER_TLS_CERT=/tmp/c.pem \
SOLFLOW_CONTROLLER_TLS_KEY=/tmp/k.pem \
SOLFLOW_CONTROLLER_AUTH_TOKEN=demo-token-abc \
./target/release/solflow-controller

# Terminal 2 — probe healthz (open even with auth on)
curl -k https://127.0.0.1:13443/healthz | jq

# {
#   "ok": true,
#   "controller_version": "0.1.0",
#   "host_spec_major": 0,
#   "name": "solflow-controller",
#   "auth_required": true
# }

# Protected route without token → 401 auth_missing
curl -k -i https://127.0.0.1:13443/controller/concurrency

# With token → 200 + metrics
curl -k -H "Authorization: Bearer demo-token-abc" \
  https://127.0.0.1:13443/controller/concurrency | jq
```

Now in the editor: paste `https://127.0.0.1:13443` + the token,
click Connect. The browser may warn about the self-signed cert
the first time; accepting it once persists for that browser
session.

## Failure modes + troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Controller refuses to start: "TLS misconfigured: SOLFLOW_CONTROLLER_TLS_CERT is set but SOLFLOW_CONTROLLER_TLS_KEY is not" | Half-configured TLS | Set both env vars, or neither |
| Controller refuses to start: "TLS cert/key load failed (...)" | File missing / permission denied / not PEM | Check `ls -la` on both paths; convert via `openssl x509 -inform DER -in cert.der -out cert.pem` if you have a DER-encoded cert |
| Editor shows "Auth rejected · auth_missing" but you set the token | Token field empty in the modal | Open Controller Settings; paste the token; click Re-check |
| Editor shows "Auth rejected · auth_mismatch" | Token mismatch | Confirm with operator; copy-paste exactly (whitespace matters) |
| Editor shows "remote · HTTP ⚠" warning | Connecting to a non-loopback host over plain HTTP | Either accept the risk (only fine if you're inside a known-private network) or set up HTTPS — see "Mint a TLS cert" above |
| Editor connects but no connectors listed | Controller is pre-C.4 or `/connectors` route is blocked by a reverse proxy | Check `curl -H "Authorization: Bearer …" https://.../connectors` — if it 404s, the controller is too old; if it 401s, your token isn't reaching through the proxy |
| Health response succeeds but `name` is shown as "(pre-C.7 controller)" | Older controller without the C.7 Health expansion | Upgrade the controller binary; the editor will still talk to it, but UX defaults are conservative |

## Deployment notes

This guide deliberately avoids prescribing a deployment topology
— SolFlow is a single binary + a SQLite file; whether you run it
behind nginx, in a systemd service, in a container, or directly
on a port your firewall opens is up to you. A few minimum
sanities:

- **Never deploy HTTPS-without-auth.** A controller exposed on
  the internet without a token is a remote-code-execution
  surface for anyone who reaches it. The binary logs a warning
  on startup when it detects this misconfiguration.
- **Bind to the right interface.** The default
  `SOLFLOW_CONTROLLER_BIND=127.0.0.1:3939` is loopback-only;
  switch to `0.0.0.0:3939` (or a specific network interface)
  only when you've also enabled TLS + auth.
- **Persist the SQLite file across restarts.** It carries run
  history + schedules + cancel-requested bits that recovery
  relies on. A fresh DB on every boot loses observability +
  re-fires schedules from `next_fire_at = NULL` which is rarely
  what you want.
- **Mount cert/key paths read-only.** axum-server reads them
  once at startup; rotation requires a restart.

## Related docs

- [Local controller](./CONTROLLER_LOCAL.md) — the local-dev
  walkthrough this guide builds on
- [Controller operations](./CONTROLLER_OPERATIONS.md) — full env
  var reference + log format + lifecycle of a request
- [Run lifecycle](./RUN_LIFECYCLE.md) — what happens INSIDE the
  controller once a run is enqueued (Phase C.6)
- [Phase C architecture](./PHASE_C_ARCHITECTURE.md) — the
  locked design Phase C builds against
