/**
 * SolFlow Phase A — in-browser SOL interpreter.
 *
 * PHASE A — TEMPORARY IMPLEMENTATION.
 * Walks the workflow graph and simulates execution so users can test their
 * workflows in the browser without the SOL controller. Replacement target:
 * Phase B WASM `compile() + VM::run()` running real bytecode against the
 * actual SOL VM.
 *
 * Scope:
 * - All 22 wired node kinds (let, assign, print, return, branch, while,
 *   for-each, binaryOp, unaryOp, varGet, literal, arrayLiteral,
 *   structLiteral, fieldAccess, fieldSet, indexRead, indexSet,
 *   enumVariant, call, plus start).
 * - Inline expressions (`node.expressions[portId]`) are evaluated via the
 *   `Function` constructor with a small SOL→JS translation for `E::V`
 *   enum syntax. Complex inline syntax (struct literals, user-function
 *   calls inside inline expressions) is not supported — wire it instead.
 * - Safe guards: max 100k execution steps, max 1000-frame call stack,
 *   max 60s wall clock. Catches infinite loops without freezing the tab.
 */

import type {
  FunctionGraph,
  GraphNode,
  SolWorkflow,
} from '@/graph/schema';

export interface RunResult {
  ok: boolean;
  output: string[];
  returnValue?: unknown;
  error?: string;
  steps: number;
  durationMs: number;
}

const MAX_STEPS = 100_000;
const MAX_CALL_DEPTH = 1000;
const MAX_DURATION_MS = 60_000;

type Scope = Record<string, unknown>;

interface RunCtx {
  workflow: SolWorkflow;
  output: string[];
  steps: number;
  started: number;
  callDepth: number;
}

class ReturnSignal {
  constructor(public value: unknown) {}
}

class RuntimeError extends Error {}

export function run(workflow: SolWorkflow): RunResult {
  const ctx: RunCtx = {
    workflow,
    output: [],
    steps: 0,
    started: Date.now(),
    callDepth: 0,
  };

  const start = workflow.functions.find((f) => f.name === 'start');
  if (!start) {
    return {
      ok: false,
      output: [],
      error: 'No `start` function found.',
      steps: 0,
      durationMs: 0,
    };
  }

  try {
    const result = callFunction(ctx, start, []);
    return {
      ok: true,
      output: ctx.output,
      returnValue: result,
      steps: ctx.steps,
      durationMs: Date.now() - ctx.started,
    };
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return {
      ok: false,
      output: ctx.output,
      error: msg,
      steps: ctx.steps,
      durationMs: Date.now() - ctx.started,
    };
  }
}

// =============================================================
//  Function dispatch
// =============================================================

function callFunction(ctx: RunCtx, fn: FunctionGraph, args: unknown[]): unknown {
  if (ctx.callDepth >= MAX_CALL_DEPTH) {
    throw new RuntimeError(`Maximum call depth ${MAX_CALL_DEPTH} exceeded`);
  }
  ctx.callDepth++;

  const scope: Scope = Object.create(null);
  fn.params.forEach((p, i) => {
    scope[p.name] = args[i];
  });

  const start = fn.nodes.find((n) => n.data.kind === 'start');
  if (!start) {
    ctx.callDepth--;
    return undefined;
  }

  try {
    walkChain(ctx, fn, scope, start.id, 'next');
  } catch (e) {
    if (e instanceof ReturnSignal) {
      ctx.callDepth--;
      return e.value;
    }
    ctx.callDepth--;
    throw e;
  }
  ctx.callDepth--;
  return undefined;
}

// =============================================================
//  Control-chain walker
// =============================================================

function walkChain(
  ctx: RunCtx,
  fn: FunctionGraph,
  scope: Scope,
  fromNodeId: string,
  outPort: string,
): void {
  const visitedThisStep = new Set<string>();
  let edge = findOutgoingControl(fn, fromNodeId, outPort);

  while (edge) {
    tick(ctx);

    const next = fn.nodes.find((n) => n.id === edge!.target.node);
    if (!next) break;

    // Loop nodes are their own visit-targets, not flagged.
    if (visitedThisStep.has(next.id) && next.data.kind !== 'while' && next.data.kind !== 'forEach') {
      break;
    }
    visitedThisStep.add(next.id);

    executeStatement(ctx, fn, scope, next);

    if (next.data.kind === 'return') {
      const hasVal = next.data.hasValue;
      const value = hasVal ? resolveDataInput(ctx, fn, scope, next, 'value') : undefined;
      throw new ReturnSignal(value);
    }

    // Loop / branch continue via their 'after' port.
    if (
      next.data.kind === 'branch' ||
      next.data.kind === 'while' ||
      next.data.kind === 'forEach'
    ) {
      edge = findOutgoingControl(fn, next.id, 'after');
    } else {
      edge = findOutgoingControl(fn, next.id, 'next');
    }
  }
}

function findOutgoingControl(fn: FunctionGraph, nodeId: string, port: string) {
  return fn.edges.find(
    (e) => e.kind === 'control' && e.source.node === nodeId && e.source.port === port,
  );
}

function tick(ctx: RunCtx) {
  ctx.steps++;
  if (ctx.steps > MAX_STEPS) {
    throw new RuntimeError(`Step limit exceeded (${MAX_STEPS}) — likely infinite loop`);
  }
  if (Date.now() - ctx.started > MAX_DURATION_MS) {
    throw new RuntimeError(`Time limit exceeded (${MAX_DURATION_MS}ms)`);
  }
}

// =============================================================
//  Statement execution
// =============================================================

function executeStatement(
  ctx: RunCtx,
  fn: FunctionGraph,
  scope: Scope,
  node: GraphNode,
): void {
  const data = node.data;
  switch (data.kind) {
    case 'start':
      return;
    case 'let': {
      const value = resolveDataInput(ctx, fn, scope, node, 'value');
      scope[data.varName] = value;
      return;
    }
    case 'assign': {
      const value = resolveDataInput(ctx, fn, scope, node, 'value');
      if (!(data.varName in scope)) {
        throw new RuntimeError(`Cannot assign to undefined variable: ${data.varName}`);
      }
      scope[data.varName] = value;
      return;
    }
    case 'print': {
      const value = resolveDataInput(ctx, fn, scope, node, 'value');
      ctx.output.push(formatValue(value));
      return;
    }
    case 'return':
      // Handled by walkChain via ReturnSignal.
      return;
    case 'branch': {
      const cond = resolveDataInput(ctx, fn, scope, node, 'cond');
      if (toBool(cond)) {
        walkChain(ctx, fn, scope, node.id, 'then');
      } else if (data.hasElse) {
        walkChain(ctx, fn, scope, node.id, 'else');
      }
      return;
    }
    case 'while': {
      let safety = 0;
      while (true) {
        tick(ctx);
        const cond = resolveDataInput(ctx, fn, scope, node, 'cond');
        if (!toBool(cond)) break;
        walkChain(ctx, fn, scope, node.id, 'body');
        if (++safety > MAX_STEPS) {
          throw new RuntimeError('While loop safety limit hit');
        }
      }
      return;
    }
    case 'forEach': {
      const arr = resolveDataInput(ctx, fn, scope, node, 'array');
      if (!Array.isArray(arr)) {
        throw new RuntimeError(`for-each: not an array — got ${typeof arr}`);
      }
      for (const item of arr) {
        tick(ctx);
        scope[data.iteratorName] = item;
        walkChain(ctx, fn, scope, node.id, 'body');
      }
      return;
    }
    case 'fieldSet': {
      const target = resolveDataInput(ctx, fn, scope, node, 'target');
      const value = resolveDataInput(ctx, fn, scope, node, 'value');
      if (target && typeof target === 'object') {
        (target as Record<string, unknown>)[data.fieldName] = value;
      } else {
        throw new RuntimeError(`fieldSet: target is not a struct`);
      }
      return;
    }
    case 'indexSet': {
      const arr = resolveDataInput(ctx, fn, scope, node, 'array');
      const idx = toInt(resolveDataInput(ctx, fn, scope, node, 'index'));
      const value = resolveDataInput(ctx, fn, scope, node, 'value');
      if (!Array.isArray(arr)) {
        throw new RuntimeError(`indexSet: target is not an array`);
      }
      arr[idx] = value;
      return;
    }
    case 'call': {
      const callee = ctx.workflow.functions.find((f) => f.id === data.functionId);
      if (!callee) {
        throw new RuntimeError(`call: target function not found`);
      }
      const args = callee.params.map((p) =>
        resolveDataInput(ctx, fn, scope, node, `arg:${p.name}`),
      );
      callFunction(ctx, callee, args);
      return;
    }
    default:
      return;
  }
}

// =============================================================
//  Expression evaluation
// =============================================================

function resolveDataInput(
  ctx: RunCtx,
  fn: FunctionGraph,
  scope: Scope,
  node: GraphNode,
  portId: string,
): unknown {
  // Inline expression first.
  const inline = node.expressions?.[portId];
  if (inline !== undefined && inline.trim() !== '') {
    return evalInline(ctx, scope, inline);
  }
  // Wired edge.
  const edge = fn.edges.find(
    (e) => e.kind === 'data' && e.target.node === node.id && e.target.port === portId,
  );
  if (!edge) return undefined;
  const src = fn.nodes.find((n) => n.id === edge.source.node);
  if (!src) return undefined;
  return evalNode(ctx, fn, scope, src, edge.source.port);
}

function evalNode(
  ctx: RunCtx,
  fn: FunctionGraph,
  scope: Scope,
  node: GraphNode,
  outPort: string,
): unknown {
  tick(ctx);
  const data = node.data;
  switch (data.kind) {
    case 'literal':
      return parseLiteralValue(data.litType, data.value);
    case 'varGet':
      if (data.varName in scope) return scope[data.varName];
      throw new RuntimeError(`undefined variable: ${data.varName}`);
    case 'binaryOp': {
      const lhs = resolveDataInput(ctx, fn, scope, node, 'lhs');
      const rhs = resolveDataInput(ctx, fn, scope, node, 'rhs');
      return applyBinaryOp(data.op, lhs, rhs);
    }
    case 'unaryOp': {
      const operand = resolveDataInput(ctx, fn, scope, node, 'operand');
      return applyUnaryOp(data.op, operand);
    }
    case 'arrayLiteral': {
      const items: unknown[] = [];
      for (let i = 0; i < data.length; i++) {
        items.push(resolveDataInput(ctx, fn, scope, node, `item:${i}`));
      }
      return items;
    }
    case 'structLiteral': {
      const struct = ctx.workflow.structs.find((s) => s.name === data.structName);
      const obj: Record<string, unknown> = { __struct: data.structName };
      for (const f of struct?.fields ?? []) {
        obj[f.name] = resolveDataInput(ctx, fn, scope, node, `field:${f.name}`);
      }
      return obj;
    }
    case 'fieldAccess': {
      const target = resolveDataInput(ctx, fn, scope, node, 'target');
      if (target && typeof target === 'object') {
        return (target as Record<string, unknown>)[data.fieldName];
      }
      throw new RuntimeError(`fieldAccess: target is not a struct`);
    }
    case 'indexRead': {
      const arr = resolveDataInput(ctx, fn, scope, node, 'array');
      const idx = toInt(resolveDataInput(ctx, fn, scope, node, 'index'));
      if (!Array.isArray(arr)) {
        throw new RuntimeError(`indexRead: not an array`);
      }
      return arr[idx];
    }
    case 'enumVariant': {
      const e = ctx.workflow.enums.find((en) => en.name === data.enumName);
      const variant = e?.variants.find((v) => v.name === data.variantName);
      // Represent enum variants as a stable token: "E::V" for equality compare.
      return `${data.enumName}::${data.variantName}` + (variant?.value !== null && variant?.value !== undefined ? `(${variant.value})` : '');
    }
    case 'call': {
      const callee = ctx.workflow.functions.find((f) => f.id === data.functionId);
      if (!callee) throw new RuntimeError(`call: target function not found`);
      const args = callee.params.map((p) =>
        resolveDataInput(ctx, fn, scope, node, `arg:${p.name}`),
      );
      return callFunction(ctx, callee, args);
    }
    case 'forEach':
      // Iterator data-out: returns the current binding.
      if (outPort === 'item') return scope[data.iteratorName];
      return undefined;
    case 'let':
      // Stable port id 'var' reads the variable's current value;
      // 'var:<name>' kept for older saved graphs.
      if (outPort === 'var') return scope[data.varName];
      if (outPort.startsWith('var:')) return scope[outPort.slice(4)];
      return undefined;
    default:
      return undefined;
  }
}

// =============================================================
//  Operators
// =============================================================

function applyBinaryOp(op: string, a: unknown, b: unknown): unknown {
  switch (op) {
    case '+': return numOrConcat(a, b, (x, y) => x + y);
    case '-': return num(a) - num(b);
    case '*': return num(a) * num(b);
    case '/': {
      const denom = num(b);
      if (denom === 0) throw new RuntimeError('division by zero');
      return num(a) / denom;
    }
    case '==': return eq(a, b);
    case '!=': return !eq(a, b);
    case '<':  return num(a) < num(b);
    case '>':  return num(a) > num(b);
    case '<=': return num(a) <= num(b);
    case '>=': return num(a) >= num(b);
    case '&&': return toBool(a) && toBool(b);
    case '||': return toBool(a) || toBool(b);
    default: throw new RuntimeError(`unknown binary op: ${op}`);
  }
}

function applyUnaryOp(op: string, x: unknown): unknown {
  switch (op) {
    case '-': return -num(x);
    case '!': return !toBool(x);
    default: throw new RuntimeError(`unknown unary op: ${op}`);
  }
}

function num(v: unknown): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string' && !isNaN(Number(v))) return Number(v);
  throw new RuntimeError(`expected number, got ${typeof v}`);
}

function numOrConcat(a: unknown, b: unknown, f: (x: number, y: number) => number): unknown {
  if (typeof a === 'string' || typeof b === 'string') {
    return String(a) + String(b);
  }
  return f(num(a), num(b));
}

function eq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  // String-equivalent for enum variants from inline vs wired.
  if (typeof a === 'string' && typeof b === 'string') {
    // Match `E::V` <-> `E::V(N)`.
    const norm = (s: string) => s.replace(/\([^)]*\)$/, '');
    return norm(a) === norm(b);
  }
  return false;
}

function toBool(v: unknown): boolean {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  if (typeof v === 'string') return v.length > 0 && v !== 'false';
  return Boolean(v);
}

function toInt(v: unknown): number {
  const n = num(v);
  return Math.trunc(n);
}

// =============================================================
//  Inline-expression evaluation
// =============================================================

function evalInline(ctx: RunCtx, scope: Scope, expr: string): unknown {
  // Translate SOL syntax that JS can't parse.
  // E::V → "E::V" string. (Wired enumVariant nodes also produce this string,
  // so equality compares correctly.)
  const jsExpr = expr.replace(
    /\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)/g,
    '"$1::$2"',
  );

  try {
    const names = Object.keys(scope);
    const values = names.map((n) => scope[n]);
    // eslint-disable-next-line no-new-func
    const f = new Function(...names, `return (${jsExpr});`);
    return f(...values);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    throw new RuntimeError(`inline expr "${expr}": ${msg}`);
  }
}

// =============================================================
//  Literal parsing
// =============================================================

function parseLiteralValue(t: string, raw: string): unknown {
  const v = raw ?? '';
  switch (t) {
    case 'int':
      return v.trim() === '' ? 0 : parseInt(v, 10);
    case 'float':
      return v.trim() === '' ? 0 : parseFloat(v);
    case 'bool':
      return v === 'true';
    case 'str':
      return v;
    case 'char':
      return v.length > 0 ? v[0] : ' ';
    default:
      return v;
  }
}

// =============================================================
//  Display
// =============================================================

function formatValue(v: unknown): string {
  if (v === null || v === undefined) return 'void';
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return String(v);
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (Array.isArray(v)) return '[' + v.map(formatValue).join(', ') + ']';
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (typeof o.__struct === 'string') {
      const fields = Object.entries(o)
        .filter(([k]) => k !== '__struct')
        .map(([k, val]) => `${k}: ${formatValue(val)}`)
        .join(', ');
      return `${o.__struct} { ${fields} }`;
    }
    return JSON.stringify(o);
  }
  return String(v);
}
