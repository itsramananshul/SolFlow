// Mixed-error recovery: three independent top-level functions, each
// containing one semantic error. After analyzer recovery the compiler
// should report all three diagnostics rather than aborting on the
// first.
function a() -> int {
    return undefined_a;
}

function b() -> int {
    return undefined_b;
}

function c() -> int {
    return undefined_c;
}

function start() -> int {
    return 0;
}
