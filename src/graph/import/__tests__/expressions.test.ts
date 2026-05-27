/**
 * Pure unit tests for the AST → SOL expression printer.
 * No WASM needed — the printer takes typed AST values directly.
 */
import { describe, expect, it } from 'vitest';
import type { Ast } from '@/compiler/ast';
import { stringifyExpr } from '../expressions';

describe('stringifyExpr — primitives', () => {
  it('integers', () => {
    expect(stringifyExpr({ ExprInteger: 42 } as Ast)).toBe('42');
    expect(stringifyExpr({ ExprInteger: 0 } as Ast)).toBe('0');
    expect(stringifyExpr({ ExprInteger: -7 } as Ast)).toBe('-7');
  });
  it('floats always carry a decimal point', () => {
    expect(stringifyExpr({ ExprFloat: 3.14 } as Ast)).toBe('3.14');
    // 1.0 → toString() drops the .0, printer must re-add it.
    expect(stringifyExpr({ ExprFloat: 1.0 } as Ast)).toBe('1.0');
  });
  it('strings are quoted + escaped', () => {
    expect(stringifyExpr({ ExprString: 'hi' } as Ast)).toBe('"hi"');
    expect(stringifyExpr({ ExprString: 'he said "x"' } as Ast)).toBe(
      '"he said \\"x\\""',
    );
    expect(stringifyExpr({ ExprString: 'a\\b' } as Ast)).toBe('"a\\\\b"');
  });
  it('chars are single-quoted', () => {
    expect(stringifyExpr({ ExprChar: 'a' } as Ast)).toBe("'a'");
  });
  it('bools', () => {
    expect(stringifyExpr({ ExprBool: true } as Ast)).toBe('true');
    expect(stringifyExpr({ ExprBool: false } as Ast)).toBe('false');
  });
  it('vars', () => {
    expect(stringifyExpr({ ExprVar: 'count' } as Ast)).toBe('count');
  });
});

describe('stringifyExpr — operators', () => {
  it('binary ops are parenthesized', () => {
    const ast: Ast = {
      ExprBinary: {
        lhs: { ExprInteger: 1 },
        rhs: { ExprInteger: 2 },
        op: 'Plus',
      },
    };
    expect(stringifyExpr(ast)).toBe('(1 + 2)');
  });
  it('every comparison op', () => {
    const cases: Array<[string, string]> = [
      ['EqEq', '=='], ['BangEq', '!='], ['LessThan', '<'],
      ['MoreThan', '>'], ['LessEq', '<='], ['MoreEq', '>='],
    ];
    for (const [token, surface] of cases) {
      const ast = {
        ExprBinary: {
          lhs: { ExprVar: 'a' },
          rhs: { ExprVar: 'b' },
          op: token as never,
        },
      } as Ast;
      expect(stringifyExpr(ast)).toBe(`(a ${surface} b)`);
    }
  });
  it('unary ops', () => {
    expect(
      stringifyExpr({
        ExprUnary: { child: { ExprBool: true }, op: 'Bang' },
      } as Ast),
    ).toBe('!(true)');
    expect(
      stringifyExpr({
        ExprUnary: { child: { ExprInteger: 5 }, op: 'Dash' },
      } as Ast),
    ).toBe('-(5)');
  });
});

describe('stringifyExpr — compound', () => {
  it('function call', () => {
    const ast: Ast = {
      ExprFuncCall: {
        name: 'add',
        args: [{ ExprInteger: 1 }, { ExprInteger: 2 }],
      },
    };
    expect(stringifyExpr(ast)).toBe('add(1, 2)');
  });
  it('member access', () => {
    expect(
      stringifyExpr({
        ExprMemAcc: { lhs: { ExprVar: 'p' }, member: 'x' },
      } as Ast),
    ).toBe('p.x');
  });
  it('array literal', () => {
    expect(
      stringifyExpr({
        ExprArrayInit: {
          values: [{ ExprInteger: 1 }, { ExprInteger: 2 }, { ExprInteger: 3 }],
        },
      } as Ast),
    ).toBe('[1, 2, 3]');
  });
  it('enum variant', () => {
    expect(
      stringifyExpr({ ExprEnumVar: { name: 'Status', var: 'Active' } } as Ast),
    ).toBe('Status::Active');
  });
  it('struct init', () => {
    expect(
      stringifyExpr({
        ExprStructInit: {
          name: 'Point',
          fields: [
            ['x', { ExprInteger: 1 }],
            ['y', { ExprInteger: 2 }],
          ],
        },
      } as Ast),
    ).toBe('Point { x: 1, y: 2 }');
  });
  it('nested binary op recurses with parens', () => {
    const ast: Ast = {
      ExprBinary: {
        lhs: {
          ExprBinary: {
            lhs: { ExprInteger: 1 },
            rhs: { ExprInteger: 2 },
            op: 'Plus',
          },
        },
        rhs: { ExprInteger: 3 },
        op: 'Star',
      },
    };
    expect(stringifyExpr(ast)).toBe('((1 + 2) * 3)');
  });
});

describe('stringifyExpr — degenerate', () => {
  it('ExprUndefined renders as comment', () => {
    expect(stringifyExpr('ExprUndefined' as Ast)).toBe('/* undefined */');
  });
  it('unprintable statement variant degrades gracefully', () => {
    // Pass an actual Block AST — not an expression. Printer should
    // not throw; it returns a sentinel comment.
    const result = stringifyExpr({ Block: { block: [], scope: 0 } } as Ast);
    expect(result).toMatch(/unprintable/);
  });
});
