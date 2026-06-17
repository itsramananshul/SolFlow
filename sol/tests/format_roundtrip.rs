use openprem_sol_v2::{format_program, Parser};

fn roundtrip(src: &str) {
    let a = Parser::new(src).parse().unwrap_or_else(|e| panic!("parse1 failed: {e}"));
    let printed = format_program(&a);
    let b = Parser::new(&printed).parse().unwrap_or_else(|e| panic!("re-parse failed: {e}\n--- formatted ---\n{printed}"));
    assert_eq!(a, b, "AST changed after formatting.\n--- formatted ---\n{printed}");
}

#[test]
fn roundtrips_cover_the_language() {
    roundtrip(r#"import slack; workflow "main" { let x: int = 2 + 3 * 4; if (x > 10) { call("alert.send", {msg: "hi", n: x}); } else { print(x); } }"#);
    roundtrip(r#"struct S { id: str; t: float; } enum E { A; B; } workflow "w" { let s = S { id: "r", t: 1.5 }; for i in [1, 2, 3] { print(i); } return s; }"#);
    roundtrip(r#"import "send" from discord; workflow "m" { discord.send({msg: "x"}); while (true) { emit "tick"; } }"#);
    roundtrip(r#"workflow "n" { let a = [1, "two", true]; let b = a[0]; let c = -a[0]; let d = !true; }"#);
}
