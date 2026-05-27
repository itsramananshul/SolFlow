struct Point {
    x: int,
    y: int,
}

enum MyEnum {
    Inactive,
    Active,
    Busy,
}

function test_large_numbers() -> int {
    let a: int = 2147483647;
    let b: int = 2147483647;
    return a + b;
}

function test_negative_zero() -> int {
    return -0;
}

function test_chained_assign() -> int {
    let a: int = 0;
    let b: int = 0;
    let c: int = 0;
    a = b = c = 42;
    if (a != 42) { return 0; }
    if (b != 42) { return 0; }
    if (c != 42) { return 0; }
    return 1;
}

function test_assign_to_self() -> int {
    let x: int = 10;
    x = x;
    return x;
}

function test_assign_expr_result() -> int {
    let x: int = 0;
    let y: int = x = 5;
    return x + y;
}

function test_while_with_complex_cond() -> int {
    let i: int = 0;
    while i < 10 && i >= 0 {
        i = i + 3;
    }
    return i;
}

function test_mixed_ops() -> int {
    return (1 + 2 * 3 - 4 / 2) << 1;
}

function test_nested_arith() -> int {
    return ((1 + 2) * (3 + 4) - (5 + 6)) / 2;
}

function test_enum_var() -> int {
    let status: int = MyEnum::Active;
    return status;
}

function test_enum_inactive() -> int {
    return MyEnum::Inactive;
}

function test_enum_busy() -> int {
    return MyEnum::Busy;
}

function test_bool_negation() -> int {
    if (!true) { return 0; }
    if (!false) { return 1; }
    return 0;
}

function test_double_neg() -> int {
    return -(-10);
}

function test_not_bitwise() -> int {
    return ~(~5);
}

function test_many_params(a: int, b: int, c: int) -> int {
    return a - b + c;
}

function test_return_expr() -> int {
    return (1 + 2) * 3;
}

function test_empty_body_func() -> int {
    return 1;
}

function test_nested_return() -> int {
    if (true) {
        if (true) {
            if (true) {
                return 7;
            }
        }
    }
    return 0;
}

function start() -> int {
    if (test_large_numbers() != 4294967294) { return 1; }
    if (test_negative_zero() != 0) { return 2; }
    if (test_chained_assign() != 1) { return 3; }
    if (test_assign_to_self() != 10) { return 4; }
    if (test_assign_expr_result() != 10) { return 5; }
    if (test_while_with_complex_cond() != 12) { return 6; }
    if (test_mixed_ops() != 10) { return 7; }
    if (test_nested_arith() != 5) { return 8; }
    if (test_enum_var() != 5) { return 9; }
    if (test_enum_inactive() != 3) { return 10; }
    if (test_enum_busy() != 6) { return 11; }
    if (test_bool_negation() != 1) { return 12; }
    if (test_double_neg() != 10) { return 13; }
    if (test_not_bitwise() != 5) { return 14; }
    if (test_many_params(5, 3, 2) != 4) { return 15; }
    if (test_return_expr() != 9) { return 16; }
    if (test_empty_body_func() != 1) { return 17; }
    if (test_nested_return() != 7) { return 18; }
    return 0;
}
