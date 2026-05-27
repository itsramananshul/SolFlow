/**
 * Typed mirror of the SOL compiler's AST.
 *
 * These types match the JSON serde produces from
 * `compiler/src/parser.rs::Ast`, `compiler/src/parser.rs::Type`,
 * and `compiler/src/lexer.rs::Token`. The shape is pinned by the
 * `ast_json_shape_snapshot` test in `compiler/tests/serde_roundtrip.rs`
 * — any drift breaks that test and the importer in lockstep.
 *
 * Why we mirror by hand:
 * - We deliberately don't ship Rust-generated types (`tsify`, etc.)
 *   yet because the AST shape is still in flux and a wrapper costs
 *   more than it saves at this stage.
 * - Keeping the unions narrow lets the importer pattern-match
 *   exhaustively with TS discriminated-union analysis.
 */

// ----------------------------------------------------------------
//  Type — `parser.rs::Type`
// ----------------------------------------------------------------

/** Built-in primitive types — serialized as plain strings. */
export type PrimitiveType =
  | 'Void'
  | 'Integer'
  | 'Float'
  | 'String'
  | 'Char'
  | 'Bool';

export type SolType =
  | PrimitiveType
  | { Tuple: SolType[] }
  | { Array: { size: number | null; inner: SolType } }
  | { Ident: string }
  | { Function: { params: SolType[]; ret: SolType } };

/** Convenience type guards (TS narrowing). */
export const isPrimitiveType = (t: SolType): t is PrimitiveType =>
  typeof t === 'string';

// ----------------------------------------------------------------
//  Token — `lexer.rs::Token` (only the variants used as op fields)
// ----------------------------------------------------------------

/**
 * Operator tokens that appear inside `ExprBinary.op` and
 * `ExprUnary.op`. The lexer's Token enum has many more variants
 * (delimiters, keywords, literals) but those never appear in AST
 * op fields, so we keep the union narrow to what the importer
 * actually has to handle.
 */
export type BinOpToken =
  // Assignment (yes, `=` is a binary op in this AST — see notes)
  | 'Eq'
  // Arithmetic
  | 'Plus' | 'Dash' | 'Star' | 'Slash'
  // Comparison
  | 'EqEq' | 'BangEq' | 'MoreThan' | 'LessThan' | 'MoreEq' | 'LessEq'
  // Logical
  | 'AmpAmp' | 'PipePipe'
  // Bitwise
  | 'Ampersand' | 'Pipe' | 'Caret' | 'LShift' | 'RShift';

export type UnaryOpToken = 'Dash' | 'Bang' | 'Tilde';

// ----------------------------------------------------------------
//  Ast — `parser.rs::Ast`
// ----------------------------------------------------------------

/**
 * Top-level program is just `Vec<Ast>` — declarations + maybe stray
 * statements that the parser accepts.
 */
export type Program = Ast[];

/**
 * Tagged union mirroring `Ast`. Each variant is wrapped in a
 * single-key object (serde's external-tagging) so TS narrowing
 * works on the variant key:
 *
 *   if ('DeclFunc' in node) { node.DeclFunc.name; ... }
 *
 * Unit variants are plain strings (`"ExprUndefined"`).
 */
export type Ast =
  // ---------- Declarations ----------
  | { DeclFunc: DeclFunc }
  | { DeclExtFunc: DeclExtFunc }
  | { DeclVar: DeclVar }
  | { DeclStruct: DeclStruct }
  | { DeclEnum: DeclEnum }

  // ---------- Statements ----------
  | { Block: Block }
  | { StmtImport: StmtImport }
  | { StmtIf: StmtIf }
  | { StmtWhile: StmtWhile }
  | { StmtFor: StmtFor }

  // ---------- Expressions ----------
  | { ExprAssign: { var_name: string; value: Ast } }
  | { ExprBinary: { lhs: Ast; rhs: Ast; op: BinOpToken } }
  | { ExprUnary: { child: Ast; op: UnaryOpToken } }
  | { ExprFuncCall: { name: string; args: Ast[] } }
  | { ExprMemAcc: { lhs: Ast; member: string } }
  | { ExprEnumVar: { name: string; var: string } }
  | { ExprArrAcc: { lhs: Ast; index: Ast } }
  | { ExprReturn: { val: Ast | null } }
  | { ExprInteger: number }
  | { ExprFloat: number }
  | { ExprString: string }
  | { ExprChar: string } // serde serializes char as a 1-char string
  | { ExprBool: boolean }
  | 'ExprUndefined'
  | { ExprVar: string }
  | { ExprStructInit: { name: string; fields: [string, Ast][] } }
  | { ExprArrayInit: { values: Ast[] } };

// Struct payloads broken out for readability.

/**
 * Source byte range (B.D c35). Optional on each variant below — the
 * parser populates it for these 10 struct variants; old AST JSON
 * (pre-spans) deserializes with `span = undefined` and the
 * analyzer + importer fall back to enclosing-block spans or textual
 * heuristics.
 */
export interface AstSourceSpan {
  start: number;
  end: number;
}

export interface DeclFunc {
  name: string;
  /** Vec<(String, Type)> → array-of-pairs in JSON. */
  params: [string, SolType][];
  ret: SolType;
  body: Ast;
  /** TypeTableId; usize::MAX (large number) until the analyzer runs. */
  scope: number;
  span?: AstSourceSpan;
}

export interface DeclExtFunc {
  name: string;
  params: [string, SolType][];
  ret: SolType;
  span?: AstSourceSpan;
}

export interface DeclVar {
  name: string;
  kind: SolType;
  value: Ast | null;
  span?: AstSourceSpan;
}

export interface DeclStruct {
  name: string;
  /** HashMap<String, Type> → plain JSON object. Order is NOT stable
   *  (serde + HashMap); importers should sort by name. */
  fields: Record<string, SolType>;
  span?: AstSourceSpan;
}

export interface DeclEnum {
  name: string;
  /** HashMap<String, isize> → plain JSON object. */
  variants: Record<string, number>;
  span?: AstSourceSpan;
}

export interface Block {
  block: Ast[];
  scope: number;
  span?: AstSourceSpan;
}

export interface StmtImport {
  path: string[];
  alias: string | null;
  span?: AstSourceSpan;
}

export interface StmtIf {
  condition: Ast;
  body: Ast;
  alt: Ast | null;
  span?: AstSourceSpan;
}

export interface StmtWhile {
  condition: Ast;
  body: Ast;
  span?: AstSourceSpan;
}

export interface StmtFor {
  elem_name: string;
  array: Ast;
  body: Ast;
  span?: AstSourceSpan;
}

// ----------------------------------------------------------------
//  Narrow helpers
// ----------------------------------------------------------------

/** Return the variant key of an Ast node ("DeclFunc", "ExprInteger", etc.). */
export function astKind(a: Ast): string {
  if (typeof a === 'string') return a;
  // Tagged-union: each variant is a single-key object.
  for (const k of Object.keys(a)) return k;
  return 'Unknown';
}

/** Quick test for "is this an assignment expression?" The parser
 *  encodes assignment as `ExprBinary { op: 'Eq' }` rather than
 *  `ExprAssign` in current builds, so the importer needs both. */
export function isAssignment(
  a: Ast,
): a is { ExprBinary: { lhs: Ast; rhs: Ast; op: 'Eq' } } | { ExprAssign: { var_name: string; value: Ast } } {
  if (typeof a === 'string') return false;
  if ('ExprAssign' in a) return true;
  if ('ExprBinary' in a && a.ExprBinary.op === 'Eq') return true;
  return false;
}
