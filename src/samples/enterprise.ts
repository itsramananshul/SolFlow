/**
 * Large-workflow stress-test sample.
 *
 * 40+ nodes covering every primitive: triggers, branches, loops,
 * notes, frames, function calls. Used as the readability/navigation
 * benchmark — if the editor stays comprehensible on this, it stays
 * comprehensible on a real production orchestration.
 *
 * Logical regions (each wrapped in a Frame):
 *   1. INTAKE       — Webhook trigger, parse + validate, branch invalid
 *   2. ENRICHMENT   — for-each loop over line items
 *   3. RISK CHECK   — score, branch high-risk vs OK
 *   4. DISPATCH     — call out to fulfillment + notification helpers
 *   5. WRAP-UP      — return / log
 */

import {
  createBuilder,
  addEnum,
  addStruct,
  addFunction,
  node,
  ctl,
  finalize,
  getFn,
  setActiveFn,
} from './builders';
import type { SolWorkflow } from '@/graph/schema';

export function buildEnterprise(): SolWorkflow {
  const b = createBuilder('Order Processing Pipeline');
  b.workflow.meta.description =
    'A larger demo: webhook → validate → enrich → score → dispatch → notify.';

  addStruct(b, 'Order', [
    { name: 'id', type: { kind: 'str' } },
    { name: 'amount', type: { kind: 'float' } },
    { name: 'status', type: { kind: 'str' } },
  ]);
  addEnum(b, 'RiskLevel', [
    { name: 'Low', value: null },
    { name: 'Medium', value: null },
    { name: 'High', value: null },
  ]);

  // --- helper: scoreOrder() ---
  const score = addFunction(b, 'scoreOrder', [], { kind: 'int' }, false);
  const scoreId = score.id;
  node(b, 'return', { x: 240, y: 60 }, { kind: 'return', hasValue: true });

  // --- helper: sendNotification() ---
  const notify = addFunction(b, 'sendNotification', [], { kind: 'void' }, false);
  const notifyId = notify.id;
  node(b, 'print', { x: 240, y: 60 });

  // --- helper: dispatchFulfillment() ---
  const dispatch = addFunction(b, 'dispatchFulfillment', [], { kind: 'void' }, false);
  const dispatchId = dispatch.id;
  node(b, 'print', { x: 240, y: 60 });

  // --- main start() function ---
  const main = addFunction(b, 'start');
  setActiveFn(b, main.id);
  const fn = getFn(b);

  // Override the auto-placed Start so we can position it inside the
  // intake frame. The startNode is already in fn.nodes from addFunction.
  const startNode = fn.nodes.find((n) => n.data.kind === 'start')!;
  startNode.position = { x: 200, y: 120 };

  // ===== Frames (rendered behind nodes via zIndex: -1) =====
  node(b, 'frame', { x: 80, y: 40 }, {
    kind: 'frame',
    title: 'INTAKE',
    width: 760,
    height: 280,
  });
  node(b, 'frame', { x: 80, y: 360 }, {
    kind: 'frame',
    title: 'ENRICHMENT',
    width: 760,
    height: 240,
  });
  node(b, 'frame', { x: 80, y: 640 }, {
    kind: 'frame',
    title: 'RISK CHECK',
    width: 760,
    height: 260,
  });
  node(b, 'frame', { x: 80, y: 940 }, {
    kind: 'frame',
    title: 'DISPATCH',
    width: 760,
    height: 240,
  });
  node(b, 'frame', { x: 80, y: 1220 }, {
    kind: 'frame',
    title: 'WRAP-UP',
    width: 760,
    height: 200,
  });

  // ===== Notes =====
  node(b, 'note', { x: 560, y: 80 }, {
    kind: 'note',
    text: 'Webhook receives orders from\nthe storefront. Sample payload\nis used for testing only.',
  });
  node(b, 'note', { x: 560, y: 680 }, {
    kind: 'note',
    text: 'Risk score is computed by\nthe scoreOrder helper. High\nscores branch to manual review.',
  });
  node(b, 'note', { x: 560, y: 1260 }, {
    kind: 'note',
    text: 'Always notify the customer\nat the end — success or failure.',
  });

  // ===== INTAKE region =====
  const trigger = node(b, 'trigger', { x: 120, y: 100 }, {
    kind: 'trigger',
    triggerKind: 'webhook',
    eventName: 'order.received',
    payloadSchema: '{ "type": "object" }',
    samplePayload: '{\n  "order_id": "ord_123",\n  "amount": 250.0\n}',
    webhookPath: '/webhooks/orders',
  });
  const letOrderId = node(b, 'let', { x: 360, y: 100 }, {
    kind: 'let',
    varName: 'orderId',
    varType: { kind: 'str' },
  });
  letOrderId.expressions = { value: '"ord_123"' };
  const letAmount = node(b, 'let', { x: 360, y: 200 }, {
    kind: 'let',
    varName: 'amount',
    varType: { kind: 'float' },
  });
  letAmount.expressions = { value: '250.0' };
  const branchValid = node(b, 'branch', { x: 600, y: 150 }, {
    kind: 'branch',
    hasElse: true,
  });
  branchValid.expressions = { cond: 'amount > 0' };

  // ===== ENRICHMENT region =====
  const loop = node(b, 'forEach', { x: 200, y: 420 }, {
    kind: 'forEach',
    iteratorName: 'item',
    iteratorType: { kind: 'int' },
  });
  loop.expressions = { array: '[1, 2, 3, 4, 5]' };
  // T9023/T9005 — `str + str` is not valid SOL. The simulator accepts
  // it (JS-style concatenation) but the canonical analyzer rejects it
  // with E1006. Print the variable directly; the execution timeline
  // shows the context. To compose strings, declare an `ext function`
  // that the host implements.
  const enrichPrint = node(b, 'print', { x: 460, y: 420 });
  enrichPrint.expressions = { value: 'item' };
  const letEnriched = node(b, 'let', { x: 200, y: 540 }, {
    kind: 'let',
    varName: 'enriched',
    varType: { kind: 'bool' },
  });
  letEnriched.expressions = { value: 'true' };

  // ===== RISK CHECK region =====
  const callScore = node(b, 'call', { x: 200, y: 700 }, {
    kind: 'call',
    functionId: scoreId,
  });
  const letRisk = node(b, 'let', { x: 200, y: 800 }, {
    kind: 'let',
    varName: 'riskScore',
    varType: { kind: 'int' },
  });
  letRisk.expressions = { value: '42' };
  const branchRisk = node(b, 'branch', { x: 460, y: 760 }, {
    kind: 'branch',
    hasElse: true,
  });
  branchRisk.expressions = { cond: 'riskScore > 70' };
  // T9023/T9005 — see comment above; print constant strings instead of
  // concatenating. Pair each with a separate varGet → print if the
  // user wants the orderId visible.
  const printHighRisk = node(b, 'print', { x: 700, y: 700 });
  printHighRisk.expressions = { value: '"HIGH RISK"' };
  const printLowRisk = node(b, 'print', { x: 700, y: 800 });
  printLowRisk.expressions = { value: '"order cleared"' };

  // ===== DISPATCH region =====
  const callDispatch = node(b, 'call', { x: 200, y: 1000 }, {
    kind: 'call',
    functionId: dispatchId,
  });
  const printDispatched = node(b, 'print', { x: 460, y: 1000 });
  // T9023/T9005 — see enrichPrint comment.
  printDispatched.expressions = { value: '"order dispatched"' };

  // ===== WRAP-UP region =====
  const callNotify = node(b, 'call', { x: 200, y: 1280 }, {
    kind: 'call',
    functionId: notifyId,
  });
  const finalPrint = node(b, 'print', { x: 460, y: 1280 });
  // T9023/T9005 — see enrichPrint comment.
  finalPrint.expressions = { value: '"workflow complete"' };
  const ret = node(b, 'return', { x: 700, y: 1280 }, {
    kind: 'return',
    hasValue: false,
  });

  // ===== Wire the spine =====
  ctl(b, trigger, 'next', letOrderId, 'prev');
  ctl(b, letOrderId, 'next', letAmount, 'prev');
  ctl(b, letAmount, 'next', branchValid, 'prev');
  // valid path → into enrichment
  ctl(b, branchValid, 'then', loop, 'prev');
  // invalid path → straight to wrap-up notify
  ctl(b, branchValid, 'else', callNotify, 'prev');
  // enrichment loop body
  ctl(b, loop, 'body', enrichPrint, 'prev');
  ctl(b, loop, 'after', letEnriched, 'prev');
  // enrichment → risk
  ctl(b, letEnriched, 'next', callScore, 'prev');
  ctl(b, callScore, 'next', letRisk, 'prev');
  ctl(b, letRisk, 'next', branchRisk, 'prev');
  // branch risk paths
  ctl(b, branchRisk, 'then', printHighRisk, 'prev');
  ctl(b, branchRisk, 'else', printLowRisk, 'prev');
  // both risk paths merge into dispatch
  ctl(b, branchRisk, 'after', callDispatch, 'prev');
  ctl(b, callDispatch, 'next', printDispatched, 'prev');
  // dispatch → notify
  ctl(b, printDispatched, 'next', callNotify, 'prev');
  ctl(b, callNotify, 'next', finalPrint, 'prev');
  ctl(b, finalPrint, 'next', ret, 'prev');

  return finalize(b);
}
