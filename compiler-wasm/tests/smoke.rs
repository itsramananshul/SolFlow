use solflow_compiler_wasm::*;

#[test]
fn run_prints_and_returns() {
    let out = run_source_json(r#"workflow "main" { print("hello", 42); let x = 2 + 3; print(x); return x; }"#);
    println!("RUN => {out}");
    assert!(out.contains("\"ok\":true"), "{out}");
    assert!(out.contains("hello 42"), "{out}");
    assert!(out.contains("\"return_value\":5"), "{out}");
    assert!(out.contains("\"run\":{"), "{out}");
}
#[test]
fn external_action_blocked() {
    let out = run_source_json(r#"workflow "main" { call("discord.send", {msg: "hi"}); }"#);
    println!("BLOCKED => {out}");
    assert!(out.contains("\"kind\":\"ExtCallBlocked\""), "{out}");
    assert!(out.contains("discord.send"), "{out}");
}
#[test]
fn parse_error_is_reported() {
    let out = parse_source_json("workflow \"x\" { let = ; }");
    println!("PARSE => {out}");
    assert!(out.contains("\"ok\":false") && out.contains("\"phase\":\"Parser\""), "{out}");
}
#[test]
fn analyze_lists_capabilities() {
    let out = analyze_source_json(r#"import slack; workflow "main" { call("sensor.read", {}); slack.post({}); }"#);
    println!("ANALYZE => {out}");
    assert!(out.contains("sensor.read") && out.contains("\"program\""), "{out}");
}
