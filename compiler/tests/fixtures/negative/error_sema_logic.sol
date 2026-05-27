// E1009: logical operators require boolean operands.
function start() -> int {
    if (1 && 2) {
        return 1;
    }
    return 0;
}
