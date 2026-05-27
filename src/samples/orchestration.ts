/**
 * Sample: "Orchestration" — modelled on `s1.sol`.
 *
 * Demonstrates: imports + enum + struct + multiple helper functions +
 * nested if/else + enum variant comparison + struct construction.
 * A canonical multi-construct workflow used to exercise the editor's
 * full node set in one program.
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

export function buildOrchestration() {
  const b = createBuilder('orchestration');

  // imports (declarative — not callable in Phase A)
  addImport(b, ['EdgeRouter', 'SecurityControl', 'AuthApp', 'ValidateToken', 'Expiration'], 'TokenTimeout');
  addImport(b, ['GlobalRouter', 'InventoryControl', 'WarehouseApp', 'GetStock', 'Level'], 'StockLevel');

  // enum AppHealth { Offline, Warming, Stable = 200, Degraded = 503 }
  //
  // First-character set chosen to avoid T9002 collisions: the canonical
  // SOL bytecode dispatches enum variants by (first_char % 10), and the
  // simulator runs them by name — so a same-first-char workflow looks
  // correct in the editor but silently misdispatches in production.
  // Source-style names like "Initializing" and "Overloaded" collide
  // with "Stable" and "Offline" respectively; the names below are
  // semantically equivalent and all start with distinct characters
  // (O=9, W=7, S=3, D=8). See chapter 17 §17.1 of the SOL docs.
  addEnum(b, 'AppHealth', [
    { name: 'Offline', value: null },
    { name: 'Warming', value: null },
    { name: 'Stable', value: 200 },
    { name: 'Degraded', value: 503 },
  ]);

  // struct ProcessNode { id, threshold, service_name, is_active }
  addStruct(b, 'ProcessNode', [
    { name: 'id', type: { kind: 'int' } },
    { name: 'threshold', type: { kind: 'float' } },
    { name: 'service_name', type: { kind: 'str' } },
    { name: 'is_active', type: { kind: 'bool' } },
  ]);

  // -----------------------------------------------------------
  // function start_service(name: str) { print("started:"); print(name); }
  // -----------------------------------------------------------
  const startServiceFn = addFunction(b, 'start_service', [
    { name: 'name', type: { kind: 'str' } },
  ]);
  {
    const start = getStart(b);
    const lit = node(b, 'literal', { x: 80, y: 200 }, {
      kind: 'literal', litType: 'str', value: 'started service:',
    });
    const p1 = node(b, 'print', { x: 280, y: 60 });
    const nm = node(b, 'varGet', { x: 280, y: 200 }, {
      kind: 'varGet', varName: 'name', resolvedType: { kind: 'str' },
    });
    const p2 = node(b, 'print', { x: 500, y: 60 });
    dat(b, lit, 'value', p1, 'value');
    dat(b, nm, 'value', p2, 'value');
    ctl(b, start, 'next', p1, 'prev');
    ctl(b, p1, 'next', p2, 'prev');
  }

  // -----------------------------------------------------------
  // function stop_service(name: str) { print("stopped:"); print(name); }
  // -----------------------------------------------------------
  const stopServiceFn = addFunction(b, 'stop_service', [
    { name: 'name', type: { kind: 'str' } },
  ]);
  {
    const start = getStart(b);
    const lit = node(b, 'literal', { x: 80, y: 200 }, {
      kind: 'literal', litType: 'str', value: 'stopped service:',
    });
    const p1 = node(b, 'print', { x: 280, y: 60 });
    const nm = node(b, 'varGet', { x: 280, y: 200 }, {
      kind: 'varGet', varName: 'name', resolvedType: { kind: 'str' },
    });
    const p2 = node(b, 'print', { x: 500, y: 60 });
    dat(b, lit, 'value', p1, 'value');
    dat(b, nm, 'value', p2, 'value');
    ctl(b, start, 'next', p1, 'prev');
    ctl(b, p1, 'next', p2, 'prev');
  }

  // -----------------------------------------------------------
  // function verify_capacity(node: ProcessNode, current: float) -> AppHealth
  //   if (current > node.threshold) return Degraded;
  //   else if (node.is_active) return Stable; else return Warming;
  // -----------------------------------------------------------
  const verifyFn = addFunction(
    b,
    'verify_capacity',
    [
      { name: 'node', type: { kind: 'named', name: 'ProcessNode' } },
      { name: 'current', type: { kind: 'float' } },
    ],
    { kind: 'named', name: 'AppHealth' },
  );
  {
    const start = getStart(b);
    const nd = node(b, 'varGet', { x: 80, y: 200 }, {
      kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'ProcessNode' },
    });
    const thr = node(b, 'fieldAccess', { x: 280, y: 200 }, {
      kind: 'fieldAccess', structName: 'ProcessNode', fieldName: 'threshold',
    });
    const cur = node(b, 'varGet', { x: 80, y: 320 }, {
      kind: 'varGet', varName: 'current', resolvedType: { kind: 'float' },
    });
    const cmp = node(b, 'binaryOp', { x: 500, y: 260 }, {
      kind: 'binaryOp', op: '>', valueType: { kind: 'float' },
    });
    const branch1 = node(b, 'branch', { x: 700, y: 60 }, { kind: 'branch', hasElse: true });
    const over = node(b, 'enumVariant', { x: 900, y: 180 }, {
      kind: 'enumVariant', enumName: 'AppHealth', variantName: 'Degraded',
    });
    const ret1 = node(b, 'return', { x: 900, y: 60 }, { kind: 'return', hasValue: true });
    const isAct = node(b, 'fieldAccess', { x: 900, y: 320 }, {
      kind: 'fieldAccess', structName: 'ProcessNode', fieldName: 'is_active',
    });
    const ndForActive = node(b, 'varGet', { x: 700, y: 320 }, {
      kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'ProcessNode' },
    });
    const branch2 = node(b, 'branch', { x: 1120, y: 260 }, { kind: 'branch', hasElse: true });
    const stable = node(b, 'enumVariant', { x: 1320, y: 360 }, {
      kind: 'enumVariant', enumName: 'AppHealth', variantName: 'Stable',
    });
    const initz = node(b, 'enumVariant', { x: 1320, y: 480 }, {
      kind: 'enumVariant', enumName: 'AppHealth', variantName: 'Warming',
    });
    const ret2 = node(b, 'return', { x: 1320, y: 260 }, { kind: 'return', hasValue: true });
    const ret3 = node(b, 'return', { x: 1320, y: 420 }, { kind: 'return', hasValue: true });

    dat(b, nd, 'value', thr, 'target');
    dat(b, thr, 'value', cmp, 'lhs');
    dat(b, cur, 'value', cmp, 'rhs');
    dat(b, cmp, 'result', branch1, 'cond');
    dat(b, over, 'value', ret1, 'value');
    dat(b, ndForActive, 'value', isAct, 'target');
    dat(b, isAct, 'value', branch2, 'cond');
    dat(b, stable, 'value', ret2, 'value');
    dat(b, initz, 'value', ret3, 'value');

    ctl(b, start, 'next', branch1, 'prev');
    ctl(b, branch1, 'then', ret1, 'prev');
    ctl(b, branch1, 'else', branch2, 'prev');
    ctl(b, branch2, 'then', ret2, 'prev');
    ctl(b, branch2, 'else', ret3, 'prev');
  }

  // -----------------------------------------------------------
  // function start() {
  //   let node: ProcessNode = ProcessNode { ... };
  //   let status: AppHealth = verify_capacity(node, 85.2);
  //   if (status == Stable) { start_service(node.service_name); }
  //   else if (status == Degraded) { stop_service(node.service_name); }
  // }
  // -----------------------------------------------------------
  const startFn = addFunction(b, 'start', [], { kind: 'void' });
  setActiveFn(b, startFn.id);
  const main_start = getStart(b);

  // ProcessNode literal inputs
  const litId = node(b, 'literal', { x: 80, y: 200 }, {
    kind: 'literal', litType: 'int', value: '0',
  });
  const litThr = node(b, 'literal', { x: 80, y: 270 }, {
    kind: 'literal', litType: 'float', value: '90.5',
  });
  const litSvc = node(b, 'literal', { x: 80, y: 340 }, {
    kind: 'literal', litType: 'str', value: 'Inventory_Orchestrator',
  });
  const litAct = node(b, 'literal', { x: 80, y: 410 }, {
    kind: 'literal', litType: 'bool', value: 'true',
  });
  const structLit = node(b, 'structLiteral', { x: 280, y: 300 }, {
    kind: 'structLiteral', structName: 'ProcessNode',
  });
  const letNode = node(b, 'let', { x: 540, y: 60 }, {
    kind: 'let', varName: 'node', varType: { kind: 'named', name: 'ProcessNode' },
  });
  const lit85 = node(b, 'literal', { x: 540, y: 200 }, {
    kind: 'literal', litType: 'float', value: '85.2',
  });
  const nodeForCall = node(b, 'varGet', { x: 540, y: 280 }, {
    kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'ProcessNode' },
  });
  const callVerify = node(b, 'call', { x: 760, y: 60 }, {
    kind: 'call', functionId: verifyFn.id,
  });
  const letStatus = node(b, 'let', { x: 980, y: 60 }, {
    kind: 'let', varName: 'status', varType: { kind: 'named', name: 'AppHealth' },
  });
  const statusGet = node(b, 'varGet', { x: 980, y: 200 }, {
    kind: 'varGet', varName: 'status', resolvedType: { kind: 'named', name: 'AppHealth' },
  });
  const stableVar = node(b, 'enumVariant', { x: 980, y: 280 }, {
    kind: 'enumVariant', enumName: 'AppHealth', variantName: 'Stable',
  });
  const cmpStatus = node(b, 'binaryOp', { x: 1180, y: 240 }, {
    kind: 'binaryOp', op: '==', valueType: { kind: 'named', name: 'AppHealth' },
  });
  const branchMain = node(b, 'branch', { x: 1380, y: 60 }, { kind: 'branch', hasElse: true });

  // call start_service(node.service_name) inside then branch
  const nodeForSvc1 = node(b, 'varGet', { x: 1380, y: 220 }, {
    kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'ProcessNode' },
  });
  const svcName1 = node(b, 'fieldAccess', { x: 1560, y: 220 }, {
    kind: 'fieldAccess', structName: 'ProcessNode', fieldName: 'service_name',
  });
  const callStartSvc = node(b, 'call', { x: 1760, y: 60 }, {
    kind: 'call', functionId: startServiceFn.id,
  });

  // else branch — check Degraded and call stop_service
  const overVar = node(b, 'enumVariant', { x: 1380, y: 420 }, {
    kind: 'enumVariant', enumName: 'AppHealth', variantName: 'Degraded',
  });
  const statusGet2 = node(b, 'varGet', { x: 1380, y: 350 }, {
    kind: 'varGet', varName: 'status', resolvedType: { kind: 'named', name: 'AppHealth' },
  });
  const cmpOver = node(b, 'binaryOp', { x: 1580, y: 380 }, {
    kind: 'binaryOp', op: '==', valueType: { kind: 'named', name: 'AppHealth' },
  });
  const branch2 = node(b, 'branch', { x: 1780, y: 340 }, { kind: 'branch', hasElse: false });
  const nodeForSvc2 = node(b, 'varGet', { x: 1780, y: 480 }, {
    kind: 'varGet', varName: 'node', resolvedType: { kind: 'named', name: 'ProcessNode' },
  });
  const svcName2 = node(b, 'fieldAccess', { x: 1980, y: 480 }, {
    kind: 'fieldAccess', structName: 'ProcessNode', fieldName: 'service_name',
  });
  const callStopSvc = node(b, 'call', { x: 2180, y: 340 }, {
    kind: 'call', functionId: stopServiceFn.id,
  });

  // wire struct lit → let
  dat(b, litId, 'value', structLit, 'field:id');
  dat(b, litThr, 'value', structLit, 'field:threshold');
  dat(b, litSvc, 'value', structLit, 'field:service_name');
  dat(b, litAct, 'value', structLit, 'field:is_active');
  dat(b, structLit, 'value', letNode, 'value');
  dat(b, nodeForCall, 'value', callVerify, 'arg:node');
  dat(b, lit85, 'value', callVerify, 'arg:current');
  dat(b, callVerify, 'return', letStatus, 'value');
  dat(b, statusGet, 'value', cmpStatus, 'lhs');
  dat(b, stableVar, 'value', cmpStatus, 'rhs');
  dat(b, cmpStatus, 'result', branchMain, 'cond');
  dat(b, nodeForSvc1, 'value', svcName1, 'target');
  dat(b, svcName1, 'value', callStartSvc, 'arg:name');
  dat(b, statusGet2, 'value', cmpOver, 'lhs');
  dat(b, overVar, 'value', cmpOver, 'rhs');
  dat(b, cmpOver, 'result', branch2, 'cond');
  dat(b, nodeForSvc2, 'value', svcName2, 'target');
  dat(b, svcName2, 'value', callStopSvc, 'arg:name');

  ctl(b, main_start, 'next', letNode, 'prev');
  ctl(b, letNode, 'next', callVerify, 'prev');
  ctl(b, callVerify, 'next', letStatus, 'prev');
  ctl(b, letStatus, 'next', branchMain, 'prev');
  ctl(b, branchMain, 'then', callStartSvc, 'prev');
  ctl(b, branchMain, 'else', branch2, 'prev');
  ctl(b, branch2, 'then', callStopSvc, 'prev');

  return finalize(b);
}
