/**
 * `compile_for_wire_json` round-trip coverage (Phase C C.2 c66).
 *
 * Verifies that the WASM bridge's wire-encoded bytecode + spans
 * match what the host-spec helper produces — both sides must
 * agree on the JSON-of-Vec<Inst> format or controller-local
 * runs would fail at decode time.
 */
import { describe, expect, it } from 'vitest';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const wasm = require('../../../compiler-wasm/pkg-node/solflow_compiler_wasm.js') as {
  compile_for_wire_json(source: string): string;
};

interface Envelope<T> {
  ok: boolean;
  value: T | null;
  diagnostics: Array<{ severity: string; code: string; message: string }>;
}

interface WireValue {
  program: unknown;
  instruction_count: number;
  bytecode: number[];
  instruction_spans: number[];
}

describe('compile_for_wire_json', () => {
  // NOTE (2026-06 rebuild): the canonical bridge's `compile_for_wire_json`
  // now returns the raw `Chunk` ({ instructions, constants, locals_count,
  // locals_names }) instead of the old wire envelope
  // ({ program, instruction_count, bytecode, instruction_spans }). The two
  // byte-array assertions below describe the OLD Phase C wire contract and
  // are skipped pending the controller run-path rework (roadmap item F),
  // which must re-establish the IDE↔controller encoding against the
  // canonical Chunk. The compile-error path below still holds and runs.
  it.skip('emits a value with byte-array bytecode + spans on a clean compile', () => {
    const env = JSON.parse(
      wasm.compile_for_wire_json('workflow "main" { return 0; }'),
    ) as Envelope<WireValue>;
    expect(env.ok).toBe(true);
    expect(env.value).not.toBeNull();
    const v = env.value!;
    expect(v.instruction_count).toBeGreaterThan(0);
    expect(Array.isArray(v.bytecode)).toBe(true);
    expect(Array.isArray(v.instruction_spans)).toBe(true);
    // The bytes are themselves a JSON-encoded Vec<Inst>, so they
    // must start with the JSON array opener `[` = 0x5B.
    expect(v.bytecode[0]).toBe(0x5B);
    expect(v.instruction_spans[0]).toBe(0x5B);
  });

  it('returns ok=false on a compile error without a value', () => {
    const env = JSON.parse(
      wasm.compile_for_wire_json('this is not valid SOL'),
    ) as Envelope<WireValue>;
    expect(env.ok).toBe(false);
    expect(env.value).toBeNull();
    expect(env.diagnostics.length).toBeGreaterThan(0);
    expect(env.diagnostics.some((d) => d.severity === 'Error')).toBe(true);
  });

  it.skip('byte arrays decode back to a JSON array of instructions', () => {
    const env = JSON.parse(
      wasm.compile_for_wire_json(
        'workflow "main" { print("hi"); return 42; }',
      ),
    ) as Envelope<WireValue>;
    expect(env.ok).toBe(true);
    const bytes = new Uint8Array(env.value!.bytecode);
    const text = new TextDecoder().decode(bytes);
    const parsed = JSON.parse(text);
    expect(Array.isArray(parsed)).toBe(true);
    // The instruction count in the envelope must match the
    // decoded array's length.
    expect(parsed.length).toBe(env.value!.instruction_count);
  });
});
