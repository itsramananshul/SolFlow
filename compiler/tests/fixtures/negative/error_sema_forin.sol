// E1004: for-in target must be an array.
function start() -> int {
    for x in 42 {
        return x;
    }
    return 0;
}
