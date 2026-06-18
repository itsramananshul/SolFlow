/**
 * Tests for the semantic-repair + semantic-lint layer.
 *
 * Pins the deterministic fixes the server applies BEFORE handing
 * the spec back to the client. Each test mirrors a real failure
 * mode we observe from production models (Gemini Flash, Llama
 * 3.3 70B Instruct, GPT-4o, etc.).
 *
 * The headline regression: "for user in users" inside a forEach
 * node's `value` field must be repaired to just `users`, not
 * left to break semantic lint downstream.
 */
import { describe, expect, it } from 'vitest';
import { repairSemantics } from '../_semanticRepair';
import { lintSemantics } from '../_validate';
import type { GeneratedGraphSpec } from '../../../src/sol-man/types';

function specWithNode(
  partial: Partial<{ id: string; kind: string; value: string; cond: string }>,
): GeneratedGraphSpec {
  return {
    meta: { name: 't', description: '' },
    nodes: [
      // A trigger is required by the schema validator; we always
      // prefix one so the test specs are valid.
      { id: 'trigger', kind: 'trigger', triggerKind: 'manual' },
      {
        id: partial.id ?? 'n1',
        kind: (partial.kind ?? 'print') as never,
        ...(partial.value !== undefined ? { value: partial.value } : {}),
        ...(partial.cond !== undefined ? { cond: partial.cond } : {}),
      } as never,
    ],
    edges: [],
  };
}

describe('repairSemantics — for-loop unwrap (the headline case)', () => {
  it('rewrites "for user in users" → "users"', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for user in users' });
    const out = repairSemantics(spec);
    expect(out.repairs).toHaveLength(1);
    expect(out.repairs[0].kind).toBe('unwrap_for_loop');
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('users');
  });

  it('rewrites "for item of payload.items" → "payload.items"', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for item of payload.items' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('payload.items');
  });

  it('rewrites "for (i in arr)" → "arr"', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for (i in arr)' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('arr');
  });

  it('headline integration: post-repair output passes lintSemantics', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for user in users' });
    const out = repairSemantics(spec);
    const issues = lintSemantics(out.spec);
    // After the repair, the offending `for` is gone.
    expect(issues).toEqual([]);
  });
});

describe('repairSemantics — statement-keyword stripping', () => {
  it('rewrites "return user.email" → "user.email" on a return node value', () => {
    const spec = specWithNode({ kind: 'return', value: 'return user.email' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('user.email');
  });

  it('rewrites "let total = payload.total" → "payload.total"', () => {
    const spec = specWithNode({ kind: 'let', value: 'let total = payload.total' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('payload.total');
  });

  it('rewrites "if amount > 100 then send" → "amount > 100" on a branch cond', () => {
    const spec = specWithNode({ kind: 'branch', cond: 'if amount > 100 then send' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.cond).toBe('amount > 100');
  });

  it('rewrites "while user.active" → "user.active" on a while cond', () => {
    const spec = specWithNode({ kind: 'while', cond: 'while user.active' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.cond).toBe('user.active');
  });
});

describe('repairSemantics — print wrapper unwrap', () => {
  it('rewrites print("hello") → "hello"', () => {
    const spec = specWithNode({ kind: 'print', value: 'print("hello")' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('"hello"');
  });

  it('rewrites print(amount) → amount', () => {
    const spec = specWithNode({ kind: 'print', value: 'print(amount)' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('amount');
  });

  it('does NOT unwrap when the print() arguments contain unbalanced parens', () => {
    // Defensive: the regex would otherwise capture across nested
    // parens incorrectly.
    const spec = specWithNode({ kind: 'print', value: 'print(foo(bar)' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('print(foo(bar)');
  });
});

describe('repairSemantics — pseudocode brackets + bare labels', () => {
  it('rewrites "<the user email>" → "the user email"', () => {
    const spec = specWithNode({ kind: 'print', value: "<the user email>" });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    // Strip → bare label → quote.
    expect(node?.value).toBe('"the user email"');
  });

  it('rewrites "{varName}" → "varName"', () => {
    const spec = specWithNode({ kind: 'print', value: '{varName}' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    expect(node?.value).toBe('varName');
  });

  it('quotes "Send order for approval" → \'"Send order for approval"\'', () => {
    const spec = specWithNode({ kind: 'print', value: 'Send order for approval' });
    const out = repairSemantics(spec);
    const node = out.spec.nodes.find((n) => n.id === 'n1');
    // Note: "for" is a stripped keyword first, then the remainder
    // becomes a label and gets quoted.
    // The actual repair pipeline strips leading keywords only —
    // "for" is mid-string, so it survives until the bare-label
    // quoting step.
    expect(node?.value).toBe('"Send order for approval"');
  });
});

describe('repairSemantics — leaves clean expressions alone', () => {
  it('does not modify "users"', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'users' });
    const out = repairSemantics(spec);
    expect(out.repairs).toEqual([]);
    expect(out.spec.nodes.find((n) => n.id === 'n1')?.value).toBe('users');
  });

  it('does not modify "amount > 1000.0"', () => {
    const spec = specWithNode({ kind: 'branch', cond: 'amount > 1000.0' });
    const out = repairSemantics(spec);
    expect(out.repairs).toEqual([]);
  });

  it('does not modify already-quoted string literals', () => {
    const spec = specWithNode({ kind: 'print', value: '"Send for approval"' });
    const out = repairSemantics(spec);
    expect(out.repairs).toEqual([]);
  });

  it('does not strip commas inside a normal arithmetic expression', () => {
    const spec = specWithNode({ kind: 'let', value: '(a + b) * 2' });
    const out = repairSemantics(spec);
    expect(out.repairs).toEqual([]);
  });
});

describe('lintSemantics — mirrors editor lint rules', () => {
  it('flags forbidden_keyword for "for x in y"', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for x in y' });
    const issues = lintSemantics(spec);
    expect(issues).toHaveLength(1);
    expect(issues[0].kind).toBe('forbidden_keyword');
    expect(issues[0].offender).toBe('for');
    expect(issues[0].nodeId).toBe('n1');
    expect(issues[0].field).toBe('value');
    expect(issues[0].suggestion).toMatch(/forEach/);
  });

  it('flags forbidden_keyword for "return user.email"', () => {
    const spec = specWithNode({ kind: 'return', value: 'return user.email' });
    const issues = lintSemantics(spec);
    expect(issues).toHaveLength(1);
    expect(issues[0].offender).toBe('return');
  });

  it('flags js_global for "Math.floor(x)"', () => {
    const spec = specWithNode({ kind: 'let', value: 'Math.floor(x)' });
    const issues = lintSemantics(spec);
    expect(issues[0].kind).toBe('js_global');
    expect(issues[0].offender).toBe('Math');
  });

  it('allows method-call shape (module.func(args) is a valid capability call)', () => {
    // The new SOL grammar accepts import-qualified capability calls
    // like `slack.send({ ... })`, so the lint no longer flags the
    // method-call shape — it can't statically tell a real capability
    // call from a stray JS-style method.
    const spec = specWithNode({ kind: 'let', value: 'user.name.toUpperCase()' });
    const issues = lintSemantics(spec);
    expect(issues.some((i) => i.kind === 'method_call')).toBe(false);
  });

  it('flags js_syntax for arrow functions', () => {
    const spec = specWithNode({ kind: 'let', value: '(x) => x + 1' });
    const issues = lintSemantics(spec);
    expect(issues[0].kind).toBe('js_syntax');
    expect(issues[0].offender).toBe('=>');
  });

  it('passes for clean expressions', () => {
    const spec: GeneratedGraphSpec = {
      meta: { name: 't', description: '' },
      nodes: [
        { id: 't', kind: 'trigger', triggerKind: 'manual' },
        { id: 'n1', kind: 'print', value: '"hello"' },
        { id: 'n2', kind: 'let', varName: 'x', varType: 'int', value: 'payload.x' },
        { id: 'n3', kind: 'branch', cond: 'x > 0' },
      ],
      edges: [],
    };
    expect(lintSemantics(spec)).toEqual([]);
  });
});

describe('repairSemantics + lintSemantics — end-to-end recovery', () => {
  it('the headline failure ("for user in users") is fully repaired so lint passes', () => {
    const spec = specWithNode({ kind: 'forEach', value: 'for user in users' });
    // Pre-repair: lint sees the `for` keyword.
    expect(lintSemantics(spec)).toHaveLength(1);
    // Repair fixes it.
    const out = repairSemantics(spec);
    expect(out.repairs.length).toBeGreaterThan(0);
    // Post-repair: lint passes.
    expect(lintSemantics(out.spec)).toEqual([]);
  });

  it('multi-issue spec: repair fixes what it can; lint catches the rest', () => {
    const spec: GeneratedGraphSpec = {
      meta: { name: 't', description: '' },
      nodes: [
        { id: 't', kind: 'trigger', triggerKind: 'manual' },
        // Recoverable.
        { id: 'a', kind: 'forEach', value: 'for u in users' },
        // Recoverable.
        { id: 'b', kind: 'print', value: 'print("done")' },
        // Recoverable (let stripped).
        { id: 'c', kind: 'let', varName: 'x', varType: 'int', value: 'let x = 0' },
        // NOT recoverable — a JS global. The repair leaves it; lint
        // catches it; the user sees a real error.
        { id: 'd', kind: 'let', varName: 'y', varType: 'float', value: 'Math.PI' },
      ],
      edges: [],
    };
    const repaired = repairSemantics(spec).spec;
    expect(repaired.nodes.find((n) => n.id === 'a')?.value).toBe('users');
    expect(repaired.nodes.find((n) => n.id === 'b')?.value).toBe('"done"');
    expect(repaired.nodes.find((n) => n.id === 'c')?.value).toBe('0');
    // `Math.PI` is not in a shape the repair touches → still there.
    expect(repaired.nodes.find((n) => n.id === 'd')?.value).toBe('Math.PI');
    // Lint catches the one that wasn't fixed.
    const issues = lintSemantics(repaired);
    expect(issues).toHaveLength(1);
    expect(issues[0].nodeId).toBe('d');
    expect(issues[0].kind).toBe('js_global');
  });
});
