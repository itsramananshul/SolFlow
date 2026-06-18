// Live QA for Local Controller trace parity (Phase 2).
// Submits a helper workflow to a running controller, runs it, polls the
// run record, and verifies the controller returns a real execution trace
// (helper call/return events with source lines) over its HTTP API.
const BASE = process.env.CONTROLLER ?? 'http://127.0.0.1:3939';
const log = (...a) => console.log('[qa-ctl]', ...a);

const SRC = `fn dbl(x: int) <- int { return x * 2; }
workflow "start" { return dbl(21); }`;

async function main() {
  // POST /workflows — bytecode carries the SOL source bytes (the
  // controller compiles + runs from it via the canonical VM).
  const sub = await fetch(`${BASE}/workflows`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      name: 'qa-helper',
      bytecode: Array.from(new TextEncoder().encode(SRC)),
      instruction_spans: Array.from(new TextEncoder().encode('[]')),
      source: SRC,
    }),
  });
  if (!sub.ok) throw new Error(`POST /workflows ${sub.status}: ${await sub.text()}`);
  const { workflow_id } = await sub.json();
  log('workflow_id:', workflow_id);

  const runResp = await fetch(`${BASE}/runs`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ workflow_id, trigger: { kind: 'Manual' }, inputs: {} }),
  });
  if (!runResp.ok) throw new Error(`POST /runs ${runResp.status}: ${await runResp.text()}`);
  const { run_id } = await runResp.json();
  log('run_id:', run_id);

  // Poll until terminal.
  let rec = null;
  for (let i = 0; i < 100; i++) {
    const r = await fetch(`${BASE}/runs/${run_id}`);
    rec = await r.json();
    if (['Succeeded', 'Failed', 'Cancelled', 'TimedOut'].includes(rec.status)) break;
    await new Promise((res) => setTimeout(res, 50));
  }
  log('status:', rec.status, 'return_value:', rec.output?.return_value);

  const trace = rec.output?.trace ?? [];
  log('trace steps:', trace.length);
  const kinds = [...new Set(trace.map((s) => s.kind))];
  log('kinds:', JSON.stringify(kinds));
  const call = trace.find((s) => s.kind === 'call' && s.detail === 'dbl');
  const ret = trace.find((s) => s.kind === 'return' && s.function === 'dbl');
  log('call(dbl):', call ? `line ${call.line}` : 'MISSING');
  log('return(dbl):', ret ? `function ${ret.function} depth ${ret.depth}` : 'MISSING');
  const linesOk = trace.every((s) => s.span == null || s.line != null);

  const ok =
    rec.status === 'Succeeded' &&
    rec.output?.return_value === 42 &&
    trace.length > 0 &&
    !!call &&
    !!ret &&
    linesOk;
  log(ok ? 'PASS: controller returns a real trace with helper call/return' : 'FAIL');
  process.exitCode = ok ? 0 : 1;
}

main().catch((e) => { log('ERROR:', e.message); process.exitCode = 1; });
