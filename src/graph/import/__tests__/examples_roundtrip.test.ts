/**
 * Corpus test: round-trip every committed OpenPrem example .sol
 * through SolFlow's tolerant import + emit against the canonical
 * engine.
 *
 *   source -> normalize -> parse(wasm) -> importProgram -> emit -> parse(wasm)
 *
 * Guarantees every shipped example imports cleanly and re-emits valid
 * canonical SOL. Fixtures live under __fixtures__/examples (copied
 * from the OpenPrem repo) so this is self-contained.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import type { Program } from '@/compiler/ast';
import type { CompileEnvelope } from '@/compiler/types';
import { importProgram } from '../importer';
import { normalizeImportSource } from '../normalize';
import { emit } from '@/emit/emit';

const require = createRequire(import.meta.url);
const wasm = require('../../../../compiler-wasm/pkg-node/solflow_compiler_wasm.js') as {
  parse_source_json(s: string): string;
  compile_source_json(s: string): string;
};

const EX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__', 'examples');

function findSol(dir: string): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) out.push(...findSol(p));
    else if (e.endsWith('.sol')) out.push(p);
  }
  return out;
}

function parse(src: string): CompileEnvelope<Program> {
  return JSON.parse(wasm.parse_source_json(src)) as CompileEnvelope<Program>;
}

describe('OpenPrem examples — import + round-trip', () => {
  const files = findSol(EX);
  for (const f of files) {
    const name = f.slice(EX.length + 1).replace(/\\/g, '/');
    it(`${name}`, () => {
      const src = normalizeImportSource(readFileSync(f, 'utf8'));
      const p1 = parse(src);
      if (!p1.ok || !p1.value) {
        // Parser rejected the raw example — record which + why.
        throw new Error(`PARSE: ${p1.diagnostics.map((d) => d.message).join('; ')}`);
      }
      const { workflow } = importProgram(p1.value, { name });
      const emitted = emit(workflow).source;
      const p2 = parse(emitted);
      if (!p2.ok) {
        throw new Error(`RE-PARSE: ${p2.diagnostics.map((d) => d.message).join('; ')}\n${emitted}`);
      }
      // The emitted canonical source must also compile, so the editor
      // can submit it to a controller and have it run.
      const comp = JSON.parse(wasm.compile_source_json(emitted)) as CompileEnvelope<unknown>;
      if (!comp.ok) {
        throw new Error(`COMPILE: ${comp.diagnostics.map((d) => d.message).join('; ')}\n${emitted}`);
      }
      expect(comp.ok).toBe(true);
    });
  }
});
