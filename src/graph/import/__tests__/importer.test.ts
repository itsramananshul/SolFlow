/**
 * Integration tests for the AST → graph importer.
 *
 * Fixtures are pre-generated AST JSON files (one per .sol input
 * under __fixtures__/). Regenerate them with:
 *
 *   cargo run -p solflow_compiler_wasm --example dump_ast -- \
 *     src/graph/import/__fixtures__/<name>.sol \
 *     > src/graph/import/__fixtures__/<name>.ast.json
 *
 * The fixtures-as-JSON approach lets these tests run as pure
 * Node code — no WASM needed in the test runtime.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Program } from '@/compiler/ast';
import { importProgram } from '../importer';

const FIXTURES_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '__fixtures__',
);

function loadFixture(name: string): Program {
  const path = join(FIXTURES_DIR, `${name}.ast.json`);
  const raw = readFileSync(path, 'utf-8');
  return JSON.parse(raw) as Program;
}

describe('importProgram — linear flow', () => {
  it('imports let/print/return cleanly', () => {
    const program = loadFixture('linear_flow');
    const { workflow, report } = importProgram(program);

    expect(workflow.functions).toHaveLength(1);
    const fn = workflow.functions[0]!;
    expect(fn.name).toBe('start');

    // 1 start node + 2 let + 2 print + 1 return = 6
    expect(fn.nodes.length).toBe(6);

    const kinds = fn.nodes.map((n) => n.data.kind);
    expect(kinds).toEqual(['start', 'let', 'let', 'print', 'print', 'return']);

    // No unsupported / source-only constructs.
    expect(report.counts.unsupported).toBe(0);
    expect(report.counts.sourceOnly).toBe(0);

    // Inline expressions present on let / print / return.
    const letNode = fn.nodes.find((n) => n.data.kind === 'let' && n.data.varName === 'x');
    expect(letNode?.expressions?.value).toBe('5');
    const ret = fn.nodes.find((n) => n.data.kind === 'return');
    expect(ret?.expressions?.value).toBe('x');
  });
});

describe('importProgram — branch + while + for', () => {
  it('reconstructs control-flow nodes with bodies wired', () => {
    const program = loadFixture('branch_and_loop');
    const { workflow, report } = importProgram(program);
    expect(workflow.functions).toHaveLength(1);
    const fn = workflow.functions[0]!;

    const branch = fn.nodes.find((n) => n.data.kind === 'branch');
    expect(branch).toBeTruthy();
    expect(branch?.expressions?.cond).toBe('(x == 0)');

    const wh = fn.nodes.find((n) => n.data.kind === 'while');
    expect(wh).toBeTruthy();
    expect(wh?.expressions?.cond).toBe('(x < 5)');

    const fe = fn.nodes.find((n) => n.data.kind === 'forEach');
    expect(fe).toBeTruthy();
    if (fe?.data.kind === 'forEach') {
      expect(fe.data.iteratorName).toBe('item');
    }
    expect(fe?.expressions?.array).toBe('[1, 2, 3]');

    // Then/else arms have prints inside.
    const prints = fn.nodes.filter((n) => n.data.kind === 'print');
    const printValues = prints.map((p) => p.expressions?.value ?? '');
    expect(printValues).toContain('"zero"');
    expect(printValues).toContain('"nonzero"');

    // Branch should have wired then + else edges.
    const branchOutEdges = fn.edges.filter(
      (e) => e.source.node === branch?.id,
    );
    const branchPorts = new Set(branchOutEdges.map((e) => e.source.port));
    expect(branchPorts.has('then')).toBe(true);
    expect(branchPorts.has('else')).toBe(true);

    // Control flow includes an assign inside the while body.
    const assignNodes = fn.nodes.filter((n) => n.data.kind === 'assign');
    expect(assignNodes.length).toBeGreaterThan(0);

    // Recovery + classification: every statement landed; nothing
    // unsupported (the importer DOES classify branches as 'partial'
    // because conditions live as inline text, not as sub-graphs).
    expect(report.counts.unsupported).toBe(0);
    expect(report.counts.partial).toBeGreaterThan(0);
  });
});

describe('importProgram — multi-function', () => {
  it('preserves all functions and wires statement-level calls', () => {
    const program = loadFixture('multi_function');
    const { workflow, report } = importProgram(program);

    expect(workflow.functions.map((f) => f.name)).toEqual([
      'add', 'notify', 'start',
    ]);

    // start() has two statement-level `notify(...)` calls — those
    // become `call` nodes resolved to notify's functionId. Calls
    // embedded inside expressions (e.g. `let x = add(1, 2)`) are
    // preserved as inline text on the consuming statement, NOT
    // lifted to separate call nodes (that would be a lossy
    // transformation; B.8 territory if ever).
    const start = workflow.functions.find((f) => f.name === 'start')!;
    const callNodes = start.nodes.filter((n) => n.data.kind === 'call');
    expect(callNodes.length).toBe(2);

    const notify = workflow.functions.find((f) => f.name === 'notify')!;
    for (const c of callNodes) {
      if (c.data.kind === 'call') {
        expect(c.data.functionId).toBe(notify.id);
      }
    }

    // Parameters propagate.
    const add = workflow.functions.find((f) => f.name === 'add')!;
    expect(add.params.map((p) => p.name)).toEqual(['a', 'b']);

    // Per-function report entries exist.
    expect(report.functions.map((f) => f.name)).toEqual([
      'add', 'notify', 'start',
    ]);
  });
});

describe('importProgram — structs + enums', () => {
  it('imports top-level type declarations', () => {
    const program = loadFixture('with_struct_enum');
    const { workflow, report } = importProgram(program);

    expect(workflow.structs).toHaveLength(1);
    expect(workflow.structs[0]!.name).toBe('Point');
    // Importer sorts fields alphabetically for determinism (HashMap
    // order isn't stable in the serialized AST).
    expect(workflow.structs[0]!.fields.map((f) => f.name)).toEqual(['x', 'y']);

    expect(workflow.enums).toHaveLength(1);
    expect(workflow.enums[0]!.name).toBe('Status');
    // Variants sorted by parser-assigned ordinal.
    expect(workflow.enums[0]!.variants.map((v) => v.name)).toEqual([
      'Active', 'Inactive',
    ]);

    expect(report.topLevel.structs).toBe(1);
    expect(report.topLevel.enums).toBe(1);
  });
});

describe('importProgram — empty / degenerate', () => {
  it('empty program yields empty workflow + no notices', () => {
    const { workflow, report } = importProgram([]);
    expect(workflow.functions).toHaveLength(0);
    expect(workflow.structs).toHaveLength(0);
    expect(report.notices).toHaveLength(0);
  });
});

describe('importProgram — source attachment (B.6 c25)', () => {
  it('attaches sourceLine to functions when source is provided', () => {
    const source = `// header comment

function alpha() -> int {
    return 0;
}

function beta() -> int {
    return alpha();
}
`;
    const program = loadFixture('linear_flow');
    // Use a hand-crafted source whose function names match the
    // fixture so we can verify the lookup works without needing
    // an exact-match fixture. The importer's textual scan only
    // reads function names from the source string.
    const { workflow, report } = importProgram(
      program,
      { name: 't' },
      source.replace('function alpha', 'function start'),
    );
    // linear_flow has one function named "start".
    const start = workflow.functions.find((f) => f.name === 'start');
    expect(start?.meta?.sourceLine).toBe(3);
    // Mirror onto the report summary.
    const summary = report.functions.find((f) => f.name === 'start');
    expect(summary?.sourceLine).toBe(3);
  });

  it('omits sourceLine when source is not provided', () => {
    const program = loadFixture('linear_flow');
    const { workflow, report } = importProgram(program, { name: 't' });
    const start = workflow.functions.find((f) => f.name === 'start');
    expect(start?.meta?.sourceLine).toBeUndefined();
    const summary = report.functions.find((f) => f.name === 'start');
    expect(summary?.sourceLine).toBeUndefined();
  });

  it('falls back gracefully on missing match (function not in source)', () => {
    const program = loadFixture('linear_flow');
    // Pass source that does NOT contain `function start` — the
    // importer should still produce the workflow, just without
    // a sourceLine.
    const { workflow } = importProgram(
      program,
      { name: 't' },
      '// no function declarations here\n',
    );
    expect(workflow.functions[0]?.meta?.sourceLine).toBeUndefined();
  });
});

describe('importProgram — report counts', () => {
  it('headline counts roll up across functions', () => {
    const program = loadFixture('branch_and_loop');
    const { report } = importProgram(program);
    // Every classified statement lands in exactly one bucket.
    const sum = report.counts.full
      + report.counts.partial
      + report.counts.sourceOnly
      + report.counts.unsupported;
    expect(sum).toBeGreaterThan(0);
  });
});
