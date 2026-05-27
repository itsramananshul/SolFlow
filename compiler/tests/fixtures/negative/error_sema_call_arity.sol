// E1017: wrong number of arguments.
function add(a: int, b: int) -> int {
    return a + b;
}

function start() -> int {
    return add(1, 2, 3);
}
