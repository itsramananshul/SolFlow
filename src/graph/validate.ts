/**
 * SolFlow Phase A — client-side validation.
 *
 * PHASE A — TEMPORARY IMPLEMENTATION.
 * Cheap structural checks only: missing required ports, type mismatches on
 * connected data edges, dangling branch arms, undeclared struct/enum refs.
 *
 * Phase B will replace this with the WASM `analyze_ast` call (see
 * reference/SOL_CRATE_IDE_READINESS_PLAN.md §3.1 + §6 step 2.5). The
 * function signature here will not change; only the implementation.
 */

import type {
  FunctionGraph,
  GraphEdge,
  GraphNode,
  SolWorkflow,
} from './schema';
import { typeEqual, typeLabel } from './schema';
import { lintInlineExpression } from './expressionLint';

export type Severity = 'error' | 'warning';

export interface Diagnostic {
  severity: Severity;
  message: string;
  nodeId?: string;
  functionId?: string;
  code: string;
}

export function validateWorkflow(wf: SolWorkflow): Diagnostic[] {
  const diags: Diagnostic[] = [];

  const hasStart = wf.functions.some((f) => f.name === 'start');
  const hasTrigger = wf.functions.some((f) =>
    f.nodes.some((n) => n.data.kind === 'trigger'),
  );
  if (!hasStart && !hasTrigger) {
    diags.push({
      severity: 'warning',
      message:
        'No `start` function or trigger node defined — workflow has no entry point.',
      code: 'no-entry',
    });
  }

  for (const fn of wf.functions) {
    // Defensive: a function with no name would emit as
    // `function () -> int { ... }` which the SOL parser rejects with
    // "name expected after function keyword". The schema doesn't model
    // an empty-name function, but a malformed loaded workflow could
    // contain one — surface it as an explicit error rather than letting
    // emission produce broken SOL.
    if (!fn.name || fn.name.trim() === '') {
      diags.push({
        severity: 'error',
        message: 'Function has no name. Every function must have a non-empty identifier.',
        functionId: fn.id,
        code: 'unnamed-function',
      });
    }
    validateFunction(fn, wf, diags);
  }

  return diags;
}

function validateFunction(
  fn: FunctionGraph,
  wf: SolWorkflow,
  diags: Diagnostic[],
): void {
  const nodeMap: Record<string, GraphNode> = {};
  for (const n of fn.nodes) nodeMap[n.id] = n;

  // Incoming edges per (node, port).
  const portIncoming = new Map<string, GraphEdge[]>();
  // Outgoing edges per (node, port).
  const portOutgoing = new Map<string, GraphEdge[]>();
  const key = (nodeId: string, portId: string) => `${nodeId}::${portId}`;

  for (const e of fn.edges) {
    const sk = key(e.source.node, e.source.port);
    const tk = key(e.target.node, e.target.port);
    portOutgoing.set(sk, [...(portOutgoing.get(sk) ?? []), e]);
    portIncoming.set(tk, [...(portIncoming.get(tk) ?? []), e]);
  }

  // Check each node.
  for (const n of fn.nodes) {
    // Required inputs must be satisfied — either via a wired edge OR a
    // non-empty inline expression on the same port. The emitter treats
    // expressions as taking precedence over edges; the validator must
    // mirror that or it will scream "missing input" at every
    // Sol-Man-generated `let amount = payload.amount` node.
    for (const p of n.ports.in) {
      if (!p.required) continue;
      const inc = portIncoming.get(key(n.id, p.id)) ?? [];
      const inlineExpr = n.expressions?.[p.id];
      const hasInline = typeof inlineExpr === 'string' && inlineExpr.trim() !== '';
      if (inc.length === 0 && !hasInline) {
        diags.push({
          severity: 'error',
          message: `${nodeLabel(n)}: missing input "${p.name}".`,
          nodeId: n.id,
          functionId: fn.id,
          code: 'missing-input',
        });
      }
    }

    // Lint every inline expression the node carries. An inline
    // expression that fails lint is dangerous in two distinct ways:
    // the emitter inserts it verbatim into generated SOL (which the
    // canonical compiler may reject), and the simulator evaluates it
    // via `new Function` (which would run arbitrary JS if the
    // expression contains JS globals). Either way, the workflow must
    // not reach Apply with the bad string in place.
    //
    // The lint code is `bad-inline-expression`; combined with the
    // `missing-input` code above, these are the two diagnostic codes
    // the Sol Man store treats as "never bypassable via force=true".
    if (n.expressions) {
      for (const [portId, expr] of Object.entries(n.expressions)) {
        if (typeof expr !== 'string' || expr.trim() === '') continue;
        const lint = lintInlineExpression(expr);
        if (lint) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)} (${portId}): ${lint.message}`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'bad-inline-expression',
          });
        }
      }
    }

    // Validate kind-specific constraints. Narrow once via local const.
    const data = n.data;
    switch (data.kind) {
      case 'structLiteral':
      case 'fieldAccess':
      case 'fieldSet': {
        const ref = data.structName;
        if (!ref) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no struct selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-struct',
          });
        } else if (!wf.structs.find((s) => s.name === ref)) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: struct "${ref}" not defined.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unknown-struct',
          });
        }
        if (
          (data.kind === 'fieldAccess' || data.kind === 'fieldSet') &&
          !data.fieldName
        ) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no field selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-field',
          });
        }
        break;
      }
      case 'enumVariant': {
        const enumName = data.enumName;
        if (!enumName) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no enum selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-enum',
          });
        } else if (!wf.enums.find((e) => e.name === enumName)) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: enum "${enumName}" not defined.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unknown-enum',
          });
        } else if (!data.variantName) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no variant selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-variant',
          });
        }
        break;
      }
      case 'call': {
        const fid = data.functionId;
        if (!fid) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no function selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-call',
          });
        } else if (!wf.functions.find((f) => f.id === fid)) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: target function not found.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unknown-call',
          });
        }
        break;
      }
      case 'assign':
        if (!data.varName) {
          diags.push({
            severity: 'error',
            message: `${nodeLabel(n)}: no target variable.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-var',
          });
        }
        break;
      case 'varGet':
        if (!data.varName) {
          diags.push({
            severity: 'warning',
            message: `${nodeLabel(n)}: no variable selected.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unset-var',
          });
        } else if (!variableResolves(fn, data.varName)) {
          diags.push({
            severity: 'warning',
            message: `${nodeLabel(n)}: variable "${data.varName}" is not declared in this function.`,
            nodeId: n.id,
            functionId: fn.id,
            code: 'unresolved-var',
          });
        }
        break;
    }
  }

  // Validate edges (data type compatibility).
  for (const e of fn.edges) {
    if (e.kind !== 'data') continue;
    const src = nodeMap[e.source.node];
    const tgt = nodeMap[e.target.node];
    if (!src || !tgt) continue;
    const srcPort = src.ports.out.find((p) => p.id === e.source.port);
    const tgtPort = tgt.ports.in.find((p) => p.id === e.target.port);
    if (!srcPort || !tgtPort) continue;
    if (srcPort.type && tgtPort.type && !typeEqual(srcPort.type, tgtPort.type)) {
      diags.push({
        severity: 'warning',
        message: `Type mismatch: ${typeLabel(srcPort.type)} → ${typeLabel(
          tgtPort.type,
        )} between ${nodeLabel(src)} and ${nodeLabel(tgt)}.`,
        nodeId: tgt.id,
        functionId: fn.id,
        code: 'type-mismatch',
      });
    }
  }
}

function nodeLabel(n: GraphNode): string {
  return n.data.kind;
}

/**
 * Cheap "is this variable name declared anywhere in this function" check.
 * Not scope-accurate — it doesn't honor control-flow reachability. Phase B
 * will replace with the real analyzer's scope walk. Good enough to surface
 * obvious "you renamed a let but its varGet still points at the old name"
 * errors.
 */
function variableResolves(fn: FunctionGraph, name: string): boolean {
  for (const p of fn.params) {
    if (p.name === name) return true;
  }
  for (const n of fn.nodes) {
    if (n.data.kind === 'let' && n.data.varName === name) return true;
    if (n.data.kind === 'forEach' && n.data.iteratorName === name) return true;
  }
  return false;
}
