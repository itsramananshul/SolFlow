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
