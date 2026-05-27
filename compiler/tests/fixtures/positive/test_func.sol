function noop() -> int {
    return 0;
}

function identity(x: int) -> int {
    return x;
}

function add_two(a: int, b: int) -> int {
    return a + b;
}

function add_three(a: int, b: int, c: int) -> int {
    return a + b + c;
}

function sub_then_mul(a: int, b: int, c: int) -> int {
    return (a - b) * c;
}

function nested_calls() -> int {
    return add_two(add_two(1, 2), add_two(3, 4));
}

function deep_nested() -> int {
    return add_two(add_two(add_two(1, 2), 3), 4);
}

function factorial(n: int) -> int {
    if (n <= 1) { return 1; }
    return n * factorial(n - 1);
}

function fib(n: int) -> int {
    if (n <= 1) { return n; }
    return fib(n - 1) + fib(n - 2);
}

function call_many() -> int {
    let a: int = identity(42);
    let b: int = add_two(10, 20);
    let c: int = add_three(1, 2, 3);
    let d: int = sub_then_mul(10, 3, 2);
    return a + b + c + d;
}

function identity_bool(x: bool) -> int {
    if (x) { return 1; }
    return 0;
}

function sum_range(n: int) -> int {
    let total: int = 0;
    let i: int = 1;
    while i <= n {
        total = total + i;
        i = i + 1;
    }
    return total;
}

function double(x: int) -> int { return x * 2; }
function quadruple(x: int) -> int { return double(double(x)); }

function test_void_func() -> int {
    print("void func ran");
    return 1;
}

function multi_param_sum(a: int, b: int, c: int, d: int) -> int {
    return a + b + c + d;
}

function early_return() -> int {
    return 99;
    return 0;
}

function start() -> int {
    if (noop() != 0) { return 1; }
    if (identity(42) != 42) { return 2; }
    if (identity(-7) != -7) { return 3; }
    if (add_two(10, 20) != 30) { return 4; }
    if (add_three(1, 2, 3) != 6) { return 5; }
    if (sub_then_mul(10, 3, 2) != 14) { return 6; }
    if (nested_calls() != 10) { return 7; }
    if (deep_nested() != 10) { return 8; }
    if (factorial(5) != 120) { return 9; }
    if (factorial(1) != 1) { return 10; }
    if (factorial(0) != 1) { return 11; }
    if (fib(7) != 13) { return 12; }
    if (call_many() != 92) { return 13; }
    if (identity_bool(true) != 1) { return 14; }
    if (identity_bool(false) != 0) { return 15; }
    if (sum_range(10) != 55) { return 16; }
    if (sum_range(1) != 1) { return 17; }
    if (quadruple(5) != 20) { return 18; }
    if (test_void_func() != 1) { return 19; }
    if (multi_param_sum(1, 2, 3, 4) != 10) { return 20; }
    if (early_return() != 99) { return 21; }
    return 0;
}
