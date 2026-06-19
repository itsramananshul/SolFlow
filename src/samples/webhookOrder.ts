/**
 * Sample: "Webhook Order" — a payload-driven workflow.
 *
 * It reads `payload.total` (the event data a webhook/trigger delivers),
 * prints it, and returns it. Running it manually needs a test payload, so
 * this sample ships one (`{ "total": 1200 }`) and is badged "Needs test
 * payload". With that payload provided in the Run panel it runs end to end
 * in both Browser Simulation and the Local Controller.
 *
 * Emits:
 *   workflow "webhook-order" <- int {
 *     let total: int = payload.total;
 *     print(total);
 *     return total;
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

export function buildWebhookOrder() {
  const b = createBuilder('webhook-order');
  addFunction(b, 'webhook-order', [], { kind: 'int' }, true);
  const start = getStart(b);

  // A webhook trigger documents where the payload comes from.
  const trigger = node(b, 'trigger', { x: 80, y: 60 }, {
    kind: 'trigger',
    triggerKind: 'webhook',
    eventName: 'order.received',
    webhookPath: '/webhooks/order',
    payloadSchema: '{ "total": "number" }',
    samplePayload: '{\n  "total": 1200\n}',
  });

  // let total: int = payload.total;  (inline expression reads the payload)
  const letTotal = node(b, 'let', { x: 360, y: 80 }, {
    kind: 'let',
    varName: 'total',
    varType: { kind: 'int' },
  });
  letTotal.expressions = { value: 'payload.total' };

  const getTotal1 = node(b, 'varGet', { x: 360, y: 280 }, {
    kind: 'varGet',
    varName: 'total',
    resolvedType: { kind: 'int' },
  });
  const printTotal = node(b, 'print', { x: 620, y: 220 });

  const getTotal2 = node(b, 'varGet', { x: 360, y: 420 }, {
    kind: 'varGet',
    varName: 'total',
    resolvedType: { kind: 'int' },
  });
  const ret = node(b, 'return', { x: 620, y: 400 }, { kind: 'return', hasValue: true });

  // Control flow: start -> let -> print -> return.
  ctl(b, start, 'next', letTotal, 'prev');
  ctl(b, letTotal, 'next', printTotal, 'prev');
  ctl(b, printTotal, 'next', ret, 'prev');

  // Data flow: total feeds print + return.
  dat(b, getTotal1, 'value', printTotal, 'value');
  dat(b, getTotal2, 'value', ret, 'value');

  return finalize(b);
}
