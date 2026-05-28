#!/usr/bin/env node
/**
 * Phase C C.8 c103 — release validation gate.
 *
 * Runs every check that has to pass before cutting a release. One
 * command, machine-readable summary, non-zero exit on any failure.
 *
 *   npm run release:check
 *
 * Stages, in order:
 *   1. typecheck   (vue-tsc)
 *   2. vitest      (TS unit + integration)
 *   3. cargo test  (Rust workspace)
 *   4. controller build (release profile)
 *   5. editor build (vite, with typecheck)
 *
 * Each stage prints a [PASS|FAIL] line; the trailing summary lists
 * timing per stage. Stage failures abort immediately — we don't
 * keep running once one thing's broken, to keep the output focused.
 *
 * Designed to be both human- and CI-friendly: exit code is the only
 * thing CI looks at; humans get the per-stage breakdown.
 */
import { spawn } from 'node:child_process';
import { platform } from 'node:os';
import { performance } from 'node:perf_hooks';

const STAGES = [
  { id: 'typecheck',  cmd: 'npm', args: ['run', 'typecheck'],            label: 'vue-tsc typecheck' },
  { id: 'vitest',     cmd: 'npm', args: ['run', 'test'],                 label: 'vitest (TS)' },
  { id: 'cargo-test', cmd: 'cargo', args: ['test', '--workspace', '--quiet'], label: 'cargo test --workspace' },
  { id: 'cargo-build',cmd: 'cargo', args: ['build', '--release', '--bin', 'solflow-controller', '--quiet'], label: 'controller release build' },
  { id: 'vite-build', cmd: 'npm', args: ['run', 'build'],                label: 'editor build (vite + typecheck)' },
];

main().catch((err) => {
  console.error(`✗ release-check crashed: ${err.message ?? err}`);
  process.exit(2);
});

async function main() {
  console.log(`◆ solflow release-check — ${STAGES.length} stages`);
  const results = [];
  for (const stage of STAGES) {
    const t0 = performance.now();
    console.log(`\n→ ${stage.label}  (${stage.cmd} ${stage.args.join(' ')})`);
    const ok = await run(stage.cmd, stage.args);
    const elapsed = ((performance.now() - t0) / 1000).toFixed(1);
    results.push({ id: stage.id, label: stage.label, ok, elapsed });
    if (!ok) {
      summarize(results);
      console.log(`\n✗ release-check FAILED at "${stage.label}" — fix that, then re-run.`);
      process.exit(1);
    }
  }
  summarize(results);
  console.log(`\n✓ release-check PASSED — safe to package + ship.`);
}

function summarize(results) {
  console.log(`\n  ── summary ──`);
  for (const r of results) {
    const tag = r.ok ? '\x1b[32mPASS\x1b[0m' : '\x1b[31mFAIL\x1b[0m';
    console.log(`  ${tag}  ${r.label.padEnd(34)}  ${r.elapsed}s`);
  }
}

function run(cmd, args) {
  return new Promise((resolvePromise) => {
    const child = spawn(cmd, args, {
      stdio: 'inherit',
      // shell:true on Windows so `npm.cmd` resolves; on POSIX it's
      // unnecessary but harmless.
      shell: platform() === 'win32',
    });
    child.on('exit', (code) => resolvePromise(code === 0));
    child.on('error', () => resolvePromise(false));
  });
}
