/**
 * Sample: "Hello, Person" — modelled on `jjsi.sol`.
 *
 * Demonstrates: struct definition, struct literal, field access via
 * function parameter, multi-function call, return.
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

export function buildHello() {
  const b = createBuilder('hello');

  // -----------------------------------------------------------
  // struct Person { name: str, age: int }
  // -----------------------------------------------------------
  addStruct(b, 'Person', [
    { name: 'name', type: { kind: 'str' } },
    { name: 'age', type: { kind: 'int' } },
  ]);

  // -----------------------------------------------------------
  // function print_person(p: Person) {
  //   print(p.name);
  //   print(p.age);
  // }
  // -----------------------------------------------------------
  const printPersonFn = addFunction(b, 'print_person', [
    { name: 'p', type: { kind: 'named', name: 'Person' } },
  ], { kind: 'void' }, false);
  const pp_start = getStart(b);
  const pp_print1 = node(b, 'print', { x: 280, y: 60 });
  const pp_fld_name = node(b, 'fieldAccess', { x: 80, y: 160 }, {
    kind: 'fieldAccess',
    structName: 'Person',
    fieldName: 'name',
  });
  const pp_print2 = node(b, 'print', { x: 280, y: 220 });
  const pp_fld_age = node(b, 'fieldAccess', { x: 80, y: 320 }, {
    kind: 'fieldAccess',
    structName: 'Person',
    fieldName: 'age',
  });
  // The `p` parameter needs to be wired into field-access targets via varGet.
  const pp_p_for_name = node(b, 'varGet', { x: 80, y: 80 }, {
    kind: 'varGet',
    varName: 'p',
    resolvedType: { kind: 'named', name: 'Person' },
  });
  const pp_p_for_age = node(b, 'varGet', { x: 80, y: 240 }, {
    kind: 'varGet',
    varName: 'p',
    resolvedType: { kind: 'named', name: 'Person' },
  });
  ctl(b, pp_start, 'next', pp_print1, 'prev');
  ctl(b, pp_print1, 'next', pp_print2, 'prev');
  dat(b, pp_p_for_name, 'value', pp_fld_name, 'target');
  dat(b, pp_fld_name, 'value', pp_print1, 'value');
  dat(b, pp_p_for_age, 'value', pp_fld_age, 'target');
  dat(b, pp_fld_age, 'value', pp_print2, 'value');

  // -----------------------------------------------------------
  // function start() -> int {
  //   let p: Person = Person { name: "evan", age: 19 };
  //   print_person(p);
  //   return 0;
  // }
  // -----------------------------------------------------------
  const startFn = addFunction(b, 'start', [], { kind: 'int' });
  setActiveFn(b, startFn.id);
  const s_start = getStart(b);
  const s_litName = node(b, 'literal', { x: 80, y: 200 }, {
    kind: 'literal',
    litType: 'str',
    value: 'evan',
  });
  const s_litAge = node(b, 'literal', { x: 80, y: 280 }, {
    kind: 'literal',
    litType: 'int',
    value: '19',
  });
  const s_struct = node(b, 'structLiteral', { x: 300, y: 220 }, {
    kind: 'structLiteral',
    structName: 'Person',
  });
  const s_let = node(b, 'let', { x: 540, y: 60 }, {
    kind: 'let',
    varName: 'p',
    varType: { kind: 'named', name: 'Person' },
  });
  const s_call = node(b, 'call', { x: 800, y: 60 }, {
    kind: 'call',
    functionId: printPersonFn.id,
  });
  const s_pforCall = node(b, 'varGet', { x: 540, y: 240 }, {
    kind: 'varGet',
    varName: 'p',
    resolvedType: { kind: 'named', name: 'Person' },
  });
  const s_lit0 = node(b, 'literal', { x: 800, y: 240 }, {
    kind: 'literal',
    litType: 'int',
    value: '0',
  });
  const s_ret = node(b, 'return', { x: 1050, y: 60 }, { kind: 'return', hasValue: true });

  ctl(b, s_start, 'next', s_let, 'prev');
  ctl(b, s_let, 'next', s_call, 'prev');
  ctl(b, s_call, 'next', s_ret, 'prev');
  dat(b, s_litName, 'value', s_struct, 'field:name');
  dat(b, s_litAge, 'value', s_struct, 'field:age');
  dat(b, s_struct, 'value', s_let, 'value');
  dat(b, s_pforCall, 'value', s_call, 'arg:p');
  dat(b, s_lit0, 'value', s_ret, 'value');

  return finalize(b);
}
