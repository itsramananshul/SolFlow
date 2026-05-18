/**
 * SolFlow Phase A — palette catalog.
 *
 * Single source of truth for the node-kind list shown in the left palette
 * and the per-kind port builder.
 */

import type { NodeKind, SolType } from './schema';

export type Category =
  | 'flow'
  | 'variable'
  | 'operator'
  | 'literal'
  | 'access'
  | 'call'
  | 'io'
  | 'entry';

export interface PaletteEntry {
  kind: NodeKind;
  label: string;
  category: Category;
  description: string;
  draggable: boolean; // start is not draggable; auto-placed
}

export const PALETTE: PaletteEntry[] = [
  // entry — present but not in palette
  { kind: 'start', label: 'Start', category: 'entry', description: 'Function entry', draggable: false },

  // flow
  { kind: 'branch', label: 'Branch', category: 'flow', description: 'if / else', draggable: true },
  { kind: 'while', label: 'While', category: 'flow', description: 'while loop', draggable: true },
  { kind: 'forEach', label: 'For Each', category: 'flow', description: 'for x in array', draggable: true },
  { kind: 'return', label: 'Return', category: 'flow', description: 'return [value]', draggable: true },

  // variables
  { kind: 'let', label: 'Let', category: 'variable', description: 'let x: T = …', draggable: true },
  { kind: 'assign', label: 'Assign', category: 'variable', description: 'x = …', draggable: true },
  { kind: 'varGet', label: 'Var Get', category: 'variable', description: 'reference a variable', draggable: true },

  // operators
  { kind: 'binaryOp', label: 'Binary Op', category: 'operator', description: '+ − × / compare, logic', draggable: true },
  { kind: 'unaryOp', label: 'Unary Op', category: 'operator', description: '−x, !x', draggable: true },

  // literals
  { kind: 'literal', label: 'Literal', category: 'literal', description: 'int / float / bool / str / char', draggable: true },
  { kind: 'arrayLiteral', label: 'Array Literal', category: 'literal', description: '[a, b, c]', draggable: true },
  { kind: 'structLiteral', label: 'Struct Literal', category: 'literal', description: 'Name { field: … }', draggable: true },

  // access
  { kind: 'fieldAccess', label: 'Field Get', category: 'access', description: 'expr.field', draggable: true },
  { kind: 'fieldSet', label: 'Field Set', category: 'access', description: 'expr.field = …', draggable: true },
  { kind: 'indexRead', label: 'Index Get', category: 'access', description: 'arr[i]', draggable: true },
  { kind: 'indexSet', label: 'Index Set', category: 'access', description: 'arr[i] = …', draggable: true },
  { kind: 'enumVariant', label: 'Enum Variant', category: 'access', description: 'E::V', draggable: true },

  // io / calls
  { kind: 'print', label: 'Print', category: 'io', description: 'print(…)', draggable: true },
  { kind: 'call', label: 'Call', category: 'call', description: 'call a function', draggable: true },
];

export function paletteByCategory(): Record<Category, PaletteEntry[]> {
  const map: Record<Category, PaletteEntry[]> = {
    flow: [],
    variable: [],
    operator: [],
    literal: [],
    access: [],
    call: [],
    io: [],
    entry: [],
  };
  for (const entry of PALETTE) {
    if (entry.draggable) map[entry.category].push(entry);
  }
  return map;
}

export const CATEGORY_LABELS: Record<Category, string> = {
  flow: 'Flow',
  variable: 'Variables',
  operator: 'Operators',
  literal: 'Literals',
  access: 'Access',
  call: 'Calls',
  io: 'I/O',
  entry: 'Entry',
};

export function categoryColor(c: Category): string {
  switch (c) {
    case 'flow':
      return 'var(--sf-cat-flow)';
    case 'variable':
      return 'var(--sf-cat-variable)';
    case 'operator':
      return 'var(--sf-cat-operator)';
    case 'literal':
      return 'var(--sf-cat-literal)';
    case 'access':
      return 'var(--sf-cat-access)';
    case 'call':
      return 'var(--sf-cat-call)';
    case 'io':
      return 'var(--sf-cat-io)';
    case 'entry':
      return 'var(--sf-cat-entry)';
  }
}

export function categoryForKind(kind: NodeKind): Category {
  return PALETTE.find((p) => p.kind === kind)?.category ?? 'flow';
}

export function defaultLetType(): SolType {
  return { kind: 'int' };
}
