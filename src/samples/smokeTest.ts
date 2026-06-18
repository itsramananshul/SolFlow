/**
 * Sample: "Smoke Test" — the known-good, self-contained workflow.
 *
 * Everything lives in the workflow body: direct `print` calls, a `let`,
 * and a `return`. It does NOT call helper functions or external
 * capabilities, so it runs end to end in Browser Simulation and through
 * a controller with no providers configured. Use it to confirm the run
 * path works before reaching for the larger samples.
 *
 * Emits:
 *   workflow "smoke-test" <- int {
 *     print("Smoke test: running on the canonical SOL VM.");
 *     let answer: int = 42;
 *     print("Smoke test: answer computed.");
 *     print("Smoke test: done.");
 *     return answer;
 *   }
 */
import {
  addFunction,
  createBuilder,
  ctl,
  dat,
  finalize,
  getStart,
  node,
} from './builders';

export function buildSmokeTest() {
  const b = createBuilder('smoke-test');
  // One function, marked as the runnable workflow (isWorkflow = true).
  addFunction(b, 'smoke-test', [], { kind: 'int' }, true);
  const start = getStart(b);

  const lit1 = node(b, 'literal', { x: 80, y: 220 }, {
    kind: 'literal',
    litType: 'str',
    value: 'Smoke test: running on the canonical SOL VM.',
  });
  const print1 = node(b, 'print', { x: 340, y: 60 });

  const lit42 = node(b, 'literal', { x: 80, y: 360 }, {
    kind: 'literal',
    litType: 'int',
    value: '42',
  });
  const letAnswer = node(b, 'let', { x: 340, y: 200 }, {
    kind: 'let',
    varName: 'answer',
    varType: { kind: 'int' },
  });

  const lit2 = node(b, 'literal', { x: 80, y: 480 }, {
    kind: 'literal',
    litType: 'str',
    value: 'Smoke test: answer computed.',
  });
  const print2 = node(b, 'print', { x: 340, y: 340 });

  const lit3 = node(b, 'literal', { x: 80, y: 600 }, {
    kind: 'literal',
    litType: 'str',
    value: 'Smoke test: done.',
  });
  const print3 = node(b, 'print', { x: 340, y: 480 });

  const getAnswer = node(b, 'varGet', { x: 340, y: 620 }, {
    kind: 'varGet',
    varName: 'answer',
    resolvedType: { kind: 'int' },
  });
  const ret = node(b, 'return', { x: 620, y: 60 }, { kind: 'return', hasValue: true });

  // Control flow: start -> print -> let -> print -> print -> return.
  ctl(b, start, 'next', print1, 'prev');
  ctl(b, print1, 'next', letAnswer, 'prev');
  ctl(b, letAnswer, 'next', print2, 'prev');
  ctl(b, print2, 'next', print3, 'prev');
  ctl(b, print3, 'next', ret, 'prev');

  // Data flow.
  dat(b, lit1, 'value', print1, 'value');
  dat(b, lit42, 'value', letAnswer, 'value');
  dat(b, lit2, 'value', print2, 'value');
  dat(b, lit3, 'value', print3, 'value');
  dat(b, getAnswer, 'value', ret, 'value');

  return finalize(b);
}
