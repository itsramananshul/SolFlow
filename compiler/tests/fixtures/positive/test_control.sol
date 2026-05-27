function test_if_true() -> int {
    if (true) { return 1; }
    return 0;
}

function test_if_false() -> int {
    if (false) { return 0; }
    return 1;
}

function test_if_else_true() -> int {
    if (true) { return 1; } else { return 0; }
}

function test_if_else_false() -> int {
    if (false) { return 0; } else { return 1; }
}

function test_nested_if() -> int {
    if (true) {
        if (true) { return 1; }
    }
    return 0;
}

function test_nested_if_else() -> int {
    if (true) {
        if (false) { return 0; } else { return 1; }
    }
    return 0;
}

function test_while_basic() -> int {
    let i: int = 0;
    while i < 5 {
        i = i + 1;
    }
    return i;
}

function test_while_zero() -> int {
    while false {
        return 0;
    }
    return 1;
}

function test_while_nested() -> int {
    let i: int = 0;
    let j: int = 0;
    while i < 3 {
        i = i + 1;
        while j < i {
            j = j + 1;
        }
    }
    return i + j;
}

function test_for_basic() -> int {
    let arr: []int = [10, 20, 30];
    let sum: int = 0;
    for item in arr {
        sum = sum + item;
    }
    return sum;
}

function test_for_empty() -> int {
    let arr: []int = [];
    for item in arr {
        return 0;
    }
    return 1;
}

function test_for_single() -> int {
    let arr: []int = [99];
    for item in arr {
        return item;
    }
    return 0;
}

function test_for_nested() -> int {
    let outer: []int = [1, 2];
    let inner: []int = [10, 20];
    let sum: int = 0;
    for a in outer {
        for b in inner {
            sum = sum + a + b;
        }
    }
    return sum;
}

function test_return_in_if() -> int {
    if (true) { return 100; }
    return 0;
}

function test_return_in_while() -> int {
    while true {
        return 50;
    }
    return 0;
}

function test_chain_compare() -> int {
    if (10 > 2 && 5 >= 5) { return 1; }
    return 0;
}

function test_mixed_logic() -> int {
    if ((true || false) && !false) { return 1; }
    return 0;
}

function test_deep_nest() -> int {
    let x: int = 0;
    if (true) {
        if (true) {
            if (true) {
                if (true) {
                    x = 42;
                }
            }
        }
    }
    return x;
}

function start() -> int {
    if (test_if_true() != 1) { return 1; }
    if (test_if_false() != 1) { return 2; }
    if (test_if_else_true() != 1) { return 3; }
    if (test_if_else_false() != 1) { return 4; }
    if (test_nested_if() != 1) { return 5; }
    if (test_nested_if_else() != 1) { return 6; }
    if (test_while_basic() != 5) { return 7; }
    if (test_while_zero() != 1) { return 8; }
    if (test_while_nested() != 6) { return 9; }
    if (test_for_basic() != 60) { return 10; }
    if (test_for_empty() != 1) { return 11; }
    if (test_for_single() != 99) { return 12; }
    if (test_for_nested() != 66) { return 13; }
    if (test_return_in_if() != 100) { return 14; }
    if (test_return_in_while() != 50) { return 15; }
    if (test_chain_compare() != 1) { return 16; }
    if (test_mixed_logic() != 1) { return 17; }
    if (test_deep_nest() != 42) { return 18; }
    return 0;
}
