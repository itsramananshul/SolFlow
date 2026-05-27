// E1018: wrong argument type.
function add(a: int, b: int) -> int {
    return a + b;
}

function start() -> int {
    return add(1, 2.0);
}
