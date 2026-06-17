/**
 * Canonical AST expression → SOL string printer.
 *
 * Used by the importer to convert AST expressions back into SOL text
 * for inline-expression node fields (e.g. `let x: int = a + b;` embeds
 * `(a + b)` as the `value` port's inline expression).
 *
 * This mirrors the canonical Rust pretty-printer `sol/src/format.rs`
 * (`fmt_expr`) exactly so the text we embed re-parses to an equal
 * expression. Round-trip stability depends on that match: if you
 * change `fmt_expr`, change this in lockstep.
 *
 * Not goals:
 * - Source-text fidelity — we re-print from AST, so whitespace and
 *   parenthesization may differ from the original input. Equivalent
 *   semantics, not byte-identical to the user's typing.
 */

import type { BinOp, Expr, UnaryOp } from '@/compiler/ast';

/** Map canonical BinOp → SOL surface syntax (mirrors fmt_binop). */
const BIN_OP_SYNTAX: Record<BinOp, string> = {
  Add: '+',
  Sub: '-',
  Mul: '*',
  Div: '/',
  Eq: '==',
  Ne: '!=',
  Lt: '<',
  Gt: '>',
  Le: '<=',
  Ge: '>=',
  And: '&&',
  Or: '||',
};

const UNARY_OP_SYNTAX: Record<UnaryOp, string> = {
  Neg: '-',
  Not: '!',
};

/** Print a canonical AST expression as SOL source. */
export function stringifyExpr(e: Expr): string {
  if ('Int' in e) return String(e.Int);
  if ('Float' in e) {
    // Always emit a decimal point so the lexer parses a Float.
    const s = String(e.Float);
    return s.includes('.') || s.includes('e') || s.includes('E') ? s : `${s}.0`;
  }
  if ('Bool' in e) return e.Bool ? 'true' : 'false';
  if ('Char' in e) return `'${escapeChar(e.Char)}'`;
  if ('Str' in e) return `"${escapeStr(e.Str)}"`;

  if ('Array' in e) {
    return `[${e.Array.map(stringifyExpr).join(', ')}]`;
  }

  if ('StructInstance' in e) {
    const { name, fields } = e.StructInstance;
    const body = fields.map(([k, v]) => `${k}: ${stringifyExpr(v)}`).join(', ');
    if (name === '') return body === '' ? '{}' : `{ ${body} }`;
    return body === '' ? `${name} {}` : `${name} { ${body} }`;
  }

  if ('EnumVariant' in e) {
    return `${e.EnumVariant.enum_name}::${e.EnumVariant.variant}`;
  }

  if ('Ident' in e) return e.Ident;

  if ('MemberAccess' in e) {
    return `${stringifyExpr(e.MemberAccess[0])}.${e.MemberAccess[1]}`;
  }

  if ('Index' in e) {
    return `${stringifyExpr(e.Index[0])}[${stringifyExpr(e.Index[1])}]`;
  }

  if ('BinOp' in e) {
    const [lhs, op, rhs] = e.BinOp;
    const sym = BIN_OP_SYNTAX[op] ?? '?';
    // Always parenthesize — matches fmt_expr, avoids precedence math.
    return `(${stringifyExpr(lhs)} ${sym} ${stringifyExpr(rhs)})`;
  }

  if ('UnaryOp' in e) {
    const [child, op] = e.UnaryOp;
    const sym = UNARY_OP_SYNTAX[op] ?? '?';
    return `(${sym}${stringifyExpr(child)})`;
  }

  if ('Call' in e) {
    const [callee, args] = e.Call;
    return `${stringifyExpr(callee)}(${args.map(stringifyExpr).join(', ')})`;
  }

  if ('WorkflowCall' in e) {
    return `call(${stringifyExpr(e.WorkflowCall.capability_expr)}, ${stringifyExpr(e.WorkflowCall.params)})`;
  }

  if ('NamespaceCall' in e) {
    const { namespace, name, args } = e.NamespaceCall;
    return `${stringifyExpr(namespace)}::${name}(${args.map(stringifyExpr).join(', ')})`;
  }

  // Exhaustive above; this is unreachable for well-formed Expr.
  return '/* unprintable */';
}

function escapeStr(s: string): string {
  return s
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\t/g, '\\t');
}

function escapeChar(c: string): string {
  if (c === '\n') return '\\n';
  if (c === '\t') return '\\t';
  if (c === '\\') return '\\\\';
  if (c === "'") return "\\'";
  return c;
}
