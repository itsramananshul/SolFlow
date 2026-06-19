//! OpenPrem SDK provider protocol: SolFlow's canonical provider system.
//!
//! SolFlow's controller plays the role of one OpenPrem controller. Real
//! upstream agents (the Python / TypeScript / Rust / ... SDKs) register with
//! it via `POST /register` and SolFlow invokes them directly using the
//! upstream controller-to-agent wire contract. This is the primary external
//! call path; the env-based `SOLFLOW_CONNECTORS` registry in `canonical_exec`
//! remains only as an internal/dev/test fallback.
//!
//! ## Wire contract (mirrors `openprem-controller-v2`)
//!
//! Registration body (`api.rs::RegisterRequest` upstream):
//! ```json
//! { "name": "printer",
//!   "actions": [{"name": "print"}],
//!   "endpoint": "http://127.0.0.1:9301",
//!   "endpoints": {"http": {"url": "http://127.0.0.1:9301"}},
//!   "public_key": "<base64 ed25519>" }
//! ```
//! Each action is stored under BOTH `"name.action"` and the bare `"action"`,
//! matching upstream's `local_caps` double-keying.
//!
//! Invocation: POST the agent's registered endpoint at its root with the
//! params flattened and the capability merged in, exactly as upstream's
//! `invoke_local_action`:
//! ```text
//! object params -> { ...params, "capability": "<cap>" }
//! scalar params -> { "capability": "<cap>", "params": <scalar> }
//! ```
//!
//! ## Auth
//!
//! SolFlow deliberately omits `controller_public_key` from the `/register`
//! response. Upstream Python/Rust agents only enforce Ed25519 request
//! signatures once they receive a controller public key, so omitting it keeps
//! real agents running unauthenticated in local/dev mode. Signing is future
//! work.

use crate::canonical_exec::{json_to_value, value_to_json};
use openprem_sol_v2::Value;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

/// A resolved agent action route: which agent owns it and where to POST.
#[derive(Clone, Debug)]
struct Route {
    agent: String,
    endpoint: String,
}

/// One agent's registration, retained for the `/providers` listing and to
/// replace prior routes when an agent re-registers.
#[derive(Clone, Debug)]
pub struct AgentRegistration {
    /// Application name (the `name` in `/register`).
    pub name: String,
    /// Endpoint URL SolFlow POSTs invocations to.
    pub endpoint: String,
    /// Bare action names the agent exposes (e.g. `["print"]`).
    pub actions: Vec<String>,
    /// Whether the agent supplied a public key. SolFlow never returns a
    /// controller public key (agents stay unauthenticated), but surfacing
    /// this lets the UI show that an agent is signing-capable.
    pub has_public_key: bool,
}

/// Body of `POST /register`. Tolerant of the upstream legacy (`endpoint`
/// string) and multi-transport (`endpoints` map) shapes simultaneously.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    #[serde(default)]
    pub actions: Vec<ActionSpec>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointInfo>,
    #[serde(default)]
    pub public_key: Option<String>,
}

/// An action entry, accepting both `{"name": "print"}` and a bare `"print"`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ActionSpec {
    Named { name: String },
    Bare(String),
}

impl ActionSpec {
    fn name(&self) -> String {
        match self {
            ActionSpec::Named { name } => name.clone(),
            ActionSpec::Bare(s) => s.clone(),
        }
    }
}

/// One transport's endpoint info (e.g. `endpoints.http`).
#[derive(Debug, Default, Deserialize)]
pub struct EndpointInfo {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub path: Option<String>,
}

/// Resolve the endpoint URL from a registration, preferring an explicit
/// `endpoint`, then `endpoints.http.url`, then a `tcp` host/port pair.
fn resolve_endpoint(req: &RegisterRequest) -> Option<String> {
    if let Some(e) = req.endpoint.as_ref().filter(|s| !s.trim().is_empty()) {
        return Some(e.clone());
    }
    if let Some(http) = req.endpoints.get("http") {
        if let Some(url) = http.url.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(url.clone());
        }
    }
    if let Some(tcp) = req.endpoints.get("tcp") {
        if let (Some(host), Some(port)) = (tcp.host.as_ref(), tcp.port) {
            return Some(format!("tcp://{host}:{port}"));
        }
    }
    None
}

/// In-memory provider registry. Process-global in production (one controller,
/// one registry); constructed directly in tests for hermetic coverage.
#[derive(Default)]
pub struct Registry {
    /// `"name.action"` -> route.
    by_capability: HashMap<String, Route>,
    /// bare `"action"` -> route (upstream double-keys; last writer wins).
    by_action: HashMap<String, Route>,
    /// agent name -> registration.
    agents: HashMap<String, AgentRegistration>,
}

impl Registry {
    /// Register (or re-register) an agent. Re-registration replaces the
    /// agent's prior routes so a restarted agent on a new port is picked up.
    pub fn register(&mut self, req: RegisterRequest) -> Result<AgentRegistration, String> {
        if req.name.trim().is_empty() {
            return Err("registration is missing `name`".into());
        }
        let endpoint = resolve_endpoint(&req).ok_or_else(|| {
            "registration is missing an endpoint (set `endpoint` or `endpoints.http.url`)"
                .to_string()
        })?;
        let actions: Vec<String> = req
            .actions
            .iter()
            .map(|a| a.name())
            .filter(|s| !s.trim().is_empty())
            .collect();

        // Drop the agent's previous routes so re-registration is clean.
        if let Some(prev) = self.agents.remove(&req.name) {
            for a in &prev.actions {
                self.by_capability.remove(&format!("{}.{}", prev.name, a));
                if self
                    .by_action
                    .get(a)
                    .map(|r| r.agent == prev.name)
                    .unwrap_or(false)
                {
                    self.by_action.remove(a);
                }
            }
        }

        for a in &actions {
            let route = Route {
                agent: req.name.clone(),
                endpoint: endpoint.clone(),
            };
            self.by_capability
                .insert(format!("{}.{}", req.name, a), route.clone());
            // Bare action: last registration wins, matching upstream.
            self.by_action.insert(a.clone(), route);
        }

        let reg = AgentRegistration {
            name: req.name.clone(),
            endpoint,
            actions,
            has_public_key: req.public_key.is_some(),
        };
        self.agents.insert(req.name.clone(), reg.clone());
        Ok(reg)
    }

    /// Resolve a `(module, func)` split capability to `(canonical_capability,
    /// endpoint)`. Handles the dotted (`numbers.get`), namespace-member
    /// (`printer::print` -> module/func `printer`/`print`), and bare-string
    /// (`produce_tomato` -> module/func `produce_tomato`/``) forms.
    pub fn resolve(&self, module: &str, func: &str) -> Option<(String, String)> {
        if !func.is_empty() {
            let cap = format!("{module}.{func}");
            if let Some(r) = self.by_capability.get(&cap) {
                return Some((cap, r.endpoint.clone()));
            }
            // Some agents register an action whose name is itself dotted, e.g.
            // `@capability("sensor.temperature")`. Then the bare action key is
            // `sensor.temperature`, which equals the `module.func` the workflow
            // wrote. Match it and send that capability (the agent's router
            // suffix-matches its `name.sensor.temperature` registration).
            if let Some(r) = self.by_action.get(&cap) {
                return Some((cap, r.endpoint.clone()));
            }
            // Otherwise the action may be registered under a different agent
            // name; fall back to the bare action key and send that.
            if let Some(r) = self.by_action.get(func) {
                return Some((func.to_string(), r.endpoint.clone()));
            }
            None
        } else {
            // Bare capability string, e.g. `call("produce_tomato", {})`.
            if let Some(r) = self.by_action.get(module) {
                return Some((module.to_string(), r.endpoint.clone()));
            }
            if let Some(r) = self.by_capability.get(module) {
                return Some((module.to_string(), r.endpoint.clone()));
            }
            None
        }
    }

    /// Snapshot of registered agents, sorted by name, for `/providers`.
    pub fn agents(&self) -> Vec<AgentRegistration> {
        let mut v: Vec<AgentRegistration> = self.agents.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

// =============================================================
//  Process-global registry + thin accessors
// =============================================================

static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

fn global() -> &'static RwLock<Registry> {
    REGISTRY.get_or_init(|| RwLock::new(Registry::default()))
}

/// Parse + register from a raw JSON body (the `/register` handler path).
pub fn register_from_json(body: serde_json::Value) -> Result<AgentRegistration, String> {
    let req: RegisterRequest =
        serde_json::from_value(body).map_err(|e| format!("invalid registration body: {e}"))?;
    global().write().unwrap().register(req)
}

/// Resolve a capability against the global registry.
pub fn resolve(module: &str, func: &str) -> Option<(String, String)> {
    global().read().unwrap().resolve(module, func)
}

/// Snapshot of registered agents from the global registry.
pub fn list_agents() -> Vec<AgentRegistration> {
    global().read().unwrap().agents()
}

// =============================================================
//  Invocation
// =============================================================

/// Build the agent request body exactly as upstream's `invoke_local_action`:
/// an object params value is flattened with `capability` merged in; any other
/// value is wrapped as `{ "capability", "params" }`.
pub(crate) fn build_invoke_body(capability: &str, params: &Value) -> serde_json::Value {
    match value_to_json(params) {
        serde_json::Value::Object(mut map) => {
            map.insert(
                "capability".to_string(),
                serde_json::Value::String(capability.to_string()),
            );
            serde_json::Value::Object(map)
        }
        other => serde_json::json!({ "capability": capability, "params": other }),
    }
}

/// Invoke a registered OpenPrem agent over HTTP and return its result as a SOL
/// `Value`. Runs on the async runtime via `handle.block_on` (the VM drives this
/// from a blocking thread).
///
/// Failure surfaces (mapped to a clear `ExtCallFailed` upstream) when:
///   - the HTTP request fails / times out,
///   - the response is not JSON,
///   - the response is a non-2xx,
///   - the response is the agent's `{"error": "..."}` failure envelope.
pub fn invoke_agent(
    handle: &Handle,
    endpoint: &str,
    capability: &str,
    params: &Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let body = build_invoke_body(capability, params);
    let url = endpoint.to_string();
    let resp: serde_json::Value = handle.block_on(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| format!("provider client init failed: {e}"))?;
        let r = client.post(&url).json(&body).send().await.map_err(|e| {
            if e.is_timeout() {
                format!("provider timed out after {timeout_ms}ms")
            } else {
                format!("provider request failed: {e}")
            }
        })?;
        let status = r.status();
        let text = r
            .text()
            .await
            .map_err(|e| format!("provider response read failed: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
            if status.is_success() {
                let snippet: String = text.chars().take(120).collect();
                format!("provider response was not JSON: {snippet}")
            } else {
                format!("provider returned HTTP {} with a non-JSON body", status.as_u16())
            }
        })?;
        if !status.is_success() {
            let msg = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("(no error message)");
            return Err(format!("provider returned HTTP {}: {}", status.as_u16(), msg));
        }
        Ok(json)
    })?;

    // The SDK failure envelope is `{"error": "..."}` even at HTTP 200. Surface
    // it as a provider failure so the trace + error UX stays honest.
    if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
        return Err(format!("provider reported an error: {err}"));
    }
    Ok(json_to_value(resp))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str, actions: &[&str], endpoint: &str, pk: Option<&str>) -> RegisterRequest {
        RegisterRequest {
            name: name.into(),
            actions: actions.iter().map(|a| ActionSpec::Bare((*a).into())).collect(),
            endpoint: Some(endpoint.into()),
            endpoints: HashMap::new(),
            public_key: pk.map(|s| s.into()),
        }
    }

    #[test]
    fn register_builds_dual_capability_and_action_maps() {
        let mut reg = Registry::default();
        let out = reg
            .register(req("printer", &["print"], "http://127.0.0.1:9301", None))
            .unwrap();
        assert_eq!(out.name, "printer");
        assert_eq!(out.actions, vec!["print".to_string()]);
        assert!(!out.has_public_key);
        // name.action resolves.
        assert_eq!(
            reg.resolve("printer", "print"),
            Some(("printer.print".into(), "http://127.0.0.1:9301".into()))
        );
        // bare action resolves (capability sent is the bare action).
        assert_eq!(
            reg.resolve("print", ""),
            Some(("print".into(), "http://127.0.0.1:9301".into()))
        );
    }

    #[test]
    fn register_parses_multi_transport_endpoints_map() {
        let body = serde_json::json!({
            "name": "numbers",
            "actions": [{"name": "get"}],
            "endpoints": { "http": { "url": "http://127.0.0.1:9300" } },
            "public_key": "abc"
        });
        let req: RegisterRequest = serde_json::from_value(body).unwrap();
        let mut reg = Registry::default();
        let out = reg.register(req).unwrap();
        assert!(out.has_public_key);
        assert_eq!(
            reg.resolve("numbers", "get"),
            Some(("numbers.get".into(), "http://127.0.0.1:9300".into()))
        );
    }

    #[test]
    fn bare_capability_string_resolves_via_action_map() {
        // bigitaly-style: `call("produce_tomato", {})`, registered as the bare
        // action of the `tomato` app.
        let mut reg = Registry::default();
        reg.register(req("tomato", &["produce_tomato"], "http://127.0.0.1:9100", None))
            .unwrap();
        assert_eq!(
            reg.resolve("produce_tomato", ""),
            Some(("produce_tomato".into(), "http://127.0.0.1:9100".into()))
        );
    }

    #[test]
    fn dotted_action_name_resolves_via_module_func() {
        // global-sensor style: agent `dc1-temp` registers an action whose name
        // is itself dotted, `sensor.temperature`. A workflow `sensor.temperature(...)`
        // (module=sensor, func=temperature) must resolve to it.
        let mut reg = Registry::default();
        reg.register(req("dc1-temp", &["sensor.temperature"], "http://127.0.0.1:9101", None))
            .unwrap();
        assert_eq!(
            reg.resolve("sensor", "temperature"),
            Some(("sensor.temperature".into(), "http://127.0.0.1:9101".into()))
        );
    }

    #[test]
    fn reregister_replaces_endpoint() {
        let mut reg = Registry::default();
        reg.register(req("printer", &["print"], "http://127.0.0.1:9301", None))
            .unwrap();
        reg.register(req("printer", &["print"], "http://127.0.0.1:9999", None))
            .unwrap();
        assert_eq!(
            reg.resolve("printer", "print"),
            Some(("printer.print".into(), "http://127.0.0.1:9999".into()))
        );
        // No duplicate agents.
        assert_eq!(reg.agents().len(), 1);
    }

    #[test]
    fn missing_endpoint_is_rejected() {
        let mut reg = Registry::default();
        let bad = RegisterRequest {
            name: "x".into(),
            actions: vec![ActionSpec::Bare("a".into())],
            endpoint: None,
            endpoints: HashMap::new(),
            public_key: None,
        };
        assert!(reg.register(bad).is_err());
    }

    #[test]
    fn unknown_capability_resolves_to_none() {
        let reg = Registry::default();
        assert_eq!(reg.resolve("nope", "missing"), None);
    }

    #[test]
    fn invoke_body_flattens_object_params_and_merges_capability() {
        let params = Value::Struct(
            [("value".to_string(), Value::Int(7))].into_iter().collect(),
        );
        let body = build_invoke_body("printer.print", &params);
        assert_eq!(body["capability"], "printer.print");
        assert_eq!(body["value"], 7);
        assert!(body.get("params").is_none(), "object params must be flattened");
    }

    #[test]
    fn invoke_body_wraps_scalar_params() {
        let body = build_invoke_body("printer.print", &Value::Str("hello".into()));
        assert_eq!(body["capability"], "printer.print");
        assert_eq!(body["params"], "hello");
    }
}
