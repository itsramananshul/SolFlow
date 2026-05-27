function start() -> int {
    let x: int = 0;
    if (x == 0) {
        print("zero");
    } else {
        print("nonzero");
    }
    while (x < 5) {
        x = x + 1;
    }
    for item in [1, 2, 3] {
        print(item);
    }
    return x;
}
