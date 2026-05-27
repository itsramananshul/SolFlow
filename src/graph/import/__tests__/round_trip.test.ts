/**
 * Round-trip + canonicalization tests (B.8 c26).
 *
 * These tests don't run the WASM compiler in Node — the test
 * runtime is pure Node, no WASM bridge wired up. Instead they
 * exercise the parts that ARE pure TS:
 *
 *   1. `import → emit` produces canonical source. Each fixture's
 *      emit output is snapshotted; future runs flag the diff so
 *      formatting drift surfaces as a code review.
 *   2. `emit(workflow) === emit(workflow)` — emit is idempotent /
 *      deterministic given the same workflow input.
 *   3. Structural invariants on the emit output (parseable
 *      proxy: balanced braces, function header per import-report
 *      entry, no `undefined` leakage from inline expressions).
 *
 * True bytes-round-trip (parse → emit → parse → compare workflows)
 * needs the WASM compiler in Node — deferred until we ship a
 * Node-target WASM build alongside the bundler target.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Program } from '@/compiler/ast';
import { importProgram } from '../importer';
import { emit } from '@/emit/emit';

const FIXTURES_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '__fixtures__',
);

function loadFixture(name: string): Program {
  return JSON.parse(
    readFileSync(join(FIXTURES_DIR, `${name}.ast.json`), 'utf-8'),
  ) as Program;
}

const FIXTURES = [
  'linear_flow',
  'branch_and_loop',
  'multi_function',
  'with_struct_enum',
];

describe('B.8 — emit is idempotent', () => {
  for (const name of FIXTURES) {
    it(`emit(import(${name})) === emit(import(${name}))`, () => {
      const program = loadFixture(name);
      const { workflow } = importProgram(program, { name });
      const out1 = emit(workflow);
      const out2 = emit(workflow);
      expect(out1.source).toBe(out2.source);
      expect(out1.warnings).toEqual(out2.warnings);
    });
  }
});

describe('B.8 — emit output is structurally sound', () => {
  for (const name of FIXTURES) {
    it(`${name}: balanced braces`, () => {
      const program = loadFixture(name);
      const { workflow } = importProgram(program, { name });
      const { source } = emit(workflow);
      // Comments and strings can contain unbalanced braces but our
      // fixtures don't have those. Strip line comments + string
      // literals before counting; if a fixture later needs them,
      // this can be loosened.
      const stripped = source
        .replace(/\/\/.*$/gm, '')
        .replace(/"(?:[^"\\]|\\.)*"/g, '""');
      const opens = (stripped.match(/\{/g) ?? []).length;
      const closes = (stripped.match(/\}/g) ?? []).length;
      expect(opens).toBe(closes);
    });

    it(`${name}: every imported function appears in the emit`, () => {
      const program = loadFixture(name);
      const { workflow, report } = importProgram(program, { name });
      const { source } = emit(workflow);
      for (const fn of report.functions) {
        expect(source).toContain(`function ${fn.name}`);
      }
    });

    it(`${name}: no "undefined" leakage from inline expressions`, () => {
      // Bug-class guard: any literal `undefined` token in emit
      // output would indicate the importer/emitter silently
      // produced unparseable SOL. The legitimate placeholder
      // sentinel `/* undefined */` is acceptable; we only flag
      // the bare identifier.
      const program = loadFixture(name);
      const { workflow } = importProgram(program, { name });
      const { source } = emit(workflow);
      const stripped = source.replace(/\/\*[\s\S]*?\*\//g, '');
      expect(stripped).not.toMatch(/\bundefined\b/);
    });
  }
});

describe('B.8 — snapshot of canonical emit output', () => {
  // Snapshot tests double as a regression alarm. A diff here means
  // either (a) the emitter changed and the snapshot needs updating
  // via `npm run test -- -u`, or (b) the importer started producing
  // structurally different graphs from the same AST. Either way the
  // diff is the alarm — both are deliberate but should be reviewed.
  for (const name of FIXTURES) {
    it(`${name}: canonical emit snapshot`, () => {
      const program = loadFixture(name);
      const { workflow } = importProgram(program, { name });
      const { source } = emit(workflow);
      expect(source).toMatchSnapshot();
    });
  }
});

describe('B.8 — emit ordering is stable across re-imports', () => {
  // Re-importing the same AST twice must produce the same emit
  // output, regardless of nanoid()-generated node ids. Tests the
  // emitter's lookup determinism: a graph with different but
  // structurally-equivalent ids should emit identically.
  for (const name of FIXTURES) {
    it(`${name}: two independent imports emit the same source`, () => {
      const program = loadFixture(name);
      const a = importProgram(program, { name });
      const b = importProgram(program, { name });
      const sourceA = emit(a.workflow).source;
      const sourceB = emit(b.workflow).source;
      expect(sourceA).toBe(sourceB);
    });
  }
});
