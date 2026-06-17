struct Point {
    x: int;
    y: int;
}

enum Status {
    Active;
    Inactive;
}

workflow "main" {
    let p: Point = Point { x: 1, y: 2 };
    return 0;
}
