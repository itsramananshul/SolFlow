workflow "loops" {
    let x: int = 0;
    if (x == 0) {
        print("zero");
    } else {
        print("nonzero");
    }
    while (x < 5) {
        print(x);
    }
    for item in [1, 2, 3] {
        print(item);
    }
    return x;
}
