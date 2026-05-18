/**
 * SolFlow Phase A — graph schema.
 *
 * Strict subset of the Phase B `SolGraph` from
 * `reference/SOL_VISUAL_EDITOR_ANALYSIS.md` §5. Forward-compatible: a Phase A
 * workflow JSON can be loaded as a Phase B graph by wrapping it.
 */

// =============================================================
//  SOL types
// =============================================================

export type SolPrimitive = 'int' | 'float' | 'bool' | 'str' | 'char';

export type SolType =
  | { kind: SolPrimitive }
  | { kind: 'array'; size: number | null; inner: SolType }
  | { kind: 'named'; name: string } // struct or enum reference
  | { kind: 'void' }
  | { kind: 'any' }; // editor-only, used for `print` input + unresolved

export type BinaryOpSymbol =
  | '+'
  | '-'
  | '*'
  | '/'
  | '=='
  | '!='
  | '<'
  | '>'
  | '<='
  | '>='
  | '&&'
  | '||';

export type UnaryOpSymbol = '-' | '!';

// =============================================================
//  Top-level workflow shape
// =============================================================

export interface SolWorkflow {
  schemaVersion: 1;
  meta: {
    name: string;
    createdAt: string;
    updatedAt: string;
  };
  imports: ImportDecl[];
  structs: StructDecl[];
  enums: EnumDecl[];
  functions: FunctionGraph[];
}

export interface ImportDecl {
  id: string;
  path: string[]; // ["EdgeRouter", "SecurityControl", ...]
  alias: string; // local symbol name
}

export interface StructField {
  name: string;
  type: SolType;
}

export interface StructDecl {
  id: string;
  name: string;
  fields: StructField[]; // insertion order preserved (array, not map)
}

export interface EnumVariant {
  name: string;
  value: number | null; // null = auto-assigned
}

export interface EnumDecl {
  id: string;
  name: string;
  variants: EnumVariant[];
}

export interface Param {
  name: string;
  type: SolType;
}

export interface FunctionGraph {
  id: string;
  name: string;
  params: Param[];
  returnType: SolType; // { kind: 'void' } if absent
  nodes: GraphNode[];
  edges: GraphEdge[];
}

// =============================================================
//  Nodes
// =============================================================

export type NodeKind =
  // flow
  | 'start'
  | 'let'
  | 'assign'
  | 'print'
  | 'return'
  | 'branch'
  | 'while'
  | 'forEach'
  // expressions
  | 'binaryOp'
  | 'unaryOp'
  | 'varGet'
  | 'literal'
  | 'arrayLiteral'
  | 'structLiteral'
  | 'fieldAccess'
  | 'fieldSet'
  | 'indexRead'
  | 'indexSet'
  | 'enumVariant'
  | 'call';

export type PortKind = 'control' | 'data';

export interface Port {
  id: string; // unique within node
  name: string; // human label
  kind: PortKind;
  type?: SolType; // present iff kind === 'data'
  required: boolean;
}

export interface NodePorts {
  in: Port[];
  out: Port[];
}

// Discriminated union for per-kind data.
export type NodeData =
  | { kind: 'start' }
  | { kind: 'let'; varName: string; varType: SolType }
  | { kind: 'assign'; varName: string } // assign-to-var (var picked from scope dropdown)
  | { kind: 'print' }
  | { kind: 'return'; hasValue: boolean }
  | { kind: 'branch'; hasElse: boolean }
  | { kind: 'while' }
  | { kind: 'forEach'; iteratorName: string; iteratorType: SolType }
  | { kind: 'binaryOp'; op: BinaryOpSymbol; valueType: SolType }
  | { kind: 'unaryOp'; op: UnaryOpSymbol; valueType: SolType }
  | { kind: 'varGet'; varName: string; resolvedType: SolType }
  | { kind: 'literal'; litType: SolPrimitive; value: string } // value is raw text
  | { kind: 'arrayLiteral'; itemType: SolType; length: number }
  | { kind: 'structLiteral'; structName: string }
  | { kind: 'fieldAccess'; structName: string; fieldName: string }
  | { kind: 'fieldSet'; structName: string; fieldName: string }
  | { kind: 'indexRead'; elementType: SolType }
  | { kind: 'indexSet'; elementType: SolType }
  | { kind: 'enumVariant'; enumName: string; variantName: string }
  | { kind: 'call'; functionId: string }; // refs FunctionGraph.id

export interface GraphNode {
  id: string; // nanoid
  data: NodeData;
  position: { x: number; y: number };
  ports: NodePorts;
}

export interface GraphEdge {
  id: string;
  source: { node: string; port: string };
  target: { node: string; port: string };
  kind: PortKind; // must match both endpoints
}

// =============================================================
//  Type helpers (Phase A — TEMPORARY, will move to WASM in Phase B)
// =============================================================

export function typeEqual(a: SolType, b: SolType): boolean {
  if (a.kind === 'any' || b.kind === 'any') return true;
  if (a.kind !== b.kind) return false;
  if (a.kind === 'array' && b.kind === 'array') {
    if (a.size !== b.size) return false;
    return typeEqual(a.inner, b.inner);
  }
  if (a.kind === 'named' && b.kind === 'named') {
    return a.name === b.name;
  }
  return true;
}

export function typeLabel(t: SolType): string {
  switch (t.kind) {
    case 'void':
      return 'void';
    case 'any':
      return 'any';
    case 'int':
    case 'float':
    case 'bool':
    case 'str':
    case 'char':
      return t.kind;
    case 'array': {
      const sizeStr = t.size === null ? '' : String(t.size);
      return `[${sizeStr}]${typeLabel(t.inner)}`;
    }
    case 'named':
      return t.name;
  }
}

export function typeCssClass(t: SolType | undefined): string {
  if (!t) return 'data-any';
  if (t.kind === 'array') return 'data-array';
  if (t.kind === 'named') return 'data-struct';
  if (t.kind === 'void') return 'data-any';
  return `data-${t.kind}`;
}

// =============================================================
//  Operator metadata
// =============================================================

export const BINARY_OPS: BinaryOpSymbol[] = [
  '+',
  '-',
  '*',
  '/',
  '==',
  '!=',
  '<',
  '>',
  '<=',
  '>=',
  '&&',
  '||',
];
export const UNARY_OPS: UnaryOpSymbol[] = ['-', '!'];

export function isArithmeticOp(op: BinaryOpSymbol): boolean {
  return op === '+' || op === '-' || op === '*' || op === '/';
}

export function isComparisonOp(op: BinaryOpSymbol): boolean {
  return (
    op === '==' || op === '!=' || op === '<' || op === '>' || op === '<=' || op === '>='
  );
}

export function isLogicalOp(op: BinaryOpSymbol): boolean {
  return op === '&&' || op === '||';
}

export function binaryOpResultType(op: BinaryOpSymbol, operandType: SolType): SolType {
  if (isComparisonOp(op) || isLogicalOp(op)) return { kind: 'bool' };
  return operandType;
}

export function unaryOpResultType(op: UnaryOpSymbol, operandType: SolType): SolType {
  if (op === '!') return { kind: 'bool' };
  return operandType;
}
