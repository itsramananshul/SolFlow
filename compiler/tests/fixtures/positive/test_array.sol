struct Point {
    x: int,
    y: int,
}

function test_array_basic() -> int {
    let arr: []int = [10, 20, 30];
    return arr[0] + arr[1] + arr[2];
}

function test_array_write() -> int {
    let arr: []int = [1, 2, 3];
    arr[0] = 100;
    arr[1] = 200;
    arr[2] = 300;
    return arr[0] + arr[1] + arr[2];
}

function test_array_single() -> int {
    let arr: []int = [42];
    return arr[0];
}

function test_array_many() -> int {
    let arr: []int = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let sum: int = 0;
    let i: int = 0;
    while i < 10 {
        sum = sum + arr[i];
        i = i + 1;
    }
    return sum;
}

function test_array_iterate() -> int {
    let arr: []int = [5, 10, 15, 20];
    let sum: int = 0;
    for item in arr {
        sum = sum + item;
    }
    return sum;
}

function test_array_in_loop() -> int {
    let i: int = 0;
    while i < 3 {
        let arr: []int = [i, i * 2, i * 3];
        if (arr[0] != i) { return 0; }
        if (arr[1] != i * 2) { return 0; }
        if (arr[2] != i * 3) { return 0; }
        i = i + 1;
    }
    return 1;
}

function test_array_write_then_read() -> int {
    let arr: []int = [0, 0, 0];
    arr[0] = 7;
    arr[1] = 8;
    arr[2] = 9;
    return arr[0] * 100 + arr[1] * 10 + arr[2];
}

function test_array_mutate_in_loop() -> int {
    let arr: []int = [1, 2, 3, 4, 5];
    let i: int = 0;
    while i < 5 {
        arr[i] = arr[i] * 2;
        i = i + 1;
    }
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4];
}

function test_array_bool() -> int {
    let arr: []bool = [true, false, true];
    let count: int = 0;
    for item in arr {
        if (item) { count = count + 1; }
    }
    return count;
}

function test_array_of_struct() -> int {
    let p1: Point = Point { x: 1, y: 2 };
    let p2: Point = Point { x: 3, y: 4 };
    let arr: []Point = [p1, p2];
    return arr[0].x + arr[0].y + arr[1].x + arr[1].y;
}

function test_array_swap() -> int {
    let arr: []int = [1, 2];
    let tmp: int = arr[0];
    arr[0] = arr[1];
    arr[1] = tmp;
    return arr[0] * 10 + arr[1];
}

function start() -> int {
    if (test_array_basic() != 60) { return 1; }
    if (test_array_write() != 600) { return 2; }
    if (test_array_single() != 42) { return 3; }
    if (test_array_many() != 45) { return 4; }
    if (test_array_iterate() != 50) { return 5; }
    if (test_array_in_loop() != 1) { return 6; }
    if (test_array_write_then_read() != 789) { return 7; }
    if (test_array_mutate_in_loop() != 30) { return 8; }
    if (test_array_bool() != 2) { return 9; }
    if (test_array_of_struct() != 10) { return 10; }
    if (test_array_swap() != 21) { return 11; }
    return 0;
}
