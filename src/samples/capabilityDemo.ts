/**
 * Sample: "Capability Call" — a workflow that calls an external provider.
 *
 * Uses the first class `action` node to emit
 * `call("demo.add", { a: 20, b: 22 })`, stores the provider's result, and
 * returns it. This RUNS for real on a Local (or Cloud) Controller that has
 * a provider registered for the `demo` module (see the bundled
 * demo-connector and docs/dev/CONNECTORS.md). In Browser Simulation the
 * external call is blocked, by design, with a clear error at the call site.
 *
 * Emits:
 *   workflow "capability-demo" <- int {
 *     let sum: int = call("demo.add", { a: 20, b: 22 });
 *     print(sum);
 *     return sum;
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

export function buildCapabilityDemo() {
  const b = createBuilder('capability-demo');
  addFunction(b, 'capability-demo', [], { kind: 'int' }, true);
  const start = getStart(b);

  // The external capability call. It produces a value, so it is wired as
  // data into the `let` (not placed in the control-flow chain); the let
  // emits `let sum: int = call("demo.add", { a: 20, b: 22 });`. Params ride
  // inline on the params port; the result leaves the return port.
  const action = node(b, 'action', { x: 80, y: 200 }, {
    kind: 'action',
    capability: 'demo.add',
  });
  action.expressions = { params: '{ a: 20, b: 22 }' };

  const letSum = node(b, 'let', { x: 360, y: 80 }, {
    kind: 'let',
    varName: 'sum',
    varType: { kind: 'int' },
  });

  const getSum1 = node(b, 'varGet', { x: 360, y: 300 }, {
    kind: 'varGet',
    varName: 'sum',
    resolvedType: { kind: 'int' },
  });
  const printSum = node(b, 'print', { x: 620, y: 220 });

  const getSum2 = node(b, 'varGet', { x: 360, y: 440 }, {
    kind: 'varGet',
    varName: 'sum',
    resolvedType: { kind: 'int' },
  });
  const ret = node(b, 'return', { x: 620, y: 400 }, { kind: 'return', hasValue: true });

  // Control flow: start -> let -> print -> return. The capability call is
  // evaluated as the let's right-hand side, not as its own statement.
  ctl(b, start, 'next', letSum, 'prev');
  ctl(b, letSum, 'next', printSum, 'prev');
  ctl(b, printSum, 'next', ret, 'prev');

  // Data flow: the provider result feeds the let; sum feeds print + return.
  dat(b, action, 'return', letSum, 'value');
  dat(b, getSum1, 'value', printSum, 'value');
  dat(b, getSum2, 'value', ret, 'value');

  return finalize(b);
}
