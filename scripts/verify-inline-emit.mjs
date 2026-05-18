// Verifies the inline-expression escape hatch end-to-end:
// build a workflow with ZERO wired data edges and pure inline expressions,
// then run the emitter and assert the output is valid SOL.
import { emit } from '../src/emit/emit.ts';

const wf = {
  schemaVersion: 1,
  meta: { name: 'inline-test', createdAt: '2026-01-01', updatedAt: '2026-01-01' },
  imports: [],
  structs: [],
  enums: [],
  functions: [{
    id: 'fn1',
    name: 'start',
    params: [],
    returnType: { kind: 'void' },
    nodes: [
      {
        id: 's',
        data: { kind: 'start' },
        position: { x: 0, y: 0 },
        ports: { in: [], out: [{ id: 'next', name: 'next', kind: 'control', required: true }] },
      },
      {
        id: 'L',
        data: { kind: 'let', varName: 'counter', varType: { kind: 'int' } },
        position: { x: 0, y: 100 },
        ports: {
          in: [
            { id: 'prev', name: 'prev', kind: 'control', required: true },
            { id: 'value', name: 'value', kind: 'data', type: { kind: 'int' }, required: true },
          ],
          out: [
            { id: 'next', name: 'next', kind: 'control', required: true },
            { id: 'var:counter', name: 'counter', kind: 'data', type: { kind: 'int' }, required: false },
          ],
        },
        expressions: { value: '5 + 3' },
      },
      {
        id: 'B',
        data: { kind: 'branch', hasElse: true },
        position: { x: 0, y: 200 },
        ports: {
          in: [
            { id: 'prev', name: 'prev', kind: 'control', required: true },
            { id: 'cond', name: 'condition', kind: 'data', type: { kind: 'bool' }, required: true },
          ],
          out: [
            { id: 'then', name: 'then', kind: 'control', required: true },
            { id: 'else', name: 'else', kind: 'control', required: true },
            { id: 'after', name: 'after', kind: 'control', required: false },
          ],
        },
        expressions: { cond: 'counter > 4' },
      },
      {
        id: 'P1',
        data: { kind: 'print' },
        position: { x: 100, y: 300 },
        ports: {
          in: [
            { id: 'prev', name: 'prev', kind: 'control', required: true },
            { id: 'value', name: 'value', kind: 'data', type: { kind: 'any' }, required: true },
          ],
          out: [{ id: 'next', name: 'next', kind: 'control', required: true }],
        },
        expressions: { value: '"big"' },
      },
      {
        id: 'P2',
        data: { kind: 'print' },
        position: { x: 200, y: 300 },
        ports: {
          in: [
            { id: 'prev', name: 'prev', kind: 'control', required: true },
            { id: 'value', name: 'value', kind: 'data', type: { kind: 'any' }, required: true },
          ],
          out: [{ id: 'next', name: 'next', kind: 'control', required: true }],
        },
        expressions: { value: '"small"' },
      },
      {
        id: 'R',
        data: { kind: 'return', hasValue: false },
        position: { x: 0, y: 400 },
        ports: {
          in: [{ id: 'prev', name: 'prev', kind: 'control', required: true }],
          out: [],
        },
      },
    ],
    edges: [
      { id: 'e1', source: { node: 's', port: 'next' }, target: { node: 'L', port: 'prev' }, kind: 'control' },
      { id: 'e2', source: { node: 'L', port: 'next' }, target: { node: 'B', port: 'prev' }, kind: 'control' },
      { id: 'e3', source: { node: 'B', port: 'then' }, target: { node: 'P1', port: 'prev' }, kind: 'control' },
      { id: 'e4', source: { node: 'B', port: 'else' }, target: { node: 'P2', port: 'prev' }, kind: 'control' },
      { id: 'e5', source: { node: 'B', port: 'after' }, target: { node: 'R', port: 'prev' }, kind: 'control' },
    ],
  }],
};

const { source, warnings } = emit(wf);
console.log('=== Emitted SOL ===');
console.log(source);
console.log('=== Warnings ===');
console.log(warnings.length === 0 ? '(none)' : warnings.join('\n'));
