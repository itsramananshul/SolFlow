/**
 * Convert canonical compiler-AST `SolType` to graph-schema `SolType`.
 *
 *   compiler  →  "Int" | { Array: T } | { Named: string }
 *   graph     →  { kind: "int" } | { kind: "array", ... } | { kind: "named", name }
 *
 * Mapping:
 *   "Bool"    → { kind: "bool" }
 *   "Int"     → { kind: "int" }
 *   "Float"   → { kind: "float" }
 *   "Char"    → { kind: "char" }
 *   "Str"     → { kind: "str" }
 *   { Array } → { kind: "array", size: null, inner }
 *   { Named } → { kind: "named", name }     (struct or enum reference)
 *
 * The canonical type system has no tuple or function types, so every
 * canonical type maps cleanly with no information loss.
 */

import type { SolType as CompilerType } from '@/compiler/ast';
import type { SolType as GraphType, SolPrimitive } from '../schema';

const PRIM_MAP: Record<string, SolPrimitive> = {
  Bool: 'bool',
  Int: 'int',
  Float: 'float',
  Char: 'char',
  Str: 'str',
};

export function compilerTypeToGraphType(t: CompilerType): GraphType {
  if (typeof t === 'string') {
    const mapped = PRIM_MAP[t];
    if (mapped) return { kind: mapped };
    // Unknown primitive name — degrade to `any` rather than crash.
    return { kind: 'any' };
  }
  if ('Array' in t) {
    // The canonical array type carries no fixed size.
    return { kind: 'array', size: null, inner: compilerTypeToGraphType(t.Array) };
  }
  if ('Named' in t) {
    return { kind: 'named', name: t.Named };
  }
  return { kind: 'any' };
}

/**
 * True if compilerTypeToGraphType would lose information for this
 * type. The canonical type system maps cleanly, so this is always
 * false; kept so callers that flag "partial" conversions still
 * compile.
 */
export function isLossyConversion(_t: CompilerType): boolean {
  return false;
}
