/**
 * AST expression → SOL string printer.
 *
 * Used by the importer to convert AST expressions back into SOL
 * text for inline-expression node fields (e.g. `let x: int = a + b;`
 * embeds `a + b` as the `value` port's inline expression).
 *
 * Why we print rather than reconstruct as nodes:
 * - SolFlow's graph schema models expressions as inline strings on
 *   action/control nodes, not as a sub-graph. The expression IS the
 *   data, just expressed textually.
 * - Round-tripping through serialization preserves semantics; the
 *   user can break the expression apart into graph nodes later if
 *   they want, but the canonical compiler-parseable form is
 *   preserved as-is.
 *
 * Not goals (yet):
 * - Source-text fidelity — we re-print from AST, so whitespace and
 *   parenthesization may differ from the original. Equivalent
 *   semantics, not byte-identical output.
 * - Operator-precedence-minimal parens — we always parenthesize
 *   nested binary ops to be safe rather than computing minimum
 *   precedence levels.
 */

import type { Ast, BinOpToken, UnaryOpToken } from '@/compiler/ast';

/** Map BinOpToken → SOL surface syntax. */
const BIN_OP_SYNTAX: Record<BinOpToken, string> = {
  Eq: '=',
  Plus: '+',
  Dash: '-',
  Star: '*',
  Slash: '/',
  EqEq: '==',
  BangEq: '!=',
  MoreThan: '>',
  LessThan: '<',
  MoreEq: '>=',
  LessEq: '<=',
  AmpAmp: '&&',
  PipePipe: '||',
  Ampersand: '&',
  Pipe: '|',
  Caret: '^',
  LShift: '<<',
  RShift: '>>',
};

const UNARY_OP_SYNTAX: Record<UnaryOpToken, string> = {
  Dash: '-',
  Bang: '!',
  Tilde: '~',
};

/**
 * Print an AST node as SOL source.
 *
 * Handles every expression variant; for declarations / statements
 * it produces a best-effort representation (the importer normally
 * passes expressions, but the printer doesn't crash on a statement).
 */
export function stringifyExpr(a: Ast): string {
  if (typeof a === 'string') {
    // The only unit variant is `ExprUndefined`. It only appears in
    // partially-initialized declarations — render as a comment so
    // the result is still parse-clean.
    return '/* undefined */';
  }

  if ('ExprInteger' in a) return String(a.ExprInteger);
  if ('ExprFloat' in a) {
    // Always emit a decimal point so the SOL lexer parses it as Float
    // (otherwise `1` becomes Integer and `let x: float = 1` is a type
    // mismatch). Float.toString() drops trailing `.0` so we re-add it.
    const s = String(a.ExprFloat);
    return s.includes('.') ? s : `${s}.0`;
  }
  if ('ExprString' in a) return `"${escapeStr(a.ExprString)}"`;
  if ('ExprChar' in a) return `'${a.ExprChar}'`;
  if ('ExprBool' in a) return a.ExprBool ? 'true' : 'false';
  if ('ExprVar' in a) return a.ExprVar;

  if ('ExprEnumVar' in a) {
    return `${a.ExprEnumVar.name}::${a.ExprEnumVar.var}`;
  }

  if ('ExprMemAcc' in a) {
    return `${stringifyExpr(a.ExprMemAcc.lhs)}.${a.ExprMemAcc.member}`;
  }

  if ('ExprArrAcc' in a) {
    return `${stringifyExpr(a.ExprArrAcc.lhs)}[${stringifyExpr(a.ExprArrAcc.index)}]`;
  }

  if ('ExprFuncCall' in a) {
    const args = a.ExprFuncCall.args.map(stringifyExpr).join(', ');
    return `${a.ExprFuncCall.name}(${args})`;
  }

  if ('ExprBinary' in a) {
    const op = BIN_OP_SYNTAX[a.ExprBinary.op] ?? '?';
    // Always parenthesize children — safe rather than computing
    // precedence (see file header).
    return `(${stringifyExpr(a.ExprBinary.lhs)} ${op} ${stringifyExpr(a.ExprBinary.rhs)})`;
  }

  if ('ExprUnary' in a) {
    const op = UNARY_OP_SYNTAX[a.ExprUnary.op] ?? '?';
    return `${op}(${stringifyExpr(a.ExprUnary.child)})`;
  }

  if ('ExprAssign' in a) {
    return `${a.ExprAssign.var_name} = ${stringifyExpr(a.ExprAssign.value)}`;
  }

  if ('ExprArrayInit' in a) {
    const items = a.ExprArrayInit.values.map(stringifyExpr).join(', ');
    return `[${items}]`;
  }

  if ('ExprStructInit' in a) {
    const fields = a.ExprStructInit.fields
      .map(([k, v]) => `${k}: ${stringifyExpr(v)}`)
      .join(', ');
    return `${a.ExprStructInit.name} { ${fields} }`;
  }

  if ('ExprReturn' in a) {
    return a.ExprReturn.val === null
      ? 'return'
      : `return ${stringifyExpr(a.ExprReturn.val)}`;
  }

  // Statement / declaration variants — the importer rarely calls
  // stringifyExpr on these, but render them as best-effort so the
  // output is at least debuggable.
  const variantKey = Object.keys(a)[0] ?? 'unknown';
  return `/* unprintable: ${variantKey} */`;
}

function escapeStr(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}
