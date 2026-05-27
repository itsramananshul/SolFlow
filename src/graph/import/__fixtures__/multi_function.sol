function add(a: int, b: int) -> int {
    return a + b;
}

function notify(msg: str) -> int {
    print(msg);
    return 0;
}

function start() -> int {
    // Statement-level calls become `call` nodes.
    notify("starting");
    notify("running");
    let result: int = 42;
    return result;
}
