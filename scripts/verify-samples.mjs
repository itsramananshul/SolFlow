// Verifies all four samples still emit valid SOL after the schema change.
import { emit } from '../src/emit/emit.ts';
import { SAMPLES } from '../src/samples/index.ts';

let ok = true;
for (const s of SAMPLES) {
  const wf = s.build();
  const { source, warnings } = emit(wf);
  const fnCount = wf.functions.length;
  const lines = source.split('\n').length;
  const summary = `${s.id.padEnd(15)} ${fnCount} fns · ${lines} lines · ${warnings.length} warnings`;
  console.log(summary);
  if (warnings.length > 0) {
    for (const w of warnings.slice(0, 3)) console.log('  ! ' + w);
    if (warnings.length > 3) console.log(`  ... +${warnings.length - 3}`);
  }
}
console.log(ok ? '\nAll samples emit successfully.' : 'FAILED');
