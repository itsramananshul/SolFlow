/**
 * Port metadata — single source of truth for the human-facing strings
 * attached to each (NodeKind, portId) pair: a friendly label, a helper
 * blurb, a placeholder, and example values.
 *
 * Used by:
 *   - Inspector (full label + helper text below the field + example chips)
 *   - SolNode card (placeholder on the inline input)
 *
 * Centralizing this means we never let "port labelled `value`" leak
 * into the UI again. Tweak strings here, both surfaces inherit.
 */

import type { NodeKind } from './schema';

export interface PortMeta {
  /** Human label shown in the Inspector above the input. */
  label?: string;
  /** Helper blurb shown below the input. Explains what to type / wire. */
  helper?: string;
  /** Concrete example values. Rendered as click-to-fill chips. */
  examples?: string[];
  /** Placeholder for the input element. */
  placeholder?: string;
}

const META: Record<string, PortMeta> = {
  // ---------- print -------------------------------------------------
  'print:value': {
    label: 'What should it print?',
    helper:
      'A SOL expression or a wired value. Strings, numbers, struct fields — anything you can read.',
    examples: ['"hello"', 'p.age', 'status', 'count + 1'],
    placeholder: '"hello, world"',
  },

  // ---------- return ------------------------------------------------
  'return:value': {
    label: 'What should this function return?',
    helper: 'The value handed back to whoever called this function.',
    examples: ['0', 'result', 'order.id'],
    placeholder: '0',
  },

  // ---------- let / assign ------------------------------------------
  'let:value': {
    label: 'Initial value',
    helper: 'The value this variable starts with.',
    examples: ['0', '"untitled"', 'Person { name: "evan", age: 19 }'],
    placeholder: '0',
  },
  'assign:value': {
    label: 'New value',
    helper: 'The value to write into this variable.',
    examples: ['counter + 1', '"done"', 'true'],
    placeholder: 'counter + 1',
  },

  // ---------- branch / while ---------------------------------------
  'branch:cond': {
    label: 'When should this path run?',
    helper: 'A condition that evaluates to true or false.',
    examples: ['age > 18', 'status == "approved"', 'count < 10'],
    placeholder: 'age > 18',
  },
  'while:cond': {
    label: 'Keep looping while…',
    helper: 'A condition checked before each pass. When false, the loop exits.',
    examples: ['i < 10', 'queue.size() > 0', '!done'],
    placeholder: 'i < 10',
  },

  // ---------- forEach ----------------------------------------------
  'forEach:array': {
    label: 'Which array to walk through?',
    helper: 'An array. Each element will be handed to the body steps one at a time.',
    examples: ['orders', 'rows', '[1, 2, 3]'],
    placeholder: 'orders',
  },

  // ---------- operator inputs --------------------------------------
  'binaryOp:lhs': {
    label: 'Left value',
    helper: 'The left-hand operand.',
    examples: ['a', '10', 'p.age'],
    placeholder: '0',
  },
  'binaryOp:rhs': {
    label: 'Right value',
    helper: 'The right-hand operand.',
    examples: ['b', '5', '"done"'],
    placeholder: '0',
  },
  'unaryOp:operand': {
    label: 'Value',
    helper: 'The value to operate on.',
    examples: ['x', 'true', 'count'],
    placeholder: 'x',
  },

  // ---------- access / index ---------------------------------------
  'fieldAccess:target': {
    label: 'Which struct to read from?',
    helper: 'The struct value that holds the field.',
    examples: ['p', 'order', 'response.body'],
    placeholder: 'p',
  },
  'fieldSet:target': {
    label: 'Which struct to update?',
    helper: 'The struct whose field will be written.',
    examples: ['p', 'order'],
    placeholder: 'p',
  },
  'fieldSet:value': {
    label: 'New field value',
    helper: 'The value to write into the field.',
    examples: ['"approved"', '42', 'true'],
    placeholder: '"approved"',
  },
  'indexRead:array': {
    label: 'Array to read from',
    helper: 'The array containing the element.',
    examples: ['rows', 'numbers'],
    placeholder: 'rows',
  },
  'indexRead:index': {
    label: 'Index',
    helper: 'Zero-based position of the element.',
    examples: ['0', 'i', 'rows.size() - 1'],
    placeholder: 'i',
  },
  'indexSet:array': {
    label: 'Array to write into',
    helper: 'The array whose element will be replaced.',
    examples: ['rows', 'numbers'],
    placeholder: 'rows',
  },
  'indexSet:index': {
    label: 'Index',
    helper: 'Zero-based position to write to.',
    examples: ['0', 'i'],
    placeholder: 'i',
  },
  'indexSet:value': {
    label: 'New value',
    helper: 'The value to put at that index.',
    examples: ['"done"', '42'],
    placeholder: '0',
  },

  // ---------- array / struct literal field inputs ------------------
  // Falls through to generic per-kind defaults below.
};

const KIND_FALLBACK: Partial<Record<NodeKind, (portId: string) => PortMeta>> = {
  arrayLiteral: (portId) => {
    if (portId.startsWith('item:')) {
      const i = portId.slice(5);
      return {
        label: `Item ${i}`,
        helper: 'A single array element.',
        examples: ['0', '"foo"', 'true'],
        placeholder: `item ${i}`,
      };
    }
    return {};
  },
  structLiteral: (portId) => {
    if (portId.startsWith('field:')) {
      const name = portId.slice(6);
      return {
        label: name,
        helper: `Value for the "${name}" field.`,
        examples: [],
        placeholder: name,
      };
    }
    return {};
  },
  call: (portId) => {
    if (portId.startsWith('arg:')) {
      const name = portId.slice(4);
      return {
        label: `Argument: ${name}`,
        helper: `Value passed in as the "${name}" parameter.`,
        examples: [],
        placeholder: name,
      };
    }
    return {};
  },
};

export function portMeta(kind: NodeKind, portId: string): PortMeta {
  const key = `${kind}:${portId}`;
  if (META[key]) return META[key];
  const fb = KIND_FALLBACK[kind];
  if (fb) return fb(portId);
  return {};
}
