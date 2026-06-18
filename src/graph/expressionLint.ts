/**
 * SolFlow Phase A — inline-expression linter.
 *
 * Inline expressions live in `node.expressions[portId]` and are
 * (a) emitted verbatim into the generated SOL source by `src/emit/emit.ts`,
 * and (b) evaluated by the in-browser simulator via JavaScript's
 * `new Function` constructor in `src/runtime/interpret.ts`.
 *
 * Both consumers are dangerous when the string contains unsafe content:
 *   - the SOL compiler will reject syntax it doesn't recognize
 *   - the simulator will happily execute arbitrary JavaScript
 *
 * This linter is the single gatekeeper. It runs in two places:
 *   1. `validateWorkflow` per inline expression — surfaces an error
 *      diagnostic the user sees in the DiagnosticsDrawer / Sol Man
 *      preview, and gates Apply.
 *   2. `evalInline` (simulator) just before `new Function` — refuses
 *      to evaluate. Defense in depth so a workflow that somehow
 *      reaches the simulator with a bad expression still can't
 *      execute arbitrary JS.
 *
 * Rule set is deliberately conservative. False positives are
 * preferable to false negatives — the recovery from "this expression
 * looks fine but exfiltrated your cookies" is much worse than the
 * recovery from "rewrite this expression without `Math.floor`".
 */

export interface LintError {
  /** Stable machine-readable code; matches one of the rule names below. */
  code:
    | 'lint-keyword'
    | 'lint-js-global'
    | 'lint-method-call'
    | 'lint-js-syntax'
    | 'lint-disallowed-char';
  /** Human-readable message ready for display. */
  message: string;
  /** The offending substring (for highlighting if the UI wants to). */
  offender: string;
}

/**
 * SOL keywords that are NOT valid inside an inline expression. Note that
 * `true` and `false` ARE valid (they're literal boolean expressions).
 *
 * Source: lexer.rs keyword set (`docs/sol-language/03-syntax.md` §3.2).
 */
const SOL_STATEMENT_KEYWORDS = new Set([
  'if',
  'else',
  'while',
  'for',
  'let',
  'return',
  'struct',
  'enum',
  'import',
  'function',
  'ext',
  'as',
]);

/**
 * JavaScript globals / identifiers that should never appear in an inline
 * expression. The simulator runs expressions via `new Function`, so any of
 * these names — if accessible from the function's scope — would let the
 * expression escape the SOL sandbox.
 *
 * Conservative — biased toward false-positive (rejecting a valid-looking
 * expression) rather than false-negative (allowing something dangerous).
 */
const JS_GLOBALS = new Set([
  // Network / I/O
  'fetch',
  'XMLHttpRequest',
  'WebSocket',
  'EventSource',
  'navigator',
  // Storage / DOM
  'document',
  'window',
  'globalThis',
  'localStorage',
  'sessionStorage',
  'indexedDB',
  'cookieStore',
  // Execution
  'eval',
  'Function',
  'setTimeout',
  'setInterval',
  'queueMicrotask',
  'requestAnimationFrame',
  // Module / Node
  'require',
  'process',
  'module',
  '__dirname',
  '__filename',
  // Common JS utilities — SOL has no equivalent so referencing them is a sign
  // someone wrote JS instead of SOL.
  'Math',
  'Date',
  'JSON',
  'Object',
  'Array',
  'String',
  'Number',
  'Boolean',
  'RegExp',
  'Error',
  'Symbol',
  'Promise',
  'Map',
  'Set',
  'WeakMap',
  'WeakSet',
  'Proxy',
  'Reflect',
  // User-visible UI
  'alert',
  'confirm',
  'prompt',
  'console',
]);

/**
 * JS-only operators / keywords that the SOL parser does not recognize.
 * Detected by string match on the raw expression text — good enough for
 * the common cases without parsing.
 */
const JS_SYNTAX_PATTERNS: Array<{ pattern: RegExp; offender: string; description: string }> = [
  { pattern: /\btypeof\b/, offender: 'typeof', description: '`typeof` is a JavaScript operator; SOL has no equivalent.' },
  { pattern: /\binstanceof\b/, offender: 'instanceof', description: '`instanceof` is a JavaScript operator; SOL has no equivalent.' },
  { pattern: /\bnew\b/, offender: 'new', description: '`new` is a JavaScript operator; SOL has no equivalent.' },
  { pattern: /\bdelete\b/, offender: 'delete', description: '`delete` is a JavaScript operator; SOL has no equivalent.' },
  { pattern: /\bvoid\b/, offender: 'void', description: '`void` is a JavaScript operator; SOL has no equivalent.' },
  { pattern: /=>/, offender: '=>', description: 'Arrow functions (`=>`) are JavaScript; SOL has no first-class functions.' },
  { pattern: /\?\?/, offender: '??', description: 'Nullish coalescing (`??`) is JavaScript; SOL has no equivalent.' },
  { pattern: /\?\./, offender: '?.', description: 'Optional chaining (`?.`) is JavaScript; SOL has no equivalent.' },
  { pattern: /\.\.\./, offender: '...', description: 'Spread / rest (`...`) is JavaScript; SOL has no equivalent.' },
  // Template literals (backtick strings). The exact regex would be too
  // permissive — any backtick triggers the check.
  { pattern: /`/, offender: '`', description: 'Backtick template literals are JavaScript; use plain SOL string literals with `"..."`.' },
];

/**
 * Method-call shapes: identifier dot identifier open-paren. Matches
 * `payload.amount.toFixed(2)`, `name.length()`, etc. SOL's `.` is only
 * field access; it is not callable.
 */
const METHOD_CALL_PATTERN = /\b[A-Za-z_][A-Za-z0-9_]*\s*\.\s*[A-Za-z_][A-Za-z0-9_]*\s*\(/;

/**
 * Run the lint rules in order. Returns the first matching error, or
 * `null` if every rule passes.
 *
 * The expression is examined as plain text — we do not tokenize. False
 * positives are preferable to false negatives.
 */
export function lintInlineExpression(expr: string): LintError | null {
  const trimmed = expr.trim();
  if (trimmed === '') return null; // empty = handled by the missing-input rule

  // 1. Disallowed SOL keywords. Walk word-tokens looking for any keyword
  //    that doesn't belong in an expression.
  // Exclude string/char literal contents so words inside a valid string
  // (e.g. "reminder for the meeting") are not flagged as keywords.
  const scan = trimmed
    .replace(/"(?:[^"\\]|\\.)*"/g, '""')
    .replace(/'(?:[^'\\]|\\.)*'/g, "''");
  const wordTokens = scan.match(/\b[A-Za-z_][A-Za-z0-9_]*\b/g) ?? [];
  for (const tok of wordTokens) {
    if (SOL_STATEMENT_KEYWORDS.has(tok)) {
      return {
        code: 'lint-keyword',
        offender: tok,
        message: `Inline expression contains the keyword "${tok}", which is only valid in statement position. Move the statement into a separate node, or rewrite the expression without it.`,
      };
    }
  }

  // 2. JavaScript globals. Same word-token walk against the global set.
  for (const tok of wordTokens) {
    if (JS_GLOBALS.has(tok)) {
      return {
        code: 'lint-js-global',
        offender: tok,
        message: `Inline expression references "${tok}", a JavaScript global. SOL has no equivalent; declare an \`ext function\` if you need this capability.`,
      };
    }
  }

  // 3. Method-call shape.
  const methodMatch = scan.match(METHOD_CALL_PATTERN);
  if (methodMatch) {
    return {
      code: 'lint-method-call',
      offender: methodMatch[0],
      message: `Inline expression looks like a method call ("${methodMatch[0]}"). SOL's \`.\` is field access only; methods do not exist. Rewrite as a free-function call or declare an \`ext function\`.`,
    };
  }

  // 4. JS-only syntax patterns.
  for (const { pattern, offender, description } of JS_SYNTAX_PATTERNS) {
    if (pattern.test(scan)) {
      return {
        code: 'lint-js-syntax',
        offender,
        message: description,
      };
    }
  }

  return null;
}

/**
 * Convenience for callers that want a boolean "is this safe to send to
 * the simulator / emit verbatim?" answer.
 */
export function isInlineExpressionSafe(expr: string): boolean {
  return lintInlineExpression(expr) === null;
}
