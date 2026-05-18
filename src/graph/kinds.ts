/**
 * SolFlow Phase A — palette catalog.
 *
 * Single source of truth for the node-kind list shown in the left palette
 * and the per-kind port builder.
 */

import type { NodeData, NodeKind, SolType } from './schema';

export type Category =
  | 'trigger'
  | 'flow'
  | 'variable'
  | 'operator'
  | 'literal'
  | 'access'
  | 'call'
  | 'io'
  | 'entry';

export interface PaletteEntry {
  kind: NodeKind;
  label: string;
  category: Category;
  description: string;
  draggable: boolean; // start is not draggable; auto-placed
  /**
   * Optional partial data merged with defaultData(kind) when creating a
   * node from this entry. Used for trigger sub-kinds (manual / webhook /
   * timer / event / http) so all five share the 'trigger' NodeKind but
   * land with different initial state + visuals.
   */
  initialData?: Partial<NodeData>;
}

// Tiny helper to coin webhook paths so two new Webhook triggers don't
// collide. Demo-grade — Phase B replaces with server-issued URLs.
function newWebhookPath(): string {
  const slug = Math.random().toString(36).slice(2, 9);
  return `/webhooks/${slug}`;
}

export const PALETTE: PaletteEntry[] = [
  // entry — present but not in palette
  { kind: 'start', label: 'Start', category: 'entry', description: 'Function entry', draggable: false },

  // triggers — first-class event-driven entrypoints
  {
    kind: 'trigger',
    label: 'Manual Trigger',
    category: 'trigger',
    description: 'Manually invoked entry point',
    draggable: true,
    initialData: {
      kind: 'trigger',
      triggerKind: 'manual',
      eventName: 'manual.run',
      payloadSchema: '{ "type": "object" }',
      samplePayload: '{}',
    },
  },
  {
    kind: 'trigger',
    label: 'Webhook',
    category: 'trigger',
    description: 'POST to a generated URL fires this workflow',
    draggable: true,
    initialData: {
      kind: 'trigger',
      triggerKind: 'webhook',
      eventName: 'webhook.received',
      webhookPath: newWebhookPath(),
      payloadSchema: '{ "type": "object" }',
      samplePayload: '{\n  "id": "evt_abc123",\n  "body": {}\n}',
    },
  },
  {
    kind: 'trigger',
    label: 'Timer',
    category: 'trigger',
    description: 'Schedule (cron-style)',
    draggable: true,
    initialData: {
      kind: 'trigger',
      triggerKind: 'timer',
      eventName: 'timer.tick',
      cronExpr: '*/5 * * * *',
      payloadSchema: '{ "at": "string" }',
      samplePayload: '{\n  "at": "2026-01-01T00:00:00Z"\n}',
    },
  },
  {
    kind: 'trigger',
    label: 'Event',
    category: 'trigger',
    description: 'React to a named domain event',
    draggable: true,
    initialData: {
      kind: 'trigger',
      triggerKind: 'event',
      eventName: 'invoice.created',
      payloadSchema: '{ "id": "string", "amount": "number" }',
      samplePayload: '{\n  "id": "inv_001",\n  "amount": 4250\n}',
    },
  },
  {
    kind: 'trigger',
    label: 'HTTP Trigger',
    category: 'trigger',
    description: 'REST endpoint',
    draggable: true,
    initialData: {
      kind: 'trigger',
      triggerKind: 'http',
      eventName: 'http.request',
      httpMethod: 'POST',
      httpPath: '/api/orders',
      payloadSchema: '{ "body": "object" }',
      samplePayload: '{\n  "body": {}\n}',
    },
  },

  // flow
  { kind: 'branch', label: 'Branch', category: 'flow', description: 'if / else', draggable: true },
  { kind: 'while', label: 'While', category: 'flow', description: 'while loop', draggable: true },
  { kind: 'forEach', label: 'For Each', category: 'flow', description: 'for x in array', draggable: true },
  { kind: 'return', label: 'Return', category: 'flow', description: 'return [value]', draggable: true },

  // variables
  { kind: 'let', label: 'Let', category: 'variable', description: 'let x: T = …', draggable: true },
  { kind: 'assign', label: 'Assign', category: 'variable', description: 'x = …', draggable: true },
  { kind: 'varGet', label: 'Var Get', category: 'variable', description: 'reference a variable', draggable: true },

  // operators
  { kind: 'binaryOp', label: 'Binary Op', category: 'operator', description: '+ − × / compare, logic', draggable: true },
  { kind: 'unaryOp', label: 'Unary Op', category: 'operator', description: '−x, !x', draggable: true },

  // literals
  { kind: 'literal', label: 'Literal', category: 'literal', description: 'int / float / bool / str / char', draggable: true },
  { kind: 'arrayLiteral', label: 'Array Literal', category: 'literal', description: '[a, b, c]', draggable: true },
  { kind: 'structLiteral', label: 'Struct Literal', category: 'literal', description: 'Name { field: … }', draggable: true },

  // access
  { kind: 'fieldAccess', label: 'Field Get', category: 'access', description: 'expr.field', draggable: true },
  { kind: 'fieldSet', label: 'Field Set', category: 'access', description: 'expr.field = …', draggable: true },
  { kind: 'indexRead', label: 'Index Get', category: 'access', description: 'arr[i]', draggable: true },
  { kind: 'indexSet', label: 'Index Set', category: 'access', description: 'arr[i] = …', draggable: true },
  { kind: 'enumVariant', label: 'Enum Variant', category: 'access', description: 'E::V', draggable: true },

  // io / calls
  { kind: 'print', label: 'Print', category: 'io', description: 'print(…)', draggable: true },
  { kind: 'call', label: 'Call', category: 'call', description: 'call a function', draggable: true },
];

export function paletteByCategory(): Record<Category, PaletteEntry[]> {
  const map: Record<Category, PaletteEntry[]> = {
    trigger: [],
    flow: [],
    variable: [],
    operator: [],
    literal: [],
    access: [],
    call: [],
    io: [],
    entry: [],
  };
  for (const entry of PALETTE) {
    if (entry.draggable) map[entry.category].push(entry);
  }
  return map;
}

/**
 * User-facing category names. Internal NodeKind names stay technical
 * (`binaryOp`, `structLiteral`, etc.) because they're tied to SOL's AST;
 * these labels are what humans read in the palette.
 */
export const CATEGORY_LABELS: Record<Category, string> = {
  trigger: 'Triggers',
  flow: 'Decisions & loops',
  variable: 'Variables',
  operator: 'Math & logic',
  literal: 'Fixed values',
  access: 'Read & update',
  call: 'Reuse a workflow',
  io: 'Output',
  entry: 'Entry',
};

/**
 * Categories that are useful but heavy on language-theory terminology
 * (binary ops, struct/enum literals, field/index access). Hidden behind
 * an "Advanced" disclosure in the palette so first-time users see a
 * short, human-friendly list instead of a wall of AST nodes.
 */
export const ADVANCED_CATEGORIES: Category[] = ['operator', 'literal', 'access'];

export function isAdvancedCategory(c: Category): boolean {
  return ADVANCED_CATEGORIES.includes(c);
}

export function categoryColor(c: Category): string {
  switch (c) {
    case 'trigger':
      return 'var(--sf-cat-trigger)';
    case 'flow':
      return 'var(--sf-cat-flow)';
    case 'variable':
      return 'var(--sf-cat-variable)';
    case 'operator':
      return 'var(--sf-cat-operator)';
    case 'literal':
      return 'var(--sf-cat-literal)';
    case 'access':
      return 'var(--sf-cat-access)';
    case 'call':
      return 'var(--sf-cat-call)';
    case 'io':
      return 'var(--sf-cat-io)';
    case 'entry':
      return 'var(--sf-cat-entry)';
  }
}

export function categoryForKind(kind: NodeKind): Category {
  return PALETTE.find((p) => p.kind === kind)?.category ?? 'flow';
}

export function defaultLetType(): SolType {
  return { kind: 'int' };
}
