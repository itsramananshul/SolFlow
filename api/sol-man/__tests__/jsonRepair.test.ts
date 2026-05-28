/**
 * Tests for the JSON repair / extraction layer.
 *
 * Coverage map: each LLM-response shape we observe in the wild gets
 * one test. Failures here are user-visible regressions in Sol Man's
 * reliability — the validator can't fix what the parser refuses to
 * accept.
 */
import { describe, expect, it } from 'vitest';
import {
  closeUnterminated,
  extractOutermostObject,
  repairJson,
  stripCodeFences,
  stripTrailingCommas,
} from '../_jsonRepair';

describe('repairJson — happy + canonical paths', () => {
  it('parses clean JSON without modification', () => {
    const input = '{"a": 1, "b": [1, 2, 3]}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1, b: [1, 2, 3] });
      expect(r.modified).toBe(false);
      expect(r.log).toEqual(['parse_ok']);
    }
  });

  it('trims leading + trailing whitespace', () => {
    const r = repairJson('\n\n   {"a": 1}\n\n');
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1 });
      expect(r.log).toContain('trim');
    }
  });
});

describe('repairJson — fenced + prose extraction', () => {
  it('strips ```json fences', () => {
    const input = '```json\n{"a": 1}\n```';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1 });
      expect(r.log).toContain('strip_fences');
      expect(r.modified).toBe(true);
    }
  });

  it('strips bare ``` fences', () => {
    const input = '```\n{"a": 1, "b": "x"}\n```';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ a: 1, b: 'x' });
  });

  it('extracts JSON from prose preamble + postamble', () => {
    const input =
      'Here\'s the workflow you asked for:\n{"meta": {"name": "x"}, "nodes": []}\n\nLet me know if you\'d like adjustments!';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ meta: { name: 'x' }, nodes: [] });
      expect(r.log).toContain('extract_object');
    }
  });

  it('handles fenced JSON inside prose', () => {
    const input =
      'Sure! Here is the workflow:\n```json\n{"a": 1}\n```\nHope that helps.';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ a: 1 });
  });

  it('handles a single-line ```{...}``` fence', () => {
    const r = repairJson('```{"a":1}```');
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ a: 1 });
  });
});

describe('repairJson — common malformations', () => {
  it('repairs trailing comma before closing brace', () => {
    const input = '{"a": 1, "b": 2,}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1, b: 2 });
      expect(r.log).toContain('strip_trailing_commas');
    }
  });

  it('repairs trailing comma before closing bracket', () => {
    const input = '{"arr": [1, 2, 3,]}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ arr: [1, 2, 3] });
  });

  it('preserves commas that are NOT trailing (mid-array)', () => {
    const input = '{"a": 1, "b": 2}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1, b: 2 });
      expect(r.log).not.toContain('strip_trailing_commas');
    }
  });

  it('does not strip commas that appear inside string literals', () => {
    // The string "1, 2, 3," contains a trailing comma but it's
    // inside a quoted string — must not be touched.
    const input = '{"label": "a, b, c,", "n": 1}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ label: 'a, b, c,', n: 1 });
  });
});

describe('repairJson — truncation recovery', () => {
  it('closes a truncated object', () => {
    const input = '{"a": 1, "b": {"c": 2';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ a: 1, b: { c: 2 } });
      expect(r.log).toContain('close_unterminated');
    }
  });

  it('closes a truncated array inside an object', () => {
    const input = '{"nodes": [{"id": "n1"}, {"id": "n2"}';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ nodes: [{ id: 'n1' }, { id: 'n2' }] });
  });

  it('closes a truncated string + then unterminated object', () => {
    // Model ran out of token budget mid-key: closeUnterminated
    // first finishes the string then the surrounding object.
    const input = '{"name": "long-running-job';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ name: 'long-running-job' });
  });

  it('drops a dangling backslash before closing the string', () => {
    // Truncated mid-escape: `"foo\` would become `"foo\"` if we
    // naively appended `"`. Strip the backslash first so we close
    // cleanly.
    const input = '{"k": "foo\\';
    const r = repairJson(input);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ k: 'foo' });
  });
});

describe('repairJson — give-up cases', () => {
  it('returns RepairFailure for complete prose with no JSON', () => {
    const input = 'I cannot generate that workflow. Sorry.';
    const r = repairJson(input);
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toMatch(/JSON|Unexpected/);
      expect(r.log).toContain('parse_failed');
      expect(r.rawExcerpt).toMatch(/cannot generate/i);
    }
  });

  it('returns RepairFailure for a clearly malformed object that cannot be closed', () => {
    // Nonsense like `}}}}` has no matching opener.
    const input = '}}}}';
    const r = repairJson(input);
    expect(r.ok).toBe(false);
  });
});

describe('extractOutermostObject — internals', () => {
  it('returns the slice from first { to its matching }', () => {
    expect(extractOutermostObject('xxx{"a":1}yyy')).toBe('{"a":1}');
  });

  it('handles braces inside string literals correctly', () => {
    // The `}` inside the string should NOT close the outer object.
    expect(extractOutermostObject('{"k": "}"}')).toBe('{"k": "}"}');
  });

  it('handles escaped quotes inside string literals', () => {
    const input = '{"k": "a\\"b"}';
    expect(extractOutermostObject(input)).toBe(input);
  });

  it('returns null when no opener is present', () => {
    expect(extractOutermostObject('no json here')).toBeNull();
  });

  it('returns null when the object is unbalanced (no matching close)', () => {
    expect(extractOutermostObject('{"a": 1')).toBeNull();
  });
});

describe('stripCodeFences — internals', () => {
  it('no-op when input is bare JSON', () => {
    expect(stripCodeFences('{"a":1}')).toBe('{"a":1}');
  });

  it('strips a leading ```json tag + trailing fence', () => {
    expect(stripCodeFences('```json\n{"a":1}\n```')).toBe('{"a":1}');
  });

  it('strips bare ``` fence', () => {
    expect(stripCodeFences('```\n{"a":1}\n```')).toBe('{"a":1}');
  });

  it('handles trailing whitespace + closing fence', () => {
    expect(stripCodeFences('```json\n{"a":1}\n\n```\n   ')).toBe('{"a":1}');
  });
});

describe('stripTrailingCommas — internals', () => {
  it('strips comma before closing brace', () => {
    expect(stripTrailingCommas('{"a":1,}')).toBe('{"a":1}');
  });

  it('strips comma before closing bracket', () => {
    expect(stripTrailingCommas('[1,2,]')).toBe('[1,2]');
  });

  it('preserves comma inside strings', () => {
    expect(stripTrailingCommas('{"k":"x,y,",}')).toBe('{"k":"x,y,"}');
  });
});

describe('closeUnterminated — internals', () => {
  it('closes a truncated string + brace stack', () => {
    expect(closeUnterminated('{"a":"hello')).toBe('{"a":"hello"}');
  });

  it('closes a stack of nested brackets in reverse order', () => {
    expect(closeUnterminated('{"x":[{"y":')).toBe('{"x":[{"y":}]}');
  });

  it('no-op when the input is already balanced', () => {
    expect(closeUnterminated('{"a":1}')).toBe('{"a":1}');
  });
});
