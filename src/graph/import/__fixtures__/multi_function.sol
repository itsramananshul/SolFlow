fn add(a: int, b: int) <- int {
    return a + b;
}

fn notify(msg: str) <- int {
    print(msg);
    return 0;
}

workflow "start" {
    notify("starting");
    notify("running");
    let result: int = 42;
    return result;
}
