// E1008: comparison between mismatched types.
function start() -> int {
    if (5 == 3.0) {
        return 1;
    }
    return 0;
}
