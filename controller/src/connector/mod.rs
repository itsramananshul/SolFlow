//! Connector framework — Phase C C.4.
//!
//! ## Architecture
//!
//! The canonical VM's `Inst::ExtCall` reaches an installed
//! `ExtCallHandler` (defined in `solflow_runtime`); the controller
//! installs a handler that parses the call's URL (`connector://name?...`),
//! looks the connector up in this crate's `ConnectorRegistry`, and
//! invokes it. The connector returns either a JSON value (pushed
//! back to the VM stack) or a structured `ConnectorError`.
//!
//! ```text
//!  VM (synchronous)                 controller (async)
//!  ──────────────────               ────────────────────────────
//!  Inst::ExtCall   ────────────►   ExtCallHandler::handle()
//!                                       │
//!                                       │  parse_connector_url("connector://http?...")
//!                                       │  registry.lookup("http")
//!                                       ▼
//!                                   Connector::invoke(invocation)
//!                                       │
//!                                       │  block_on (executor wraps in spawn_blocking)
//!                                       ▼
//!                                   ConnectorOutcome | ConnectorError
//!  Ok(value) ◄────────────────────  marshal back to VM value
//! ```
//!
//! ## Disciplines
//!
//! - **No `connector://` knowledge in the VM** — the VM hands the
//!   raw URL string to the handler; the handler parses + dispatches.
//!   The VM stays browser-safe.
//! - **No HTTP knowledge in this module** — the trait is generic.
//!   `http::HttpConnector` (c75) is just one implementation.
//! - **Structured errors only** — every failure mode maps to a
//!   `ConnectorError` variant the editor can render distinctly.
//! - **Defensive defaults** — `InvocationPolicy::default()`
//!   enforces a 10s wall-clock cap, 1MB response-body cap, and
//!   zero retries unless explicitly enabled.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use thiserror::Error;

pub mod http;

// =============================================================
//  Trait + invocation shapes
// =============================================================

/// One ExtCall arriving at a connector. Carries the SOL function
/// name (informational, mostly for observability), the parsed
/// URL parameters, the JSON-marshalled args, and the per-call
/// policy.
///
/// `fn_name` is the SOL `ext function NAME` identifier. For an
/// HTTP connector it isn't load-bearing; for connectors that
/// dispatch (e.g. `slack`'s `slack:post_message`) the name is
/// the dispatch key.
#[derive(Clone)]
pub struct ConnectorInvocation {
    pub fn_name: String,
    pub url_params: HashMap<String, String>,
    /// Marshalled SOL args. Shape is a JSON value; connectors
    /// decide how to consume it (HTTP: body / query string).
    pub args: serde_json::Value,
    pub policy: InvocationPolicy,
    /// Phase C C.6 c94 — orchestration cancel/timeout flag.
    /// Connectors check this before each retry attempt + during
    /// in-flight I/O (via `tokio::select!` race against the call
    /// future) so a long HTTP call doesn't block cancellation.
    /// `None` when the connector is invoked outside an
    /// orchestrated run (e.g. a future "test connector"
    /// affordance).
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl std::fmt::Debug for ConnectorInvocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // AtomicBool doesn't impl Debug — skip the flag.
        f.debug_struct("ConnectorInvocation")
            .field("fn_name", &self.fn_name)
            .field("url_params", &self.url_params)
            .field("args", &self.args)
            .field("policy", &self.policy)
            .field("cancel_flag", &self.cancel_flag.as_ref().map(|_| "<flag>"))
            .finish()
    }
}

/// The result of a connector invocation.
///
/// `value` is what the runtime pushes back onto the VM stack —
/// connectors are expected to produce a JSON value matching the
/// ext function's declared return type (the executor handles the
/// JSON → VM marshalling).
#[derive(Debug, Clone)]
pub struct ConnectorOutcome {
    pub value: serde_json::Value,
    /// Wall-clock duration of the call. Includes retries.
    pub duration_ms: u64,
    /// Number of retry attempts performed (0 if the first try
    /// succeeded). Surfaced to the run record / event log so the
    /// editor can show "succeeded after 2 retries".
    pub retry_attempts: u32,
    /// Connector-specific human-readable metadata (e.g. HTTP
    /// status code). Goes into the run-event payload, never used
    /// for control flow.
    pub meta: serde_json::Value,
}

/// Per-call execution policy. Defaults are conservative — every
/// connector must work safely without explicit policy from the
/// SOL author.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InvocationPolicy {
    /// Hard wall-clock ceiling. Includes all retries + backoff.
    pub timeout_ms: u64,
    /// Number of additional attempts after the first failure.
    /// `retry_attempts=0` (default) means no retry.
    pub retry_attempts: u32,
    /// Exponential backoff base. Attempt N sleeps for
    /// `backoff_base_ms * 2^N` clamped to `timeout_ms`.
    pub backoff_base_ms: u64,
    /// Maximum response body size the connector will read into
    /// memory. Default 1 MiB; oversize responses produce
    /// `ConnectorError::ResponseTooLarge`.
    pub max_response_bytes: u64,
}

impl Default for InvocationPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 10_000,
            retry_attempts: 0,
            backoff_base_ms: 100,
            max_response_bytes: 1 * 1024 * 1024,
        }
    }
}

/// Connector metadata exposed at registration time. Lets the
/// editor's `/connectors` listing show what's available without
/// dispatching a real call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorMeta {
    /// Stable name; matches the `connector://NAME?...` URL host.
    pub name: String,
    /// One-line human description. Editor renders this in the
    /// connector help list.
    pub description: String,
    /// SemVer-style version string the connector implementation
    /// reports; useful for debugging behavior drift.
    pub version: String,
    /// Connector default policy (for display). The actual call
    /// merges per-invocation overrides on top.
    pub default_policy: InvocationPolicy,
}

/// What every connector implements. Async because real connectors
/// do I/O. `Send + Sync` because the registry stores them as
/// `Arc<dyn Connector>` shared across run-execution tasks.
#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    /// Metadata identifying + describing the connector.
    fn meta(&self) -> ConnectorMeta;

    /// Invoke the connector with one parsed call.
    ///
    /// The implementation owns timeout + retry policy enforcement.
    /// Callers don't wrap this in `tokio::time::timeout` — that
    /// would race the connector's own retry/backoff and produce
    /// confusing failure attributions.
    async fn invoke(
        &self,
        invocation: ConnectorInvocation,
    ) -> Result<ConnectorOutcome, ConnectorError>;
}

// =============================================================
//  Connector errors
// =============================================================

/// Every connector failure surfaces as one of these. Editor +
/// run-event log discriminate on `kind` for distinct UX.
///
/// `cancelled` is reserved for the future cancel-run path
/// (C.6) — connectors can return it when their AbortSignal fires.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectorError {
    /// The connector name in `connector://NAME?...` doesn't match
    /// any registered connector.
    #[error("connector not found: {name}")]
    ConnectorNotFound { name: String },

    /// The URL didn't parse as `connector://<name>?<params>`.
    #[error("invalid connector URL: {reason}")]
    InvalidConnectorUrl { reason: String },

    /// Required URL parameter missing.
    #[error("connector `{connector}` missing required parameter: {param}")]
    MissingParam { connector: String, param: String },

    /// Required URL parameter present but didn't parse.
    #[error("connector `{connector}` invalid parameter `{param}`: {reason}")]
    InvalidParam {
        connector: String,
        param: String,
        reason: String,
    },

    /// Wall-clock policy exhausted before the call completed.
    #[error("connector `{connector}` timed out after {elapsed_ms}ms (limit {limit_ms}ms)")]
    Timeout {
        connector: String,
        elapsed_ms: u64,
        limit_ms: u64,
    },

    /// Authentication required / rejected (HTTP 401/403, etc.).
    #[error("connector `{connector}` authentication failed: {reason}")]
    AuthFailed { connector: String, reason: String },

    /// DNS resolution failure.
    #[error("connector `{connector}` could not resolve `{host}`: {reason}")]
    DnsFailure {
        connector: String,
        host: String,
        reason: String,
    },

    /// HTTP-style status error — used by the HTTP connector for
    /// non-2xx responses that aren't auth-related.
    #[error("connector `{connector}` returned HTTP {status}: {message}")]
    HttpStatus {
        connector: String,
        status: u16,
        message: String,
    },

    /// Server returned a response the connector couldn't decode
    /// (bad JSON, malformed framing, etc.).
    #[error("connector `{connector}` invalid response: {reason}")]
    InvalidResponse { connector: String, reason: String },

    /// Retry budget exhausted. `last_error` carries the final
    /// failure cause so the UI can show what actually broke.
    #[error("connector `{connector}` retry exhausted ({attempts} attempts); last: {last_error}")]
    RetryExhausted {
        connector: String,
        attempts: u32,
        last_error: String,
    },

    /// Request payload would exceed the connector's per-call
    /// payload limit.
    #[error("connector `{connector}` payload too large: {actual_bytes} bytes (limit {limit_bytes})")]
    PayloadTooLarge {
        connector: String,
        actual_bytes: u64,
        limit_bytes: u64,
    },

    /// Response body exceeded `max_response_bytes`.
    #[error("connector `{connector}` response too large: limit {limit_bytes} bytes")]
    ResponseTooLarge {
        connector: String,
        limit_bytes: u64,
    },

    /// URL rejected by the connector's allowlist (security
    /// boundary — prevents accidental SSRF).
    #[error("connector `{connector}` URL not allowed: {url}")]
    UrlNotAllowed { connector: String, url: String },

    /// Caller-side cancellation (the run was cancelled).
    /// C.6 will actually surface this end-to-end; until then,
    /// connectors that propagate AbortSignal can return it.
    #[error("connector `{connector}` cancelled")]
    Cancelled { connector: String },

    /// Any other transport-level failure (TCP refused, TLS
    /// handshake, etc.) the connector decides isn't worth a more
    /// specific variant.
    #[error("connector `{connector}` network error: {reason}")]
    Network { connector: String, reason: String },
}

// =============================================================
//  Registry
// =============================================================

/// Holds the connectors the controller knows about. Lookup is by
/// name (matches the URL host). Lookups are lock-free reads since
/// the registry is build-time-only (no register-after-start
/// semantics — connectors come from controller config + the
/// binary's default set).
#[derive(Clone, Default)]
pub struct ConnectorRegistry {
    inner: Arc<HashMap<String, Arc<dyn Connector>>>,
}

impl ConnectorRegistry {
    pub fn builder() -> ConnectorRegistryBuilder {
        ConnectorRegistryBuilder { items: HashMap::new() }
    }

    pub fn lookup(&self, name: &str) -> Result<Arc<dyn Connector>, ConnectorError> {
        match self.inner.get(name) {
            Some(c) => Ok(c.clone()),
            None => Err(ConnectorError::ConnectorNotFound { name: name.to_string() }),
        }
    }

    pub fn list_meta(&self) -> Vec<ConnectorMeta> {
        let mut out: Vec<ConnectorMeta> = self.inner.values().map(|c| c.meta()).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

pub struct ConnectorRegistryBuilder {
    items: HashMap<String, Arc<dyn Connector>>,
}

impl ConnectorRegistryBuilder {
    /// Register a connector. Duplicate-name registrations replace
    /// the prior entry; that's the simplest behavior for tests and
    /// matches "config file overrides defaults" semantics.
    pub fn register(mut self, connector: Arc<dyn Connector>) -> Self {
        let name = connector.meta().name;
        self.items.insert(name, connector);
        self
    }

    pub fn build(self) -> ConnectorRegistry {
        ConnectorRegistry { inner: Arc::new(self.items) }
    }
}

// =============================================================
//  URL parser
// =============================================================

/// Result of parsing `connector://name?key=val&key2=val2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedConnectorRef {
    pub name: String,
    pub params: HashMap<String, String>,
}

/// Parse a `connector://...` URL. Returns:
///
///   - `name` — the URL host (between `//` and `?` / end)
///   - `params` — query-string key/value pairs (URL-decoded)
///
/// Path components (anything between host + query) are
/// intentionally rejected for now — we want all configuration in
/// query params so the wire format stays uniform across connectors.
///
/// Examples:
///
///   `connector://http?url=https://x&method=GET`
///       → { name: "http", params: { url, method } }
///
///   `connector://slack?channel=alerts`
///       → { name: "slack", params: { channel } }
pub fn parse_connector_url(raw: &str) -> Result<ParsedConnectorRef, ConnectorError> {
    let parsed = url::Url::parse(raw).map_err(|e| ConnectorError::InvalidConnectorUrl {
        reason: format!("not a URL: {e}"),
    })?;
    if parsed.scheme() != "connector" {
        return Err(ConnectorError::InvalidConnectorUrl {
            reason: format!(
                "expected `connector://` scheme, got `{}://`",
                parsed.scheme()
            ),
        });
    }
    let name = parsed
        .host_str()
        .ok_or_else(|| ConnectorError::InvalidConnectorUrl {
            reason: "missing connector name (the URL host)".into(),
        })?
        .to_string();
    if name.is_empty() {
        return Err(ConnectorError::InvalidConnectorUrl {
            reason: "empty connector name".into(),
        });
    }
    // Path must be empty (or just `/`). Path components are
    // disallowed for now — connectors expose dispatch via the
    // SOL fn_name + query params, never via URL path segments.
    let path = parsed.path();
    if !path.is_empty() && path != "/" {
        return Err(ConnectorError::InvalidConnectorUrl {
            reason: format!(
                "path segments not allowed in connector URLs; got `{path}`",
            ),
        });
    }
    let params: HashMap<String, String> =
        parsed.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    Ok(ParsedConnectorRef { name, params })
}

// =============================================================
//  Tests
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_connector_url_basic() {
        let p = parse_connector_url("connector://http?url=https://x.example/api&method=GET")
            .expect("parses");
        assert_eq!(p.name, "http");
        assert_eq!(p.params.get("url").map(|s| s.as_str()), Some("https://x.example/api"));
        assert_eq!(p.params.get("method").map(|s| s.as_str()), Some("GET"));
    }

    #[test]
    fn parse_connector_url_no_params() {
        let p = parse_connector_url("connector://slack").expect("parses");
        assert_eq!(p.name, "slack");
        assert!(p.params.is_empty());
    }

    #[test]
    fn parse_connector_url_url_decodes_params() {
        let p = parse_connector_url("connector://http?url=https%3A%2F%2Fa.b%2F%3Fq%3D1")
            .expect("parses");
        assert_eq!(p.params.get("url").unwrap(), "https://a.b/?q=1");
    }

    #[test]
    fn parse_connector_url_rejects_wrong_scheme() {
        let err = parse_connector_url("https://api.example.com/widgets")
            .expect_err("must reject");
        assert!(matches!(err, ConnectorError::InvalidConnectorUrl { .. }));
    }

    #[test]
    fn parse_connector_url_rejects_missing_name() {
        let err = parse_connector_url("connector:///?url=x").expect_err("must reject");
        assert!(matches!(err, ConnectorError::InvalidConnectorUrl { .. }));
    }

    #[test]
    fn parse_connector_url_rejects_path() {
        let err =
            parse_connector_url("connector://http/foo/bar?url=x").expect_err("must reject");
        assert!(matches!(err, ConnectorError::InvalidConnectorUrl { .. }));
    }

    #[test]
    fn parse_connector_url_rejects_malformed() {
        let err = parse_connector_url("not a url").expect_err("must reject");
        assert!(matches!(err, ConnectorError::InvalidConnectorUrl { .. }));
    }

    // ----- registry -----

    struct StubConnector(ConnectorMeta);

    #[async_trait::async_trait]
    impl Connector for StubConnector {
        fn meta(&self) -> ConnectorMeta { self.0.clone() }
        async fn invoke(
            &self,
            _: ConnectorInvocation,
        ) -> Result<ConnectorOutcome, ConnectorError> {
            unreachable!("registry tests don't invoke")
        }
    }

    fn stub(name: &str) -> Arc<dyn Connector> {
        Arc::new(StubConnector(ConnectorMeta {
            name: name.into(),
            description: format!("stub {name}"),
            version: "0.0.0".into(),
            default_policy: InvocationPolicy::default(),
        }))
    }

    #[test]
    fn registry_lookup_by_name() {
        let reg = ConnectorRegistry::builder()
            .register(stub("http"))
            .register(stub("slack"))
            .build();
        assert_eq!(reg.lookup("http").unwrap().meta().name, "http");
        assert_eq!(reg.lookup("slack").unwrap().meta().name, "slack");
    }

    #[test]
    fn registry_lookup_unknown_errors() {
        let reg = ConnectorRegistry::builder().build();
        let result = reg.lookup("nope");
        assert!(matches!(
            result,
            Err(ConnectorError::ConnectorNotFound { .. }),
        ));
    }

    #[test]
    fn registry_list_meta_sorted_by_name() {
        let reg = ConnectorRegistry::builder()
            .register(stub("zulip"))
            .register(stub("http"))
            .register(stub("slack"))
            .build();
        let names: Vec<String> = reg.list_meta().into_iter().map(|m| m.name).collect();
        assert_eq!(names, vec!["http", "slack", "zulip"]);
    }

    #[test]
    fn registry_duplicate_registration_replaces() {
        let reg = ConnectorRegistry::builder()
            .register(stub("http"))
            .register(stub("http"))
            .build();
        assert_eq!(reg.list_meta().len(), 1);
    }

    // ----- default policy -----

    #[test]
    fn default_policy_is_conservative() {
        let p = InvocationPolicy::default();
        assert_eq!(p.timeout_ms, 10_000);
        assert_eq!(p.retry_attempts, 0);
        assert_eq!(p.max_response_bytes, 1 * 1024 * 1024);
        assert!(p.backoff_base_ms > 0);
    }
}
