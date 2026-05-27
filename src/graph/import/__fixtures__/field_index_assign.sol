struct Point { x: int, y: int }

function start() -> int {
    let p: Point = Point { x: 0, y: 0 };
    p.x = 42;
    p.y = 99;
    let arr: [3]int = [1, 2, 3];
    arr[0] = 100;
    return p.x;
}
