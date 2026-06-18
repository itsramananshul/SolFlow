/**
 * Tolerant import normalization.
 *
 * Some hand-written / example SOL uses lenient syntax variants that the
 * strict canonical parser rejects but that are unambiguous in intent.
 * SolFlow normalizes these to canonical form on import so the editor
 * accepts real-world `.sol` files, then re-emits strict canonical
 * source (which the controller runs).
 *
 * Handled variants:
 *   1. Unquoted workflow names:  `workflow foo {`  ->  `workflow "foo" {`
 *      (the canonical parser requires a string-literal name).
 *   2. Parens-less conditions:   `if x < 3 {`      ->  `if (x < 3) {`
 *                                `while x < 3 {`   ->  `while (x < 3) {`
 *      (the canonical parser requires `if (cond)` / `while (cond)`).
 *
 * Conservative by design: each rule skips inputs already in canonical
 * form, and operates line-by-line outside `#` comments so we never
 * touch comment text. Strings on a code line are a known edge (a `{`
 * inside a string literal on an `if` line would confuse rule 2); none
 * of the OpenPrem examples hit it, and the worst case is a parse error
 * surfaced to the user, not silent corruption.
 */

export function normalizeImportSource(src: string): string {
  return src
    .split('\n')
    .map((line) => normalizeLine(line))
    .join('\n');
}

function normalizeLine(line: string): string {
  // Leave full-line and trailing comments untouched: only rewrite the
  // code portion before a `#`.
  const hashAt = indexOfCommentHash(line);
  const code = hashAt === -1 ? line : line.slice(0, hashAt);
  const comment = hashAt === -1 ? '' : line.slice(hashAt);

  let out = code;
  // 1. Unquoted workflow name -> quoted.
  out = out.replace(/\bworkflow\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/g, 'workflow "$1" {');
  // 2. Parens-less if/while condition -> parenthesized. The negative
  //    lookahead skips conditions that already start with `(`.
  out = out.replace(/\b(if|while)\s+(?!\()([^{]+?)\s*\{/g, '$1 ($2) {');

  return out + comment;
}

/** Index of the `#` that starts a comment, ignoring `#` inside strings. */
function indexOfCommentHash(line: string): number {
  let inStr = false;
  let quote = '';
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') i++;
      else if (c === quote) inStr = false;
    } else if (c === '"' || c === "'") {
      inStr = true;
      quote = c;
    } else if (c === '#') {
      return i;
    }
  }
  return -1;
}
