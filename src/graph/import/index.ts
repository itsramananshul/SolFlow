/**
 * Public surface for the AST → graph importer.
 *
 * Consumers (graph.store, the import modal, tests) should import
 * from this file rather than reaching into the internal modules.
 */

export { importProgram, type ImportResult } from './importer';
export type {
  ImportNotice,
  ImportReport,
  ImportSupport,
  FunctionImportSummary,
} from './report';
export { stringifyExpr } from './expressions';
