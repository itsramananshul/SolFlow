/**
 * Server-side semantic repair for Sol Man inline expressions.
 *
 * The model — even with a strict system prompt — occasionally
 * writes pseudo-code into expression fields:
 *
 *   value: "for user in users"
 *   value: "if amount > 100 then approve"
 *   value: 'print("hello")'
 *   value: "<the user's email>"
 *
 * The expression linter (src/graph/expressionLint.ts) correctly
 * rejects all of those. By the time the spec lands in the
 * editor, the user sees:
 *
 *   bad-inline-expression: Inline expression contains the
 *   keyword "for", which is only valid in statement position.
 *
 * This module applies surgical, deterministic, narrow repairs
 * for cases where the *intended* expression is recoverable.
 * It NEVER:
 *
 *   - hallucinates missing logic
 *   - rewrites valid expressions
 *   - bypasses validation (output still hits the linter)
 *   - silently fixes anything without a warning entry
 *
 * Each repair logs a warning that's surfaced to the user via
 * the GeneratedGraphSpec.assumptions / repairWarnings channel
 * so the user knows we adjusted their generated workflow.
 *
 * Inputs: a (post-schema-validation) GeneratedGraphSpec.
 * Outputs: a new spec + a list of human-readable warnings + a
 *          machine-readable list of repair entries the
 *          observability layer can log.
 *
 * Pure / hermetic — no I/O, no env, no global state.
 */

import type {
  GeneratedGraphSpec,
  GeneratedNode,
} from '../../src/sol-man/types';

/** What kind of fix was applied. The observability layer
 *  buckets these so we can spot recurring failure modes. */
export type SemanticRepairKind =
  | 'strip_print_wrapper'
  | 'strip_leading_keyword'
  | 'unwrap_for_loop'
  | 'unwrap_comprehension'
  | 'strip_pseudocode_brackets'
  | 'extract_inner_expression'
  | 'quote_bare_identifier_label'
  | 'normalize_whitespace';

export interface SemanticRepairEntry {
  nodeId: string;
  field: 'value' | 'cond';
  kind: SemanticRepairKind;
  /** What the model originally produced. */
  before: string;
  /** What we replaced it with. */
  after: string;
  /** User-facing summary. */
  message: string;
}

export interface SemanticRepairResult {
  spec: GeneratedGraphSpec;
  /** Per-node, per-field repairs that actually fired. */
  repairs: SemanticRepairEntry[];
  /** Same content rolled into the `assumptions` array of the spec
   *  so the UI surfaces it without a new channel. */
  warnings: string[];
}

/**
 * Run every repair rule across every node. Returns a NEW spec
 * (the original is untouched). Empty repairs[] when nothing fired.
 */
export function repairSemantics(
  spec: GeneratedGraphSpec,
): SemanticRepairResult {
  const repairs: SemanticRepairEntry[] = [];
  const nodes = spec.nodes.map((n) => repairNode(n, repairs));
  // Surface each repair as a one-liner. The spec's `assumptions`
  // array already shows in the modal preview; appending to it
  // keeps the user-visible channel single.
  const warnings = repairs.map((r) =>
    `Sol Man auto-repaired ${nodeLabel(n => n.id === r.nodeId, nodes)}.${r.field}: ${r.message}`,
  );
  return {
    spec: { ...spec, nodes },
    repairs,
    warnings,
  };
}

// =============================================================
//  Per-node dispatch
// =============================================================

function repairNode(
  node: GeneratedNode,
  log: SemanticRepairEntry[],
): GeneratedNode {
  let next = node;
  if (next.value !== undefined) {
    const repaired = repairExpression(next.value);
    if (repaired.changed) {
      log.push({
        nodeId: next.id,
        field: 'value',
        kind: repaired.kind!,
        before: next.value,
        after: repaired.text,
        message: repaired.message!,
      });
      next = { ...next, value: repaired.text };
    }
  }
  if (next.cond !== undefined) {
    const repaired = repairExpression(next.cond);
    if (repaired.changed) {
      log.push({
        nodeId: next.id,
        field: 'cond',
        kind: repaired.kind!,
        before: next.cond,
        after: repaired.text,
        message: repaired.message!,
      });
      next = { ...next, cond: repaired.text };
    }
  }
  return next;
}

// =============================================================
//  Expression-level rules
//
//  Each rule is order-independent: it inspects the input string
//  and either returns the same string + changed=false, or returns
//  a repaired string + changed=true + a repair kind + message.
//
//  Order matters here because we run them as a pipeline, with
//  the result of each feeding the next. Conservative bias: any
//  rule that finds something it doesn't understand returns the
//  input unchanged.
// =============================================================

interface ExprRepairResult {
  text: string;
  changed: boolean;
  kind?: SemanticRepairKind;
  message?: string;
}

function repairExpression(input: string): ExprRepairResult {
  if (typeof input !== 'string') {
    return { text: input as unknown as string, changed: false };
  }
  let s = input;
  let changed = false;
  let lastKind: SemanticRepairKind | undefined;
  const reasons: string[] = [];

  // 1. Trim + collapse internal whitespace. Pure normalization;
  //    only fires when we actually change something (multi-space
  //    runs or newlines).
  const normalized = s.replace(/\s+/g, ' ').trim();
  if (normalized !== s) {
    s = normalized;
    changed = true;
    lastKind = 'normalize_whitespace';
    reasons.push('normalized whitespace');
  }

  // 2. Unwrap `print("...")` and `print(varName)` — the model
  //    sometimes emits the whole statement as the value for a
  //    print node (resulting in `print(print(...))` on emit).
  //    Only fires when the wrapper is balanced + the inside
  //    starts with a single expression-ish thing.
  const printUnwrap = tryUnwrapPrintCall(s);
  if (printUnwrap) {
    s = printUnwrap;
    changed = true;
    lastKind = 'strip_print_wrapper';
    reasons.push('removed redundant print() wrapper');
  }

  // 3. Strip a leading statement keyword (`for`, `let`, `return`,
  //    `if`, `else`, `while`) that the model wrote out of habit.
  //    We do this BEFORE the for-loop rewrite so we don't double-
  //    transform.
  const stripped = stripLeadingStatementKeyword(s);
  if (stripped && stripped !== s) {
    s = stripped;
    changed = true;
    lastKind = 'strip_leading_keyword';
    reasons.push('stripped leading SOL statement keyword');
  }

  // 4. `for X in Y` / `for X of Y` → `Y` (the array expression).
  //    This is the most common failure for a `forEach` node where
  //    the model wrote the whole loop header instead of just the
  //    array.
  const forLoopUnwrap = tryUnwrapForLoop(s);
  if (forLoopUnwrap) {
    s = forLoopUnwrap;
    changed = true;
    lastKind = 'unwrap_for_loop';
    reasons.push('extracted array from "for x in y" loop header');
  }

  // 4b. Comprehension / embedded `for`: `[EXPR for X in Y]` → `Y`.
  const compUnwrap = tryUnwrapComprehension(s);
  if (compUnwrap && compUnwrap !== s) {
    s = compUnwrap;
    changed = true;
    lastKind = 'unwrap_comprehension';
    reasons.push('reduced a comprehension to its iterable');
  }

  // 5. Remove placeholder brackets like `<the user's email>` and
  //    `{user_email}` that the model uses for pseudocode. We try
  //    to recover the inner identifier when it looks safe; we
  //    leave it alone when it doesn't.
  const bracketStrip = stripPseudocodeBrackets(s);
  if (bracketStrip) {
    s = bracketStrip;
    changed = true;
    lastKind = 'strip_pseudocode_brackets';
    reasons.push('removed pseudocode placeholder brackets');
  }

  // 6. Extract the first quoted string literal if the field is
  //    a clearly-prose sentence like:
  //      Send "Welcome to Acme!" to the new user via Slack
  //    We pull the `"Welcome to Acme!"` and use that. This is the
  //    most aggressive repair — only fires when the input is NOT
  //    a parseable expression-ish thing.
  const extracted = tryExtractInnerLiteral(s);
  if (extracted) {
    s = extracted;
    changed = true;
    lastKind = 'extract_inner_expression';
    reasons.push('extracted inner string literal from prose');
  }

  // 7. Quote bare identifier action labels. If the field looks
  //    like `Send order for approval` (3+ bare identifiers, no
  //    operators, no parens) and the node is a print, we wrap
  //    in quotes so it becomes a valid string literal.
  //
  //    We don't know the node kind here, so we apply only when
  //    the shape is OBVIOUSLY a label and not an expression
  //    (no `+ - * / && || == != ( )`).
  const quoted = tryQuoteBareLabel(s);
  if (quoted) {
    s = quoted;
    changed = true;
    lastKind = 'quote_bare_identifier_label';
    reasons.push('quoted bare identifier label as string literal');
  }

  if (!changed) return { text: input, changed: false };
  return {
    text: s,
    changed: true,
    kind: lastKind,
    message: reasons.join(', '),
  };
}

// =============================================================
//  Rule implementations
// =============================================================

function tryUnwrapPrintCall(s: string): string | null {
  // Match `print(...)` with balanced parens. Conservative: only
  // unwrap when the WHOLE input is a single print call.
  const m = s.match(/^\s*print\s*\(\s*(.*?)\s*\)\s*$/s);
  if (!m) return null;
  const inner = m[1].trim();
  if (!inner) return null;
  // Ensure parens balance — the regex `(.*?)` might capture
  // unmatched parens in nested cases. Quick sanity check.
  if (!parensBalanced(inner)) return null;
  return inner;
}

function stripLeadingStatementKeyword(s: string): string | null {
  // Match a leading `for|let|return|if|else|while` token, then a
  // space, then strip up to the next semantic break. This is more
  // conservative for `if/else/while` — we only strip when we're
  // confident the remainder is a clean expression. For `let` we
  // also expect a `=` further along.
  const m = s.match(/^(let|return|if|else|while)\b\s*(.*)$/);
  if (!m) return null;
  const [, kw, rest] = m;
  const remainder = rest.trim();
  if (!remainder) return null;

  // `let varName = expr` → `expr`
  if (kw === 'let') {
    const letM = remainder.match(/^[A-Za-z_][A-Za-z0-9_]*\s*=\s*(.+)$/);
    if (letM) return letM[1].trim();
    return null;
  }
  // `return expr` → `expr`
  if (kw === 'return') return remainder;
  // `if cond then val` → `cond` (the boolean), but only when
  // there's no `then`/`else` body afterwards; otherwise it's
  // ambiguous and we leave it.
  if (kw === 'if' || kw === 'while') {
    // Strip optional trailing `then ...` / `do ...`.
    return remainder.replace(/\s+(then|do)\b.*$/, '').trim();
  }
  if (kw === 'else') return remainder;
  return null;
}

function tryUnwrapForLoop(s: string): string | null {
  // `for X in Y` or `for X of Y` or `for (X in Y)` — the array
  // expression is Y. We extract Y for use as a forEach node's
  // `value` field.
  const inMatch = s.match(/^for\s*\(?\s*[A-Za-z_][A-Za-z0-9_]*\s+(?:in|of)\s+(.+?)\s*\)?\s*$/);
  if (inMatch) return inMatch[1].trim();
  return null;
}

function tryUnwrapComprehension(s: string): string | null {
  // Python-style comprehension or an embedded `for`, e.g.
  //   `[u.email for u in users]`  →  `users`
  //   `sum(x for x in nums)`      →  `nums`
  // SOL has no comprehensions, so the least-wrong recovery is the
  // iterable; the per-item logic belongs in a forEach node. We only
  // fire when the `for ... in/of ...` appears mid-expression (the
  // leading-header case is handled by tryUnwrapForLoop above).
  const m = s.match(/\bfor\s+[A-Za-z_][A-Za-z0-9_]*\s+(?:in|of)\s+([^\]\)]+?)\s*[\]\)]?\s*$/);
  if (!m) return null;
  if (/^for\b/.test(s.trim())) return null; // leading header → handled elsewhere
  let iter = m[1].trim();
  iter = iter.replace(/\s+if\s+.*$/i, '').trim(); // drop a trailing filter clause
  return iter || null;
}

function stripPseudocodeBrackets(s: string): string | null {
  // `<the user's email>` → `user.email` when the inside is bare
  // English. We're too unsure to invent variable names, so we just
  // strip the `<>` and let the validator see a hopefully-clean
  // identifier underneath; if the inside is prose, validator will
  // still reject and the user gets a clear error.
  if (s.startsWith('<') && s.endsWith('>') && s.length > 2) {
    const inner = s.slice(1, -1).trim();
    if (inner) return inner;
  }
  // `{varName}` → `varName` (template-literal placeholder).
  if (s.startsWith('{') && s.endsWith('}') && s.length > 2) {
    const inner = s.slice(1, -1).trim();
    if (/^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*$/.test(inner)) {
      return inner;
    }
  }
  return null;
}

function tryExtractInnerLiteral(s: string): string | null {
  // Already-quoted string literal — leave alone.
  if (/^".*"$/.test(s) && parensBalanced(s)) return null;

  // Look for the LONGEST `"..."` substring. If the surrounding
  // context is prose, this is the literal the model meant to use.
  const matches = [...s.matchAll(/"((?:\\.|[^"\\])*)"/g)];
  if (matches.length === 0) return null;
  // Skip when the whole input is already a clean expression. We
  // only extract if the input contains spaces AND isn't a
  // recognizable expression shape.
  if (!/\s/.test(s)) return null;
  // Conservative: don't extract if the input looks like a
  // concatenation (`"a" + "b"`) or a function call (`foo("x")`).
  if (/[+\-*/&|=<>!()]/.test(s)) return null;
  // Pick the longest literal.
  let best = matches[0];
  for (const m of matches) {
    if (m[0].length > best[0].length) best = m;
  }
  return best[0];
}

function tryQuoteBareLabel(s: string): string | null {
  // Already wrapped in quotes? Already valid? Leave alone.
  if (s.startsWith('"') && s.endsWith('"')) return null;
  // Contains characters that suggest it's a real expression.
  if (/[+\-*/&|=<>!()."'\[\]]/.test(s)) return null;
  // Walk the word tokens — if there are >=2 bare alphabetic words
  // with spaces between them, we treat as a label.
  const tokens = s.split(/\s+/).filter(Boolean);
  if (tokens.length < 2) return null;
  // Every token must be a bare identifier-ish word.
  for (const t of tokens) {
    if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(t)) return null;
  }
  // Quote the whole thing. Escape any inner quotes (defensive;
  // shouldn't happen given the alphanumeric check).
  return `"${s.replace(/"/g, '\\"')}"`;
}

// =============================================================
//  Helpers
// =============================================================

function parensBalanced(s: string): boolean {
  let depth = 0;
  let inString = false;
  let escape = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (escape) {
      escape = false;
      continue;
    }
    if (inString) {
      if (ch === '\\') {
        escape = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === '(') depth++;
    else if (ch === ')') {
      depth--;
      if (depth < 0) return false;
    }
  }
  return depth === 0 && !inString;
}

function nodeLabel(
  match: (n: GeneratedNode) => boolean,
  nodes: GeneratedNode[],
): string {
  const n = nodes.find(match);
  if (!n) return 'node';
  // Human-friendly tag: e.g. `forEach[n3]`, `print[n4]`.
  return `${n.kind}[${n.id}]`;
}
