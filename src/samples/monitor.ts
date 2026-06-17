/**
 * Sample: "Monitor" — modelled on `jj_comp.sol`.
 *
 * Demonstrates: while loop, struct in a loop, helper function call,
 * arithmetic, comparison, assignment, multi-function.
 */

import {
  addFunction,
  addStruct,
  createBuilder,
  ctl,
  dat,
  finalize,
  getStart,
  node,
  setActiveFn,
} from './builders';

export function buildMonitor() {
  const b = createBuilder('monitor');

  // -----------------------------------------------------------
  // struct SystemNode { id: int, threshold: float }
  // -----------------------------------------------------------
  addStruct(b, 'SystemNode', [
    { name: 'id', type: { kind: 'int' } },
    { name: 'threshold', type: { kind: 'float' } },
  ]);

  // -----------------------------------------------------------
  // function assess_node(node: SystemNode, limit: float) -> bool {
  //   if (node.threshold > limit) {
  //     print("ALERT");
  //     return true;
  //   }
  //   return false;
  // }
  // -----------------------------------------------------------
  const assessFn = addFunction(
    b,
    'assess_node',
    [
      { name: 'node', type: { kind: 'named', name: 'SystemNode' } },
      { name: 'limit', type: { kind: 'float' } },
    ],
    { kind: 'bool' },
    false, // helper fn, not the runnable workflow
  );
  const a_start = getStart(b);
  const a_node = node(b, 'varGet', { x: 80, y: 200 }, {
    kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'SystemNode' },
  });
  const a_thr = node(b, 'fieldAccess', { x: 280, y: 200 }, {
    kind: 'fieldAccess', structName: 'SystemNode', fieldName: 'threshold',
  });
  const a_limit = node(b, 'varGet', { x: 280, y: 320 }, {
    kind: 'varGet', varName: 'limit', resolvedType: { kind: 'float' },
  });
  const a_cmp = node(b, 'binaryOp', { x: 500, y: 260 }, {
    kind: 'binaryOp', op: '>', valueType: { kind: 'float' },
  });
  const a_branch = node(b, 'branch', { x: 720, y: 60 }, { kind: 'branch', hasElse: false });
  const a_alert = node(b, 'literal', { x: 800, y: 240 }, {
    kind: 'literal', litType: 'str', value: 'ALERT: Node exceeded limit!',
  });
  const a_print = node(b, 'print', { x: 940, y: 60 });
  const a_true = node(b, 'literal', { x: 940, y: 220 }, {
    kind: 'literal', litType: 'bool', value: 'true',
  });
  const a_retTrue = node(b, 'return', { x: 1160, y: 60 }, { kind: 'return', hasValue: true });
  const a_false = node(b, 'literal', { x: 720, y: 400 }, {
    kind: 'literal', litType: 'bool', value: 'false',
  });
  const a_retFalse = node(b, 'return', { x: 900, y: 360 }, { kind: 'return', hasValue: true });

  dat(b, a_node, 'value', a_thr, 'target');
  dat(b, a_thr, 'value', a_cmp, 'lhs');
  dat(b, a_limit, 'value', a_cmp, 'rhs');
  dat(b, a_cmp, 'result', a_branch, 'cond');
  ctl(b, a_start, 'next', a_branch, 'prev');
  ctl(b, a_branch, 'then', a_print, 'prev');
  ctl(b, a_print, 'next', a_retTrue, 'prev');
  dat(b, a_alert, 'value', a_print, 'value');
  dat(b, a_true, 'value', a_retTrue, 'value');
  ctl(b, a_branch, 'after', a_retFalse, 'prev');
  dat(b, a_false, 'value', a_retFalse, 'value');

  // -----------------------------------------------------------
  // workflow "start" {
  //   let limit: float = 85.0;
  //   let counter: int = 1;
  //   while (counter < 4) {
  //     let nd: SystemNode = SystemNode { id: counter, threshold: 90.0 };
  //     assess_node(nd, limit);
  //     counter = counter + 1;
  //   }
  //   return 0;
  // }
  // -----------------------------------------------------------
  const startFn = addFunction(b, 'start', [], { kind: 'int' }); // workflow entry
  setActiveFn(b, startFn.id);
  const s_start = getStart(b);
  const s_lit85 = node(b, 'literal', { x: 80, y: 200 }, {
    kind: 'literal', litType: 'float', value: '85.0',
  });
  const s_letLimit = node(b, 'let', { x: 280, y: 60 }, {
    kind: 'let', varName: 'limit', varType: { kind: 'float' },
  });
  const s_lit1 = node(b, 'literal', { x: 280, y: 200 }, {
    kind: 'literal', litType: 'int', value: '1',
  });
  const s_letCounter = node(b, 'let', { x: 460, y: 60 }, {
    kind: 'let', varName: 'counter', varType: { kind: 'int' },
  });
  const s_counterCond = node(b, 'varGet', { x: 460, y: 200 }, {
    kind: 'varGet', varName: 'counter', resolvedType: { kind: 'int' },
  });
  const s_lit4 = node(b, 'literal', { x: 460, y: 280 }, {
    kind: 'literal', litType: 'int', value: '4',
  });
  const s_cmp = node(b, 'binaryOp', { x: 640, y: 240 }, {
    kind: 'binaryOp', op: '<', valueType: { kind: 'int' },
  });
  const s_while = node(b, 'while', { x: 820, y: 60 });
  // --- while body ---
  const s_counterId = node(b, 'varGet', { x: 1040, y: 200 }, {
    kind: 'varGet', varName: 'counter', resolvedType: { kind: 'int' },
  });
  const s_thr = node(b, 'literal', { x: 1040, y: 280 }, {
    kind: 'literal', litType: 'float', value: '90.0',
  });
  const s_struct = node(b, 'structLiteral', { x: 1240, y: 120 }, {
    kind: 'structLiteral', structName: 'SystemNode',
  });
  const s_letNd = node(b, 'let', { x: 1460, y: 60 }, {
    kind: 'let', varName: 'nd', varType: { kind: 'named', name: 'SystemNode' },
  });
  const s_ndGet = node(b, 'varGet', { x: 1460, y: 220 }, {
    kind: 'varGet', varName: 'nd', resolvedType: { kind: 'named', name: 'SystemNode' },
  });
  const s_limitGet = node(b, 'varGet', { x: 1460, y: 300 }, {
    kind: 'varGet', varName: 'limit', resolvedType: { kind: 'float' },
  });
  const s_call = node(b, 'call', { x: 1680, y: 60 }, {
    kind: 'call', functionId: assessFn.id,
  });
  // counter = counter + 1
  const s_assign = node(b, 'assign', { x: 1900, y: 60 }, {
    kind: 'assign', varName: 'counter',
  });
  const s_counterInc = node(b, 'varGet', { x: 1900, y: 200 }, {
    kind: 'varGet', varName: 'counter', resolvedType: { kind: 'int' },
  });
  const s_one = node(b, 'literal', { x: 1900, y: 280 }, {
    kind: 'literal', litType: 'int', value: '1',
  });
  const s_add = node(b, 'binaryOp', { x: 2080, y: 240 }, {
    kind: 'binaryOp', op: '+', valueType: { kind: 'int' },
  });
  const s_lit0 = node(b, 'literal', { x: 820, y: 280 }, {
    kind: 'literal', litType: 'int', value: '0',
  });
  const s_ret = node(b, 'return', { x: 1000, y: 360 }, { kind: 'return', hasValue: true });

  dat(b, s_lit85, 'value', s_letLimit, 'value');
  dat(b, s_lit1, 'value', s_letCounter, 'value');
  dat(b, s_counterCond, 'value', s_cmp, 'lhs');
  dat(b, s_lit4, 'value', s_cmp, 'rhs');
  dat(b, s_cmp, 'result', s_while, 'cond');
  dat(b, s_counterId, 'value', s_struct, 'field:id');
  dat(b, s_thr, 'value', s_struct, 'field:threshold');
  dat(b, s_struct, 'value', s_letNd, 'value');
  dat(b, s_ndGet, 'value', s_call, 'arg:node');
  dat(b, s_limitGet, 'value', s_call, 'arg:limit');
  dat(b, s_counterInc, 'value', s_add, 'lhs');
  dat(b, s_one, 'value', s_add, 'rhs');
  dat(b, s_add, 'result', s_assign, 'value');
  dat(b, s_lit0, 'value', s_ret, 'value');

  ctl(b, s_start, 'next', s_letLimit, 'prev');
  ctl(b, s_letLimit, 'next', s_letCounter, 'prev');
  ctl(b, s_letCounter, 'next', s_while, 'prev');
  ctl(b, s_while, 'body', s_letNd, 'prev');
  ctl(b, s_letNd, 'next', s_call, 'prev');
  ctl(b, s_call, 'next', s_assign, 'prev');
  ctl(b, s_while, 'after', s_ret, 'prev');

  return finalize(b);
}
