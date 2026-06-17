/**
 * End-to-end round-trip + execution tests.
 *
 * Runs the canonical compiler/VM via the Node-target WASM bridge
 * (`compiler-wasm/pkg-node/`). Two halves:
 *
 *   1. parse → import → emit → parse → import → compare. Byte-identical
 *      source is NOT expected (canonical emit normalizes parens /
 *      whitespace), but the workflow shape and a second emit must be
 *      stable.
 *   2. compile + run paths through the same `run_source_json` bridge
 *      the editor uses.
 *
 * Regenerate fixtures with `node scripts/regen-import-fixtures.mjs`.
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

const require = createRequire(import.meta.url);
const wasm = require('../../../../compiler-wasm/pkg-node/solflow_compiler_wasm.js') as {
  parse_source_json(source: string): string;
  analyze_source_json(source: string): string;
  compile_source_json(source: string): string;
  run_source_json(source: string): string;
  version(): string;
};

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');

function parse(source: string): Program {
  const env = JSON.parse(wasm.parse_source_json(source)) as CompileEnvelope<Program>;
  if (!env.ok || !env.value) {
    throw new Error(`parse failed: ${env.diagnostics.map((d) => d.message).join('; ')}`);
  }
  return env.value;
}

/** Structural normalization — ignores ids, positions, timestamps. */
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
    imports: wf.imports.map((i) => ({ path: i.path, alias: i.alias, from: i.from ?? null })),
    functions: wf.functions.map((fn) => ({
      name: fn.name,
      isWorkflow: fn.isWorkflow ?? false,
      params: fn.params,
      returnType: fn.returnType,
      nodeKinds: fn.nodes.map((n) => n.data.kind),
      inlineExpressions: fn.nodes.map((n) => n.expressions ?? null),
      edgeShape: fn.edges.map((e) => ({
        sourcePort: e.source.port,
        targetPort: e.target.port,
        kind: e.kind,
      })),
    })),
  };
}

const FIXTURES = ['linear_flow', 'branch_and_loop', 'multi_function', 'with_struct_enum'];

describe('canonical round-trip — parse→import→emit→parse→compare', () => {
  for (const name of FIXTURES) {
    it(`${name}: workflow shape stable across one round-trip`, () => {
      const source = readFileSync(join(FIXTURES_DIR, `${name}.sol`), 'utf-8');

      const programA = parse(source);
      const { workflow: wfA } = importProgram(programA, { name }, source);
      const emittedA = emit(wfA).source;

      // Re-parse the emitted source — proves emit produces valid SOL.
      const programB = parse(emittedA);
      const { workflow: wfB } = importProgram(programB, { name }, emittedA);
      const emittedB = emit(wfB).source;

      // Idempotence converges in one step.
      expect(emittedB).toBe(emittedA);
      // Structural equivalence ignoring ids/positions.
      expect(normalize(wfB)).toEqual(normalize(wfA));
    });
  }

  it('emits source that parses clean (no diagnostics introduced)', () => {
    for (const name of FIXTURES) {
      const source = readFileSync(join(FIXTURES_DIR, `${name}.sol`), 'utf-8');
      const { workflow } = importProgram(parse(source), { name }, source);
      // Throws if the emitted source fails to parse.
      parse(emit(workflow).source);
    }
  });
});

// =============================================================
//  Canonical-VM execution paths via Node WASM
// =============================================================

interface RunEnvelopeShape {
  ok: boolean;
  value: { instruction_count: number } | null;
  diagnostics: Array<{ code: string; severity: string; message: string }>;
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

describe('canonical-VM execution via Node WASM', () => {
  it('compile + run produces canonical output + return value', () => {
    const env = runWasm(
      `workflow "main" {
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

  it('runs a for-loop program end-to-end', () => {
    const env = runWasm(
      `workflow "main" {
         for n in [1, 2, 3] { print(n); }
         return 0;
       }`,
    );
    expect(env.ok).toBe(true);
    expect(env.run!.output).toEqual(['1', '2', '3']);
    expect(env.run!.return_value).toBe(0);
    expect(env.run!.runtime_error).toBeNull();
  });

  it('invalid source returns parse diagnostics, skips execution', () => {
    const env = runWasm('workflow "main" { let = 5; }'); // missing binding name
    expect(env.ok).toBe(false);
    expect(env.run).toBeNull();
    expect(env.diagnostics.length).toBeGreaterThan(0);
    expect(env.diagnostics.some((d) => d.code === 'E_PARSE')).toBe(true);
  });

  it('a program with no workflow fails codegen', () => {
    const env = runWasm('fn add(a: int, b: int) <- int { return a + b; }');
    expect(env.ok).toBe(false);
    expect(env.run).toBeNull();
    expect(env.diagnostics.some((d) => d.code === 'E_CODEGEN')).toBe(true);
  });

  it('a capability call (Action) is blocked in browser sim', () => {
    const env = runWasm(
      `import "fetch" from http;
       workflow "main" {
         http.fetch({ url: "https://x" });
         return 0;
       }`,
    );
    expect(env.ok).toBe(true);
    expect(env.run!.runtime_error?.kind).toBe('ExtCallBlocked');
  });

  it('trace is empty until source spans land (Phase 4 deferred)', () => {
    const env = runWasm(`workflow "main" { print("x"); return 1; }`);
    expect(env.ok).toBe(true);
    expect(env.run!.trace).toEqual([]);
    expect(env.run!.trace_truncated).toBe(false);
    expect(env.run!.runtime_error_source_span).toBeNull();
  });

  it('canonical execution from EMITTED-back-from-graph source', () => {
    // Round-trip THEN execute: emit must not change semantics.
    const original = `workflow "main" {
      print("a");
      print("b");
      return 5;
    }`;
    const origEnv = runWasm(original);
    expect(origEnv.ok).toBe(true);
    const { workflow } = importProgram(parse(original), { name: 'e2e' }, original);
    const emitted = emit(workflow).source;
    const emittedEnv = runWasm(emitted);
    expect(emittedEnv.ok).toBe(true);
    expect(emittedEnv.run!.return_value).toBe(origEnv.run!.return_value);
    expect(emittedEnv.run!.output).toEqual(origEnv.run!.output);
  });
});
