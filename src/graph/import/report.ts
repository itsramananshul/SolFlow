/**
 * Import-report types.
 *
 * The importer is intentionally honest about what it can and can't
 * represent visually. Every meaningful decision it makes — every
 * unsupported construct, every degradation, every inline-text
 * fallback — surfaces here so the user sees what just happened to
 * their source.
 */

/** Per-construct classification. */
export type ImportSupport =
  /** Clean visual representation; the graph is the canonical form. */
  | 'full'
  /** Graphable core (a node exists) but at least one subexpression
   *  is preserved as inline SOL text rather than as a sub-graph. */
  | 'partial'
  /** Cannot graph yet; preserved in source mode only. The construct
   *  produces a notice but no graph node. */
  | 'source-only'
  /** Actively rejected with a notice; the owning function is
   *  flagged degraded. Reserved for AST shapes the importer doesn't
   *  understand (vs. just chooses not to graph). */
  | 'unsupported';

export interface ImportNotice {
  severity: 'info' | 'warning';
  message: string;
  /** Which function this notice is about (if any). Top-level
   *  notices (struct decl, import, parse error) omit this. */
  functionName?: string;
  /** Construct classification, if applicable. */
  support?: ImportSupport;
}

/** Per-function rollup. */
export interface FunctionImportSummary {
  name: string;
  /** Worst classification any statement in this function received. */
  support: ImportSupport;
  statementCount: number;
  /** How many statements landed as source-only / unsupported. */
  unsupportedCount: number;
}

/** What the importer hands back to the caller. */
export interface ImportReport {
  /** Did the parser produce a usable AST? When false, the caller
   *  should not load the workflow — the report's notices explain
   *  why (compiler diagnostics are surfaced separately). */
  ok: boolean;
  notices: ImportNotice[];
  functions: FunctionImportSummary[];
  /** Top-level construct counts. */
  topLevel: {
    structs: number;
    enums: number;
    imports: number;
    /** External (`ext function …`) declarations — preserved in
     *  source but not yet representable as graph nodes. */
    extFunctions: number;
  };
  /** Headline counts across every statement in every function. */
  counts: {
    full: number;
    partial: number;
    sourceOnly: number;
    unsupported: number;
  };
}

export function emptyReport(): ImportReport {
  return {
    ok: true,
    notices: [],
    functions: [],
    topLevel: { structs: 0, enums: 0, imports: 0, extFunctions: 0 },
    counts: { full: 0, partial: 0, sourceOnly: 0, unsupported: 0 },
  };
}
