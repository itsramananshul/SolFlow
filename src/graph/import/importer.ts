/**
 * Canonical AST → graph importer.
 *
 * Walks a parsed `Program` (the canonical `openprem-sol-v2` AST) and
 * produces a `SolWorkflow` plus an `ImportReport` describing every
 * decision the importer made.
 *
 *   parseSource (compiler-wasm)
 *      → Program (typed AST in src/compiler/ast.ts)
 *      → importProgram() in this file
 *      → { workflow: SolWorkflow, report: ImportReport }
 *      → graph.store.loadWorkflow(workflow)
 *
 * Mapping to the graph schema:
 *
 *   - A canonical `workflow "name" { body }` and a helper
 *     `fn name(params) <- ret { body }` both become a
 *     `FunctionGraph`. The workflow is tagged `isWorkflow: true` so
 *     the emitter can round-trip the distinction.
 *   - Statements the canonical parser produces — `let`, `if`,
 *     `while`, `for`, `return`, and expression statements (`print`,
 *     local calls) — map to real graph nodes.
 *   - Capability calls / Actions (`call(...)`, `module.fn(...)`,
 *     `ns::fn(...)`) and `emit "x";` have no dedicated graph node
 *     yet. They are preserved as inline text on a placeholder node
 *     and classified `partial` — never silently dropped.
 *
 * Expression complexity: any non-trivial expression in a value /
 * condition slot is stringified to canonical SOL text via
 * `stringifyExpr` and embedded as the node's inline expression for
 * that port. The SOL stays canonical and parseable; the user just
 * doesn't get a sub-graph view of it.
 *
 * NOTE (2026-06): the canonical parser does not currently parse
 * assignment statements, so `assign` / `fieldSet` / `indexSet` nodes
 * are not produced from real source. The `Stmt::Assign` handlers
 * below are kept for forward-compatibility but are unreachable from
 * today's parser output.
 */

import { nanoid } from 'nanoid';

import type {
  Block,
  EnumDecl as AstEnumDecl,
  Expr,
  FunctionDecl,
  ImportDecl as AstImportDecl,
  Program,
  Stmt,
  StructDecl as AstStructDecl,
  Target,
  WorkflowDecl,
} from '@/compiler/ast';
import { createNode, rebuildPorts, type WorkflowCtx } from '@/graph/factory';
import type {
  EnumDecl,
  FunctionGraph,
  GraphEdge,
  GraphNode,
  ImportDecl,
  NodeData,
  NodeKind,
  Param,
  SolWorkflow,
  StructDecl,
  StructField,
} from '@/graph/schema';
import { autoLayout } from '@/sol-man/autoLayout';
import type {
  GeneratedGraphSpec,
  GeneratedNode,
  GeneratedNodeKind,
} from '@/sol-man/types';

import { stringifyExpr } from './expressions';
import { emptyReport, type ImportReport, type ImportSupport } from './report';
import { compilerTypeToGraphType } from './types';

// =============================================================
//  Public API
// =============================================================

export interface ImportResult {
  workflow: SolWorkflow;
  report: ImportReport;
}

/** Normalized view of a top-level callable (fn or workflow). */
interface CallableDecl {
  name: string;
  params: Param[];
  returnType: ReturnType<typeof compilerTypeToGraphType>;
  body: Block;
  isWorkflow: boolean;
}

/**
 * Walk a parsed Program and produce a fresh SolWorkflow plus an
 * import report. Caller decides whether to commit via
 * `graph.store.loadWorkflow(result.workflow)`.
 *
 * @param program  parsed canonical AST
 * @param meta     workflow-level metadata (name / description)
 * @param source   optional raw SOL source; when provided, callable
 *                 nodes get `meta.sourceLine` populated by scanning
 *                 the source for `fn <name>` / `workflow "<name>"`.
 */
export function importProgram(
  program: Program,
  meta: { name: string; description?: string } = { name: 'Imported workflow' },
  source?: string,
): ImportResult {
  const report = emptyReport();
  const workflow: SolWorkflow = {
    schemaVersion: 1,
    meta: {
      name: meta.name,
      description: meta.description ?? 'Imported from SOL source',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    imports: [],
    structs: [],
    enums: [],
    functions: [],
  };

  const items = program?.items ?? [];

  // ---- Pass 1: imports + type declarations (direct graph reps) ----
  for (const item of items) {
    if ('Struct' in item) {
      workflow.structs.push(importStruct(item.Struct));
      report.topLevel.structs++;
    } else if ('Enum' in item) {
      workflow.enums.push(importEnum(item.Enum));
      report.topLevel.enums++;
    } else if ('Import' in item) {
      workflow.imports.push(importImport(item.Import));
      report.topLevel.imports++;
    }
  }

  // ---- Collect callables (functions + workflows) ----
  const callables = collectCallables(items);

  // ---- Pre-allocate FunctionGraph stubs so calls resolve cross-decl ----
  const ctxStubs: WorkflowCtx = {
    structs: [],
    enums: [],
    functions: callables.map((c) => ({
      id: nanoid(8),
      name: c.name,
      params: c.params,
      returnType: c.returnType,
      isWorkflow: c.isWorkflow,
      nodes: [],
      edges: [],
    })),
  };

  // ---- Source-line lookup (optional) ----
  const sourceLines = source ? scanDeclLines(source) : new Map<string, number>();

  // ---- Pass 2: callable bodies → FunctionGraph ----
  for (let i = 0; i < callables.length; i++) {
    const decl = callables[i]!;
    const stub = ctxStubs.functions[i]!;
    const fn = importCallable(decl, stub, ctxStubs, report);
    const sourceLine = sourceLines.get(decl.name);
    if (sourceLine !== undefined) {
      fn.meta = { ...(fn.meta ?? {}), sourceLine };
      const summary = report.functions.find((f) => f.name === fn.name);
      if (summary) summary.sourceLine = sourceLine;
    }
    workflow.functions.push(fn);
  }

  // ---- Post-pass: rebuild every node's ports against the full ctx ----
  const fullCtx: WorkflowCtx = {
    structs: workflow.structs,
    enums: workflow.enums,
    functions: workflow.functions,
  };
  for (const fn of workflow.functions) {
    for (const n of fn.nodes) {
      n.ports = rebuildPorts(n.data, fullCtx);
    }
  }

  return { workflow, report };
}

// =============================================================
//  Top-level translators
// =============================================================

function importStruct(decl: AstStructDecl): StructDecl {
  // Canonical struct fields are an ordered Vec — preserve that order.
  const fields: StructField[] = decl.fields.map((f) => ({
    name: f.name,
    type: compilerTypeToGraphType(f.type_),
  }));
  return { id: nanoid(8), name: decl.name, fields };
}

function importEnum(decl: AstEnumDecl): EnumDecl {
  // Canonical enums carry variant names only (no explicit values).
  const variants = decl.variants.map((name) => ({ name, value: null }));
  return { id: nanoid(8), name: decl.name, variants };
}

function importImport(decl: AstImportDecl): ImportDecl {
  if ('Module' in decl.spec) {
    const m = decl.spec.Module;
    return { id: nanoid(8), path: [m], alias: m };
  }
  // Named: `import "name" from module;`
  const { name, module } = decl.spec.Named;
  return { id: nanoid(8), path: [module], alias: name, from: module };
}

/** Normalize functions + workflows into a single callable list. */
function collectCallables(items: Program['items']): CallableDecl[] {
  const out: CallableDecl[] = [];
  for (const item of items) {
    if ('Function' in item) {
      out.push(fromFunction(item.Function));
    } else if ('Workflow' in item) {
      out.push(fromWorkflow(item.Workflow));
    }
  }
  return out;
}

function fromFunction(decl: FunctionDecl): CallableDecl {
  return {
    name: decl.name,
    params: decl.params.map((p) => ({
      name: p.name,
      type: compilerTypeToGraphType(p.type_),
    })),
    returnType: decl.return_type
      ? compilerTypeToGraphType(decl.return_type)
      : { kind: 'void' },
    body: decl.body,
    isWorkflow: false,
  };
}

function fromWorkflow(decl: WorkflowDecl): CallableDecl {
  return {
    name: decl.name,
    params: [],
    returnType: { kind: 'void' },
    body: decl.body,
    isWorkflow: true,
  };
}

// =============================================================
//  Per-callable importer
// =============================================================

interface FuncImportState {
  nodes: GraphNode[];
  edges: GraphEdge[];
  spec: GeneratedGraphSpec;
  stmtCount: number;
  unsupportedCount: number;
  worst: ImportSupport;
  ctx: WorkflowCtx;
  funcName: string;
  report: ImportReport;
  /** Local variable → struct/enum name, for fieldSet inference. */
  varTypes: Map<string, string>;
}

/** Scan a block recursively for `let v: Name` declarations. */
function scanVarTypes(block: Block): Map<string, string> {
  const result = new Map<string, string>();
  function walkBlock(b: Block) {
    for (const stmt of b.stmts) walkStmt(stmt);
  }
  function walkStmt(stmt: Stmt) {
    if ('Let' in stmt) {
      const t = stmt.Let.type_;
      if (typeof t !== 'string' && 'Named' in t) {
        result.set(stmt.Let.name, t.Named);
      }
    } else if ('If' in stmt) {
      walkBlock(stmt.If.then);
      if (stmt.If.else_) walkBlock(stmt.If.else_);
    } else if ('While' in stmt) {
      walkBlock(stmt.While.body);
    } else if ('For' in stmt) {
      walkBlock(stmt.For.body);
    }
  }
  walkBlock(block);
  return result;
}

function importCallable(
  decl: CallableDecl,
  stub: FunctionGraph,
  ctx: WorkflowCtx,
  report: ImportReport,
): FunctionGraph {
  const varTypes = scanVarTypes(decl.body);
  for (const p of decl.params) {
    if (p.type.kind === 'named') varTypes.set(p.name, p.type.name);
  }

  const state: FuncImportState = {
    nodes: [],
    edges: [],
    spec: { meta: { name: decl.name, description: '' }, nodes: [], edges: [] },
    stmtCount: 0,
    unsupportedCount: 0,
    worst: 'full',
    ctx,
    funcName: decl.name,
    report,
    varTypes,
  };

  // Every callable starts with a `start` node.
  const startReal = createNode('start', { x: 0, y: 0 }, ctx, { kind: 'start' });
  const startSpec: GeneratedNode = { id: stableId(), kind: 'trigger' };
  state.nodes.push(startReal);
  state.spec.nodes.push(startSpec);

  // Walk the body, chaining statements off the start node.
  const seq = importStmtSequence(decl.body.stmts, state);
  if (seq) {
    state.spec.edges.push({
      from: startSpec.id,
      to: seq.entrySpecId,
      fromPort: 'next',
      toPort: 'prev',
      kind: 'control',
    });
    state.edges.push({
      id: nanoid(8),
      source: { node: startReal.id, port: 'next' },
      target: { node: seq.entryRealId, port: 'prev' },
      kind: 'control',
    });
  }

  // Layout the spec, copy positions onto the real nodes.
  const layout = autoLayout(state.spec);
  for (let i = 0; i < state.nodes.length; i++) {
    const specId = state.spec.nodes[i]!.id;
    state.nodes[i]!.position = layout.get(specId) ?? { x: 0, y: 0 };
  }

  report.functions.push({
    name: decl.name,
    support: state.worst,
    statementCount: state.stmtCount,
    unsupportedCount: state.unsupportedCount,
  });

  return {
    ...stub,
    isWorkflow: decl.isWorkflow,
    nodes: state.nodes,
    edges: state.edges,
  };
}

// =============================================================
//  Statement sequence + dispatch
// =============================================================

interface StmtImportResult {
  entrySpecId: string;
  entryRealId: string;
  exitSpecId: string;
  exitRealId: string;
  /** Port on the exit node the next statement should wire to. */
  exitPort: 'next' | 'after';
}

/** Import a sequence of statements, wiring them into a control chain. */
function importStmtSequence(
  stmts: Stmt[],
  state: FuncImportState,
): StmtImportResult | null {
  let first: StmtImportResult | null = null;
  let prev: StmtImportResult | null = null;
  for (const stmt of stmts) {
    const r = importStatement(stmt, state);
    if (!r) continue;
    state.stmtCount++;
    if (!first) first = r;
    if (prev) {
      state.spec.edges.push({
        from: prev.exitSpecId,
        to: r.entrySpecId,
        fromPort: prev.exitPort,
        toPort: 'prev',
        kind: 'control',
      });
      state.edges.push({
        id: nanoid(8),
        source: { node: prev.exitRealId, port: prev.exitPort },
        target: { node: r.entryRealId, port: 'prev' },
        kind: 'control',
      });
    }
    prev = r;
  }
  if (!first || !prev) return null;
  return {
    entrySpecId: first.entrySpecId,
    entryRealId: first.entryRealId,
    exitSpecId: prev.exitSpecId,
    exitRealId: prev.exitRealId,
    exitPort: prev.exitPort,
  };
}

function importStatement(stmt: Stmt, state: FuncImportState): StmtImportResult | null {
  // ---- let ----
  if ('Let' in stmt) {
    const v = stmt.Let;
    const data: NodeData = {
      kind: 'let',
      varName: v.name,
      varType: compilerTypeToGraphType(v.type_),
    };
    const realNode = createNode('let', { x: 0, y: 0 }, state.ctx, data);
    realNode.expressions = { value: stringifyExpr(v.value) };
    return pushSimpleStatement(state, realNode, 'let', 'partial');
  }

  // ---- assign (unreachable from today's parser; kept forward-compatible) ----
  if ('Assign' in stmt) {
    return importAssign(stmt.Assign.target, stmt.Assign.value, state);
  }

  // ---- return ----
  if ('Return' in stmt) {
    const value = stmt.Return;
    const hasValue = value !== null;
    const data: NodeData = { kind: 'return', hasValue };
    const realNode = createNode('return', { x: 0, y: 0 }, state.ctx, data);
    if (hasValue) realNode.expressions = { value: stringifyExpr(value) };
    return pushSimpleStatement(state, realNode, 'return', 'partial');
  }

  // ---- if / else ----
  if ('If' in stmt) {
    return importBranch(stmt.If, state);
  }

  // ---- while ----
  if ('While' in stmt) {
    return importLoop(stmt.While.condition, stmt.While.body, state);
  }

  // ---- for ----
  if ('For' in stmt) {
    return importForEach(stmt.For.item, stmt.For.iter, stmt.For.body, state);
  }

  // ---- emit ----
  if ('Emit' in stmt) {
    // No dedicated graph node yet — preserve honestly as a placeholder.
    return makeUnsupportedPlaceholder(
      state,
      `emit "${stmt.Emit}"`,
      'partial',
      `"/* emit: ${stmt.Emit.replace(/\*\//g, '* /')} */"`,
    );
  }

  // ---- expression statement ----
  if ('Expr' in stmt) {
    return importExprStatement(stmt.Expr, state);
  }

  return makeUnsupportedPlaceholder(state, 'unrecognized statement', 'unsupported');
}

/** Expression statements: print(...), local calls, and Actions. */
function importExprStatement(expr: Expr, state: FuncImportState): StmtImportResult | null {
  // print(...) — `Call(Ident("print"), args)`
  if ('Call' in expr) {
    const [callee, args] = expr.Call;
    if (typeof callee !== 'string' && 'Ident' in callee) {
      const name = callee.Ident;
      if (name === 'print') {
        const valueExpr =
          args.length === 0
            ? '""'
            : args.length === 1
              ? stringifyExpr(args[0]!)
              : `[${args.map(stringifyExpr).join(', ')}]`;
        const data: NodeData = { kind: 'print' };
        const realNode = createNode('print', { x: 0, y: 0 }, state.ctx, data);
        realNode.expressions = { value: valueExpr };
        return pushSimpleStatement(state, realNode, 'print', 'partial');
      }
      // Local function call — resolves to a `call` node if known.
      const fn = state.ctx.functions.find((f) => f.name === name && !f.isWorkflow);
      if (fn) {
        const data: NodeData = { kind: 'call', functionId: fn.id };
        const realNode = createNode('call', { x: 0, y: 0 }, state.ctx, data);
        const argPorts = realNode.ports.in.filter((p) => p.kind === 'data');
        const exprs: Record<string, string> = {};
        for (let i = 0; i < args.length; i++) {
          const port = argPorts[i];
          if (port) exprs[port.id] = stringifyExpr(args[i]!);
        }
        realNode.expressions = exprs;
        return pushSimpleStatement(state, realNode, 'call', 'partial');
      }
    }
  }

  // Capability calls / Actions (call(...), module.fn(...), ns::fn(...))
  // and unknown calls: preserve the call text honestly.
  const isAction =
    'WorkflowCall' in expr || 'NamespaceCall' in expr || 'Call' in expr;
  return makeUnsupportedPlaceholder(
    state,
    isAction ? 'capability call (Action)' : 'expression statement',
    'partial',
    stringifyExpr(expr),
  );
}

// =============================================================
//  Assignment (forward-compatible; not produced by today's parser)
// =============================================================

function importAssign(
  target: Target,
  value: Expr,
  state: FuncImportState,
): StmtImportResult {
  // x = expr
  if ('Ident' in target) {
    const data: NodeData = { kind: 'assign', varName: target.Ident };
    const realNode = createNode('assign', { x: 0, y: 0 }, state.ctx, data);
    realNode.expressions = { value: stringifyExpr(value) };
    return pushSimpleStatement(state, realNode, 'assign', 'partial');
  }
  // obj.field = expr
  if ('MemberAccess' in target) {
    const [inner, fieldName] = target.MemberAccess;
    if ('Ident' in inner) {
      const targetVar = inner.Ident;
      const structName = state.varTypes.get(targetVar) ?? '';
      const data: NodeData = { kind: 'fieldSet', structName, fieldName };
      const realNode = createNode('fieldSet', { x: 0, y: 0 }, state.ctx, data);
      realNode.expressions = { target: targetVar, value: stringifyExpr(value) };
      return pushSimpleStatement(state, realNode, 'assign', 'partial');
    }
  }
  // arr[idx] = expr
  if ('Index' in target) {
    const [arr, idx] = target.Index;
    const data: NodeData = { kind: 'indexSet', elementType: { kind: 'any' } };
    const realNode = createNode('indexSet', { x: 0, y: 0 }, state.ctx, data);
    realNode.expressions = {
      array: stringifyTarget(arr),
      index: stringifyExpr(idx),
      value: stringifyExpr(value),
    };
    return pushSimpleStatement(state, realNode, 'assign', 'partial');
  }
  return makeUnsupportedPlaceholder(state, 'complex assignment target', 'partial');
}

function stringifyTarget(t: Target): string {
  if ('Ident' in t) return t.Ident;
  if ('MemberAccess' in t) return `${stringifyTarget(t.MemberAccess[0])}.${t.MemberAccess[1]}`;
  return `${stringifyTarget(t.Index[0])}[${stringifyExpr(t.Index[1])}]`;
}

// =============================================================
//  Control-flow importers
// =============================================================

function importBranch(
  decl: { condition: Expr; then: Block; else_: Block | null },
  state: FuncImportState,
): StmtImportResult {
  const hasElse = decl.else_ !== null;
  const branchReal = createNode('branch', { x: 0, y: 0 }, state.ctx, {
    kind: 'branch',
    hasElse,
  });
  branchReal.expressions = { cond: stringifyExpr(decl.condition) };
  const branchSpec: GeneratedNode = { id: stableId(), kind: 'branch', hasElse };
  state.nodes.push(branchReal);
  state.spec.nodes.push(branchSpec);

  // Then arm.
  const thenResult = importStmtSequence(decl.then.stmts, state);
  if (thenResult) {
    wireArm(state, branchSpec.id, branchReal.id, 'then', thenResult);
  }

  // Else arm.
  if (decl.else_) {
    const elseResult = importStmtSequence(decl.else_.stmts, state);
    if (elseResult) {
      wireArm(state, branchSpec.id, branchReal.id, 'else', elseResult);
    }
  }

  bumpSupport(state, 'partial');
  return {
    entrySpecId: branchSpec.id,
    entryRealId: branchReal.id,
    exitSpecId: branchSpec.id,
    exitRealId: branchReal.id,
    exitPort: 'after',
  };
}

function importLoop(
  condition: Expr,
  body: Block,
  state: FuncImportState,
): StmtImportResult {
  const real = createNode('while', { x: 0, y: 0 }, state.ctx, { kind: 'while' });
  real.expressions = { cond: stringifyExpr(condition) };
  const spec: GeneratedNode = { id: stableId(), kind: 'while' };
  state.nodes.push(real);
  state.spec.nodes.push(spec);

  const bodyResult = importStmtSequence(body.stmts, state);
  if (bodyResult) {
    wireArm(state, spec.id, real.id, 'body', bodyResult);
  }

  bumpSupport(state, 'partial');
  return {
    entrySpecId: spec.id,
    entryRealId: real.id,
    exitSpecId: spec.id,
    exitRealId: real.id,
    exitPort: 'after',
  };
}

function importForEach(
  iteratorName: string,
  array: Expr,
  body: Block,
  state: FuncImportState,
): StmtImportResult {
  const data: NodeData = {
    kind: 'forEach',
    iteratorName,
    // The canonical AST ships no inferred element type; default to any.
    iteratorType: { kind: 'any' },
  };
  const real = createNode('forEach', { x: 0, y: 0 }, state.ctx, data);
  real.expressions = { array: stringifyExpr(array) };
  const spec: GeneratedNode = { id: stableId(), kind: 'forEach', iteratorName };
  state.nodes.push(real);
  state.spec.nodes.push(spec);

  const bodyResult = importStmtSequence(body.stmts, state);
  if (bodyResult) {
    wireArm(state, spec.id, real.id, 'body', bodyResult);
  }

  bumpSupport(state, 'partial');
  return {
    entrySpecId: spec.id,
    entryRealId: real.id,
    exitSpecId: spec.id,
    exitRealId: real.id,
    exitPort: 'after',
  };
}

/** Wire a branch/loop control-out port to the entry of an arm. */
function wireArm(
  state: FuncImportState,
  specNodeId: string,
  realNodeId: string,
  port: 'then' | 'else' | 'body',
  arm: StmtImportResult,
): void {
  state.spec.edges.push({
    from: specNodeId,
    to: arm.entrySpecId,
    fromPort: port,
    toPort: 'prev',
    kind: 'control',
  });
  state.edges.push({
    id: nanoid(8),
    source: { node: realNodeId, port },
    target: { node: arm.entryRealId, port: 'prev' },
    kind: 'control',
  });
}

// =============================================================
//  Helpers
// =============================================================

function pushSimpleStatement(
  state: FuncImportState,
  node: GraphNode,
  specKind: GeneratedNodeKind,
  support: ImportSupport,
): StmtImportResult {
  const spec: GeneratedNode = { id: stableId(), kind: specKind };
  state.nodes.push(node);
  state.spec.nodes.push(spec);
  bumpSupport(state, support);
  state.report.counts[supportBucket(support)]++;
  return {
    entrySpecId: spec.id,
    entryRealId: node.id,
    exitSpecId: spec.id,
    exitRealId: node.id,
    exitPort: 'next',
  };
}

function makeUnsupportedPlaceholder(
  state: FuncImportState,
  what: string,
  support: ImportSupport = 'unsupported',
  inlineSolText?: string,
): StmtImportResult {
  const data: NodeData = { kind: 'print' };
  const real = createNode('print', { x: 0, y: 0 }, state.ctx, data);
  real.expressions = { value: inlineSolText ?? `"/* unsupported: ${what} */"` };
  const spec: GeneratedNode = { id: stableId(), kind: 'print' };
  state.nodes.push(real);
  state.spec.nodes.push(spec);
  state.unsupportedCount++;
  bumpSupport(state, support);
  state.report.counts[supportBucket(support)]++;
  state.report.notices.push({
    severity: 'warning',
    message: `In "${state.funcName}": ${what} preserved as inline text on a placeholder node.`,
    functionName: state.funcName,
    support,
  });
  return {
    entrySpecId: spec.id,
    entryRealId: real.id,
    exitSpecId: spec.id,
    exitRealId: real.id,
    exitPort: 'next',
  };
}

function bumpSupport(state: FuncImportState, candidate: ImportSupport): void {
  const order: ImportSupport[] = ['full', 'partial', 'source-only', 'unsupported'];
  if (order.indexOf(candidate) > order.indexOf(state.worst)) {
    state.worst = candidate;
  }
}

function supportBucket(s: ImportSupport): keyof ImportReport['counts'] {
  if (s === 'source-only') return 'sourceOnly';
  return s;
}

let _idCounter = 0;
function stableId(): string {
  return `n${++_idCounter}`;
}

// Keep type-only imports referenced.
type _NodeKindCheck = NodeKind;

/**
 * Scan source for `fn <name>(` and `workflow "<name>"` declarations
 * and return a `Map<name, lineNumber>` (1-indexed). Used to attach
 * `FunctionGraph.meta.sourceLine` so the import report panel can
 * scroll the source pane to a declaration on click.
 */
function scanDeclLines(source: string): Map<string, number> {
  const result = new Map<string, number>();
  const fnPattern = /^\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/;
  const workflowPattern = /^\s*workflow\s+"([^"]*)"/;
  const lines = source.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const fnMatch = line.match(fnPattern);
    if (fnMatch && fnMatch[1] && !result.has(fnMatch[1])) {
      result.set(fnMatch[1], i + 1);
      continue;
    }
    const wfMatch = line.match(workflowPattern);
    if (wfMatch && wfMatch[1] !== undefined && !result.has(wfMatch[1])) {
      result.set(wfMatch[1], i + 1);
    }
  }
  return result;
}
