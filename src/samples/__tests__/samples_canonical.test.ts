/**
 * Productization gate (Prod c50): every shipped sample must
 * emit SOL that's clean against the canonical compiler.
 *
 * The samples appear on the welcome screen as the new-user
 * entry points. If one regresses (emits SOL the parser or
 * analyzer rejects), the whole first-run experience breaks
 * silently — the user clicks the sample, the canvas loads,
 * looks fine, then Run produces compile errors. This test
 * catches that at CI time.
 */
import { describe, expect, it } from 'vitest';
import { createRequire } from 'node:module';
import { SAMPLES } from '..';
import { emit } from '@/emit/emit';

interface CompileEnvelope<T> {
  ok: boolean;
  value: T | null;
  diagnostics: Array<{
    code: string;
    severity: string;
    phase: string;
    message: string;
  }>;
}

// Use the Node-target WASM to run the canonical compiler in
// tests (same approach as the e2e_round_trip suite).
const require = createRequire(import.meta.url);
const wasm = require('../../../compiler-wasm/pkg-node/solflow_compiler_wasm.js') as {
  parse_source_json(source: string): string;
  analyze_source_json(source: string): string;
};

describe('Prod c50 — sample workflows compile cleanly via canonical compiler', () => {
  for (const sample of SAMPLES) {
    it(`${sample.id} — ${sample.name}: emit + parse clean`, () => {
      const workflow = sample.build();
      const { source, warnings } = emit(workflow);

      // The emit pipeline shouldn't issue warnings on the
      // curated samples. (Warnings come from things like
      // dropped edges or missing required inputs.)
      expect(warnings).toEqual([]);

      const env = JSON.parse(
        wasm.parse_source_json(source),
      ) as CompileEnvelope<unknown>;
      if (!env.ok) {
        const messages = env.diagnostics
          .filter((d) => d.severity === 'Error')
          .map((d) => `[${d.code}] ${d.phase}: ${d.message}`)
          .join('\n  ');
        throw new Error(
          `Sample "${sample.id}" emitted unparseable SOL:\n  ${messages}\n\nFull source:\n${source}`,
        );
      }
      expect(env.ok).toBe(true);
    });

    it(`${sample.id} — analyzer-clean (no semantic errors)`, () => {
      const workflow = sample.build();
      const { source } = emit(workflow);
      const env = JSON.parse(
        wasm.analyze_source_json(source),
      ) as CompileEnvelope<unknown>;
      // Samples may produce WARNINGS (notes about deprecated
      // patterns etc.) but should have ZERO Error-severity
      // diagnostics from the analyzer.
      const errors = env.diagnostics.filter(
        (d) => d.severity === 'Error',
      );
      if (errors.length > 0) {
        const messages = errors
          .map((d) => `[${d.code}] ${d.phase}: ${d.message}`)
          .join('\n  ');
        throw new Error(
          `Sample "${sample.id}" failed semantic analysis:\n  ${messages}\n\nFull source:\n${source}`,
        );
      }
      expect(errors).toEqual([]);
    });
  }
});
