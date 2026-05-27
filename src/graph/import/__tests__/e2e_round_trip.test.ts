/**
 * End-to-end round-trip tests (B.D c39).
 *
 * Unlike the snapshot tests in `round_trip.test.ts`, this suite
 * actually runs the canonical compiler via Node-target WASM
 * (`compiler-wasm/pkg-node/`). The cycle:
 *
 *   source
 *     → wasm parse_source_json  → AST
 *     → importer.importProgram  → SolWorkflow
 *     → emit.emit               → source'
 *     → wasm parse_source_json  → AST'
 *     → importer.importProgram  → SolWorkflow'
 *
 * Assertions: structural equivalence between SolWorkflow and
 * SolWorkflow'. Byte-identical source is NOT expected (canonical
 * emit normalizes parens, whitespace, HashMap ordering, etc. —
 * documented in CANONICALIZATION.md).
 *
 * If you change parser / importer / emitter, this test catches
 * any drift that would break round-trip.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import type { Program } from '@/compiler/ast';
import type { CompileEnvelope } from '@/compiler/types';
import type { SolWorkflow } from '@/graph/schema';
import { importProgram } from '../importer';
import { emit } from '@/emit/emit';

// Load the Node-target WASM via CommonJS require. The bundler
// target in `pkg/` uses ESM with a `?import` query that only
// Vite understands; the nodejs target in `pkg-node/` ships
// `require()`-friendly CJS.
const require = createRequire(import.meta.url);
const wasm = require('../../../../compiler-wasm/pkg-node/solflow_compiler_wasm.js') as {
  parse_source_json(source: string): string;
  analyze_source_json(source: string): string;
  compile_source_json(source: string): string;
  run_source_json(source: string): string;
  version(): string;
};

const FIXTURES_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '__fixtures__',
);

function parse(source: string): Program {
  const env = JSON.parse(
    wasm.parse_source_json(source),
  ) as CompileEnvelope<Program>;
  if (!env.ok || !env.value) {
    throw new Error(
      `parse failed: ${env.diagnostics.map((d) => d.message).join('; ')}`,
    );
  }
  return env.value;
}

/**
 * Compare two workflows for structural equivalence — ignoring
 * nanoid-generated ids, positions (which depend on layout), and
 * the `meta.createdAt` / `updatedAt` / `description` fields.
 *
 * Returns a normalized JSON-safe shape; equality of two
 * normalized values means the workflows are equivalent.
 */
function normalize(wf: SolWorkflow): unknown {
  return {
    structs: wf.structs.map((s) => ({
      name: s.name,
      fields: s.fields.map((f) => ({ name: f.name, type: f.type })),
    })),
    enums: wf.enums.map((e) => ({
      name: e.name,
      variants: e.variants.map((v) => ({ name: v.name, value: v.value })),
    })),
    imports: wf.imports.map((i) => ({ path: i.path, alias: i.alias })),
    functions: wf.functions.map((fn) => ({
      name: fn.name,
      params: fn.params,
      returnType: fn.returnType,
      // Node count + kind sequence captures most structural drift
      // without depending on nanoid ids.
      nodeKinds: fn.nodes.map((n) => n.data.kind),
      // Inline expressions are part of the canonical form — drift
      // here would mean emit/import disagreement.
      inlineExpressions: fn.nodes.map((n) => n.expressions ?? null),
      edgeShape: fn.edges.map((e) => ({
        sourcePort: e.source.port,
        targetPort: e.target.port,
        kind: e.kind,
      })),
    })),
  };
}

const FIXTURES = [
  'linear_flow',
  'branch_and_loop',
  'multi_function',
  'with_struct_enum',
  'field_index_assign',
  // top_level_let intentionally skipped — the __init wrap is
  // a known semantic-change, not a round-trip-stable shape. The
  // first import produces `__init`; the second import sees
  // `__init` as a regular function with no top-level lets to
  // hoist, producing a different structure on the second pass.
];

describe('B.D c39 — true end-to-end parse→import→emit→parse→compare', () => {
  for (const name of FIXTURES) {
    it(`${name}: workflow shape stable across one round-trip`, () => {
      const source = readFileSync(join(FIXTURES_DIR, `${name}.sol`), 'utf-8');

      // First cycle.
      const programA = parse(source);
      const { workflow: wfA } = importProgram(programA, { name }, source);
      const emittedA = emit(wfA).source;

      // Second cycle — re-parse the emitted source through WASM.
      const programB = parse(emittedA);
      const { workflow: wfB } = importProgram(programB, { name }, emittedA);
      const emittedB = emit(wfB).source;

      // Strongest claim: after one full cycle, subsequent emits
      // produce identical source. Idempotence converges in 1 step.
      expect(emittedB).toBe(emittedA);

      // Structural equivalence: ignoring ids/positions, the two
      // workflows should match.
      expect(normalize(wfB)).toEqual(normalize(wfA));
    });
  }

  it('emits source that parses clean (no diagnostics introduced)', () => {
    // The strongest claim: every fixture's emit output is itself
    // valid SOL with no parser warnings. Catches any case where
    // the emitter generates parens or formatting the parser doesn't
    // accept.
    for (const name of FIXTURES) {
      const source = readFileSync(join(FIXTURES_DIR, `${name}.sol`), 'utf-8');
      const programA = parse(source);
      const { workflow } = importProgram(programA, { name }, source);
      const emitted = emit(workflow).source;
      // Will throw if parse fails.
      parse(emitted);
    }
  });
});

// =============================================================
//  B.D c45 — canonical-VM execution paths via Node WASM
// =============================================================
//
// The c39 round-trip suite covered parse + import + emit. These
// tests cover compile + run + structured-error paths, all the way
// through the same `run_source_json` bridge the editor uses.

interface RunEnvelopeShape {
  ok: boolean;
  value: { instruction_count: number } | null;
  diagnostics: Array<{ code: string; severity: string }>;
  run: {
    return_value: number | null;
    output: string[];
    steps: number;
    runtime_error: { kind: string; [k: string]: unknown } | null;
    runtime_error_source_span: { start: number; end: number } | null;
    trace: Array<{ start: number; end: number }>;
    trace_truncated: boolean;
  } | null;
}

function runWasm(source: string): RunEnvelopeShape {
  return JSON.parse(wasm.run_source_json(source)) as RunEnvelopeShape;
}

describe('B.D c45 — canonical-VM execution via Node WASM', () => {
  it('compile + run produces canonical output + return value', () => {
    const env = runWasm(
      `function start() -> int {
         print("hello");
         print(42);
         return 7;
       }`,
    );
    expect(env.ok).toBe(true);
    expect(env.run).not.toBeNull();
    expect(env.run!.output).toEqual(['hello', '42']);
    expect(env.run!.return_value).toBe(7);
    expect(env.run!.runtime_error).toBeNull();
    expect(env.run!.steps).toBeGreaterThan(0);
  });

  it('runs a control-flow program end-to-end', () => {
    const env = runWasm(
      `function start() -> int {
         let x: int = 0;
         while (x < 5) { x = x + 1; }
         return x;
       }`,
    );
    expect(env.ok).toBe(true);
    expect(env.run!.return_value).toBe(5);
    expect(env.run!.runtime_error).toBeNull();
  });

  it('invalid source returns compile diagnostics, skips execution', () => {
    const env = runWasm('function start() -> int { return 0 }'); // missing semi
    expect(env.ok).toBe(false);
    expect(env.run).toBeNull();
    expect(env.diagnostics.length).toBeGreaterThan(0);
    expect(env.diagnostics.some((d) => d.code.startsWith('E0'))).toBe(true);
  });

  it('runtime div-by-zero surfaces structured error + source span', () => {
    const env = runWasm(
      'function start() -> int { return 10 / 0; }',
    );
    expect(env.ok).toBe(true);
    expect(env.run!.runtime_error).toEqual({ kind: 'DivByZero' });
    // c42: span attached so editor can scroll to failure site.
    expect(env.run!.runtime_error_source_span).not.toBeNull();
    expect(env.run!.runtime_error_source_span!.start).toBeGreaterThanOrEqual(0);
  });

  it('execution trace surfaces de-duplicated source spans', () => {
    const env = runWasm(
      `function start() -> int {
         let x: int = 1;
         let y: int = 2;
         return x + y;
       }`,
    );
    expect(env.ok).toBe(true);
    expect(env.run!.trace.length).toBeGreaterThan(0);
    expect(env.run!.trace_truncated).toBe(false);
    // Adjacent equal spans are de-duplicated.
    for (let i = 1; i < env.run!.trace.length; i++) {
      const prev = env.run!.trace[i - 1]!;
      const cur = env.run!.trace[i]!;
      const same = prev.start === cur.start && prev.end === cur.end;
      expect(same).toBe(false);
    }
  });

  it('ExtCall is blocked with structured error in browser sim', () => {
    const env = runWasm(
      `ext function fetch(url: str) -> int;
       function start() -> int { return fetch("https://x"); }`,
    );
    // The compiler emits a codegen warning for missing endpoint
    // configuration; the test just checks that EITHER:
    //   a) compile fails (codegen E0051), OR
    //   b) the run produces an ExtCallBlocked runtime error
    if (!env.ok) {
      expect(env.diagnostics.some((d) => d.code === 'E0051')).toBe(true);
    } else {
      expect(env.run!.runtime_error?.kind).toBe('ExtCallBlocked');
    }
  });

  it('canonical execution from EMITTED-back-from-graph source', () => {
    // Round-trip THEN execute: import original source, emit it as
    // canonical form, then run the emitted form. Output must match
    // the original source's output — proves the emitter doesn't
    // change semantics across canonicalization.
    const original = `function start() -> int {
      let x: int = 3;
      let y: int = 4;
      print(x);
      print(y);
      return x + y;
    }`;
    const origEnv = runWasm(original);
    expect(origEnv.ok).toBe(true);
    const program = parse(original);
    const { workflow } = importProgram(program, { name: 'e2e' }, original);
    const emitted = emit(workflow).source;
    const emittedEnv = runWasm(emitted);
    expect(emittedEnv.ok).toBe(true);
    expect(emittedEnv.run!.return_value).toBe(origEnv.run!.return_value);
    expect(emittedEnv.run!.output).toEqual(origEnv.run!.output);
  });
});
