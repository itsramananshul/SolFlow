use solflow_compiler_wasm::*;

#[test]
fn run_prints_and_returns() {
    let out = run_source_json(r#"workflow "main" { print("hello", 42); let x = 2 + 3; print(x); return x; }"#, "");
    println!("RUN => {out}");
    assert!(out.contains("\"ok\":true"), "{out}");
    assert!(out.contains("hello 42"), "{out}");
    assert!(out.contains("\"return_value\":5"), "{out}");
    assert!(out.contains("\"run\":{"), "{out}");
}
#[test]
fn external_action_blocked() {
    let out = run_source_json(r#"workflow "main" { call("discord.send", {msg: "hi"}); }"#, "");
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

#[test]
fn run_emits_a_non_empty_trace() {
    let out = run_source_json(r#"workflow "main" { print("a"); return 0; }"#, "");
    println!("TRACE => {out}");
    assert!(out.contains("\"trace\":["), "{out}");
    // A real run must never produce an empty trace.
    assert!(!out.contains("\"trace\":[]"), "trace was empty: {out}");
    // Trace steps carry a 1-based source line for click-to-highlight.
    assert!(out.contains("\"line\":1"), "{out}");
    assert!(out.contains("\"kind\":\"stmt\""), "{out}");
}

#[test]
fn helper_call_shows_call_and_return_in_trace() {
    let src = r#"
        fn dbl(x: int) <- int { return x * 2; }
        workflow "main" { return dbl(21); }
    "#;
    let out = run_source_json(src, "");
    println!("HELPER TRACE => {out}");
    assert!(out.contains("\"return_value\":42"), "{out}");
    assert!(out.contains("\"kind\":\"call\""), "{out}");
    assert!(out.contains("\"kind\":\"return\""), "{out}");
    // The callee name is carried as the call detail and the active function.
    assert!(out.contains("dbl"), "{out}");
}

#[test]
fn payload_injection_resolves_payload_variable() {
    let src = r#"workflow "main" { let t: int = payload.total; print(t); return t; }"#;
    // Without a payload the run fails clearly.
    let missing = run_source_json(src, "");
    assert!(missing.contains("variable 'payload' not found"), "{missing}");
    // With a test payload injected, it runs and returns the value.
    let ok = run_source_json(src, r#"{ "total": 1200 }"#);
    assert!(ok.contains("\"return_value\":1200"), "{ok}");
}

#[test]
fn blocked_external_call_traces_extcall_and_points_at_call_site() {
    let src = r#"import http; workflow "main" { http.fetch({ url: "x" }); return 0; }"#;
    let out = run_source_json(src, "");
    println!("BLOCK TRACE => {out}");
    // Browser sim blocks the call, but the trace still shows the external
    // call and a source mapped error at the call site.
    assert!(out.contains("\"kind\":\"extcall\""), "{out}");
    assert!(out.contains("\"kind\":\"error\""), "{out}");
    assert!(out.contains("blocked in Browser Simulation"), "{out}");
    assert!(out.contains("\"runtime_error_source_span\":{"), "{out}");
}

#[test]
fn runtime_error_trace_points_at_failing_statement() {
    let src = r#"
        fn risky(n: int) <- int { return n / 0; }
        workflow "main" { return risky(10); }
    "#;
    let out = run_source_json(src, "");
    println!("ERR TRACE => {out}");
    assert!(out.contains("\"kind\":\"error\""), "{out}");
    assert!(out.contains("division by zero"), "{out}");
    // The failing statement's span is surfaced for highlighting.
    assert!(out.contains("\"runtime_error_source_span\":{"), "{out}");
}
