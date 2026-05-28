/**
 * JSON repair / extraction layer for Sol Man.
 *
 * LLM responses we want to handle (in order of severity):
 *
 *   1. Clean JSON          { ... }                              — passes through
 *   2. Fenced JSON         ```json\n{...}\n```                  — strip fences
 *   3. Prose + JSON        Here's the workflow: { ... } Hope... — extract object
 *   4. Trailing commas     { "a": 1, }                          — strip commas
 *   5. Truncated object    { "nodes": [ { "id": "n1" — close braces / quotes / brackets to recover
 *   6. Complete garbage    "I cannot do that."                  — give up, structured failure
 *
 * Returns a single discriminated result so callers can log what
 * happened (which steps fired) and decide whether to retry.
 *
 * Importantly: every step is idempotent. Re-running `repair()` on
 * already-clean JSON returns it unchanged.
 *
 * NOT used: JSON5, eval, Function constructor. We never execute
 * the LLM output. Repair is string surgery only.
 */

export type RepairLogStep =
  | 'trim'
  | 'strip_fences'
  | 'extract_object'
  | 'strip_trailing_commas'
  | 'close_unterminated'
  | 'parse_ok'
  | 'parse_failed';

export interface RepairResult {
  ok: true;
  value: unknown;
  /** Each step that actually changed the string (or was the
   *  successful parse). Surfaced into the error / success envelope
   *  for diagnostics. */
  log: RepairLogStep[];
  /** True iff any repair step modified the input. False when the
   *  LLM already returned clean JSON. */
  modified: boolean;
}

export interface RepairFailure {
  ok: false;
  /** Why the repair gave up — the final parse error message. */
  error: string;
  log: RepairLogStep[];
  /** First ~200 chars of the un-parsed input, for the error envelope. */
  rawExcerpt: string;
}

/**
 * Attempt to extract + parse a JSON object from arbitrary LLM
 * text. Returns the parsed value on success; on failure returns a
 * RepairFailure with the log of attempted steps + an excerpt.
 *
 * Caller is responsible for schema validation; this function only
 * guarantees the output is *valid JSON*.
 */
export function repairJson(input: string): RepairResult | RepairFailure {
  const log: RepairLogStep[] = [];

  // 0. Initial trim — almost always needed; tracked so we know when
  //    the LLM emitted any leading/trailing whitespace.
  let s = input.trim();
  if (s !== input) log.push('trim');

  // 1. Quick path — try parsing as-is BEFORE any surgery. Many
  //    requests already return clean JSON; we should not pay the
  //    repair cost for them or pollute the log with steps we didn't
  //    need.
  const direct = tryParse(s);
  if (direct.ok) {
    log.push('parse_ok');
    return { ok: true, value: direct.value, log, modified: log.length > 1 };
  }

  // 2. Strip code fences. Models still wrap output in ```json … ```
  //    despite the prompt saying not to. Generic implementation
  //    handles ```json, ```js, bare ``` — any language tag.
  const beforeFences = s;
  s = stripCodeFences(s);
  if (s !== beforeFences) log.push('strip_fences');

  // 3. Extract the outermost JSON object. Slices off prose preamble
  //    ("Here's the workflow you asked for:") and postamble ("Let
  //    me know if you'd like adjustments."). Works by finding the
  //    first `{` and the matching `}` accounting for string
  //    literals.
  const extracted = extractOutermostObject(s);
  if (extracted !== null && extracted !== s) {
    s = extracted;
    log.push('extract_object');
  }

  // Try parsing after extraction — this fixes the majority of the
  // common cases (fenced JSON, prose + JSON).
  const afterExtract = tryParse(s);
  if (afterExtract.ok) {
    log.push('parse_ok');
    return { ok: true, value: afterExtract.value, log, modified: true };
  }

  // 4. Strip trailing commas. Some models emit `,` before `}` or `]`
  //    which is valid JS but invalid JSON.
  const beforeCommas = s;
  s = stripTrailingCommas(s);
  if (s !== beforeCommas) log.push('strip_trailing_commas');

  const afterCommas = tryParse(s);
  if (afterCommas.ok) {
    log.push('parse_ok');
    return { ok: true, value: afterCommas.value, log, modified: true };
  }

  // 5. Close unterminated objects / arrays / strings. This recovers
  //    truncated responses: the model ran out of token budget mid-
  //    object. We close every open brace / bracket / quote in
  //    reverse-stack order. Lossy by design — we lose whatever the
  //    model would have written next — but recovers the prefix.
  const beforeClose = s;
  s = closeUnterminated(s);
  if (s !== beforeClose) log.push('close_unterminated');

  const afterClose = tryParse(s);
  if (afterClose.ok) {
    log.push('parse_ok');
    return { ok: true, value: afterClose.value, log, modified: true };
  }

  log.push('parse_failed');
  return {
    ok: false,
    error: afterClose.error,
    log,
    rawExcerpt: input.slice(0, 200).replace(/\s+/g, ' ').trim(),
  };
}

// =============================================================
//  Step implementations
// =============================================================

function tryParse(s: string): { ok: true; value: unknown } | { ok: false; error: string } {
  try {
    return { ok: true, value: JSON.parse(s) };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
}

/**
 * Strip a leading ``` fence (with optional language tag) and a
 * trailing ``` fence. No-op when the input isn't fenced.
 */
export function stripCodeFences(input: string): string {
  let s = input.trim();
  if (!s.startsWith('```')) return s;
  const firstNL = s.indexOf('\n');
  if (firstNL === -1) {
    // Single-line fence — degenerate but handle: ```{"a":1}```
    if (s.endsWith('```')) {
      s = s.slice(3, -3);
    }
    return s.trim();
  }
  s = s.slice(firstNL + 1);
  if (s.endsWith('```')) s = s.slice(0, -3);
  return s.trim();
}

/**
 * Find the first `{` and walk forward to the matching `}`,
 * respecting string literals + escape sequences. Returns the
 * substring `{...}` or null if no balanced object is present.
 *
 * Does NOT attempt to handle multiple top-level objects — Sol
 * Man's schema is a single root object. Anything outside the
 * first balanced object is treated as discardable prose.
 */
export function extractOutermostObject(input: string): string | null {
  const start = input.indexOf('{');
  if (start === -1) return null;
  let depth = 0;
  let inString = false;
  let escape = false;
  for (let i = start; i < input.length; i++) {
    const ch = input[i];
    if (escape) {
      escape = false;
      continue;
    }
    if (ch === '\\' && inString) {
      escape = true;
      continue;
    }
    if (ch === '"') {
      inString = !inString;
      continue;
    }
    if (inString) continue;
    if (ch === '{') depth++;
    else if (ch === '}') {
      depth--;
      if (depth === 0) {
        return input.slice(start, i + 1);
      }
    }
  }
  // Unbalanced — caller falls through to closeUnterminated.
  return null;
}

/**
 * Strip `,` followed by whitespace + `}` or `]`. Valid JS, invalid
 * JSON. Lossless for valid input (a comma in the middle of an
 * array or object stays).
 */
export function stripTrailingCommas(input: string): string {
  // Run a state machine instead of a regex so we don't accidentally
  // touch commas inside string literals.
  let out = '';
  let inString = false;
  let escape = false;
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (inString) {
      if (escape) {
        escape = false;
      } else if (ch === '\\') {
        escape = true;
      } else if (ch === '"') {
        inString = false;
      }
      out += ch;
      continue;
    }
    if (ch === '"') {
      inString = true;
      out += ch;
      continue;
    }
    if (ch === ',') {
      // Look ahead through whitespace for the next non-space char.
      let j = i + 1;
      while (j < input.length && /\s/.test(input[j])) j++;
      if (j < input.length && (input[j] === '}' || input[j] === ']')) {
        // Skip the comma; preserve any whitespace before the closer
        // so line numbers stay stable for diagnostics.
        continue;
      }
    }
    out += ch;
  }
  return out;
}

/**
 * Close unterminated strings, arrays, and objects in stack order
 * so a truncated response becomes parseable.
 *
 * Strategy:
 *   - Walk the input, tracking the structural-token stack.
 *   - When we hit EOF inside a string, close it with `"`.
 *   - Then emit the matching closer for every still-open `{` / `[`.
 *
 * Lossy: we discard whatever the model would have written next,
 * but we recover everything before the truncation point.
 */
export function closeUnterminated(input: string): string {
  const stack: Array<'{' | '['> = [];
  let inString = false;
  let escape = false;
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (inString) {
      if (escape) {
        escape = false;
      } else if (ch === '\\') {
        escape = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }
    if (ch === '"') {
      inString = true;
      continue;
    }
    if (ch === '{' || ch === '[') {
      stack.push(ch);
    } else if (ch === '}' || ch === ']') {
      const open = stack.pop();
      // Unbalanced close — preserve the close in the output anyway
      // (matches what tryParse will see) but don't pop a phantom
      // entry off an empty stack. JSON.parse will reject it; that's
      // fine — caller falls through to parse_failed.
      if (open === undefined) {
        // No-op: stack stays empty.
      }
    }
  }

  // If we ended inside a string, an unescaped backslash at EOF
  // would otherwise produce an invalid `\"` when we close. Strip
  // a dangling backslash before adding the closing quote.
  let trimmed = input;
  if (inString) {
    if (trimmed.endsWith('\\')) trimmed = trimmed.slice(0, -1);
    trimmed += '"';
  }

  // Close every open structural token in reverse.
  while (stack.length > 0) {
    const open = stack.pop();
    trimmed += open === '{' ? '}' : ']';
  }
  return trimmed;
}
