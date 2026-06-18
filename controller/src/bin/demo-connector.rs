//! A tiny demo connector for SolFlow capability/provider workflows.
//!
//! It speaks the controller's connector contract: the controller POSTs
//! `{ "module": <str>, "function": <str>, "params": <json> }` and the
//! connector returns a JSON value that becomes the SOL `call(...)` result.
//!
//! Run it, then register it so the Local Controller resolves `demo.*`:
//!
//! ```sh
//! cargo run -p solflow_controller --bin demo-connector            # :8099
//! # in the controller's environment:
//! SOLFLOW_CONNECTORS='{"demo":"http://127.0.0.1:8099"}' \
//!   cargo run -p solflow_controller --bin solflow-controller
//! ```
//!
//! Functions:
//!   - `echo`     → returns `params` unchanged
//!   - `add`      → returns `params.a + params.b` (ints)
//!   - `greeting` → returns `"hello, <params.name>"`
//!
//! Bind address is `DEMO_CONNECTOR_BIND` (default `127.0.0.1:8099`).

use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Invocation {
    #[serde(default)]
    module: String,
    #[serde(default)]
    function: String,
    #[serde(default)]
    params: Value,
}

async fn invoke(Json(inv): Json<Invocation>) -> Json<Value> {
    let p = &inv.params;
    let out = match inv.function.as_str() {
        "echo" => p.clone(),
        "add" => {
            let a = p.get("a").and_then(Value::as_i64).unwrap_or(0);
            let b = p.get("b").and_then(Value::as_i64).unwrap_or(0);
            json!(a + b)
        }
        "greeting" => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or("world");
            json!(format!("hello, {name}"))
        }
        // Unknown function: echo back a tagged value so the caller still
        // gets a usable response rather than a hard failure.
        other => json!({ "unknown_function": other, "module": inv.module }),
    };
    Json(out)
}

#[tokio::main]
async fn main() {
    let bind = std::env::var("DEMO_CONNECTOR_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8099".to_string());
    let app = Router::new().route("/", post(invoke));
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|e| panic!("demo-connector: cannot bind {bind}: {e}"));
    println!("demo-connector listening on http://{bind} (functions: echo, add, greeting)");
    axum::serve(listener, app).await.expect("demo-connector serve failed");
}
