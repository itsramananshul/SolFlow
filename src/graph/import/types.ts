/**
 * Convert compiler-AST `SolType` to graph-schema `SolType`.
 *
 * The two type unions look similar but aren't structurally
 * identical:
 *
 *   compiler  →  "Integer" | { Array: { size, inner } } | { Ident } | ...
 *   graph     →  { kind: "int" } | { kind: "array", ... } | { kind: "named", name } | ...
 *
 * Mapping:
 *   "Void"     → { kind: "void" }
 *   "Integer"  → { kind: "int" }
 *   "Float"    → { kind: "float" }
 *   "String"   → { kind: "str" }
 *   "Char"     → { kind: "char" }
 *   "Bool"     → { kind: "bool" }
 *   { Array }  → { kind: "array", size, inner }
 *   { Ident }  → { kind: "named", name }     (struct or enum reference)
 *   { Tuple }  → { kind: "any" }             (graph has no tuple type)
 *   { Function } → { kind: "any" }           (graph can't model fn types)
 */

import type { SolType as CompilerType } from '@/compiler/ast';
import type { SolType as GraphType, SolPrimitive } from '../schema';

const PRIM_MAP: Record<string, SolPrimitive | 'void'> = {
  Void: 'void',
  Integer: 'int',
  Float: 'float',
  String: 'str',
  Char: 'char',
  Bool: 'bool',
};

export function compilerTypeToGraphType(t: CompilerType): GraphType {
  if (typeof t === 'string') {
    const mapped = PRIM_MAP[t];
    if (mapped === 'void') return { kind: 'void' };
    if (mapped) return { kind: mapped };
    // Unknown primitive name — degrade to `any` rather than crash.
    return { kind: 'any' };
  }
  if ('Array' in t) {
    return {
      kind: 'array',
      size: t.Array.size,
      inner: compilerTypeToGraphType(t.Array.inner),
    };
  }
  if ('Ident' in t) {
    return { kind: 'named', name: t.Ident };
  }
  // Tuple / Function — no direct graph rep. Use `any` so the editor
  // doesn't reject the workflow; the importer flags this as partial.
  return { kind: 'any' };
}

/**
 * True if compilerTypeToGraphType would lose information for this
 * type (importer uses this to flag the owning construct as partial).
 */
export function isLossyConversion(t: CompilerType): boolean {
  if (typeof t === 'string') return false;
  if ('Array' in t) return isLossyConversion(t.Array.inner);
  if ('Ident' in t) return false;
  return true; // Tuple / Function
}
