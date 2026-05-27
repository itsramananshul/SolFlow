struct Point {
    x: int,
    y: int,
}

function test_deep_block() -> int {
    let x: int = 1;
    {
        let y: int = 2;
        {
            let z: int = 3;
            x = x + y + z;
        }
    }
    return x;
}

function test_block_in_if() -> int {
    if (true) {
        let x: int = 42;
        return x;
    }
    return 0;
}

function test_block_in_while() -> int {
    let result: int = 0;
    while result < 3 {
        let tmp: int = result + 10;
        result = result + 1;
    }
    return result;
}

function test_scope_in_for() -> int {
    let arr: []int = [1, 2, 3];
    let sum: int = 0;
    for item in arr {
        let doubled: int = item * 2;
        sum = sum + doubled;
    }
    return sum;
}

function test_var_in_if_branch() -> int {
    if (true) {
        let x: int = 100;
        return x;
    }
    return 0;
}

function test_var_in_else_branch() -> int {
    if (false) {
        return 0;
    } else {
        let x: int = 200;
        return x;
    }
}

function test_nested_blocks() -> int {
    let a: int = 1;
    {
        let b: int = 2;
        {
            let c: int = 3;
            a = a + b + c;
        }
    }
    return a;
}

function test_block_expr() -> int {
    let x: int = 10;
    {
        let y: int = 20;
        x + y;
    }
    return x;
}

function test_block_return() -> int {
    {
        return 42;
    }
    return 0;
}

function test_mixed_scopes() -> int {
    let x: int = 0;
    let i: int = 0;
    while i < 3 {
        let y: int = i * 2;
        x = x + y;
        i = i + 1;
    }
    return x;
}

function test_block_vars_isolated() -> int {
    let outer_val: int = 5;
    {
        let inner_val: int = 10;
        outer_val = outer_val + inner_val;
    }
    return outer_val;
}

function start() -> int {
    if (test_deep_block() != 6) { return 1; }
    if (test_block_in_if() != 42) { return 2; }
    if (test_block_in_while() != 3) { return 3; }
    if (test_scope_in_for() != 12) { return 4; }
    if (test_var_in_if_branch() != 100) { return 5; }
    if (test_var_in_else_branch() != 200) { return 6; }
    if (test_nested_blocks() != 6) { return 7; }
    if (test_block_expr() != 10) { return 8; }
    if (test_block_return() != 42) { return 9; }
    if (test_mixed_scopes() != 6) { return 10; }
    if (test_block_vars_isolated() != 15) { return 11; }
    return 0;
}
