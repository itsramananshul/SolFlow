/**
 * Phase 3.1 — the first class external-capability node.
 *
 * `call("module.function", params)` imports to an `action` node (not a
 * placeholder) and emits back to the same `call(...)` form, so a
 * capability workflow round trips through the visual editor.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Program } from '@/compiler/ast';
import { importProgram } from '../importer';
import { emit } from '@/emit/emit';
import { validateWorkflow } from '@/graph/validate';
import { buildCapabilityDemo } from '@/samples/capabilityDemo';

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');

function loadFixture(name: string): Program {
  return JSON.parse(readFileSync(join(FIXTURES_DIR, `${name}.ast.json`), 'utf-8')) as Program;
}

describe('capability call → action node', () => {
  it('imports call("alert.fire", {...}) to an action node, not a placeholder', () => {
    const program = loadFixture('actions_emit');
    const { workflow } = importProgram(program, { name: 'notify' });
    const fn = workflow.functions[0]!;
    const action = fn.nodes.find((n) => n.data.kind === 'action');
    expect(action).toBeDefined();
    expect(action!.data.kind === 'action' && action!.data.capability).toBe('alert.fire');
    // Params ride on the node's params port as an inline expression.
    expect(action!.expressions?.params).toContain('level');
    // The node exposes a params input and a return output.
    expect(action!.ports.in.some((p) => p.id === 'params')).toBe(true);
    expect(action!.ports.out.some((p) => p.id === 'return')).toBe(true);
  });

  it('emits the action node back to call("module.fn", params)', () => {
    const program = loadFixture('actions_emit');
    const { workflow } = importProgram(program, { name: 'notify' });
    const out = emit(workflow);
    expect(out.source).toContain('call("alert.fire"');
    expect(out.source).toContain('level');
    // emit is deterministic.
    expect(emit(workflow).source).toBe(out.source);
  });
});

describe('Capability Call sample', () => {
  it('emits a single capability call bound to a let (not duplicated)', () => {
    const { source } = emit(buildCapabilityDemo());
    // The value-producing call is the let's RHS, emitted exactly once.
    const occurrences = source.split('call("demo.add"').length - 1;
    expect(occurrences).toBe(1);
    expect(source).toContain('let sum: int = call("demo.add", { a: 20, b: 22 });');
    expect(source).toContain('print(sum);');
    expect(source).toContain('return sum;');
  });

  it('validates with no graph errors', () => {
    // A capability node consumed as a value is data-wired (not in the
    // control-flow chain), so its optional control ports stay unwired
    // without tripping the "missing input" validator.
    const diags = validateWorkflow(buildCapabilityDemo());
    expect(diags.filter((d) => d.severity === 'error')).toEqual([]);
  });
});
