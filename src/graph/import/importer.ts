/**
 * AST → graph importer.
 *
 * Walks a parsed `Program` and produces a `SolWorkflow` along with
 * an `ImportReport` describing every decision the importer made.
 *
 * Architecture:
 *
 *   parseSource (compiler-wasm)
 *      → Program (typed AST in src/compiler/ast.ts)
 *      → importProgram() in this file
 *      → { workflow: SolWorkflow, report: ImportReport }
 *      → graph.store.loadWorkflow(workflow)
 *
 * Per-function strategy:
 *
 *   For each `DeclFunc` we build a `GeneratedGraphSpec`-shaped
 *   intermediate (just structural — node id + kind + edges). The
 *   intermediate exists for two reasons:
 *     1. `autoLayout()` already understands it, so we get layout
 *        for free.
 *     2. It mirrors the shape sol-man already uses, so the surface
 *        feels consistent.
 *
 *   We then create real `GraphNode` objects via `createNode()`
 *   alongside the intermediate, populate `expressions` with
 *   inline SOL text for any subexpressions, and copy the
 *   computed positions onto the real nodes.
 *
 * Unsupported constructs:
 *
 *   - **Statement-level unsupported** (e.g. an `ExprFuncCall` inside
 *     a body that's NOT `print`) → emitted as a `print` placeholder
 *     with the original call as a string, plus an `ImportNotice`
 *     with `support: 'partial'`. We never silently drop a statement.
 *   - **AST shapes the importer doesn't understand at all** (this
 *     should be rare; it represents an importer gap, not a SOL
 *     limitation) → emitted as `print("/* unsupported: <kind> *\/")`
 *     with `support: 'unsupported'`.
 *   - **Expression complexity** (any non-trivial expression in a
 *     value/condition slot) → stringified to SOL text via
 *     `stringifyExpr` and embedded as the node's inline expression
 *     for that port. This is the *full* solution for expressions —
 *     the SOL stays canonical and parseable, the user just doesn't
 *     get a sub-graph view of it.
 */

import { nanoid } from 'nanoid';

import type {
  Ast,
  DeclEnum,
  DeclFunc,
  DeclStruct,
  Program,
  StmtImport,
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
  GeneratedEdge,
  GeneratedGraphSpec,
  GeneratedNode,
  GeneratedNodeKind,
} from '@/sol-man/types';

import { stringifyExpr } from './expressions';
import { emptyReport, type ImportNotice, type ImportReport, type ImportSupport } from './report';
import { compilerTypeToGraphType, isLossyConversion } from './types';

// =============================================================
//  Public API
// =============================================================

export interface ImportResult {
  workflow: SolWorkflow;
  report: ImportReport;
}

/**
 * Walk a parsed Program and produce a fresh SolWorkflow plus an
 * import report. Caller decides whether to commit via
 * `graph.store.loadWorkflow(result.workflow)`.
 *
 * @param program  parsed AST
 * @param meta     workflow-level metadata (name / description)
 * @param source   optional raw SOL source; when provided, function
 *                 nodes get `meta.sourceLine` populated by scanning
 *                 the source for `function <name>` (B.6 c25).
 *                 Used by the import report's "show source" UX.
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

  /** Collected top-level let declarations — auto-wrapped into a
   *  synthetic `__init()` function after pass 2 (B.D c37). */
  const pendingTopLevelLets: Ast[] = [];

  // ---- Pass 1: top-level declarations that have direct graph reps ----
  for (const node of program) {
    if (typeof node === 'string') {
      report.notices.push({
        severity: 'warning',
        message: `Top-level unit AST node "${node}" — ignored.`,
        support: 'unsupported',
      });
      continue;
    }
    if ('DeclStruct' in node) {
      workflow.structs.push(importStruct(node.DeclStruct, report));
      report.topLevel.structs++;
      continue;
    }
    if ('DeclEnum' in node) {
      workflow.enums.push(importEnum(node.DeclEnum));
      report.topLevel.enums++;
      continue;
    }
    if ('StmtImport' in node) {
      const imp = importStmtImport(node.StmtImport);
      if (imp) {
        workflow.imports.push(imp);
        report.topLevel.imports++;
      }
      continue;
    }
    if ('DeclExtFunc' in node) {
      report.topLevel.extFunctions++;
      report.notices.push({
        severity: 'info',
        message: `External function declaration "${node.DeclExtFunc.name}" preserved in source only — graph has no representation yet.`,
        functionName: node.DeclExtFunc.name,
        support: 'source-only',
      });
      report.counts.sourceOnly++;
      continue;
    }
    if ('DeclFunc' in node) continue; // handled in pass 2
    if ('DeclVar' in node) {
      // B.D c37: top-level lets are collected here, then auto-
      // wrapped into a synthetic `__init()` function after pass 2.
      // SolFlow's graph schema doesn't model module-scoped lets,
      // and the emit pipeline can't produce them either — so the
      // round-trip choice is: lose them, or hoist them. We hoist.
      // The semantic change is documented: hoisted lets are
      // function-scoped to `__init()` and not visible to other
      // functions. (The original was already broken — anything
      // referencing the module-scoped let from another function
      // would have failed analyzer's `SEMA_UNDEFINED_NAME`.)
      pendingTopLevelLets.push(node);
      continue;
    }
    report.notices.push({
      severity: 'warning',
      message: `Unrecognized top-level AST node "${Object.keys(node)[0] ?? 'unknown'}" — ignored.`,
      support: 'unsupported',
    });
    report.counts.unsupported++;
  }

  // ---- Pass 2: functions. Need pass 1 to be done so call-nodes can resolve. ----
  const ctxStubs = buildCtxStubs(program); // pre-allocated function id stubs
  // Source-line lookup map for B.6 c25 click-to-source: only built
  // when raw source is supplied. Scans for `function <name>` at
  // line-start; tolerant of leading whitespace + comments.
  const sourceLines = source ? scanFunctionLines(source) : new Map<string, number>();
  for (const node of program) {
    if (typeof node === 'string' || !('DeclFunc' in node)) continue;
    const stub = ctxStubs.functions.find((f) => f.name === node.DeclFunc.name)!;
    const fn = importFunction(node.DeclFunc, stub, ctxStubs, report);

    // B.D c37: prefer the AST's own span (B.D c35) over the
    // textual function-line scan when present. AST spans come
    // from the parser and are byte-exact; the textual scan is
    // a regex heuristic. Fall back to scan for old fixtures or
    // pre-span builds.
    let sourceLine: number | undefined;
    if (node.DeclFunc.span && source) {
      sourceLine = lineNumberAt(source, node.DeclFunc.span.start);
    }
    if (sourceLine === undefined) {
      sourceLine = sourceLines.get(fn.name);
    }
    if (sourceLine !== undefined) {
      fn.meta = { ...(fn.meta ?? {}), sourceLine };
      const summary = report.functions.find((f) => f.name === fn.name);
      if (summary) summary.sourceLine = sourceLine;
    }
    workflow.functions.push(fn);
  }

  // ---- B.D c37: wrap collected top-level lets into __init() ----
  if (pendingTopLevelLets.length > 0) {
    const initDecl: DeclFunc = {
      name: '__init',
      params: [],
      ret: 'Void',
      // Synthetic body containing the collected let declarations
      // wrapped in a Block. No span — we created it after parse.
      body: {
        Block: {
          block: pendingTopLevelLets,
          scope: Number.MAX_SAFE_INTEGER,
        },
      },
      scope: Number.MAX_SAFE_INTEGER,
    };
    // Pre-allocate the function stub like pass 2 does, then run
    // importFunction so the body's let nodes get materialized
    // through the normal path.
    const initStub: FunctionGraph = {
      id: nanoid(8),
      name: '__init',
      params: [],
      returnType: { kind: 'void' },
      nodes: [],
      edges: [],
    };
    ctxStubs.functions.push(initStub);
    const initFn = importFunction(initDecl, initStub, ctxStubs, report);
    workflow.functions.push(initFn);
    report.notices.push({
      severity: 'info',
      message: `Hoisted ${pendingTopLevelLets.length} top-level \`let\` declaration(s) into a synthetic \`__init()\` function. SolFlow's graph schema doesn't model module-scoped lets; this preserves the bindings for round-trip but changes their scope.`,
      functionName: '__init',
      support: 'partial',
    });
    report.counts.partial += pendingTopLevelLets.length;
  }

  // ---- Post-pass: rebuild every node's ports against the full workflow ctx ----
  // call-nodes need to see the real FunctionGraph ids to populate
  // their argument ports.
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

function importStruct(decl: DeclStruct, _report: ImportReport): StructDecl {
  // HashMap order is non-deterministic — sort by field name so the
  // resulting graph is stable across imports.
  const fieldNames = Object.keys(decl.fields).sort();
  const fields: StructField[] = fieldNames.map((name) => ({
    name,
    type: compilerTypeToGraphType(decl.fields[name]!),
  }));
  return { id: nanoid(8), name: decl.name, fields };
}

function importEnum(decl: DeclEnum): EnumDecl {
  // Sort by parser-assigned ordinal so iota-style enums keep their
  // order. (HashMap order isn't stable, but the iota values are.)
  const variants = Object.entries(decl.variants)
    .sort((a, b) => a[1] - b[1])
    .map(([name, value]) => ({ name, value }));
  return { id: nanoid(8), name: decl.name, variants };
}

function importStmtImport(stmt: StmtImport): ImportDecl | null {
  if (stmt.path.length === 0) return null;
  return {
    id: nanoid(8),
    path: stmt.path,
    alias: stmt.alias ?? stmt.path[stmt.path.length - 1]!,
  };
}

/** Reserve a FunctionGraph id for every DeclFunc up-front so per-function
 *  imports can resolve cross-function calls without two passes inside
 *  body translation. */
function buildCtxStubs(program: Program): WorkflowCtx {
  const functions: FunctionGraph[] = [];
  for (const node of program) {
    if (typeof node === 'string' || !('DeclFunc' in node)) continue;
    functions.push({
      id: nanoid(8),
      name: node.DeclFunc.name,
      params: node.DeclFunc.params.map(([n, t]) => ({
        name: n,
        type: compilerTypeToGraphType(t),
      })),
      returnType: compilerTypeToGraphType(node.DeclFunc.ret),
      nodes: [],
      edges: [],
    });
  }
  // Struct/enum stubs not needed during body import (they're already
  // committed in pass 1).
  return { structs: [], enums: [], functions };
}

// =============================================================
//  Per-function importer
// =============================================================

interface FuncImportState {
  /** Real GraphNodes assembled so far. */
  nodes: GraphNode[];
  /** Real edges. */
  edges: GraphEdge[];
  /** Parallel `GeneratedGraphSpec` for layout. */
  spec: GeneratedGraphSpec;
  /** Counts for the per-function summary. */
  stmtCount: number;
  unsupportedCount: number;
  /** Worst support classification encountered so far. */
  worst: ImportSupport;
  /** Function-scoped ctx for createNode + rebuildPorts. */
  ctx: WorkflowCtx;
  /** Function name for notice attribution. */
  funcName: string;
  /** Mutable report. */
  report: ImportReport;
  /**
   * Local-variable type map built up-front by scanning the
   * function body for `let varName: TypeName` declarations.
   * Used by fieldSet/indexSet importers (B.D c37) to infer the
   * struct name from the assignment's LHS variable.
   *
   * Limitation: no shadowing support (uses last-wins); no scope
   * tracking. Sufficient for the editor's typical workflows where
   * names don't shadow within a function.
   */
  varTypes: Map<string, string>;
}

/**
 * Scan a function body's AST recursively for `let varName: TypeIdent`
 * declarations and return a `Map<varName, TypeIdent>` for struct
 * name inference in the fieldSet/indexSet importers (B.D c37).
 *
 * Only collects `{ Ident: string }` types (struct/enum references);
 * primitives like `Integer` aren't useful for field-set inference.
 */
function scanVarTypes(body: Ast): Map<string, string> {
  const result = new Map<string, string>();
  function walk(node: Ast) {
    if (typeof node === 'string') return;
    if ('DeclVar' in node) {
      const t = node.DeclVar.kind;
      if (typeof t !== 'string' && 'Ident' in t) {
        result.set(node.DeclVar.name, t.Ident);
      }
      if (node.DeclVar.value) walk(node.DeclVar.value);
      return;
    }
    if ('Block' in node) {
      for (const stmt of node.Block.block) walk(stmt);
      return;
    }
    if ('StmtIf' in node) {
      walk(node.StmtIf.condition);
      walk(node.StmtIf.body);
      if (node.StmtIf.alt) walk(node.StmtIf.alt);
      return;
    }
    if ('StmtWhile' in node) {
      walk(node.StmtWhile.condition);
      walk(node.StmtWhile.body);
      return;
    }
    if ('StmtFor' in node) {
      walk(node.StmtFor.array);
      walk(node.StmtFor.body);
      return;
    }
    // Other AST kinds: don't recurse; let-declarations only
    // appear at statement positions.
  }
  walk(body);
  return result;
}

function importFunction(
  decl: DeclFunc,
  stub: FunctionGraph,
  ctx: WorkflowCtx,
  report: ImportReport,
): FunctionGraph {
  // B.D c37: pre-scan body for let-declarations so fieldSet /
  // indexSet importers can infer struct names. Parameters with
  // Ident types also feed the map.
  const varTypes = scanVarTypes(decl.body);
  for (const [paramName, paramType] of decl.params) {
    if (typeof paramType !== 'string' && 'Ident' in paramType) {
      varTypes.set(paramName, paramType.Ident);
    }
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

  // Start node — every function starts with one. (`start` is a real
  // node kind, NOT the same as the workflow's entry-point function.)
  const startReal = createNode('start', { x: 0, y: 0 }, ctx, { kind: 'start' });
  const startSpec: GeneratedNode = { id: stableId(), kind: 'trigger' };
  // Use 'trigger' for the spec entry so autoLayout treats it as the
  // root; the real node is a `start`, which is fine — autoLayout
  // only looks at the spec for layout.
  state.nodes.push(startReal);
  state.spec.nodes.push(startSpec);
  const startSpecId = startSpec.id;
  const startRealId = startReal.id;

  // Walk the body. `body` is normally a `Block`, but we tolerate a
  // bare statement too.
  const body = unwrapBlock(decl.body);
  let prevSpecId = startSpecId;
  let prevRealId = startRealId;
  // The `start` node continues on its own `next` port. Branch /
  // while / forEach continue on `after`; importStatement encodes
  // this in `exitPort`.
  let prevExitPort: 'next' | 'after' = 'next';
  for (const stmt of body) {
    const result = importStatement(stmt, state);
    if (!result) continue; // statement produced nothing (rare; pure noop)
    state.stmtCount++;
    // Wire prev → entry of this statement using the previous
    // statement's correct exit port. Using `next` on a branch
    // would silently drop everything after it from the emit
    // pipeline (caught by B.8 round-trip snapshot tests).
    state.spec.edges.push({
      from: prevSpecId,
      to: result.entrySpecId,
      fromPort: prevExitPort,
      toPort: 'prev',
      kind: 'control',
    });
    state.edges.push({
      id: nanoid(8),
      source: { node: prevRealId, port: prevExitPort },
      target: { node: result.entryRealId, port: 'prev' },
      kind: 'control',
    });
    prevSpecId = result.exitSpecId;
    prevRealId = result.exitRealId;
    prevExitPort = result.exitPort;
  }

  // Layout the spec, copy positions onto the real nodes.
  const layout = autoLayout(state.spec);
  const specIdToRealId = new Map<string, string>();
  for (let i = 0; i < state.spec.nodes.length; i++) {
    specIdToRealId.set(state.spec.nodes[i]!.id, state.nodes[i]!.id);
  }
  for (let i = 0; i < state.nodes.length; i++) {
    const specId = state.spec.nodes[i]!.id;
    const pos = layout.get(specId) ?? { x: 0, y: 0 };
    state.nodes[i]!.position = pos;
  }

  // Per-function summary.
  report.functions.push({
    name: decl.name,
    support: state.worst,
    statementCount: state.stmtCount,
    unsupportedCount: state.unsupportedCount,
  });

  return {
    ...stub,
    nodes: state.nodes,
    edges: state.edges,
  };
}

function unwrapBlock(body: Ast): Ast[] {
  if (typeof body === 'string') return [];
  if ('Block' in body) return body.Block.block;
  return [body];
}

// =============================================================
//  Statement translators
// =============================================================

interface StmtImportResult {
  /** id of the first node in this statement (spec + real). */
  entrySpecId: string;
  entryRealId: string;
  /** id of the last node in the linear continuation. For branches /
   *  loops this is the JOIN point on `after`. */
  exitSpecId: string;
  exitRealId: string;
  /**
   * Which port on the exit node the NEXT statement should wire to.
   * Simple statements continue on `next`; branch / while / forEach
   * continue on `after` (the emitter walks `after` last to ensure
   * correct nesting — wiring on `next` would silently make
   * subsequent statements vanish from the emit output).
   */
  exitPort: 'next' | 'after';
}

/**
 * Public entry: dispatches to {@link importStatementInner}, then
 * attaches the AST node's source span (B.D c43) to the resulting
 * entry node so the editor can map execution-trace spans back to
 * graph nodes.
 *
 * Only struct-variant Ast nodes carry spans (`DeclVar`, `StmtIf`,
 * `StmtWhile`, `StmtFor`, `DeclFunc`, etc.); the importer
 * gracefully skips attachment for the leaf-expression cases that
 * don't have one. The editor consumer treats absence as
 * "no graph mapping for this span."
 */
function importStatement(stmt: Ast, state: FuncImportState): StmtImportResult | null {
  const result = importStatementInner(stmt, state);
  if (result) {
    const span = astStatementSpan(stmt);
    if (span) {
      const entryNode = state.nodes.find((n) => n.id === result.entryRealId);
      if (entryNode) {
        entryNode.meta = {
          ...(entryNode.meta ?? {}),
          sourceSpan: { start: span.start, end: span.end },
        };
      }
    }
  }
  return result;
}

/** Read a span field from any AST struct variant that carries one.
 *  Mirrors the Rust `Analyzer::node_span` helper added in c35. */
function astStatementSpan(stmt: Ast): { start: number; end: number } | undefined {
  if (typeof stmt === 'string') return undefined;
  if ('DeclFunc' in stmt) return stmt.DeclFunc.span;
  if ('DeclExtFunc' in stmt) return stmt.DeclExtFunc.span;
  if ('DeclVar' in stmt) return stmt.DeclVar.span;
  if ('DeclStruct' in stmt) return stmt.DeclStruct.span;
  if ('DeclEnum' in stmt) return stmt.DeclEnum.span;
  if ('Block' in stmt) return stmt.Block.span;
  if ('StmtImport' in stmt) return stmt.StmtImport.span;
  if ('StmtIf' in stmt) return stmt.StmtIf.span;
  if ('StmtWhile' in stmt) return stmt.StmtWhile.span;
  if ('StmtFor' in stmt) return stmt.StmtFor.span;
  return undefined;
}

function importStatementInner(stmt: Ast, state: FuncImportState): StmtImportResult | null {
  if (typeof stmt === 'string') {
    return makeUnsupportedPlaceholder(state, `bare unit AST "${stmt}"`);
  }

  // ---- DeclVar → `let` ----
  if ('DeclVar' in stmt) {
    const v = stmt.DeclVar;
    const data: NodeData = {
      kind: 'let',
      varName: v.name,
      varType: compilerTypeToGraphType(v.kind),
    };
    const realNode = createNode('let', { x: 0, y: 0 }, state.ctx, data);
    if (v.value !== null) {
      realNode.expressions = { value: stringifyExpr(v.value) };
    }
    return pushSimpleStatement(state, realNode, 'let', isLossyConversion(v.kind) ? 'partial' : 'partial');
    // Always 'partial' for let — the value expression is preserved
    // as inline text, never as a sub-graph.
  }

  // ---- Assignment (parser emits ExprBinary { op: 'Eq' }) ----
  //
  // B.D c37 expanded coverage to three LHS shapes:
  //   varName = expr            → assign
  //   varName.field = expr      → fieldSet  (struct inferred from scope)
  //   array[idx] = expr         → indexSet
  // Anything else still falls through to a placeholder.
  if (('ExprBinary' in stmt && stmt.ExprBinary.op === 'Eq') || 'ExprAssign' in stmt) {
    if ('ExprAssign' in stmt) {
      // ExprAssign was the old parser form; LHS is always a plain
      // variable name in this shape. Modern parser uses ExprBinary.
      const data: NodeData = { kind: 'assign', varName: stmt.ExprAssign.var_name };
      const realNode = createNode('assign', { x: 0, y: 0 }, state.ctx, data);
      realNode.expressions = { value: stringifyExpr(stmt.ExprAssign.value) };
      return pushSimpleStatement(state, realNode, 'assign', 'partial');
    }
    const lhs = stmt.ExprBinary.lhs;
    const value = stmt.ExprBinary.rhs;

    // varName = expr
    if (typeof lhs !== 'string' && 'ExprVar' in lhs) {
      const data: NodeData = { kind: 'assign', varName: lhs.ExprVar };
      const realNode = createNode('assign', { x: 0, y: 0 }, state.ctx, data);
      realNode.expressions = { value: stringifyExpr(value) };
      return pushSimpleStatement(state, realNode, 'assign', 'partial');
    }

    // varName.field = expr → fieldSet
    if (
      typeof lhs !== 'string'
      && 'ExprMemAcc' in lhs
      && typeof lhs.ExprMemAcc.lhs !== 'string'
      && 'ExprVar' in lhs.ExprMemAcc.lhs
    ) {
      const targetVar = lhs.ExprMemAcc.lhs.ExprVar;
      const fieldName = lhs.ExprMemAcc.member;
      // Infer struct name from scope (let-declarations + params).
      // If we can't infer, use '' — the graph validator will flag
      // it; better than dropping the assignment entirely.
      const structName = state.varTypes.get(targetVar) ?? '';
      if (!structName) {
        state.report.notices.push({
          severity: 'warning',
          message: `In function "${state.funcName}": couldn't infer struct type for \`${targetVar}.${fieldName} = ...\` — node imported with empty structName; you may need to set it manually.`,
          functionName: state.funcName,
          support: 'partial',
        });
      }
      const data: NodeData = { kind: 'fieldSet', structName, fieldName };
      const realNode = createNode('fieldSet', { x: 0, y: 0 }, state.ctx, data);
      // fieldSet has two data inputs: `target` (the struct ref)
      // + `value`. Inline both as text since we're not lifting
      // them to sub-graphs.
      realNode.expressions = {
        target: targetVar,
        value: stringifyExpr(value),
      };
      return pushSimpleStatement(state, realNode, 'assign', 'partial');
    }

    // array[idx] = expr → indexSet
    if (typeof lhs !== 'string' && 'ExprArrAcc' in lhs) {
      const data: NodeData = {
        kind: 'indexSet',
        // No analyzer info, so elementType defaults to any. User
        // can retype in the Inspector. This matches the existing
        // forEach iteratorType fallback strategy.
        elementType: { kind: 'any' },
      };
      const realNode = createNode('indexSet', { x: 0, y: 0 }, state.ctx, data);
      realNode.expressions = {
        array: stringifyExpr(lhs.ExprArrAcc.lhs),
        index: stringifyExpr(lhs.ExprArrAcc.index),
        value: stringifyExpr(value),
      };
      return pushSimpleStatement(state, realNode, 'assign', 'partial');
    }

    // Other LHS shapes — keep the placeholder fallback for safety.
    return makeUnsupportedPlaceholder(
      state,
      'complex assignment LHS (multi-level member access or other)',
      'partial',
      stringifyExpr(stmt),
    );
  }

  // ---- print(...) → `print` ----
  if ('ExprFuncCall' in stmt && stmt.ExprFuncCall.name === 'print') {
    const args = stmt.ExprFuncCall.args;
    const valueExpr =
      args.length === 0
        ? '""'
        : args.length === 1
        ? stringifyExpr(args[0]!)
        : `[${args.map(stringifyExpr).join(', ')}]`; // multi-arg → array
    const data: NodeData = { kind: 'print' };
    const realNode = createNode('print', { x: 0, y: 0 }, state.ctx, data);
    realNode.expressions = { value: valueExpr };
    return pushSimpleStatement(state, realNode, 'print', 'partial');
  }

  // ---- Generic ExprFuncCall — `call` node when target is known,
  //      otherwise a `print` placeholder ----
  if ('ExprFuncCall' in stmt) {
    const name = stmt.ExprFuncCall.name;
    const fn = state.ctx.functions.find((f) => f.name === name);
    if (fn) {
      const data: NodeData = { kind: 'call', functionId: fn.id };
      const realNode = createNode('call', { x: 0, y: 0 }, state.ctx, data);
      // Inline expression per arg port. The factory should have
      // produced one port per param of the called function.
      const argPorts = realNode.ports.in.filter((p) => p.kind === 'data');
      const exprs: Record<string, string> = {};
      for (let i = 0; i < stmt.ExprFuncCall.args.length; i++) {
        const port = argPorts[i];
        if (port) exprs[port.id] = stringifyExpr(stmt.ExprFuncCall.args[i]!);
      }
      realNode.expressions = exprs;
      return pushSimpleStatement(state, realNode, 'call', 'partial');
    }
    // Unknown function — preserve as a print placeholder with the
    // original call text inline.
    return makeUnsupportedPlaceholder(
      state,
      `call to unknown function "${name}"`,
      'partial',
      stringifyExpr(stmt),
    );
  }

  // ---- ExprReturn → `return` ----
  if ('ExprReturn' in stmt) {
    const hasValue = stmt.ExprReturn.val !== null;
    const data: NodeData = { kind: 'return', hasValue };
    const realNode = createNode('return', { x: 0, y: 0 }, state.ctx, data);
    if (hasValue && stmt.ExprReturn.val !== null) {
      realNode.expressions = { value: stringifyExpr(stmt.ExprReturn.val) };
    }
    return pushSimpleStatement(state, realNode, 'return', 'partial');
  }

  // ---- StmtIf → `branch` + bodies ----
  if ('StmtIf' in stmt) return importBranch(stmt.StmtIf, state);

  // ---- StmtWhile → `while` + body ----
  if ('StmtWhile' in stmt) return importLoop(stmt.StmtWhile.condition, stmt.StmtWhile.body, state, 'while');

  // ---- StmtFor → `forEach` + body ----
  if ('StmtFor' in stmt) return importForEach(stmt.StmtFor.elem_name, stmt.StmtFor.array, stmt.StmtFor.body, state);

  // ---- Block-as-statement — flatten ----
  if ('Block' in stmt) {
    // A bare block at statement level — flatten it.
    const inner = stmt.Block.block;
    if (inner.length === 0) return null;
    let first: StmtImportResult | null = null;
    let prev: StmtImportResult | null = null;
    for (const s of inner) {
      const r = importStatement(s, state);
      if (!r) continue;
      if (!first) first = r;
      if (prev) {
        // Wire prev → r using prev's own exitPort (next vs after).
        // Hard-coding `next` here would silently drop statements
        // after nested branches/loops inside a block — same class
        // of bug B.8 round-trip tests caught at the function level.
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
    // The block's overall exit is the last statement's exit — same
    // node + same exit port (so nested blocks compose correctly).
    return {
      entrySpecId: first.entrySpecId,
      entryRealId: first.entryRealId,
      exitSpecId: prev.exitSpecId,
      exitRealId: prev.exitRealId,
      exitPort: prev.exitPort,
    };
  }

  // ---- Everything else — placeholder ----
  const kind = Object.keys(stmt)[0] ?? 'unknown';
  return makeUnsupportedPlaceholder(state, `AST node "${kind}"`, 'unsupported');
}

function importBranch(
  decl: { condition: Ast; body: Ast; alt: Ast | null },
  state: FuncImportState,
): StmtImportResult {
  const hasElse = decl.alt !== null;
  const branchReal = createNode('branch', { x: 0, y: 0 }, state.ctx, {
    kind: 'branch',
    hasElse,
  });
  branchReal.expressions = { cond: stringifyExpr(decl.condition) };
  const branchSpec: GeneratedNode = {
    id: stableId(),
    kind: 'branch',
    hasElse,
  };
  state.nodes.push(branchReal);
  state.spec.nodes.push(branchSpec);

  // Then arm.
  const thenResult = importStatement(unwrapToFirstOrBlock(decl.body), state);
  if (thenResult) {
    state.spec.edges.push({
      from: branchSpec.id,
      to: thenResult.entrySpecId,
      fromPort: 'then',
      toPort: 'prev',
      kind: 'control',
    });
    state.edges.push({
      id: nanoid(8),
      source: { node: branchReal.id, port: 'then' },
      target: { node: thenResult.entryRealId, port: 'prev' },
      kind: 'control',
    });
  }

  // Else arm.
  if (hasElse && decl.alt !== null) {
    const elseResult = importStatement(unwrapToFirstOrBlock(decl.alt), state);
    if (elseResult) {
      state.spec.edges.push({
        from: branchSpec.id,
        to: elseResult.entrySpecId,
        fromPort: 'else',
        toPort: 'prev',
        kind: 'control',
      });
      state.edges.push({
        id: nanoid(8),
        source: { node: branchReal.id, port: 'else' },
        target: { node: elseResult.entryRealId, port: 'prev' },
        kind: 'control',
      });
    }
  }

  bumpSupport(state, 'partial');
  return {
    entrySpecId: branchSpec.id,
    entryRealId: branchReal.id,
    exitSpecId: branchSpec.id, // continuation hangs off `after`
    exitRealId: branchReal.id,
    exitPort: 'after',
  };
}

function importLoop(
  condition: Ast,
  body: Ast,
  state: FuncImportState,
  kind: 'while',
): StmtImportResult {
  const real = createNode(kind, { x: 0, y: 0 }, state.ctx, { kind } as NodeData);
  real.expressions = { cond: stringifyExpr(condition) };
  const spec: GeneratedNode = { id: stableId(), kind };
  state.nodes.push(real);
  state.spec.nodes.push(spec);

  const bodyResult = importStatement(unwrapToFirstOrBlock(body), state);
  if (bodyResult) {
    state.spec.edges.push({
      from: spec.id,
      to: bodyResult.entrySpecId,
      fromPort: 'body',
      toPort: 'prev',
      kind: 'control',
    });
    state.edges.push({
      id: nanoid(8),
      source: { node: real.id, port: 'body' },
      target: { node: bodyResult.entryRealId, port: 'prev' },
      kind: 'control',
    });
    // Loop body's last node loops BACK to the while node via `prev`-of-spec.
    // (No real edge needed — semantically the `body` port re-enters,
    // and the editor renders it cleanly without an explicit back-edge.)
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
  array: Ast,
  body: Ast,
  state: FuncImportState,
): StmtImportResult {
  const data: NodeData = {
    kind: 'forEach',
    iteratorName,
    // The compiler doesn't ship inferred element types in the AST;
    // default to `any` and let the user retype if they care.
    iteratorType: { kind: 'any' },
  };
  const real = createNode('forEach', { x: 0, y: 0 }, state.ctx, data);
  real.expressions = { array: stringifyExpr(array) };
  const spec: GeneratedNode = {
    id: stableId(),
    kind: 'forEach',
    iteratorName,
  };
  state.nodes.push(real);
  state.spec.nodes.push(spec);

  const bodyResult = importStatement(unwrapToFirstOrBlock(body), state);
  if (bodyResult) {
    state.spec.edges.push({
      from: spec.id,
      to: bodyResult.entrySpecId,
      fromPort: 'body',
      toPort: 'prev',
      kind: 'control',
    });
    state.edges.push({
      id: nanoid(8),
      source: { node: real.id, port: 'body' },
      target: { node: bodyResult.entryRealId, port: 'prev' },
      kind: 'control',
    });
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
  // Emit a `print` node with the unsupported text as its value so
  // nothing is silently dropped. Marks the function partial/degraded.
  const data: NodeData = { kind: 'print' };
  const real = createNode('print', { x: 0, y: 0 }, state.ctx, data);
  real.expressions = {
    value: inlineSolText ?? `"/* unsupported: ${what} */"`,
  };
  const spec: GeneratedNode = { id: stableId(), kind: 'print' };
  state.nodes.push(real);
  state.spec.nodes.push(spec);
  state.unsupportedCount++;
  bumpSupport(state, support);
  state.report.counts[supportBucket(support)]++;
  state.report.notices.push({
    severity: 'warning',
    message: `In function "${state.funcName}": ${what} preserved as inline text on a placeholder node.`,
    functionName: state.funcName,
    support,
  });
  return {
    entrySpecId: spec.id,
    entryRealId: real.id,
    exitSpecId: spec.id,
    exitRealId: real.id,
    exitPort: 'next', // placeholder is a print, which continues on `next`
  };
}

function unwrapToFirstOrBlock(a: Ast): Ast {
  // We always pass a single AST node to importStatement; if the
  // branch/loop body is a Block, importStatement handles it; if
  // it's a single statement, importStatement handles that too.
  return a;
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
  // Spec ids only need to be unique within one importFunction call;
  // we use a process-monotonic counter to keep them short for
  // debug-log readability.
  return `n${++_idCounter}`;
}

// Type sanity check — keeps the unused-import warning at bay if a
// type only appears in a comment.
type _NodeKindCheck = NodeKind;
type _ParamCheck = Param;

/**
 * Scan source for `function <name>` declarations and return a
 * `Map<name, lineNumber>` (1-indexed). Used to attach
 * `FunctionGraph.meta.sourceLine` so the import report can scroll
 * the source pane to a function on click.
 *
 * Why textual not AST-based:
 *   - The parsed AST doesn't yet carry source spans on nodes
 *     (deferred — see compiler/REMAINING_PANICS.md).
 *   - A simple regex scan is accurate enough for the
 *     editor-typical case: function declarations at line start.
 *   - First-match-wins on duplicate names (which would also fail
 *     the analyzer's `redefinition` check upstream of import).
 *
 * Limitations the user would only hit by trying:
 *   - `function foo` appearing inside a string literal would
 *     mis-match. Tolerable; the worst outcome is a misleading line
 *     hint, not a crash.
 *   - Function declared on the same line as other text (rare):
 *     reports the line correctly but scroll-into-view lands at
 *     the start of that line.
 */
/**
 * Convert a 0-indexed byte offset to a 1-indexed line number.
 * Mirrors `SourceSpan::to_line_col` on the Rust side. ASCII-only
 * safe (multi-byte UTF-8 chars in strings could shift column but
 * not line, which is what this helper returns).
 */
function lineNumberAt(source: string, byteOffset: number): number {
  let line = 1;
  for (let i = 0; i < byteOffset && i < source.length; i++) {
    if (source.charCodeAt(i) === 10) line++;
  }
  return line;
}

function scanFunctionLines(source: string): Map<string, number> {
  const result = new Map<string, number>();
  // Matches:  optional whitespace, `function`, mandatory ws, then
  // identifier capture, then optional whitespace, then `(`.
  // No multiline flag — we scan line by line so the line number
  // is naturally available.
  const fnPattern = /^\s*function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/;
  const lines = source.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i]!.match(fnPattern);
    if (m && m[1] && !result.has(m[1])) {
      result.set(m[1], i + 1); // 1-indexed
    }
  }
  return result;
}
