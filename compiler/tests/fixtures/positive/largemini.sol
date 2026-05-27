struct Person {
    name: string,
    age: int,
}

struct Vector {
    x: float,
    y: float,
}

struct Stats {
    score: int,
    ratio: float,
    active: bool,
}

function valInt() -> int {
    print("[TESTING]: valInt");
    print(0);
    print(42);
    print(-1);
    print(-500);
    return 4;
}

function valFloat() -> int {
    print("[TESTING]: valFloat");
    print(0.0);
    print(3.14159);
    print(-1.1);
    print(-400.0);
    return 4;
}

function valChar() -> int {
    print("[TESTING]: valChar");
    print('A');
    print('z');
    print('?');
    return 3;
}

function valString() -> int {
    print("[TESTING]: valString");
    print("");
    print("sol_lang");
    print("orchestration");
    return 3;
}

function valBool() -> int {
    print("[TESTING]: valBool");
    print(true);
    print(false);
    return 2;
}

function addInt() -> int {
    print("[TESTING]: addInt");
    print(10 + 5);
    print(0 + 0);
    print(-5 + 5);
    return 10 + 5;
}

function subInt() -> int {
    print("[TESTING]: subInt");
    print(20 - 7);
    print(0 - 10);
    print(-5 - -5);
    return 20 - 7;
}

function mulInt() -> int {
    print("[TESTING]: mulInt");
    print(6 * 7);
    print(100 * 0);
    print(-4 * 5);
    return 6 * 7;
}

function divInt() -> int {
    print("[TESTING]: divInt");
    print(40 / 8);
    print(10 / 3);
    print(-20 / 5);
    return 40 / 8;
}

function negInt() -> int {
    print("[TESTING]: negInt");
    print(-15);
    print(-0);
    return -15;
}

function treeInt() -> int {
    print("[TESTING]: treeInt");
    print(2 + 3 * 4);
    print((2 + 3) * 4);
    print(10 - 5 - 2);
    return 2 + 3 * 4;
}

function addFloat() -> int {
    print("[TESTING]: addFloat");
    print(1.5 + 2.25);
    print(0.0 + -0.5);
    return 2;
}

function subFloat() -> int {
    print("[TESTING]: subFloat");
    print(5.5 - 2.0);
    print(0.0 - 1.1);
    return 2;
}

function mulFloat() -> int {
    print("[TESTING]: mulFloat");
    print(2.5 * 4.0);
    print(-1.5 * -2.0);
    return 2;
}

function divFloat() -> int {
    print("[TESTING]: divFloat");
    print(10.0 / 4.0);
    print(-7.5 / 2.5);
    return 2;
}

function negFloat() -> int {
    print("[TESTING]: negFloat");
    print(-40.0);
    return 1;
}

function eqInt() -> int {
    print("[TESTING]: eqInt");
    print(5 == 5);
    print(5 != 5);
    print(10 == 2);
    return 1;
}

function orderInt() -> int {
    print("[TESTING]: orderInt");
    print(10 > 2);
    print(3 < 1);
    print(5 >= 5);
    print(4 <= 5);
    return 3;
}

function eqFloat() -> int {
    print("[TESTING]: eqFloat");
    print(4.4 == 4.4);
    print(4.4 != 5.5);
    return 2;
}

function orderFloat() -> int {
    print("[TESTING]: orderFloat");
    print(5.5 > 4.4);
    print(1.1 < 2.2);
    return 2;
}

function eqChar() -> int {
    print("[TESTING]: eqChar");
    print('a' == 'b');
    print('z' == 'z');
    return 1;
}

function eqString() -> int {
    print("[TESTING]: eqString");
    print("abc" == "abc");
    print("abc" != "xyz");
    return 2;
}

function andLogic() -> int {
    print("[TESTING]: andLogic");
    print(true && false);
    print(true && true);
    return 1;
}

function orLogic() -> int {
    print("[TESTING]: orLogic");
    print(true || false);
    print(false || false);
    return 1;
}

function notLogic() -> int {
    print("[TESTING]: notLogic");
    print(!true);
    print(!false);
    return 1;
}

function compoundLogic() -> int {
    print("[TESTING]: compoundLogic");
    print((true || false) && !false);
    print(!(true && false));
    return 2;
}

function andBit() -> int {
    print("[TESTING]: andBit");
    print(12 & 25);
    return 8;
}

function orBit() -> int {
    print("[TESTING]: orBit");
    print(12 | 25);
    return 29;
}

function xorBit() -> int {
    print("[TESTING]: xorBit");
    print(12 ^ 25);
    return 21;
}

function notBit() -> int {
    print("[TESTING]: notBit");
    print(~1);
    return -2;
}

function shiftLeft() -> int {
    print("[TESTING]: shiftLeft");
    print(1 << 4);
    return 16;
}

function shiftRight() -> int {
    print("[TESTING]: shiftRight");
    print(32 >> 2);
    return 8;
}

function assignSimple() -> int {
    print("[TESTING]: assignSimple");
    let target: int = 101;
    print(target);
    target = 202;
    print(target);
    return target;
}

function assignChained() -> int {
    print("[TESTING]: assignChained");
    let a: int = 0;
    let b: int = 0;
    a = b = 99;
    print(a);
    print(b);
    return a;
}

function assignAccumulator() -> int {
    print("[TESTING]: assignAccumulator");
    let float_var: float = 40.0;
    float_var = float_var * 2.0;
    print(float_var);
    return 80;
}

function blockIsolation() -> int {
    print("[TESTING]: blockIsolation");
    let check: int = 42;
    {
        let hidden: int = 500;
        100 + 200;
    }
    print(check);
    return check;
}

function branchIf() -> int {
    print("[TESTING]: branchIf");
    if (true) {
        print(77);
    }
    return 77;
}

function branchIfElse() -> int {
    print("[TESTING]: branchIfElse");
    if (false) {
        print(11);
        return 0;
    } else {
        print(22);
    }
    return 22;
}

function loopWhile() -> int {
    print("[TESTING]: loopWhile");
    let loop_counter: int = 0;
    while loop_counter < 2 {
        print(loop_counter);
        loop_counter = loop_counter + 1;
    }
    return loop_counter;
}

function loopFor() -> int {
    print("[TESTING]: loopFor");
    let arr: []int = [8, 9];
    for item in arr {
        print(item);
    }
    return 2;
}

function arrayRead() -> int {
    print("[TESTING]: arrayRead");
    let arr: []int = [100, 200, 300];
    let x: int = arr[1];
    print(x);
    return x;
}

function arrayWrite() -> int {
    print("[TESTING]: arrayWrite");
    let arr: []int = [10, 20];
    arr[0] = 999;
    print(arr[0]);
    return arr[0];
}

function arrayEmpty() -> int {
    print("[TESTING]: arrayEmpty");
    let arr: []int = [];
    print(0);
    return 0;
}

function structRead() -> int {
    print("[TESTING]: structRead");
    let p: Person = Person { name: "evan", age: 19 };
    print(p.name);
    print(p.age);
    return p.age;
}

function structWrite() -> int {
    print("[TESTING]: structWrite");
    let p: Person = Person { name: "evan", age: 19 };
    p.age = 20;
    print(p.age);
    return p.age;
}

function structMixed() -> int {
    print("[TESTING]: structMixed");
    let s: Stats = Stats { score: 100, ratio: 4.5, active: true };
    print(s.score);
    print(s.ratio);
    print(s.active);
    return s.score;
}

function start() -> int {
    if (valInt() != 4) { return 1; }
    if (valFloat() != 4) { return 2; }
    if (valChar() != 3) { return 3; }
    if (valString() != 3) { return 4; }
    if (valBool() != 2) { return 5; }
    if (addInt() != 15) { return 6; }
    if (subInt() != 13) { return 7; }
    if (mulInt() != 42) { return 8; }
    if (divInt() != 5) { return 9; }
    if (negInt() != -15) { return 10; }
    if (treeInt() != 14) { return 11; }
    if (addFloat() != 2) { return 12; }
    if (subFloat() != 2) { return 13; }
    if (mulFloat() != 2) { return 14; }
    if (divFloat() != 2) { return 15; }
    if (negFloat() != 1) { return 16; }
    if (eqInt() != 1) { return 17; }
    if (orderInt() != 3) { return 18; }
    if (eqFloat() != 2) { return 19; }
    if (orderFloat() != 2) { return 20; }
    if (eqChar() != 1) { return 21; }
    if (eqString() != 2) { return 22; }
    if (andLogic() != 1) { return 23; }
    if (orLogic() != 1) { return 24; }
    if (notLogic() != 1) { return 25; }
    if (compoundLogic() != 2) { return 26; }
    if (andBit() != 8) { return 27; }
    if (orBit() != 29) { return 28; }
    if (xorBit() != 21) { return 29; }
    if (notBit() != -2) { return 30; }
    if (shiftLeft() != 16) { return 31; }
    if (shiftRight() != 8) { return 32; }
    if (assignSimple() != 202) { return 33; }
    if (assignChained() != 99) { return 34; }
    if (assignAccumulator() != 80) { return 35; }
    if (blockIsolation() != 42) { return 36; }
    if (branchIf() != 77) { return 37; }
    if (branchIfElse() != 22) { return 38; }
    if (loopWhile() != 2) { return 39; }
    if (loopFor() != 2) { return 40; }
    if (arrayRead() != 200) { return 41; }
    if (arrayWrite() != 999) { return 42; }
    if (arrayEmpty() != 0) { return 43; }
    if (structRead() != 19) { return 44; }
    if (structWrite() != 20) { return 45; }
    if (structMixed() != 100) { return 46; }
    return 0;
}
