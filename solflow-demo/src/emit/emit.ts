/**
 * SolFlow Phase A — Graph → SOL exporter.
 *
 * PHASE A — TEMPORARY IMPLEMENTATION.
 * Replacement target: Phase B WASM `emit_sol(graph_to_ast(graph))`.
 * Replace by: SOL_CRATE_IDE_READINESS_PLAN.md §6 step 2.7 + 2.8.
 *
 * Algorithm:
 *   1. For each function, emit signature + body block.
 *   2. Body = topological walk of the control chain from start.
 *   3. Each statement's data inputs are resolved by recursive expression emission.
 *   4. Branch / while / forEach recurse into their body sub-walks.
 *   5. Missing required inputs become `/* missing *​/` markers + a warning.
 */

import type {
  EnumDecl,
  FunctionGraph,
  GraphEdge,
  GraphNode,
  SolWorkflow,
  StructDecl,
} from '@/graph/schema';
import { typeLabel } from '@/graph/schema';

export interface EmitResult {
  source: string;
  warnings: string[];
}

interface EmitCtx {
  fn: FunctionGraph;
  workflow: SolWorkflow;
  warnings: string[];
  // Index for fast lookups.
  nodeMap: Map<string, GraphNode>;
  // incoming[node][port] = edge
  incoming: Map<string, GraphEdge>;
  // outgoing[node][port] = edge[]
  outgoing: Map<string, GraphEdge[]>;
}

function key(node: string, port: string): string {
  return `${node}::${port}`;
}

function buildCtx(fn: FunctionGraph, workflow: SolWorkflow): EmitCtx {
  const nodeMap = new Map<string, GraphNode>();
  for (const n of fn.nodes) nodeMap.set(n.id, n);
  const incoming = new Map<string, GraphEdge>();
  const outgoing = new Map<string, GraphEdge[]>();
  for (const e of fn.edges) {
    incoming.set(key(e.target.node, e.target.port), e);
    const k = key(e.source.node, e.source.port);
    outgoing.set(k, [...(outgoing.get(k) ?? []), e]);
  }
  return { fn, workflow, warnings: [], nodeMap, incoming, outgoing };
}

// =============================================================
//  Public entry point
// =============================================================

export function emit(workflow: SolWorkflow): EmitResult {
  const warnings: string[] = [];
  const out: string[] = [];

  for (const imp of workflow.imports) {
    out.push(emitImport(imp.path, imp.alias));
  }
  if (workflow.imports.length > 0) out.push('');

  for (const s of workflow.structs) {
    out.push(emitStruct(s));
    out.push('');
  }
  for (const e of workflow.enums) {
    out.push(emitEnum(e));
    out.push('');
  }

  for (const fn of workflow.functions) {
    const ctx = buildCtx(fn, workflow);
    out.push(emitFunction(ctx));
    warnings.push(...ctx.warnings);
    out.push('');
  }

  return { source: out.join('\n').replace(/\n{3,}/g, '\n\n').trimEnd() + '\n', warnings };
}

// =============================================================
//  Declarations
// =============================================================

function emitImport(path: string[], alias: string): string {
  return `import ${path.join('.')} as ${alias};`;
}

function emitStruct(s: StructDecl): string {
  if (s.fields.length === 0) {
    return `struct ${s.name} {}`;
  }
  const lines: string[] = [`struct ${s.name} {`];
  for (const f of s.fields) {
    lines.push(`  ${f.name}: ${typeLabel(f.type)},`);
  }
  lines.push('}');
  return lines.join('\n');
}

function emitEnum(e: EnumDecl): string {
  if (e.variants.length === 0) {
    return `enum ${e.name} {}`;
  }
  const lines: string[] = [`enum ${e.name} {`];
  for (const v of e.variants) {
    if (v.value === null) {
      lines.push(`  ${v.name},`);
    } else {
      lines.push(`  ${v.name} = ${v.value},`);
    }
  }
  lines.push('}');
  return lines.join('\n');
}

function emitFunction(ctx: EmitCtx): string {
  const fn = ctx.fn;
  const params = fn.params.map((p) => `${p.name}: ${typeLabel(p.type)}`).join(', ');
  const ret = fn.returnType.kind === 'void' ? '' : ` -> ${typeLabel(fn.returnType)}`;

  const start = fn.nodes.find((n) => n.data.kind === 'start');
  if (!start) {
    ctx.warnings.push(`function ${fn.name}: no start node`);
    return `function ${fn.name}(${params})${ret} {\n}`;
  }
  const body = emitChain(ctx, start.id, 'next', 1);
  const bodyLines = body.length === 0 ? [] : [body];
  return `function ${fn.name}(${params})${ret} {\n${bodyLines.join('')}}`;
}

// =============================================================
//  Statement chain walking
// =============================================================

/**
 * Follow control-out `outPort` of `fromNodeId`, then walk forward through
 * the resulting statement chain, emitting each statement on its own line at
 * the given indent.
 *
 * Returns a string ending with `\n` if it emitted anything, or `''` if the
 * chain was empty.
 */
function emitChain(
  ctx: EmitCtx,
  fromNodeId: string,
  outPort: string,
  indent: number,
): string {
  const visited = new Set<string>();
  const lines: string[] = [];
  let currentEdge = ctx.outgoing.get(key(fromNodeId, outPort))?.[0];

  while (currentEdge) {
    const nextNode = ctx.nodeMap.get(currentEdge.target.node);
    if (!nextNode) break;
    if (visited.has(nextNode.id)) {
      // cycle detected — break to avoid infinite emit
      break;
    }
    visited.add(nextNode.id);

    const emittedStmt = emitStatement(ctx, nextNode, indent);
    if (emittedStmt) lines.push(emittedStmt);

    // Terminators stop the chain.
    if (nextNode.data.kind === 'return') break;

    // Most statements continue along `next`. Branch/while/forEach are special.
    if (
      nextNode.data.kind === 'branch' ||
      nextNode.data.kind === 'while' ||
      nextNode.data.kind === 'forEach'
    ) {
      const after = ctx.outgoing.get(key(nextNode.id, 'after'))?.[0];
      currentEdge = after;
    } else {
      currentEdge = ctx.outgoing.get(key(nextNode.id, 'next'))?.[0];
    }
  }

  return lines.join('');
}

// =============================================================
//  Statement-form node emitters
// =============================================================

function emitStatement(ctx: EmitCtx, node: GraphNode, indent: number): string {
  const pad = '  '.repeat(indent);
  const data = node.data;
  switch (data.kind) {
    case 'let': {
      const ty = typeLabel(data.varType);
      const val = emitDataInput(ctx, node.id, 'value');
      return `${pad}let ${data.varName}: ${ty} = ${val};\n`;
    }
    case 'assign': {
      const val = emitDataInput(ctx, node.id, 'value');
      return `${pad}${data.varName} = ${val};\n`;
    }
    case 'print': {
      const val = emitDataInput(ctx, node.id, 'value');
      return `${pad}print(${val});\n`;
    }
    case 'return': {
      if (data.hasValue) {
        const val = emitDataInput(ctx, node.id, 'value');
        return `${pad}return ${val};\n`;
      }
      return `${pad}return;\n`;
    }
    case 'branch': {
      const cond = emitDataInput(ctx, node.id, 'cond');
      const thenBody = emitChain(ctx, node.id, 'then', indent + 1);
      const out: string[] = [];
      out.push(`${pad}if (${cond}) {\n`);
      out.push(thenBody);
      if (data.hasElse) {
        const elseBody = emitChain(ctx, node.id, 'else', indent + 1);
        out.push(`${pad}} else {\n`);
        out.push(elseBody);
      }
      out.push(`${pad}}\n`);
      return out.join('');
    }
    case 'while': {
      const cond = emitDataInput(ctx, node.id, 'cond');
      const body = emitChain(ctx, node.id, 'body', indent + 1);
      return `${pad}while (${cond}) {\n${body}${pad}}\n`;
    }
    case 'forEach': {
      const arr = emitDataInput(ctx, node.id, 'array');
      const body = emitChain(ctx, node.id, 'body', indent + 1);
      return `${pad}for ${data.iteratorName} in ${arr} {\n${body}${pad}}\n`;
    }
    case 'fieldSet': {
      const target = emitDataInput(ctx, node.id, 'target');
      const val = emitDataInput(ctx, node.id, 'value');
      return `${pad}${target}.${data.fieldName} = ${val};\n`;
    }
    case 'indexSet': {
      const arr = emitDataInput(ctx, node.id, 'array');
      const idx = emitDataInput(ctx, node.id, 'index');
      const val = emitDataInput(ctx, node.id, 'value');
      return `${pad}${arr}[${idx}] = ${val};\n`;
    }
    case 'call': {
      const fn = ctx.workflow.functions.find((f) => f.id === data.functionId);
      const fname = fn?.name ?? '/* unknown */';
      const args = (fn?.params ?? [])
        .map((p) => emitDataInput(ctx, node.id, `arg:${p.name}`))
        .join(', ');
      return `${pad}${fname}(${args});\n`;
    }
    default:
      return '';
  }
}

// =============================================================
//  Expression emitters (recursive)
// =============================================================

function emitDataInput(ctx: EmitCtx, nodeId: string, portId: string): string {
  const edge = ctx.incoming.get(key(nodeId, portId));
  if (!edge) {
    ctx.warnings.push(`${nodeId}::${portId}: missing input`);
    return '/* missing */';
  }
  const src = ctx.nodeMap.get(edge.source.node);
  if (!src) return '/* missing */';
  return emitExpression(ctx, src, edge.source.port);
}

function emitExpression(ctx: EmitCtx, node: GraphNode, outPort: string): string {
  const data = node.data;
  switch (data.kind) {
    case 'literal': {
      return formatLiteral(data.litType, data.value);
    }
    case 'varGet': {
      return data.varName || '/* unset */';
    }
    case 'binaryOp': {
      const lhs = emitDataInput(ctx, node.id, 'lhs');
      const rhs = emitDataInput(ctx, node.id, 'rhs');
      return `(${lhs} ${data.op} ${rhs})`;
    }
    case 'unaryOp': {
      const operand = emitDataInput(ctx, node.id, 'operand');
      return `${data.op}${operand}`;
    }
    case 'arrayLiteral': {
      const items: string[] = [];
      for (let i = 0; i < data.length; i++) {
        items.push(emitDataInput(ctx, node.id, `item:${i}`));
      }
      return `[${items.join(', ')}]`;
    }
    case 'structLiteral': {
      const structName = data.structName;
      const struct = ctx.workflow.structs.find((s) => s.name === structName);
      const lines: string[] = [];
      for (const f of struct?.fields ?? []) {
        const v = emitDataInput(ctx, node.id, `field:${f.name}`);
        lines.push(`${f.name}: ${v}`);
      }
      if (lines.length === 0) return `${structName} {}`;
      return `${structName} { ${lines.join(', ')} }`;
    }
    case 'fieldAccess': {
      const target = emitDataInput(ctx, node.id, 'target');
      return `${target}.${data.fieldName}`;
    }
    case 'indexRead': {
      const arr = emitDataInput(ctx, node.id, 'array');
      const idx = emitDataInput(ctx, node.id, 'index');
      return `${arr}[${idx}]`;
    }
    case 'enumVariant': {
      return `${data.enumName}::${data.variantName}`;
    }
    case 'call': {
      if (outPort !== 'return') {
        ctx.warnings.push(`call node used in expression position via non-return port`);
      }
      const fn = ctx.workflow.functions.find((f) => f.id === data.functionId);
      const fname = fn?.name ?? '/* unknown */';
      const args = (fn?.params ?? [])
        .map((p) => emitDataInput(ctx, node.id, `arg:${p.name}`))
        .join(', ');
      return `${fname}(${args})`;
    }
    case 'forEach': {
      if (outPort === 'item') return data.iteratorName;
      return '/* invalid */';
    }
    case 'let': {
      if (outPort.startsWith('var:')) return data.varName;
      return '/* invalid */';
    }
    default:
      return '/* missing */';
  }
}

// =============================================================
//  Literal formatting
// =============================================================

function formatLiteral(litType: string, value: string): string {
  const v = value ?? '';
  switch (litType) {
    case 'int':
      return v.trim() === '' ? '0' : v;
    case 'float': {
      if (v.trim() === '') return '0.0';
      return v.includes('.') ? v : `${v}.0`;
    }
    case 'bool':
      return v === 'true' ? 'true' : 'false';
    case 'str': {
      const escaped = v.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
      return `"${escaped}"`;
    }
    case 'char': {
      const c = v.length > 0 ? v[0] : ' ';
      const escaped = c === "'" ? "\\'" : c === '\\' ? '\\\\' : c;
      return `'${escaped}'`;
    }
    default:
      return v;
  }
}
