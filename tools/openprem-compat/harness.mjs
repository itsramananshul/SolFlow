/**
 * OpenPrem upstream examples compatibility harness.
 *
 * For each example it: starts a fresh SolFlow controller, TCP-proxies the
 * upstream controller ports the example's agents expect onto SolFlow, launches
 * the real OpenPrem SDK agents UNCHANGED, then submits each workflow to SolFlow
 * Local Controller and records the real result (status, output, return value,
 * trace EXTCALL/EXTRESULT counts).
 *
 * SolFlow plays the role of one OpenPrem controller: agents register with it
 * via POST /register and it invokes them directly. The proxy exists only so an
 * agent that hardcodes `controller="http://localhost:8082"` reaches SolFlow
 * without editing the agent.
 *
 * Usage:
 *   node tools/openprem-compat/harness.mjs            # all manifest entries
 *   node tools/openprem-compat/harness.mjs auth-demo  # one example by id
 */
import net from 'node:net';
import { spawn } from 'node:child_process';
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const REPO = 'D:/DATA/WORK/OpenPrem/Apps/SolFlow';
const ROOT = REPO + '/reference/open-prem-cleaning';
const SDK_PY = ROOT + '/sdk/python';
const CTRL_BIN = REPO + '/target/debug/solflow-controller.exe';
const CTRL = 'http://127.0.0.1:3939';

const TERMINAL = ['Succeeded', 'Failed', 'Cancelled', 'TimedOut', 'Rejected'];

// ---------------------------------------------------------------------------
//  Controller client
// ---------------------------------------------------------------------------

async function jget(path) {
  const r = await fetch(CTRL + path);
  const t = await r.text();
  try {
    return { status: r.status, body: JSON.parse(t) };
  } catch {
    return { status: r.status, body: t };
  }
}
async function jpost(path, body) {
  const r = await fetch(CTRL + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const t = await r.text();
  try {
    return { status: r.status, body: JSON.parse(t) };
  } catch {
    return { status: r.status, body: t };
  }
}

async function waitHealthy(ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      const r = await fetch(CTRL + '/healthz');
      if (r.ok) return true;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }
  return false;
}

async function runWorkflow(source, displayName) {
  const sub = await jpost('/workflows', {
    name: displayName,
    description: null,
    bytecode: [...Buffer.from(source, 'utf8')],
    instruction_spans: [...Buffer.from('[]', 'utf8')],
    source,
  });
  if (sub.status !== 200 || !sub.body?.workflow_id) {
    return { status: 'SubmitFailed', detail: sub.body };
  }
  const run = await jpost('/runs', {
    workflow_id: sub.body.workflow_id,
    trigger: { kind: 'Manual' },
    inputs: {},
  });
  if (!run.body?.run_id) return { status: 'RunFailed', detail: run.body };
  const runId = run.body.run_id;

  let last = null;
  for (let i = 0; i < 60; i++) {
    await sleep(250);
    const g = await jget('/runs/' + runId);
    last = g.body;
    if (TERMINAL.includes(last?.status)) return last;
  }
  // Still running (e.g. while(true) worker). Cancel and report the snapshot.
  await fetch(CTRL + '/runs/' + runId, { method: 'DELETE' });
  await sleep(300);
  const g = await jget('/runs/' + runId);
  return { ...g.body, _longRunning: true };
}

// ---------------------------------------------------------------------------
//  TCP proxy: upstream controller port -> SolFlow (3939)
// ---------------------------------------------------------------------------

function startProxy(listenPort) {
  return new Promise((resolve, reject) => {
    const srv = net.createServer((c) => {
      const up = net.connect(3939, '127.0.0.1');
      c.pipe(up);
      up.pipe(c);
      c.on('error', () => up.destroy());
      up.on('error', () => c.destroy());
    });
    srv.on('error', reject);
    srv.listen(listenPort, '127.0.0.1', () => resolve(srv));
  });
}

// ---------------------------------------------------------------------------
//  Process management
// ---------------------------------------------------------------------------

function startController(dbPath) {
  const env = {
    ...process.env,
    SOLFLOW_CONTROLLER_BIND: '127.0.0.1:3939',
    SOLFLOW_CONTROLLER_DB: dbPath,
    SOLFLOW_CONTROLLER_TIMEOUT_SECS: '30',
    RUST_LOG: 'warn',
  };
  return spawn(CTRL_BIN, [], { env, stdio: 'ignore' });
}

const SHIM = REPO + '/tools/openprem-compat/shim.py';
const NODE_SHIM = REPO + '/tools/openprem-compat/node-shim.cjs';

function startAgent(dir, file, args, logBuf, opts = {}) {
  const env = {
    ...process.env,
    PYTHONPATH: SDK_PY,
    PYTHONUNBUFFERED: '1',
    // Force UTF-8 stdio so agents that print non-ASCII (e.g. "→", "°C") don't
    // raise a cp1252 UnicodeEncodeError on Windows consoles.
    PYTHONUTF8: '1',
    PYTHONIOENCODING: 'utf-8',
  };
  // Fixture agents (SolFlow compatibility providers) live under tools/, not
  // under the upstream examples tree.
  const cwd = opts.fixture ? join(REPO, 'tools', 'openprem-compat') : join(ROOT, 'examples', dir);
  let p;
  if (opts.node) {
    // TypeScript/JS SDK agent through the Node shim.
    p = spawn('node', [NODE_SHIM, CTRL, file, ...args], { cwd, env });
  } else if (opts.raw) {
    // Non-SDK agent (e.g. simple-demo's raw echo server) runs directly.
    p = spawn('python', [file, ...args], { cwd, env });
  } else {
    // Python SDK agent through the shim (repoints controller URL at SolFlow).
    p = spawn('python', [SHIM, CTRL, file, ...args], { cwd, env });
  }
  const cap = (d) => logBuf.push(d.toString());
  p.stdout.on('data', cap);
  p.stderr.on('data', cap);
  return p;
}

function kill(p) {
  if (!p || p.killed) return;
  try {
    // Windows: kill the tree.
    spawn('taskkill', ['/pid', String(p.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    try {
      p.kill('SIGKILL');
    } catch {
      /* ignore */
    }
  }
}

// ---------------------------------------------------------------------------
//  Workflow extraction (run each workflow in a multi-workflow file)
// ---------------------------------------------------------------------------

function extractWorkflows(src) {
  const imports = src
    .split('\n')
    .filter((l) => l.trim().startsWith('import '))
    .join('\n');
  const out = [];
  const re = /workflow\s+("([^"]+)"|[A-Za-z_][A-Za-z0-9_-]*)\s*\{/g;
  let m;
  while ((m = re.exec(src))) {
    const name = m[2] ?? m[1];
    const braceStart = re.lastIndex - 1;
    let depth = 0;
    let inStr = false;
    let esc = false;
    let end = -1;
    for (let j = braceStart; j < src.length; j++) {
      const ch = src[j];
      if (inStr) {
        if (esc) esc = false;
        else if (ch === '\\') esc = true;
        else if (ch === '"') inStr = false;
        continue;
      }
      if (ch === '"') inStr = true;
      else if (ch === '{') depth++;
      else if (ch === '}') {
        depth--;
        if (depth === 0) {
          end = j;
          break;
        }
      }
    }
    if (end < 0) continue;
    const block = src.slice(m.index, end + 1);
    out.push({ name, source: (imports ? imports + '\n\n' : '') + block });
  }
  return out;
}

// ---------------------------------------------------------------------------
//  Manifest: how to stand up each example's agents
// ---------------------------------------------------------------------------
//
//  proxies: upstream controller ports the agents hardcode -> forwarded to 3939
//  agents:  { dir, file, args }  (args may include the controller URL / port)
//  sols:    [{ path, run: [workflowNames] | 'first' | 'all' }]

const MANIFEST = [
  {
    id: 'auth-demo',
    proxies: [],
    agents: [
      { dir: 'auth-demo', file: 'printer.py' },
      { dir: 'auth-demo', file: 'build/reporter.js', node: true },
    ],
    sols: [
      { path: 'auth-demo/session1.sol', run: 'all' },
      { path: 'auth-demo/session2.sol', run: 'all' },
    ],
    note: 'session1 = Python printer agent; session2 = TypeScript reporter agent.',
  },
  {
    id: 'cache-demo',
    proxies: [], // shim redirects Application agents to 3939
    agents: [
      { dir: 'cache-demo', file: 'numbers_app.py' },
      { dir: 'cache-demo', file: 'printer_app.py' },
    ],
    sols: [{ path: 'cache-demo/cache_test.sol', run: 'all' }],
  },
  {
    id: 'hierarchy-demo',
    proxies: [],
    agents: [
      { dir: 'hierarchy-demo', file: 'numbers_app.py' },
      { dir: 'hierarchy-demo', file: 'printer_app.py' },
    ],
    sols: [{ path: 'hierarchy-demo/hierarchy_test.sol', run: 'all' }],
  },
  {
    // chain.sol uses numbers (9300) + printer (9301).
    id: 'my-first-network-chain',
    proxies: [],
    agents: [
      { dir: 'my-first-network', file: 'number_app.py' },
      { dir: 'my-first-network', file: 'printer_app.py' },
    ],
    sols: [{ path: 'my-first-network/chain.sol', run: 'all' }],
  },
  {
    // workflows.sol uses greeter (app.py, also 9300) — run separately so it
    // does not collide with number_app on 9300 (an upstream example detail:
    // the two agent sets are for different workflows).
    id: 'my-first-network-greeter',
    proxies: [],
    agents: [{ dir: 'my-first-network', file: 'app.py' }],
    sols: [{ path: 'my-first-network/workflows.sol', run: 'all' }],
  },
  {
    id: 'diagnostic',
    proxies: [],
    agents: [{ dir: 'diagnostic', file: 'agent.py', args: ['system', '9300', 'http://127.0.0.1:3939'] }],
    sols: [{ path: 'diagnostic/workflows.sol', run: 'all' }],
  },
  {
    id: 'multi-session',
    proxies: [],
    agents: [
      { dir: 'multi-session', file: 'numbers.py' },
      { dir: 'multi-session', file: 'printer_uno.py' },
      { dir: 'multi-session', file: 'printer_dos.py' },
    ],
    sols: [{ path: 'multi-session/workflow.sol', run: 'all' }],
    note: 'workflows are while(true) workers; harness cancels after first iterations.',
  },
  {
    id: 'three-node',
    proxies: [],
    agents: [
      { dir: 'three-node', file: 'app_b1.py' },
      { dir: 'three-node', file: 'app_b2.py' },
      { dir: 'three-node', file: 'app_c1.py' },
    ],
    sols: [{ path: 'three-node/workflow.sol', run: 'all' }],
    note: 'workflow is a while(true) worker; harness cancels after first iterations.',
  },
  {
    id: 'bigitaly',
    proxies: [],
    // TypeScript apps (built with `npm i && npx tsc` in apps/production and
    // apps/factory). Bare-capability-string dialect: call("produce_tomato",{}).
    agents: [
      { dir: 'bigitaly', file: 'apps/production/dist/index.js', args: ['tomato'], node: true },
      { dir: 'bigitaly', file: 'apps/production/dist/index.js', args: ['bread'], node: true },
      { dir: 'bigitaly', file: 'apps/production/dist/index.js', args: ['cheese'], node: true },
      { dir: 'bigitaly', file: 'apps/production/dist/index.js', args: ['pasta'], node: true },
      { dir: 'bigitaly', file: 'apps/factory/dist/index.js', args: ['pizza'], node: true },
      { dir: 'bigitaly', file: 'apps/factory/dist/index.js', args: ['spaghetti'], node: true },
    ],
    sols: [{ path: 'bigitaly/workflow.sol', run: 'all' }],
  },
  {
    id: 'finance-demo',
    proxies: [],
    // finance.get_data (Python) + statistics.summarize / visualization.graph
    // (Node JS, Ed25519-capable). Upstream submits via a signed web frontend;
    // here we submit to SolFlow directly and the agents serve unauthenticated.
    agents: [
      { dir: 'finance-demo', file: 'data_app.py' },
      { dir: 'finance-demo', file: 'stats_capability.js', node: true },
      { dir: 'finance-demo', file: 'viz_capability.js', node: true },
    ],
    sols: [
      { path: 'finance-demo/workflow_stats.sol', run: 'all' },
      { path: 'finance-demo/workflow_viz.sol', run: 'all' },
    ],
    note: 'workflow result is a trailing bare expression; SolFlow runs the calls but returns Unit.',
  },
  {
    id: 'global-sensor-network',
    proxies: [],
    agents: [
      { dir: 'global-sensor-network', file: 'apps/sensor.py', args: ['sensor', '9101'] },
      { dir: 'global-sensor-network', file: 'apps/gateway.py', args: ['gateway', '9201'] },
      { dir: 'global-sensor-network', file: 'apps/analytics_db.py', args: ['analytics', '9401'] },
      { dir: 'global-sensor-network', file: 'apps/alert_engine.py', args: ['alert', '9301'] },
    ],
    sols: [
      { path: 'global-sensor-network/workflows/sensor-ingest.sol', run: 'all' },
      { path: 'global-sensor-network/workflows/cross-region-alert.sol', run: 'all' },
      { path: 'global-sensor-network/workflows/dashboard-query.sol', run: 'all' },
      { path: 'global-sensor-network/workflows/load-balanced-ingest.sol', run: 'all' },
    ],
  },
  {
    id: 'simple-demo',
    proxies: [],
    // The echo app is a raw HTTP server (not the SDK) and does not
    // self-register; upstream declares its capabilities in the controller
    // TOML. We register them here (the SolFlow equivalent of that config).
    agents: [
      { dir: 'simple-demo', file: 'app.py', args: ['weather_station', '9100'], raw: true },
      { dir: 'simple-demo', file: 'app.py', args: ['discord_bot', '9200'], raw: true },
    ],
    register: [
      { name: 'weather_station', actions: ['read', 'status'], endpoint: 'http://127.0.0.1:9100' },
      { name: 'discord_bot', actions: ['send', 'status'], endpoint: 'http://127.0.0.1:9200' },
    ],
    sols: [{ path: 'simple-demo/workflow.sol', run: 'all' }],
    note: 'echo app declared via /register (upstream uses controller TOML [apps.*]).',
  },
  {
    // The upstream supply-chain example ships NO provider implementation
    // (central-warehouse caps are declared only in its controller TOML). This
    // entry uses SolFlow's compatibility fixture, a real OpenPrem SDK agent
    // under tools/openprem-compat/. See OPENPREM_COMPAT_MATRIX.md.
    id: 'supply-chain',
    proxies: [],
    agents: [{ fixture: true, file: 'central_warehouse_fixture.py' }],
    sols: [{ path: 'supply-chain/check-inventory.sol', run: 'all' }],
    note: 'central-warehouse provider is a SolFlow compatibility fixture; upstream ships no implementation.',
  },
  {
    id: 'supply-chain-demo',
    proxies: [8082, 8083],
    agents: [
      { dir: 'supply-chain-demo', file: 'app_brick_store.py' },
      { dir: 'supply-chain-demo', file: 'app_logistics.py' },
    ],
    sols: [{ path: 'supply-chain-demo/workflow.sol', run: 'all' }],
    note: 'uses sleep() + while(true); harness cancels after first iterations.',
  },
];

// ---------------------------------------------------------------------------
//  Runner
// ---------------------------------------------------------------------------

function summarizeRun(r) {
  if (!r) return { ok: false, status: 'no-result' };
  const out = r.output ?? {};
  const trace = out.trace ?? [];
  const extcall = trace.filter((t) => t.kind === 'extcall').length;
  const extresult = trace.filter((t) => t.kind === 'extresult').length;
  const errStep = trace.find((t) => t.kind === 'error');
  return {
    status: r.status + (r._longRunning ? ' (cancelled long-running)' : ''),
    return_value: out.return_value ?? null,
    output: out.output ?? [],
    extcall,
    extresult,
    error: errStep?.detail ?? r.diagnostics?.[0]?.message ?? null,
  };
}

async function runExample(ex) {
  const result = { id: ex.id, note: ex.note ?? null, controller: false, agents: [], providers: [], workflows: [] };
  const dbDir = mkdtempSync(join(tmpdir(), 'solflow-compat-'));
  const dbPath = join(dbDir, 'c.db');
  const procs = [];
  const proxies = [];
  const logs = [];

  const ctrl = startController(dbPath);
  procs.push(ctrl);
  result.controller = await waitHealthy();
  if (!result.controller) {
    result.error = 'controller did not become healthy';
    kill(ctrl);
    return result;
  }

  try {
    for (const port of ex.proxies ?? []) {
      proxies.push(await startProxy(port));
    }
    for (const a of ex.agents ?? []) {
      const buf = [];
      logs.push({ file: a.file + (a.args?.[0] ? ` ${a.args[0]}` : ''), buf });
      procs.push(startAgent(a.dir, a.file, a.args ?? [], buf, { raw: a.raw, node: a.node, fixture: a.fixture }));
    }
    // Give agents time to boot + register (registration loop retries every 3s).
    await sleep(4500);

    // Config-declared providers: upstream declares some providers in the
    // controller TOML ([apps.*]) rather than having the agent self-register
    // (e.g. simple-demo's raw echo app). Register them here on the agent's
    // behalf, which is the SolFlow equivalent of that TOML block.
    for (const reg of ex.register ?? []) {
      await jpost('/register', {
        name: reg.name,
        actions: reg.actions.map((name) => ({ name })),
        endpoint: reg.endpoint,
      });
    }

    const provs = await jget('/providers');
    result.providers = Array.isArray(provs.body)
      ? provs.body.map((p) => ({ module: p.module, actions: p.actions, kind: p.kind }))
      : [];
    const agentTail = () =>
      logs.map((l) => ({
        file: l.file,
        registered: l.buf.join('').includes('Registered with controller'),
        log: l.buf
          .join('')
          .split('\n')
          .map((s) => s.trim())
          .filter(Boolean)
          .slice(-6),
      }));
    result.agents = agentTail();

    for (const sol of ex.sols) {
      const src = readFileSync(join(ROOT, 'examples', sol.path), 'utf8');
      const wfs = extractWorkflows(src);
      const targets =
        sol.run === 'all' ? wfs : sol.run === 'first' ? wfs.slice(0, 1) : wfs.filter((w) => sol.run.includes(w.name));
      for (const wf of targets) {
        const r = await runWorkflow(wf.source, wf.name);
        result.workflows.push({ file: sol.path, workflow: wf.name, ...summarizeRun(r) });
      }
    }
    // Refresh agent logs so invocation side effects (e.g. the printer's
    // "[printer] PRINT:" lines) are captured as proof of real execution.
    result.agents = agentTail();
  } finally {
    for (const pr of proxies) pr.close();
    for (const p of procs) kill(p);
    await sleep(500);
    try {
      rmSync(dbDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
  return result;
}

async function main() {
  const only = process.argv[2];
  const set = only ? MANIFEST.filter((e) => e.id === only) : MANIFEST;
  if (!set.length) {
    console.error(`no manifest entry for "${only}"`);
    process.exit(1);
  }
  const all = [];
  for (const ex of set) {
    process.stderr.write(`\n### ${ex.id} ###\n`);
    const r = await runExample(ex);
    all.push(r);
    process.stderr.write(JSON.stringify(r, null, 2) + '\n');
  }
  console.log('\n===RESULTS-JSON===');
  console.log(JSON.stringify(all, null, 2));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
