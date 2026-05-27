struct Point {
    x: int,
    y: int,
}

struct Empty {}

struct Person {
    name: str,
    age: int,
    active: bool,
}

struct Nested {
    p: Point,
    label: str,
}

function test_empty_struct() -> int {
    let e: Empty = Empty {};
    return 1;
}

function test_point() -> int {
    let p: Point = Point { x: 10, y: 20 };
    if (p.x != 10) { return 0; }
    if (p.y != 20) { return 0; }
    return 1;
}

function test_person() -> int {
    let p: Person = Person { name: "test", age: 25, active: true };
    if (p.name == "test") {
        if (p.age != 25) { return 0; }
        if (p.active != true) { return 0; }
        return 1;
    }
    return 0;
}

function test_mutate_field() -> int {
    let p: Point = Point { x: 1, y: 2 };
    p.x = 100;
    p.y = 200;
    if (p.x != 100) { return 0; }
    if (p.y != 200) { return 0; }
    return 1;
}

function test_mutate_person() -> int {
    let p: Person = Person { name: "old", age: 10, active: false };
    p.name = "new";
    p.age = 99;
    p.active = true;
    if (p.name == "new") {
        if (p.age != 99) { return 0; }
        if (p.active != true) { return 0; }
        return 1;
    }
    return 0;
}

function test_multiple_structs() -> int {
    let a: Point = Point { x: 1, y: 2 };
    let b: Point = Point { x: 3, y: 4 };
    return a.x + a.y + b.x + b.y;
}

function test_struct_in_func(p: Point) -> int {
    return p.x + p.y;
}

function test_pass_struct() -> int {
    let p: Point = Point { x: 5, y: 15 };
    return test_struct_in_func(p);
}

function test_struct_in_loop() -> int {
    let i: int = 0;
    while i < 5 {
        let p: Point = Point { x: i, y: i * 10 };
        if (p.x != i) { return 0; }
        if (p.y != i * 10) { return 0; }
        i = i + 1;
    }
    return 1;
}

function test_field_order() -> int {
    let p: Point = Point { y: 99, x: 11 };
    if (p.x != 11) { return 0; }
    if (p.y != 99) { return 0; }
    return 1;
}

function test_struct_in_struct() -> int {
    let n: Nested = Nested { p: Point { x: 7, y: 8 }, label: "point" };
    if (n.label == "point") {
        if (n.p.x != 7) { return 0; }
        if (n.p.y != 8) { return 0; }
        return 1;
    }
    return 0;
}

function test_swap_fields() -> int {
    let a: Point = Point { x: 1, y: 2 };
    let tmp: int = a.x;
    a.x = a.y;
    a.y = tmp;
    if (a.x != 2) { return 0; }
    if (a.y != 1) { return 0; }
    return 1;
}

function start() -> int {
    if (test_empty_struct() != 1) { return 1; }
    if (test_point() != 1) { return 2; }
    if (test_person() != 1) { return 3; }
    if (test_mutate_field() != 1) { return 4; }
    if (test_mutate_person() != 1) { return 5; }
    if (test_multiple_structs() != 10) { return 6; }
    if (test_pass_struct() != 20) { return 7; }
    if (test_struct_in_loop() != 1) { return 8; }
    if (test_field_order() != 1) { return 9; }
    if (test_struct_in_struct() != 1) { return 10; }
    if (test_swap_fields() != 1) { return 11; }
    return 0;
}
