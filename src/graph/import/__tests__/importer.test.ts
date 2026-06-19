/**
 * Integration tests for the canonical AST → graph importer.
 *
 * Fixtures are pre-generated AST JSON files (one per .sol input under
 * __fixtures__/). Regenerate them with:
 *
 *   node scripts/regen-import-fixtures.mjs
 *
 * The fixtures-as-JSON approach lets these tests run as pure Node code
 * — no WASM needed in the test runtime.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Program } from '@/compiler/ast';
import { importProgram } from '../importer';

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');

function loadFixture(name: string): Program {
  const raw = readFileSync(join(FIXTURES_DIR, `${name}.ast.json`), 'utf-8');
  return JSON.parse(raw) as Program;
}

describe('importProgram — linear workflow', () => {
  it('imports let/print/return cleanly', () => {
    const program = loadFixture('linear_flow');
    const { workflow, report } = importProgram(program);

    expect(workflow.functions).toHaveLength(1);
    const fn = workflow.functions[0]!;
    expect(fn.name).toBe('start');
    // The runnable unit is a workflow, tagged for round-trip.
    expect(fn.isWorkflow).toBe(true);

    // 1 start node + 2 let + 2 print + 1 return = 6
    expect(fn.nodes.length).toBe(6);
    const kinds = fn.nodes.map((n) => n.data.kind);
    expect(kinds).toEqual(['start', 'let', 'let', 'print', 'print', 'return']);

    expect(report.counts.unsupported).toBe(0);
    expect(report.counts.sourceOnly).toBe(0);

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
    expect(branch?.expressions?.cond).toBe('(x == 0)');

    const wh = fn.nodes.find((n) => n.data.kind === 'while');
    expect(wh?.expressions?.cond).toBe('(x < 5)');

    const fe = fn.nodes.find((n) => n.data.kind === 'forEach');
    expect(fe).toBeTruthy();
    if (fe?.data.kind === 'forEach') {
      expect(fe.data.iteratorName).toBe('item');
    }
    expect(fe?.expressions?.array).toBe('[1, 2, 3]');

    // Then/else arms have prints inside.
    const printValues = fn.nodes
      .filter((n) => n.data.kind === 'print')
      .map((p) => p.expressions?.value ?? '');
    expect(printValues).toContain('"zero"');
    expect(printValues).toContain('"nonzero"');

    // Branch wired then + else edges.
    const branchPorts = new Set(
      fn.edges.filter((e) => e.source.node === branch?.id).map((e) => e.source.port),
    );
    expect(branchPorts.has('then')).toBe(true);
    expect(branchPorts.has('else')).toBe(true);

    expect(report.counts.unsupported).toBe(0);
    expect(report.counts.partial).toBeGreaterThan(0);
  });
});

describe('importProgram — functions + workflow', () => {
  it('preserves all callables and wires statement-level local calls', () => {
    const program = loadFixture('multi_function');
    const { workflow, report } = importProgram(program);

    expect(workflow.functions.map((f) => f.name)).toEqual(['add', 'notify', 'start']);

    // Helper fns are not workflows; the entry is.
    const add = workflow.functions.find((f) => f.name === 'add')!;
    const start = workflow.functions.find((f) => f.name === 'start')!;
    expect(add.isWorkflow).toBe(false);
    expect(start.isWorkflow).toBe(true);

    // start() has two statement-level `notify(...)` calls — those become
    // `call` nodes resolved to notify's functionId.
    const callNodes = start.nodes.filter((n) => n.data.kind === 'call');
    expect(callNodes.length).toBe(2);
    const notify = workflow.functions.find((f) => f.name === 'notify')!;
    for (const c of callNodes) {
      if (c.data.kind === 'call') expect(c.data.functionId).toBe(notify.id);
    }

    // Parameters propagate.
    expect(add.params.map((p) => p.name)).toEqual(['a', 'b']);

    expect(report.functions.map((f) => f.name)).toEqual(['add', 'notify', 'start']);
  });
});

describe('importProgram — structs + enums', () => {
  it('imports top-level type declarations preserving field order', () => {
    const program = loadFixture('with_struct_enum');
    const { workflow, report } = importProgram(program);

    expect(workflow.structs).toHaveLength(1);
    expect(workflow.structs[0]!.name).toBe('Point');
    // Canonical struct fields are an ordered Vec — order is preserved.
    expect(workflow.structs[0]!.fields.map((f) => f.name)).toEqual(['x', 'y']);

    expect(workflow.enums).toHaveLength(1);
    expect(workflow.enums[0]!.name).toBe('Status');
    expect(workflow.enums[0]!.variants.map((v) => v.name)).toEqual(['Active', 'Inactive']);

    expect(report.topLevel.structs).toBe(1);
    expect(report.topLevel.enums).toBe(1);
  });
});

describe('importProgram — Actions + emit', () => {
  it('imports a capability call as an action node and the rest as placeholders', () => {
    const program = loadFixture('actions_emit');
    const { workflow, report } = importProgram(program, { name: 'a' });

    // The `from` import is captured.
    expect(workflow.imports).toHaveLength(1);
    expect(workflow.imports[0]!.alias).toBe('send');
    expect(workflow.imports[0]!.from).toBe('slack');

    const fn = workflow.functions.find((f) => f.name === 'notify')!;

    // call("alert.fire", ...) is now a first class action node.
    const action = fn.nodes.find((n) => n.data.kind === 'action');
    expect(action).toBeDefined();
    expect(action!.data.kind === 'action' && action!.data.capability).toBe('alert.fire');

    // slack.send(...) and emit "done" still land as honest placeholders
    // (no first class node for those forms yet) — nothing dropped.
    const placeholders = fn.nodes.filter(
      (n) => n.data.kind === 'print' && n.id !== fn.nodes[0]!.id,
    );
    const values = placeholders.map((p) => p.expressions?.value ?? '');
    expect(values.some((v) => v.includes('slack.send'))).toBe(true);
    expect(values.some((v) => v.includes('emit'))).toBe(true);

    // Notices surface every degradation.
    expect(report.notices.length).toBeGreaterThanOrEqual(2);
    expect(report.counts.partial).toBeGreaterThan(0);
  });
});

describe('importProgram — source attachment', () => {
  it('attaches sourceLine to the workflow when source is provided', () => {
    const source = readFileSync(join(FIXTURES_DIR, 'linear_flow.sol'), 'utf-8');
    const program = loadFixture('linear_flow');
    const { workflow, report } = importProgram(program, { name: 't' }, source);
    const start = workflow.functions.find((f) => f.name === 'start');
    // `workflow "start"` is on line 1.
    expect(start?.meta?.sourceLine).toBe(1);
    expect(report.functions.find((f) => f.name === 'start')?.sourceLine).toBe(1);
  });

  it('omits sourceLine when source is not provided', () => {
    const program = loadFixture('linear_flow');
    const { workflow } = importProgram(program, { name: 't' });
    expect(workflow.functions[0]?.meta?.sourceLine).toBeUndefined();
  });

  it('finds fn declaration lines too', () => {
    const source = readFileSync(join(FIXTURES_DIR, 'multi_function.sol'), 'utf-8');
    const program = loadFixture('multi_function');
    const { workflow } = importProgram(program, { name: 't' }, source);
    expect(workflow.functions.find((f) => f.name === 'add')?.meta?.sourceLine).toBe(1);
    expect(workflow.functions.find((f) => f.name === 'notify')?.meta?.sourceLine).toBe(5);
  });
});

describe('importProgram — empty / degenerate', () => {
  it('empty program yields empty workflow + no notices', () => {
    const { workflow, report } = importProgram({ items: [] });
    expect(workflow.functions).toHaveLength(0);
    expect(workflow.structs).toHaveLength(0);
    expect(report.notices).toHaveLength(0);
  });
});

describe('importProgram — report counts', () => {
  it('headline counts roll up across functions', () => {
    const program = loadFixture('branch_and_loop');
    const { report } = importProgram(program);
    const sum =
      report.counts.full +
      report.counts.partial +
      report.counts.sourceOnly +
      report.counts.unsupported;
    expect(sum).toBeGreaterThan(0);
  });
});
