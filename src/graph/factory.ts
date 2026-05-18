/**
 * SolFlow Phase A — node factory.
 *
 * Given a NodeKind and a context (workflow → resolved struct/enum/function
 * definitions), produce a GraphNode with the correct initial data + ports.
 *
 * Ports are recomputed any time the node's `data` changes (see
 * `rebuildPorts` below) — that lets us add/remove inputs reactively when
 * the user changes a struct literal's struct, etc.
 */

import { nanoid } from 'nanoid';

import type {
  EnumDecl,
  FunctionGraph,
  GraphNode,
  NodeData,
  NodeKind,
  NodePorts,
  Port,
  SolType,
  StructDecl,
  StructField,
} from './schema';
import { binaryOpResultType, unaryOpResultType } from './schema';

// =============================================================
//  Default `data` per kind
// =============================================================

export function defaultData(kind: NodeKind): NodeData {
  switch (kind) {
    case 'start':
      return { kind: 'start' };
    case 'let':
      return { kind: 'let', varName: 'x', varType: { kind: 'int' } };
    case 'assign':
      return { kind: 'assign', varName: '' };
    case 'print':
      return { kind: 'print' };
    case 'return':
      return { kind: 'return', hasValue: false };
    case 'branch':
      return { kind: 'branch', hasElse: true };
    case 'while':
      return { kind: 'while' };
    case 'forEach':
      return { kind: 'forEach', iteratorName: 'item', iteratorType: { kind: 'int' } };
    case 'binaryOp':
      return { kind: 'binaryOp', op: '+', valueType: { kind: 'int' } };
    case 'unaryOp':
      return { kind: 'unaryOp', op: '-', valueType: { kind: 'int' } };
    case 'varGet':
      return { kind: 'varGet', varName: '', resolvedType: { kind: 'any' } };
    case 'literal':
      return { kind: 'literal', litType: 'int', value: '0' };
    case 'arrayLiteral':
      return { kind: 'arrayLiteral', itemType: { kind: 'int' }, length: 3 };
    case 'structLiteral':
      return { kind: 'structLiteral', structName: '' };
    case 'fieldAccess':
      return { kind: 'fieldAccess', structName: '', fieldName: '' };
    case 'fieldSet':
      return { kind: 'fieldSet', structName: '', fieldName: '' };
    case 'indexRead':
      return { kind: 'indexRead', elementType: { kind: 'int' } };
    case 'indexSet':
      return { kind: 'indexSet', elementType: { kind: 'int' } };
    case 'enumVariant':
      return { kind: 'enumVariant', enumName: '', variantName: '' };
    case 'call':
      return { kind: 'call', functionId: '' };
  }
}

// =============================================================
//  Port builder — derives ports from a node's current data.
// =============================================================

const CTL = (id: string, name: string, required = true): Port => ({
  id,
  name,
  kind: 'control',
  required,
});

const DATA = (
  id: string,
  name: string,
  type: SolType,
  required = true,
): Port => ({
  id,
  name,
  kind: 'data',
  type,
  required,
});

export interface WorkflowCtx {
  structs: StructDecl[];
  enums: EnumDecl[];
  functions: FunctionGraph[];
}

export function rebuildPorts(data: NodeData, ctx: WorkflowCtx): NodePorts {
  switch (data.kind) {
    case 'start':
      return {
        in: [],
        out: [CTL('next', 'next')],
      };
    case 'let':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA('value', 'value', data.varType, true),
        ],
        out: [
          CTL('next', 'next'),
          DATA(`var:${data.varName}`, data.varName || 'var', data.varType, false),
        ],
      };
    case 'assign':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA('value', 'value', { kind: 'any' }, true),
        ],
        out: [CTL('next', 'next')],
      };
    case 'print':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA('value', 'value', { kind: 'any' }, true),
        ],
        out: [CTL('next', 'next')],
      };
    case 'return':
      return {
        in: [
          CTL('prev', 'prev'),
          ...(data.hasValue
            ? [DATA('value', 'value', { kind: 'any' }, true)]
            : []),
        ],
        out: [],
      };
    case 'branch':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA('cond', 'condition', { kind: 'bool' }, true),
        ],
        out: [
          CTL('then', 'then'),
          ...(data.hasElse ? [CTL('else', 'else')] : []),
          CTL('after', 'after', false),
        ],
      };
    case 'while':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA('cond', 'condition', { kind: 'bool' }, true),
        ],
        out: [
          CTL('body', 'body'),
          CTL('after', 'after', false),
        ],
      };
    case 'forEach':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA(
            'array',
            'array',
            { kind: 'array', size: null, inner: data.iteratorType },
            true,
          ),
        ],
        out: [
          CTL('body', 'body'),
          CTL('after', 'after', false),
          DATA('item', data.iteratorName || 'item', data.iteratorType, false),
        ],
      };
    case 'binaryOp':
      return {
        in: [
          DATA('lhs', 'lhs', data.valueType, true),
          DATA('rhs', 'rhs', data.valueType, true),
        ],
        out: [
          DATA('result', 'result', binaryOpResultType(data.op, data.valueType), false),
        ],
      };
    case 'unaryOp':
      return {
        in: [DATA('operand', 'operand', data.valueType, true)],
        out: [
          DATA('result', 'result', unaryOpResultType(data.op, data.valueType), false),
        ],
      };
    case 'varGet':
      return {
        in: [],
        out: [DATA('value', data.varName || 'value', data.resolvedType, false)],
      };
    case 'literal': {
      const t: SolType = { kind: data.litType };
      return { in: [], out: [DATA('value', 'value', t, false)] };
    }
    case 'arrayLiteral': {
      const inPorts: Port[] = [];
      for (let i = 0; i < Math.max(0, data.length); i++) {
        inPorts.push(DATA(`item:${i}`, `item ${i}`, data.itemType, true));
      }
      return {
        in: inPorts,
        out: [
          DATA(
            'array',
            'array',
            { kind: 'array', size: null, inner: data.itemType },
            false,
          ),
        ],
      };
    }
    case 'structLiteral': {
      const struct = ctx.structs.find((s) => s.name === data.structName);
      const inPorts: Port[] = [];
      if (struct) {
        for (const field of struct.fields) {
          inPorts.push(DATA(`field:${field.name}`, field.name, field.type, true));
        }
      }
      return {
        in: inPorts,
        out: [
          DATA(
            'value',
            data.structName || 'struct',
            { kind: 'named', name: data.structName },
            false,
          ),
        ],
      };
    }
    case 'fieldAccess': {
      const struct = ctx.structs.find((s) => s.name === data.structName);
      const field = struct?.fields.find((f: StructField) => f.name === data.fieldName);
      const outType: SolType = field?.type ?? { kind: 'any' };
      return {
        in: [
          DATA(
            'target',
            'target',
            { kind: 'named', name: data.structName || '?' },
            true,
          ),
        ],
        out: [DATA('value', data.fieldName || 'field', outType, false)],
      };
    }
    case 'fieldSet': {
      const struct = ctx.structs.find((s) => s.name === data.structName);
      const field = struct?.fields.find((f: StructField) => f.name === data.fieldName);
      const valType: SolType = field?.type ?? { kind: 'any' };
      return {
        in: [
          CTL('prev', 'prev'),
          DATA(
            'target',
            'target',
            { kind: 'named', name: data.structName || '?' },
            true,
          ),
          DATA('value', 'value', valType, true),
        ],
        out: [CTL('next', 'next')],
      };
    }
    case 'indexRead':
      return {
        in: [
          DATA(
            'array',
            'array',
            { kind: 'array', size: null, inner: data.elementType },
            true,
          ),
          DATA('index', 'index', { kind: 'int' }, true),
        ],
        out: [DATA('value', 'value', data.elementType, false)],
      };
    case 'indexSet':
      return {
        in: [
          CTL('prev', 'prev'),
          DATA(
            'array',
            'array',
            { kind: 'array', size: null, inner: data.elementType },
            true,
          ),
          DATA('index', 'index', { kind: 'int' }, true),
          DATA('value', 'value', data.elementType, true),
        ],
        out: [CTL('next', 'next')],
      };
    case 'enumVariant':
      return {
        in: [],
        out: [
          DATA(
            'value',
            data.variantName || 'variant',
            { kind: 'named', name: data.enumName || '?' },
            false,
          ),
        ],
      };
    case 'call': {
      const fn = ctx.functions.find((f) => f.id === data.functionId);
      const inPorts: Port[] = [CTL('prev', 'prev')];
      if (fn) {
        for (const p of fn.params) {
          inPorts.push(DATA(`arg:${p.name}`, p.name, p.type, true));
        }
      }
      const outPorts: Port[] = [CTL('next', 'next')];
      if (fn && fn.returnType.kind !== 'void') {
        outPorts.push(DATA('return', 'return', fn.returnType, false));
      }
      return { in: inPorts, out: outPorts };
    }
  }
}

// =============================================================
//  Public factory
// =============================================================

export function createNode(
  kind: NodeKind,
  position: { x: number; y: number },
  ctx: WorkflowCtx,
  overrides?: Partial<NodeData>,
): GraphNode {
  const data = { ...defaultData(kind), ...(overrides as object) } as NodeData;
  return {
    id: nanoid(8),
    data,
    position,
    ports: rebuildPorts(data, ctx),
  };
}
