/**
 * Typed mirror of the canonical SOL compiler's AST
 * (`openprem-sol-v2`, crate `sol/`).
 *
 * These types match the JSON serde produces from
 * `sol/src/ast.rs` (externally tagged enums). The shapes here were
 * confirmed empirically by calling the WASM bridge's
 * `parse_source_json` on representative programs and reading the
 * returned `value` (the `Program`). If `sol/src/ast.rs` changes its
 * serde shape, regenerate the import fixtures and update this file
 * in lockstep.
 *
 * Serde encoding rules used here:
 * - Enums are externally tagged: each variant is a single-key object
 *   (`{ "Let": { ... } }`), or a plain string for unit variants
 *   (`"Int"`, `"Add"`).
 * - Tuple variants become JSON arrays:
 *   `BinOp(Box<Expr>, BinOp, Box<Expr>)` → `{ "BinOp": [lhs, op, rhs] }`.
 * - Struct fields keep their Rust names, including the trailing
 *   underscore on `type_` (Rust reserves `type`).
 *
 * Keeping the unions narrow lets the importer pattern-match
 * exhaustively with TS discriminated-union analysis.
 */

// ----------------------------------------------------------------
//  Type — `ast.rs::Type`
// ----------------------------------------------------------------

/** Built-in primitive types — serialized as plain strings. */
export type PrimitiveType = 'Bool' | 'Int' | 'Float' | 'Char' | 'Str';

export type SolType =
  | PrimitiveType
  | { Array: SolType } // `[]T` — array of T
  | { Named: string }; // struct or enum reference

/** Convenience type guard (TS narrowing). */
export const isPrimitiveType = (t: SolType): t is PrimitiveType =>
  typeof t === 'string';

// ----------------------------------------------------------------
//  Operators — `ast.rs::BinOp` / `ast.rs::UnaryOp`
// ----------------------------------------------------------------

export type BinOp =
  | 'Add'
  | 'Sub'
  | 'Mul'
  | 'Div'
  | 'Eq'
  | 'Ne'
  | 'Lt'
  | 'Gt'
  | 'Le'
  | 'Ge'
  | 'And'
  | 'Or';

export type UnaryOp = 'Neg' | 'Not';

// ----------------------------------------------------------------
//  Expr — `ast.rs::Expr`
// ----------------------------------------------------------------

export type Expr =
  | { Int: number }
  | { Float: number }
  | { Bool: boolean }
  | { Char: string } // serde serializes char as a 1-char string
  | { Str: string }
  | { Array: Expr[] }
  | { StructInstance: { name: string; fields: [string, Expr][] } }
  | { EnumVariant: { enum_name: string; variant: string } }
  | { Ident: string }
  | { MemberAccess: [Expr, string] }
  | { Index: [Expr, Expr] }
  | { BinOp: [Expr, BinOp, Expr] }
  | { UnaryOp: [Expr, UnaryOp] }
  | { Call: [Expr, Expr[]] }
  | { WorkflowCall: { capability_expr: Expr; params: Expr } }
  | { NamespaceCall: { namespace: Expr; name: string; args: Expr[] } };

// ----------------------------------------------------------------
//  Stmt / Target — `ast.rs::Stmt`, `ast.rs::Target`
// ----------------------------------------------------------------

/**
 * Assignment target. NOTE (2026-06): the canonical parser does not
 * currently parse assignment statements at all (`parse_stmt` produces
 * Let/If/While/For/Return/Emit/Expr only), so `Stmt::Assign` and
 * `Target` are never present in real parser output yet. They are
 * mirrored here to stay faithful to `ast.rs`; the importer tolerates
 * but does not expect them.
 */
export type Target =
  | { Ident: string }
  | { MemberAccess: [Target, string] }
  | { Index: [Target, Expr] };

export type Stmt =
  | { Let: { name: string; type_: SolType; value: Expr } }
  | { Assign: { target: Target; value: Expr } }
  | { If: { condition: Expr; then: Block; else_: Block | null } }
  | { While: { condition: Expr; body: Block } }
  | { For: { item: string; iter: Expr; body: Block } }
  | { Return: Expr | null }
  | { Expr: Expr }
  | { Emit: string };

export interface Block {
  stmts: Stmt[];
}

// ----------------------------------------------------------------
//  Top-level — `ast.rs::TopLevel` and friends
// ----------------------------------------------------------------

export interface Param {
  name: string;
  type_: SolType;
}

export interface FunctionDecl {
  name: string;
  params: Param[];
  /** `null` when the function declares no return type. */
  return_type: SolType | null;
  body: Block;
}

export interface Field {
  name: string;
  type_: SolType;
}

export interface StructDecl {
  name: string;
  /** Insertion order is preserved (Vec in Rust, not HashMap). */
  fields: Field[];
}

export interface EnumDecl {
  name: string;
  /** Variant names only — the canonical enum has no explicit values. */
  variants: string[];
}

export interface WorkflowDecl {
  name: string;
  body: Block;
}

export type ImportSpec =
  | { Module: string }
  | { Named: { name: string; module: string } };

export interface ImportDecl {
  spec: ImportSpec;
}

/**
 * A top-level item — serde external tagging means exactly one key.
 *
 *   if ('Workflow' in item) { item.Workflow.name; ... }
 */
export type TopLevel =
  | { Function: FunctionDecl }
  | { Struct: StructDecl }
  | { Enum: EnumDecl }
  | { Workflow: WorkflowDecl }
  | { Import: ImportDecl };

/** A complete parsed program. */
export interface Program {
  items: TopLevel[];
}

// ----------------------------------------------------------------
//  Narrow helpers
// ----------------------------------------------------------------

/** Return the variant key of a tagged-union node ("Workflow", "Let", "BinOp", …). */
export function variantKey(node: object): string {
  for (const k of Object.keys(node)) return k;
  return 'Unknown';
}
