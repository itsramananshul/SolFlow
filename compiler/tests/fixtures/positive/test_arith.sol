function test_add() -> int { return 15 + 27; }
function test_sub() -> int { return 100 - 37; }
function test_mul() -> int { return 7 * 8; }
function test_div() -> int { return 100 / 3; }
function test_neg() -> int { return -42; }
function test_precedence_mul_before_add() -> int { return 2 + 3 * 4; }
function test_precedence_parens() -> int { return (2 + 3) * 4; }
function test_left_assoc_sub() -> int { return 10 - 5 - 2; }
function test_zero() -> int { return 0; }
function test_large() -> int { return 1000000; }
function test_neg_add() -> int { return -5 + 5; }
function test_neg_sub() -> int { return -5 - -5; }
function test_neg_mul() -> int { return -4 * 5; }
function test_neg_div() -> int { return -20 / 5; }
function test_mul_zero() -> int { return 100 * 0; }
function test_complex_arith() -> int { return (1 + 2) * (3 - 4) / -1; }

function test_int_eq_true() -> int { if (42 == 42) { return 1; } return 0; }
function test_int_eq_false() -> int { if (42 == 99) { return 0; } return 1; }
function test_int_neq_true() -> int { if (42 != 99) { return 1; } return 0; }
function test_int_neq_false() -> int { if (42 != 42) { return 0; } return 1; }
function test_int_gt() -> int { if (10 > 2) { return 1; } return 0; }
function test_int_gte() -> int { if (5 >= 5) { return 1; } return 0; }
function test_int_lt() -> int { if (3 < 1) { return 0; } return 1; }
function test_int_lte() -> int { if (4 <= 5) { return 1; } return 0; }

function test_and_true() -> int { if (true && true) { return 1; } return 0; }
function test_and_false() -> int { if (true && false) { return 0; } return 1; }
function test_or_true() -> int { if (true || false) { return 1; } return 0; }
function test_or_false() -> int { if (false || false) { return 0; } return 1; }
function test_not_true() -> int { if (!false) { return 1; } return 0; }
function test_not_false() -> int { if (!true) { return 0; } return 1; }

function test_bit_and() -> int { return 12 & 25; }
function test_bit_or() -> int { return 12 | 25; }
function test_bit_xor() -> int { return 12 ^ 25; }
function test_bit_not() -> int { return ~1; }
function test_shl() -> int { return 1 << 4; }
function test_shr() -> int { return 32 >> 2; }
function test_bit_and_zero() -> int { return 255 & 0; }
function test_bit_or_all() -> int { return 0 | 255; }
function test_bit_xor_self() -> int { return 123 ^ 123; }
function test_shl_large() -> int { return 1 << 10; }
function test_shr_large() -> int { return 1024 >> 10; }

function test_float_count() -> int {
    print(1.5 + 2.25);
    print(0.0 + -0.5);
    print(10.0 / 4.0);
    print(-7.5 / 2.5);
    return 4;
}

function test_char_count() -> int {
    print('A');
    print('z');
    print('?');
    return 3;
}

function test_string_count() -> int {
    print("");
    print("hello");
    print("world!");
    return 3;
}

function test_bool_count() -> int {
    print(true);
    print(false);
    return 2;
}

function start() -> int {
    if (test_add() != 42) { return 1; }
    if (test_sub() != 63) { return 2; }
    if (test_mul() != 56) { return 3; }
    if (test_div() != 33) { return 4; }
    if (test_neg() != -42) { return 5; }
    if (test_precedence_mul_before_add() != 14) { return 6; }
    if (test_precedence_parens() != 20) { return 7; }
    if (test_left_assoc_sub() != 3) { return 8; }
    if (test_zero() != 0) { return 9; }
    if (test_large() != 1000000) { return 10; }
    if (test_neg_add() != 0) { return 11; }
    if (test_neg_sub() != 0) { return 12; }
    if (test_neg_mul() != -20) { return 13; }
    if (test_neg_div() != -4) { return 14; }
    if (test_mul_zero() != 0) { return 15; }
    if (test_complex_arith() != 3) { return 16; }

    if (test_int_eq_true() != 1) { return 17; }
    if (test_int_eq_false() != 1) { return 18; }
    if (test_int_neq_true() != 1) { return 19; }
    if (test_int_neq_false() != 1) { return 20; }
    if (test_int_gt() != 1) { return 21; }
    if (test_int_gte() != 1) { return 22; }
    if (test_int_lt() != 1) { return 23; }
    if (test_int_lte() != 1) { return 24; }

    if (test_and_true() != 1) { return 25; }
    if (test_and_false() != 1) { return 26; }
    if (test_or_true() != 1) { return 27; }
    if (test_or_false() != 1) { return 28; }
    if (test_not_true() != 1) { return 29; }
    if (test_not_false() != 1) { return 30; }

    if (test_bit_and() != 8) { return 31; }
    if (test_bit_or() != 29) { return 32; }
    if (test_bit_xor() != 21) { return 33; }
    if (test_bit_not() != -2) { return 34; }
    if (test_shl() != 16) { return 35; }
    if (test_shr() != 8) { return 36; }
    if (test_bit_and_zero() != 0) { return 37; }
    if (test_bit_or_all() != 255) { return 38; }
    if (test_bit_xor_self() != 0) { return 39; }
    if (test_shl_large() != 1024) { return 40; }
    if (test_shr_large() != 1) { return 41; }

    if (test_float_count() != 4) { return 42; }
    if (test_char_count() != 3) { return 43; }
    if (test_string_count() != 3) { return 44; }
    if (test_bool_count() != 2) { return 45; }

    return 0;
}
