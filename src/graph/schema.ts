/**
 * SolFlow Phase A — graph schema.
 *
 * Strict subset of the Phase B `SolGraph` from
 * `reference/SOL_VISUAL_EDITOR_ANALYSIS.md` §5. Forward-compatible: a Phase A
 * workflow JSON can be loaded as a Phase B graph by wrapping it.
 */

// =============================================================
//  SOL types
// =============================================================

export type SolPrimitive = 'int' | 'float' | 'bool' | 'str' | 'char';

export type SolType =
  | { kind: SolPrimitive }
  | { kind: 'array'; size: number | null; inner: SolType }
  | { kind: 'named'; name: string } // struct or enum reference
  | { kind: 'void' }
  | { kind: 'any' }; // editor-only, used for `print` input + unresolved

export type BinaryOpSymbol =
  | '+'
  | '-'
  | '*'
  | '/'
  | '=='
  | '!='
  | '<'
  | '>'
  | '<='
  | '>='
  | '&&'
  | '||';

export type UnaryOpSymbol = '-' | '!';

// =============================================================
//  Top-level workflow shape
// =============================================================

export interface SolWorkflow {
  schemaVersion: 1;
  meta: {
    name: string;
    /** Optional one-line description shown at the top of the canvas. */
    description?: string;
    createdAt: string;
    updatedAt: string;
  };
  imports: ImportDecl[];
  structs: StructDecl[];
  enums: EnumDecl[];
  functions: FunctionGraph[];
}

export interface ImportDecl {
  id: string;
  path: string[]; // ["slack"] — canonical imports are single-segment
  alias: string; // local symbol name (the capability name for `from` imports)
  /**
   * Set for canonical `import "name" from module;` imports. Holds the
   * source module. When present the emitter produces the `from` form
   * (`import "<alias>" from <from>;`); when absent it produces the
   * plain module form (`import <path[0]>;`). Mirrors
   * `ast.rs::ImportSpec` (Module vs Named).
   */
  from?: string;
}

export interface StructField {
  name: string;
  type: SolType;
}

export interface StructDecl {
  id: string;
  name: string;
  fields: StructField[]; // insertion order preserved (array, not map)
}

export interface EnumVariant {
  name: string;
  value: number | null; // null = auto-assigned
}

export interface EnumDecl {
  id: string;
  name: string;
  variants: EnumVariant[];
}

export interface Param {
  name: string;
  type: SolType;
}

export interface FunctionGraph {
  id: string;
  name: string;
  params: Param[];
  returnType: SolType; // { kind: 'void' } if absent
  nodes: GraphNode[];
  edges: GraphEdge[];
  /**
   * True when this graph represents a canonical `workflow "name" { }`
   * (the runnable unit), false/undefined when it represents a helper
   * `fn name(params) <- ret { }`. The emitter uses this to choose
   * between `workflow "<name>" { }` and `fn <name>(...) <- <ret> { }`.
   * Set by the AST→graph importer; hand-built graphs default to a
   * workflow.
   */
  isWorkflow?: boolean;
  /**
   * Optional source-attachment metadata, populated when this
   * function was produced by the AST→graph importer.
   *
   * Hand-built workflows don't have this. Re-importing later
   * overwrites it with fresh values.
   */
  meta?: {
    /** 1-indexed line in the imported source where the function or
     *  workflow declaration begins. Used by the import report panel
     *  to scroll the source pane on click. */
    sourceLine?: number;
  };
}

// =============================================================
//  Nodes
// =============================================================

export type NodeKind =
  // entry
  | 'start'
  | 'trigger'
  // flow
  | 'let'
  | 'assign'
  | 'print'
  | 'return'
  | 'branch'
  | 'while'
  | 'forEach'
  // expressions
  | 'binaryOp'
  | 'unaryOp'
  | 'varGet'
  | 'literal'
  | 'arrayLiteral'
  | 'structLiteral'
  | 'fieldAccess'
  | 'fieldSet'
  | 'indexRead'
  | 'indexSet'
  | 'enumVariant'
  | 'call'
  | 'action'
  // annotations — non-executable visual aids for big workflows.
  // Notes hold free text; Frames wrap a region of nodes with a title.
  | 'note'
  | 'frame';

/**
 * Trigger sub-kind. A trigger node is an event-driven entrypoint to a
 * function — runs the workflow when an event arrives (webhook delivered,
 * cron tick, named event published, etc.). Phase A models this purely
 * visually + with sample payload injection during simulated runs.
 */
export type TriggerKind = 'manual' | 'webhook' | 'timer' | 'event' | 'http';

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

export type PortKind = 'control' | 'data';

export interface Port {
  id: string; // unique within node
  name: string; // human label
  kind: PortKind;
  type?: SolType; // present iff kind === 'data'
  required: boolean;
}

export interface NodePorts {
  in: Port[];
  out: Port[];
}

// Discriminated union for per-kind data.
export type NodeData =
  | { kind: 'start' }
  | {
      kind: 'trigger';
      triggerKind: TriggerKind;
      eventName: string;
      /** Free-form JSON-Schema-style payload contract (Phase A: text). */
      payloadSchema: string;
      /** Sample event payload as JSON literal text. Bound to the
       *  payload data-out port during simulated runs. */
      samplePayload: string;
      /** Webhook-only: generated path. */
      webhookPath?: string;
      /** Timer-only: cron-style schedule expression. */
      cronExpr?: string;
      /** HTTP-only: REST method. */
      httpMethod?: HttpMethod;
      /** HTTP-only: route path. */
      httpPath?: string;
    }
  | { kind: 'let'; varName: string; varType: SolType }
  | { kind: 'assign'; varName: string } // assign-to-var (var picked from scope dropdown)
  | { kind: 'print' }
  | { kind: 'return'; hasValue: boolean }
  | { kind: 'branch'; hasElse: boolean }
  | { kind: 'while' }
  | { kind: 'forEach'; iteratorName: string; iteratorType: SolType }
  | { kind: 'binaryOp'; op: BinaryOpSymbol; valueType: SolType }
  | { kind: 'unaryOp'; op: UnaryOpSymbol; valueType: SolType }
  | { kind: 'varGet'; varName: string; resolvedType: SolType }
  | { kind: 'literal'; litType: SolPrimitive; value: string } // value is raw text
  | { kind: 'arrayLiteral'; itemType: SolType; length: number }
  | { kind: 'structLiteral'; structName: string }
  | { kind: 'fieldAccess'; structName: string; fieldName: string }
  | { kind: 'fieldSet'; structName: string; fieldName: string }
  | { kind: 'indexRead'; elementType: SolType }
  | { kind: 'indexSet'; elementType: SolType }
  | { kind: 'enumVariant'; enumName: string; variantName: string }
  | { kind: 'call'; functionId: string } // refs FunctionGraph.id
  // External capability call: `call("module.function", params)`. The
  // controller resolves `capability` ("module.function") against a
  // registered provider; Browser Simulation blocks it. `params` arrives
  // on the data-in port; the provider's result leaves on the return port.
  | { kind: 'action'; capability: string }
  // -----------------------------------------------------------
  // Annotations — render-only, not part of execution semantics.
  // Skipped by interpret/emit/validate; do not declare ports.
  // -----------------------------------------------------------
  | { kind: 'note'; text: string }
  | { kind: 'frame'; title: string; width: number; height: number };

export interface GraphNode {
  id: string; // nanoid
  data: NodeData;
  position: { x: number; y: number };
  ports: NodePorts;
  /**
   * Optional source-attachment metadata. Populated by the AST→graph
   * importer (B.D c43) for nodes whose source AST carried a span.
   * The editor uses this to map an execution trace span back to
   * a graph node ("click trace step → focus this node on canvas").
   *
   * Phase-A hand-built workflows leave `sourceSpan` undefined.
   * Re-importing later overwrites with fresh values.
   */
  meta?: {
    /** Byte range in the source the node came from (0-indexed,
     *  exclusive end). Mirrors Rust's `SourceSpan`. */
    sourceSpan?: { start: number; end: number };
  };
  /**
   * Inline SOL expression text keyed by input port id. Non-empty values
   * take precedence over wired data edges during emit. This is the
   * Phase A escape hatch for fast workflow authoring — users can type
   * `status == AppHealth::Stable` directly into the condition field
   * instead of wiring a Binary Op + two Var Gets + an Enum Variant.
   *
   * Phase B keeps this field; the WASM analyzer will parse + type-check
   * each inline expression and integrate it into the same diagnostics
   * pipeline that wired graphs use today.
   */
  expressions?: Record<string, string>;
}

export interface GraphEdge {
  id: string;
  source: { node: string; port: string };
  target: { node: string; port: string };
  kind: PortKind; // must match both endpoints
}

// =============================================================
//  Type helpers (Phase A — TEMPORARY, will move to WASM in Phase B)
// =============================================================

export function typeEqual(a: SolType, b: SolType): boolean {
  if (a.kind === 'any' || b.kind === 'any') return true;
  if (a.kind !== b.kind) return false;
  if (a.kind === 'array' && b.kind === 'array') {
    if (a.size !== b.size) return false;
    return typeEqual(a.inner, b.inner);
  }
  if (a.kind === 'named' && b.kind === 'named') {
    return a.name === b.name;
  }
  return true;
}

export function typeLabel(t: SolType): string {
  switch (t.kind) {
    case 'void':
      return 'void';
    case 'any':
      return 'any';
    case 'int':
    case 'float':
    case 'bool':
    case 'str':
    case 'char':
      return t.kind;
    case 'array': {
      const sizeStr = t.size === null ? '' : String(t.size);
      return `[${sizeStr}]${typeLabel(t.inner)}`;
    }
    case 'named':
      return t.name;
  }
}

export function typeCssClass(t: SolType | undefined): string {
  if (!t) return 'data-any';
  if (t.kind === 'array') return 'data-array';
  if (t.kind === 'named') return 'data-struct';
  if (t.kind === 'void') return 'data-any';
  return `data-${t.kind}`;
}

// =============================================================
//  Operator metadata
// =============================================================

export const BINARY_OPS: BinaryOpSymbol[] = [
  '+',
  '-',
  '*',
  '/',
  '==',
  '!=',
  '<',
  '>',
  '<=',
  '>=',
  '&&',
  '||',
];
export const UNARY_OPS: UnaryOpSymbol[] = ['-', '!'];

export function isArithmeticOp(op: BinaryOpSymbol): boolean {
  return op === '+' || op === '-' || op === '*' || op === '/';
}

export function isComparisonOp(op: BinaryOpSymbol): boolean {
  return (
    op === '==' || op === '!=' || op === '<' || op === '>' || op === '<=' || op === '>='
  );
}

export function isLogicalOp(op: BinaryOpSymbol): boolean {
  return op === '&&' || op === '||';
}

export function binaryOpResultType(op: BinaryOpSymbol, operandType: SolType): SolType {
  if (isComparisonOp(op) || isLogicalOp(op)) return { kind: 'bool' };
  return operandType;
}

export function unaryOpResultType(op: UnaryOpSymbol, operandType: SolType): SolType {
  if (op === '!') return { kind: 'bool' };
  return operandType;
}
