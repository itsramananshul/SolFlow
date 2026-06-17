/**
 * Pure unit tests for the canonical AST → SOL expression printer.
 * No WASM needed — the printer takes typed AST values directly.
 * Mirrors `sol/src/format.rs::fmt_expr`.
 */
import { describe, expect, it } from 'vitest';
import type { Expr } from '@/compiler/ast';
import { stringifyExpr } from '../expressions';

describe('stringifyExpr — primitives', () => {
  it('integers', () => {
    expect(stringifyExpr({ Int: 42 })).toBe('42');
    expect(stringifyExpr({ Int: 0 })).toBe('0');
    expect(stringifyExpr({ Int: -7 })).toBe('-7');
  });
  it('floats always carry a decimal point', () => {
    expect(stringifyExpr({ Float: 3.14 })).toBe('3.14');
    // 1.0 → toString() drops the .0, printer must re-add it.
    expect(stringifyExpr({ Float: 1.0 })).toBe('1.0');
  });
  it('strings are quoted + escaped', () => {
    expect(stringifyExpr({ Str: 'hi' })).toBe('"hi"');
    expect(stringifyExpr({ Str: 'he said "x"' })).toBe('"he said \\"x\\""');
    expect(stringifyExpr({ Str: 'a\\b' })).toBe('"a\\\\b"');
  });
  it('chars are single-quoted', () => {
    expect(stringifyExpr({ Char: 'a' })).toBe("'a'");
  });
  it('bools', () => {
    expect(stringifyExpr({ Bool: true })).toBe('true');
    expect(stringifyExpr({ Bool: false })).toBe('false');
  });
  it('idents', () => {
    expect(stringifyExpr({ Ident: 'count' })).toBe('count');
  });
});

describe('stringifyExpr — operators', () => {
  it('binary ops are parenthesized', () => {
    expect(stringifyExpr({ BinOp: [{ Int: 1 }, 'Add', { Int: 2 }] })).toBe('(1 + 2)');
  });
  it('every comparison op', () => {
    const cases: Array<[string, string]> = [
      ['Eq', '=='],
      ['Ne', '!='],
      ['Lt', '<'],
      ['Gt', '>'],
      ['Le', '<='],
      ['Ge', '>='],
    ];
    for (const [op, surface] of cases) {
      const expr = { BinOp: [{ Ident: 'a' }, op, { Ident: 'b' }] } as Expr;
      expect(stringifyExpr(expr)).toBe(`(a ${surface} b)`);
    }
  });
  it('unary ops (parenthesized, matches fmt_expr)', () => {
    expect(stringifyExpr({ UnaryOp: [{ Bool: true }, 'Not'] })).toBe('(!true)');
    expect(stringifyExpr({ UnaryOp: [{ Int: 5 }, 'Neg'] })).toBe('(-5)');
  });
});

describe('stringifyExpr — compound', () => {
  it('function call', () => {
    const expr: Expr = { Call: [{ Ident: 'add' }, [{ Int: 1 }, { Int: 2 }]] };
    expect(stringifyExpr(expr)).toBe('add(1, 2)');
  });
  it('member access', () => {
    expect(stringifyExpr({ MemberAccess: [{ Ident: 'p' }, 'x'] })).toBe('p.x');
  });
  it('index access', () => {
    expect(stringifyExpr({ Index: [{ Ident: 'a' }, { Int: 0 }] })).toBe('a[0]');
  });
  it('array literal', () => {
    expect(stringifyExpr({ Array: [{ Int: 1 }, { Int: 2 }, { Int: 3 }] })).toBe('[1, 2, 3]');
  });
  it('enum variant', () => {
    expect(
      stringifyExpr({ EnumVariant: { enum_name: 'Status', variant: 'Active' } }),
    ).toBe('Status::Active');
  });
  it('named struct instance', () => {
    expect(
      stringifyExpr({
        StructInstance: {
          name: 'Point',
          fields: [
            ['x', { Int: 1 }],
            ['y', { Int: 2 }],
          ],
        },
      }),
    ).toBe('Point { x: 1, y: 2 }');
  });
  it('anonymous struct instance (call params)', () => {
    expect(
      stringifyExpr({
        StructInstance: { name: '', fields: [['msg', { Str: 'hi' }]] },
      }),
    ).toBe('{ msg: "hi" }');
  });
  it('workflow call', () => {
    expect(
      stringifyExpr({
        WorkflowCall: {
          capability_expr: { Str: 'alert.fire' },
          params: { StructInstance: { name: '', fields: [['n', { Int: 3 }]] } },
        },
      }),
    ).toBe('call("alert.fire", { n: 3 })');
  });
  it('namespace call', () => {
    expect(
      stringifyExpr({
        NamespaceCall: { namespace: { Ident: 'math' }, name: 'abs', args: [{ Int: 5 }] },
      }),
    ).toBe('math::abs(5)');
  });
  it('nested binary op recurses with parens', () => {
    const expr: Expr = {
      BinOp: [{ BinOp: [{ Int: 1 }, 'Add', { Int: 2 }] }, 'Mul', { Int: 3 }],
    };
    expect(stringifyExpr(expr)).toBe('((1 + 2) * 3)');
  });
});
