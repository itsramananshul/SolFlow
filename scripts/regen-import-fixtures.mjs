// Regenerate src/graph/import/__fixtures__/*.ast.json by parsing each
// .sol fixture through the canonical Node-target WASM bridge and saving
// the returned `value` (the Program AST). Run after the bridge or the
// canonical AST shape changes.
//
//   node scripts/regen-import-fixtures.mjs
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const wasm = require('../compiler-wasm/pkg-node/solflow_compiler_wasm.js');

const FIXTURES_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  'src',
  'graph',
  'import',
  '__fixtures__',
);

const solFiles = readdirSync(FIXTURES_DIR).filter((f) => f.endsWith('.sol'));
let failures = 0;
for (const file of solFiles) {
  const name = file.replace(/\.sol$/, '');
  const source = readFileSync(join(FIXTURES_DIR, file), 'utf-8');
  const env = JSON.parse(wasm.parse_source_json(source));
  if (!env.ok) {
    failures++;
    console.error(`FAIL ${name}: ${env.diagnostics.map((d) => d.message).join('; ')}`);
    continue;
  }
  const out = join(FIXTURES_DIR, `${name}.ast.json`);
  writeFileSync(out, JSON.stringify(env.value, null, 2) + '\n', 'utf-8');
  console.log(`ok   ${name} (${env.value.items.length} top-level items)`);
}
process.exit(failures > 0 ? 1 : 0);
