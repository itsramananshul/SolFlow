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
  run_source_json(source: string): string;
};

interface RunEnvelope {
  ok: boolean;
  diagnostics: Array<{ severity: string; phase: string; message: string }>;
  run: {
    return_value: number | null;
    output: string[];
    runtime_error: { kind: string } | null;
    runtime_error_source_span: { start: number; end: number } | null;
    trace: Array<{ kind: string }>;
  } | null;
}

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

    // Samples advertised as runnable must actually execute end to end in
    // the canonical VM (helper-function calls included) and produce
    // output, with no runtime error and no Runtime-phase diagnostics
    // (e.g. "function 'x' not found"). This is the regression guard for
    // the helper-function runtime support.
    if (sample.runnable) {
      it(`${sample.id} — runs end to end (output, no runtime error)`, () => {
        const { source } = emit(sample.build());
        const env = JSON.parse(wasm.run_source_json(source)) as RunEnvelope;
        const runtimeDiags = (env.diagnostics ?? []).filter(
          (d) => d.phase === 'Runtime',
        );
        if (env.run?.runtime_error || runtimeDiags.length > 0) {
          throw new Error(
            `Runnable sample "${sample.id}" failed at runtime:\n  ` +
              `${env.run?.runtime_error?.kind ?? ''} ` +
              `${runtimeDiags.map((d) => d.message).join('; ')}\n\n${source}`,
          );
        }
        expect(env.run).not.toBeNull();
        expect(env.run!.runtime_error).toBeNull();
        expect(env.run!.output.length).toBeGreaterThan(0);
      });
    }

    // A provider-backed sample compiles cleanly and, in Browser Simulation
    // (no providers), blocks its external call clearly at the call site.
    if (sample.requiresProvider) {
      it(`${sample.id} — blocks its external call in Browser Simulation`, () => {
        const { source } = emit(sample.build());
        const env = JSON.parse(wasm.run_source_json(source)) as RunEnvelope;
        expect(env.ok).toBe(true);
        expect(env.run).not.toBeNull();
        expect(env.run!.runtime_error?.kind).toBe('ExtCallBlocked');
        // The block is tied to the failing call's source span.
        expect(env.run!.runtime_error_source_span).not.toBeNull();
        // The trace still shows the external call attempt.
        expect(env.run!.trace.some((s) => s.kind === 'extcall')).toBe(true);
      });
    }
  }
});
