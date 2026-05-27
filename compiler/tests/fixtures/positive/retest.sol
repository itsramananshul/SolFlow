function sub_func(x: int) -> int {
    return x * 2;
}

function start() -> int {
    let y: int = sub_func(9);
    print(y);
}
