/**
 * Sample: "Payments" — modelled on `s2.sol`.
 *
 * Demonstrates: imports, enum with numeric values, struct, evaluator
 * function, deploy/shutdown dispatch. Structurally the same as
 * `orchestration` but the domain is payment processing.
 *
 * Note: the original `s2.sol` uses string concatenation `"foo" + bar`,
 * which is ambiguous in the current SOL compiler. We sidestep it by
 * using two prints — preserves the architectural showcase.
 */

import {
  addEnum,
  addFunction,
  addImport,
  addStruct,
  createBuilder,
  ctl,
  dat,
  finalize,
  getStart,
  node,
  setActiveFn,
} from './builders';

export function buildPayments() {
  const b = createBuilder('payments');

  addImport(b, ['CoreNetwork', 'FinanceGateway', 'Auth', 'ValidateSession', 'Timeout'], 'SessionTimeout');
  addImport(b, ['RegionalNetwork', 'PaymentHub', 'TransactionMonitor', 'CheckFraud', 'Score'], 'FraudScore');

  addEnum(b, 'PaymentStatus', [
    { name: 'Pending', value: null },
    { name: 'Processing', value: null },
    { name: 'Approved', value: 201 },
    { name: 'Declined', value: 403 },
  ]);

  addStruct(b, 'PaymentNode', [
    { name: 'transaction_id', type: { kind: 'int' } },
    { name: 'fraud_limit', type: { kind: 'float' } },
    { name: 'gateway_name', type: { kind: 'str' } },
    { name: 'is_verified', type: { kind: 'bool' } },
  ]);

  // deploy(service)
  const deployFn = addFunction(b, 'deploy', [
    { name: 'service', type: { kind: 'str' } },
  ]);
  {
    const start = getStart(b);
    const lit = node(b, 'literal', { x: 80, y: 200 }, {
      kind: 'literal', litType: 'str', value: 'Deploying:',
    });
    const p1 = node(b, 'print', { x: 280, y: 60 });
    const svc = node(b, 'varGet', { x: 280, y: 200 }, {
      kind: 'varGet', varName: 'service', resolvedType: { kind: 'str' },
    });
    const p2 = node(b, 'print', { x: 500, y: 60 });
    dat(b, lit, 'value', p1, 'value');
    dat(b, svc, 'value', p2, 'value');
    ctl(b, start, 'next', p1, 'prev');
    ctl(b, p1, 'next', p2, 'prev');
  }

  // shutdown(service)
  const shutdownFn = addFunction(b, 'shutdown', [
    { name: 'service', type: { kind: 'str' } },
  ]);
  {
    const start = getStart(b);
    const lit = node(b, 'literal', { x: 80, y: 200 }, {
      kind: 'literal', litType: 'str', value: 'Shutting down:',
    });
    const p1 = node(b, 'print', { x: 280, y: 60 });
    const svc = node(b, 'varGet', { x: 280, y: 200 }, {
      kind: 'varGet', varName: 'service', resolvedType: { kind: 'str' },
    });
    const p2 = node(b, 'print', { x: 500, y: 60 });
    dat(b, lit, 'value', p1, 'value');
    dat(b, svc, 'value', p2, 'value');
    ctl(b, start, 'next', p1, 'prev');
    ctl(b, p1, 'next', p2, 'prev');
  }

  // evaluate_payment(node, risk) -> PaymentStatus
  const evalFn = addFunction(
    b,
    'evaluate_payment',
    [
      { name: 'node', type: { kind: 'named', name: 'PaymentNode' } },
      { name: 'risk_score', type: { kind: 'float' } },
    ],
    { kind: 'named', name: 'PaymentStatus' },
  );
  {
    const start = getStart(b);
    const nd = node(b, 'varGet', { x: 80, y: 200 }, {
      kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'PaymentNode' },
    });
    const fld = node(b, 'fieldAccess', { x: 280, y: 200 }, {
      kind: 'fieldAccess', structName: 'PaymentNode', fieldName: 'fraud_limit',
    });
    const rsk = node(b, 'varGet', { x: 80, y: 320 }, {
      kind: 'varGet', varName: 'risk_score', resolvedType: { kind: 'float' },
    });
    const cmp = node(b, 'binaryOp', { x: 500, y: 260 }, {
      kind: 'binaryOp', op: '>', valueType: { kind: 'float' },
    });
    const branch = node(b, 'branch', { x: 720, y: 60 }, { kind: 'branch', hasElse: true });
    const decl = node(b, 'enumVariant', { x: 720, y: 220 }, {
      kind: 'enumVariant', enumName: 'PaymentStatus', variantName: 'Declined',
    });
    const ret1 = node(b, 'return', { x: 940, y: 60 }, { kind: 'return', hasValue: true });
    const ndForVer = node(b, 'varGet', { x: 940, y: 340 }, {
      kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'PaymentNode' },
    });
    const ver = node(b, 'fieldAccess', { x: 1140, y: 340 }, {
      kind: 'fieldAccess', structName: 'PaymentNode', fieldName: 'is_verified',
    });
    const branch2 = node(b, 'branch', { x: 1340, y: 260 }, { kind: 'branch', hasElse: true });
    const appr = node(b, 'enumVariant', { x: 1340, y: 420 }, {
      kind: 'enumVariant', enumName: 'PaymentStatus', variantName: 'Approved',
    });
    const pend = node(b, 'enumVariant', { x: 1340, y: 500 }, {
      kind: 'enumVariant', enumName: 'PaymentStatus', variantName: 'Pending',
    });
    const ret2 = node(b, 'return', { x: 1540, y: 260 }, { kind: 'return', hasValue: true });
    const ret3 = node(b, 'return', { x: 1540, y: 420 }, { kind: 'return', hasValue: true });

    dat(b, nd, 'value', fld, 'target');
    dat(b, fld, 'value', cmp, 'lhs');
    dat(b, rsk, 'value', cmp, 'rhs');
    dat(b, cmp, 'result', branch, 'cond');
    dat(b, decl, 'value', ret1, 'value');
    dat(b, ndForVer, 'value', ver, 'target');
    dat(b, ver, 'value', branch2, 'cond');
    dat(b, appr, 'value', ret2, 'value');
    dat(b, pend, 'value', ret3, 'value');
    ctl(b, start, 'next', branch, 'prev');
    ctl(b, branch, 'then', ret1, 'prev');
    ctl(b, branch, 'else', branch2, 'prev');
    ctl(b, branch2, 'then', ret2, 'prev');
    ctl(b, branch2, 'else', ret3, 'prev');
  }

  // process_transaction(user_id) -> int
  const processFn = addFunction(
    b,
    'process_transaction',
    [{ name: 'user_id', type: { kind: 'int' } }],
    { kind: 'int' },
  );
  setActiveFn(b, processFn.id);
  {
    const start = getStart(b);
    const uid = node(b, 'varGet', { x: 80, y: 200 }, {
      kind: 'varGet', varName: 'user_id', resolvedType: { kind: 'int' },
    });
    const fLimit = node(b, 'literal', { x: 80, y: 280 }, {
      kind: 'literal', litType: 'float', value: '75.0',
    });
    const gw = node(b, 'literal', { x: 80, y: 360 }, {
      kind: 'literal', litType: 'str', value: 'Secure_Payment_Gateway',
    });
    const verLit = node(b, 'literal', { x: 80, y: 440 }, {
      kind: 'literal', litType: 'bool', value: 'true',
    });
    const lit45 = node(b, 'literal', { x: 80, y: 520 }, {
      kind: 'literal', litType: 'float', value: '45.5',
    });
    const sLit = node(b, 'structLiteral', { x: 320, y: 340 }, {
      kind: 'structLiteral', structName: 'PaymentNode',
    });
    const letN = node(b, 'let', { x: 580, y: 60 }, {
      kind: 'let', varName: 'pay', varType: { kind: 'named', name: 'PaymentNode' },
    });
    const payGet = node(b, 'varGet', { x: 580, y: 260 }, {
      kind: 'varGet', varName: 'pay', resolvedType: { kind: 'named', name: 'PaymentNode' },
    });
    const callEval = node(b, 'call', { x: 800, y: 60 }, {
      kind: 'call', functionId: evalFn.id,
    });
    const letRes = node(b, 'let', { x: 1020, y: 60 }, {
      kind: 'let', varName: 'result', varType: { kind: 'named', name: 'PaymentStatus' },
    });
    const resGet = node(b, 'varGet', { x: 1020, y: 200 }, {
      kind: 'varGet', varName: 'result', resolvedType: { kind: 'named', name: 'PaymentStatus' },
    });
    const apprVar = node(b, 'enumVariant', { x: 1020, y: 280 }, {
      kind: 'enumVariant', enumName: 'PaymentStatus', variantName: 'Approved',
    });
    const cmp = node(b, 'binaryOp', { x: 1220, y: 240 }, {
      kind: 'binaryOp', op: '==', valueType: { kind: 'named', name: 'PaymentStatus' },
    });
    const branch = node(b, 'branch', { x: 1420, y: 60 }, { kind: 'branch', hasElse: false });
    const payGetGw = node(b, 'varGet', { x: 1420, y: 220 }, {
      kind: 'varGet', varName: 'pay', resolvedType: { kind: 'named', name: 'PaymentNode' },
    });
    const gwName = node(b, 'fieldAccess', { x: 1620, y: 220 }, {
      kind: 'fieldAccess', structName: 'PaymentNode', fieldName: 'gateway_name',
    });
    const callDeploy = node(b, 'call', { x: 1820, y: 60 }, {
      kind: 'call', functionId: deployFn.id,
    });
    const lit1 = node(b, 'literal', { x: 1820, y: 220 }, {
      kind: 'literal', litType: 'int', value: '1',
    });
    const ret1 = node(b, 'return', { x: 2040, y: 60 }, { kind: 'return', hasValue: true });
    const lit2 = node(b, 'literal', { x: 1420, y: 400 }, {
      kind: 'literal', litType: 'int', value: '2',
    });
    const ret2 = node(b, 'return', { x: 1620, y: 400 }, { kind: 'return', hasValue: true });

    dat(b, uid, 'value', sLit, 'field:transaction_id');
    dat(b, fLimit, 'value', sLit, 'field:fraud_limit');
    dat(b, gw, 'value', sLit, 'field:gateway_name');
    dat(b, verLit, 'value', sLit, 'field:is_verified');
    dat(b, sLit, 'value', letN, 'value');
    dat(b, payGet, 'value', callEval, 'arg:node');
    dat(b, lit45, 'value', callEval, 'arg:risk_score');
    dat(b, callEval, 'return', letRes, 'value');
    dat(b, resGet, 'value', cmp, 'lhs');
    dat(b, apprVar, 'value', cmp, 'rhs');
    dat(b, cmp, 'result', branch, 'cond');
    dat(b, payGetGw, 'value', gwName, 'target');
    dat(b, gwName, 'value', callDeploy, 'arg:service');
    dat(b, lit1, 'value', ret1, 'value');
    dat(b, lit2, 'value', ret2, 'value');

    ctl(b, start, 'next', letN, 'prev');
    ctl(b, letN, 'next', callEval, 'prev');
    ctl(b, callEval, 'next', letRes, 'prev');
    ctl(b, letRes, 'next', branch, 'prev');
    ctl(b, branch, 'then', callDeploy, 'prev');
    ctl(b, callDeploy, 'next', ret1, 'prev');
    ctl(b, branch, 'after', ret2, 'prev');
  }

  // function start() { process_transaction(42); }
  const startFn = addFunction(b, 'start', [], { kind: 'void' });
  setActiveFn(b, startFn.id);
  {
    const start = getStart(b);
    const lit42 = node(b, 'literal', { x: 80, y: 200 }, {
      kind: 'literal', litType: 'int', value: '42',
    });
    const c = node(b, 'call', { x: 280, y: 60 }, {
      kind: 'call', functionId: processFn.id,
    });
    dat(b, lit42, 'value', c, 'arg:user_id');
    ctl(b, start, 'next', c, 'prev');
  }

  return finalize(b);
}
