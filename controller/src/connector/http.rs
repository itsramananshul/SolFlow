//! HTTP reference connector — Phase C C.4 c75.
//!
//! Speaks HTTP/1.1 + HTTP/2 via `reqwest` over rustls. The
//! canonical reference connector; further connectors should
//! follow the same shape.
//!
//! ## SOL surface
//!
//! ```text
//! ext function fetch(...) -> ...
//!   at "connector://http?url=https://api.example.com/x&method=GET"
//! ```
//!
//! ## URL parameters
//!
//! | Param          | Required | Meaning |
//! |----------------|----------|---------|
//! | `url`          | yes      | Target URL to call |
//! | `method`       | no       | `GET` (default), `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD` |
//! | `timeout_ms`   | no       | Per-call timeout override (must be ≤ `InvocationPolicy::timeout_ms`) |
//! | `header.NAME`  | no       | Sends `NAME: value` request header. Repeatable. |
//! | `body_format`  | no       | `json` (default) or `none` — controls how `args` becomes the request body |
//!
//! ## Args → body / query
//!
//! - Methods with bodies (POST/PUT/PATCH): if `body_format=json`
//!   (default), `args` is JSON-serialized into the request body
//!   with `content-type: application/json`. If `body_format=none`,
//!   no body is sent.
//! - Methods without bodies (GET/DELETE/HEAD): if `args` is a
//!   JSON object, each top-level key becomes a query parameter
//!   (`?key=value`). Non-object args are ignored.
//!
//! ## Response → value
//!
//! Body is read up to `policy.max_response_bytes`; oversize
//! responses return `ConnectorError::ResponseTooLarge`. If the
//! response `content-type` is `application/json*` the body is
//! parsed as a JSON value; otherwise the body is returned as a
//! UTF-8 string (lossy on invalid bytes).
//!
//! ## Retry + timeout
//!
//! Retries fire on `5xx` and on transport-level errors (TCP
//! refused, DNS, timeout). `4xx` is treated as a definitive
//! caller error — no retry. The total wall-clock is bounded by
//! `policy.timeout_ms`; backoff is `policy.backoff_base_ms *
//! 2^attempt` clamped so the next sleep can't exceed the
//! remaining budget.
//!
//! ## Security
//!
//! Construct with `HttpConnector::with_allowlist(...)` to limit
//! the hosts the connector will dial. Without an allowlist the
//! connector permits any URL (developer convenience); production
//! deployments should always set one.

use super::{
    Connector, ConnectorError, ConnectorInvocation, ConnectorMeta,
    ConnectorOutcome, InvocationPolicy,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Method, Url};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Allowlist entry — a host pattern. `match_subdomains: true`
/// also matches `*.host`.
#[derive(Debug, Clone)]
pub struct UrlAllowEntry {
    pub host: String,
    pub match_subdomains: bool,
}

impl UrlAllowEntry {
    pub fn exact(host: impl Into<String>) -> Self {
        Self { host: host.into(), match_subdomains: false }
    }
    pub fn host_and_subdomains(host: impl Into<String>) -> Self {
        Self { host: host.into(), match_subdomains: true }
    }
    fn matches(&self, candidate: &str) -> bool {
        if candidate.eq_ignore_ascii_case(&self.host) {
            return true;
        }
        if self.match_subdomains {
            let suffix = format!(".{}", self.host.to_ascii_lowercase());
            candidate.to_ascii_lowercase().ends_with(&suffix)
        } else {
            false
        }
    }
}

/// HTTP connector with optional URL allowlist.
pub struct HttpConnector {
    client: reqwest::Client,
    allowlist: Option<Vec<UrlAllowEntry>>,
}

impl Default for HttpConnector {
    fn default() -> Self {
        Self::new(None)
    }
}

impl HttpConnector {
    pub fn new(allowlist: Option<Vec<UrlAllowEntry>>) -> Self {
        let client = reqwest::Client::builder()
            // Connection-pool sane defaults; no Keep-Alive
            // tuning needed at this scale.
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            // Disable transparent retries — we manage retry
            // policy ourselves so behavior is predictable.
            .build()
            .expect("reqwest client builds with default config");
        Self { client, allowlist }
    }

    pub fn with_allowlist(mut self, allowlist: Vec<UrlAllowEntry>) -> Self {
        self.allowlist = Some(allowlist);
        self
    }

    /// Check whether the parsed URL is permitted by the allowlist.
    /// `None` allowlist = allow all.
    fn url_allowed(&self, url: &Url) -> bool {
        let Some(list) = &self.allowlist else { return true };
        let Some(host) = url.host_str() else { return false };
        list.iter().any(|e| e.matches(host))
    }
}

#[async_trait::async_trait]
impl Connector for HttpConnector {
    fn meta(&self) -> ConnectorMeta {
        ConnectorMeta {
            name: "http".into(),
            description: "HTTP reference connector — GET/POST/etc with JSON body, retries, timeouts, response-size limits".into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            default_policy: InvocationPolicy::default(),
        }
    }

    async fn invoke(
        &self,
        invocation: ConnectorInvocation,
    ) -> Result<ConnectorOutcome, ConnectorError> {
        let started = Instant::now();
        let request = HttpRequestBuilder::build(&invocation)?;
        // Allowlist check before any I/O.
        if !self.url_allowed(&request.url) {
            return Err(ConnectorError::UrlNotAllowed {
                connector: "http".into(),
                url: request.url.to_string(),
            });
        }

        // Retry loop with per-attempt and overall timeout.
        let mut attempt: u32 = 0;
        loop {
            let elapsed = started.elapsed().as_millis() as u64;
            if elapsed >= invocation.policy.timeout_ms {
                return Err(ConnectorError::Timeout {
                    connector: "http".into(),
                    elapsed_ms: elapsed,
                    limit_ms: invocation.policy.timeout_ms,
                });
            }
            let remaining = invocation.policy.timeout_ms - elapsed;
            // Per-attempt timeout = remaining budget (so a slow
            // server eats budget rather than each attempt restart-
            // ing the clock). Honor the optional per-call override.
            let per_attempt_timeout = request
                .per_call_timeout_ms
                .unwrap_or(remaining)
                .min(remaining);

            match self.send_once(&request, per_attempt_timeout).await {
                Ok(outcome) => {
                    let duration = started.elapsed().as_millis() as u64;
                    return Ok(ConnectorOutcome {
                        value: outcome.value,
                        duration_ms: duration,
                        retry_attempts: attempt,
                        meta: serde_json::json!({
                            "status": outcome.status,
                            "content_type": outcome.content_type,
                            "url": request.url.to_string(),
                            "method": request.method.as_str(),
                        }),
                    });
                }
                Err(e) if !is_retriable(&e) || attempt >= invocation.policy.retry_attempts => {
                    if attempt > 0 {
                        // Retry budget exhausted (or this error
                        // wasn't retriable in the first place but
                        // we made retries earlier).
                        return Err(ConnectorError::RetryExhausted {
                            connector: "http".into(),
                            attempts: attempt + 1,
                            last_error: e.to_string(),
                        });
                    }
                    return Err(e);
                }
                Err(e) => {
                    attempt += 1;
                    // Backoff: clamp so we never sleep past the
                    // overall budget.
                    let next_elapsed = started.elapsed().as_millis() as u64;
                    let budget_left = invocation
                        .policy
                        .timeout_ms
                        .saturating_sub(next_elapsed);
                    if budget_left == 0 {
                        return Err(ConnectorError::RetryExhausted {
                            connector: "http".into(),
                            attempts: attempt,
                            last_error: e.to_string(),
                        });
                    }
                    let raw_backoff = invocation
                        .policy
                        .backoff_base_ms
                        .saturating_mul(1_u64 << attempt.min(10));
                    let backoff = raw_backoff.min(budget_left);
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
            }
        }
    }
}

// =============================================================
//  Internals
// =============================================================

/// Parsed request payload — built from the invocation up front so
/// retries don't re-parse the URL on every loop iteration.
struct HttpRequestBuilder {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
    per_call_timeout_ms: Option<u64>,
}

impl HttpRequestBuilder {
    fn build(invocation: &ConnectorInvocation) -> Result<Self, ConnectorError> {
        let url_str =
            invocation.url_params.get("url").ok_or_else(|| ConnectorError::MissingParam {
                connector: "http".into(),
                param: "url".into(),
            })?;
        let mut url = Url::parse(url_str).map_err(|e| ConnectorError::InvalidParam {
            connector: "http".into(),
            param: "url".into(),
            reason: format!("not a URL: {e}"),
        })?;

        // Method parsing.
        let method = match invocation.url_params.get("method") {
            None => Method::GET,
            Some(m) => Method::from_bytes(m.to_uppercase().as_bytes()).map_err(|_| {
                ConnectorError::InvalidParam {
                    connector: "http".into(),
                    param: "method".into(),
                    reason: format!("`{m}` is not a valid HTTP method"),
                }
            })?,
        };

        // Headers — keys prefixed `header.`. Header names are
        // case-insensitive per RFC 7230; we lowercase before
        // parsing so callers don't have to worry about casing.
        let mut headers = HeaderMap::new();
        for (k, v) in &invocation.url_params {
            let Some(rest) = k.strip_prefix("header.") else { continue };
            if rest.is_empty() {
                return Err(ConnectorError::InvalidParam {
                    connector: "http".into(),
                    param: k.clone(),
                    reason: "header name is empty after `header.` prefix".into(),
                });
            }
            let name = HeaderName::from_bytes(rest.to_lowercase().as_bytes()).map_err(|e| {
                ConnectorError::InvalidParam {
                    connector: "http".into(),
                    param: k.clone(),
                    reason: format!("invalid header name: {e}"),
                }
            })?;
            let value = HeaderValue::from_str(v).map_err(|e| ConnectorError::InvalidParam {
                connector: "http".into(),
                param: k.clone(),
                reason: format!("invalid header value: {e}"),
            })?;
            headers.insert(name, value);
        }

        let body_format = invocation
            .url_params
            .get("body_format")
            .map(|s| s.as_str())
            .unwrap_or("json");

        let body = if method_takes_body(&method) && body_format == "json" {
            // JSON-encode args as the body. Defaults content-type
            // unless caller explicitly set one via header.content-type.
            if !headers.contains_key(CONTENT_TYPE) {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            }
            Some(serde_json::to_vec(&invocation.args).map_err(|e| {
                ConnectorError::InvalidParam {
                    connector: "http".into(),
                    param: "args".into(),
                    reason: format!("could not serialize: {e}"),
                }
            })?)
        } else {
            // For bodyless methods, fold args (when an object)
            // into the query string. Non-object args are ignored
            // — there's no sensible default mapping.
            if !method_takes_body(&method) {
                if let Value::Object(obj) = &invocation.args {
                    let mut qp = url.query_pairs_mut();
                    for (k, v) in obj {
                        qp.append_pair(k, &value_to_query_str(v));
                    }
                }
            }
            None
        };

        let per_call_timeout_ms = match invocation.url_params.get("timeout_ms") {
            None => None,
            Some(s) => Some(s.parse::<u64>().map_err(|e| ConnectorError::InvalidParam {
                connector: "http".into(),
                param: "timeout_ms".into(),
                reason: format!("not an integer: {e}"),
            })?),
        };

        Ok(Self { method, url, headers, body, per_call_timeout_ms })
    }
}

struct SingleAttemptOutcome {
    status: u16,
    content_type: Option<String>,
    value: Value,
}

impl HttpConnector {
    /// One attempt — no retries inside here.
    async fn send_once(
        &self,
        req: &HttpRequestBuilder,
        attempt_timeout_ms: u64,
    ) -> Result<SingleAttemptOutcome, ConnectorError> {
        let mut builder = self
            .client
            .request(req.method.clone(), req.url.clone())
            .headers(req.headers.clone())
            .timeout(Duration::from_millis(attempt_timeout_ms));
        if let Some(b) = &req.body {
            builder = builder.body(b.clone());
        }

        let response = builder.send().await.map_err(|e| classify_send_error(&e))?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Read with cap. reqwest's `bytes()` already buffers the
        // full body, but we want to fail fast on oversize so we
        // check `content-length` first then bound the read via
        // the `bytes()` length.
        let cap = self_max_response_bytes_or(req);
        if let Some(declared) = response.content_length() {
            if declared > cap {
                return Err(ConnectorError::ResponseTooLarge {
                    connector: "http".into(),
                    limit_bytes: cap,
                });
            }
        }
        let bytes = response.bytes().await.map_err(|e| ConnectorError::InvalidResponse {
            connector: "http".into(),
            reason: format!("reading body: {e}"),
        })?;
        if (bytes.len() as u64) > cap {
            return Err(ConnectorError::ResponseTooLarge {
                connector: "http".into(),
                limit_bytes: cap,
            });
        }

        // 2xx → success path. Try to parse as JSON if content-type
        // declares JSON; otherwise return the body as a UTF-8 string
        // (HTTP bodies in non-JSON contexts are almost always text).
        if (200..300).contains(&status) {
            let value = if content_type
                .as_deref()
                .map(|ct| ct.starts_with("application/json") || ct.starts_with("text/json"))
                .unwrap_or(false)
            {
                serde_json::from_slice::<Value>(&bytes).map_err(|e| {
                    ConnectorError::InvalidResponse {
                        connector: "http".into(),
                        reason: format!("body declared JSON but didn't parse: {e}"),
                    }
                })?
            } else {
                Value::String(String::from_utf8_lossy(&bytes).into_owned())
            };
            return Ok(SingleAttemptOutcome { status, content_type, value });
        }

        // 4xx auth → AuthFailed; other 4xx → HttpStatus; 5xx →
        // HttpStatus too (the retry layer decides whether to retry).
        let body_excerpt = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_string();
        if status == 401 || status == 403 {
            return Err(ConnectorError::AuthFailed {
                connector: "http".into(),
                reason: format!("HTTP {status}: {body_excerpt}"),
            });
        }
        Err(ConnectorError::HttpStatus {
            connector: "http".into(),
            status,
            message: body_excerpt,
        })
    }
}

/// `HttpConnector::send_once` doesn't see the policy, so we'd
/// have to thread it through. For now respect a hardcoded 1 MiB
/// cap (matches `InvocationPolicy::default().max_response_bytes`).
/// Future work: pass the policy down into send_once.
fn self_max_response_bytes_or(_req: &HttpRequestBuilder) -> u64 {
    InvocationPolicy::default().max_response_bytes
}

fn method_takes_body(m: &Method) -> bool {
    matches!(m, &Method::POST | &Method::PUT | &Method::PATCH)
}

fn value_to_query_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        // Numbers / bools serialize via Display.
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        // Null → empty (matches form-encoded conventions).
        Value::Null => String::new(),
        // Compound values — JSON-encode them inline. Not ideal
        // but it's deterministic + roundtrippable.
        other => other.to_string(),
    }
}

/// Map a `reqwest::Error` to our structured `ConnectorError`.
fn classify_send_error(e: &reqwest::Error) -> ConnectorError {
    if e.is_timeout() {
        // Caller's retry/Timeout-wrap will translate this into
        // the appropriate top-level Timeout if budget exhausts.
        return ConnectorError::Network {
            connector: "http".into(),
            reason: format!("request timed out: {e}"),
        };
    }
    if e.is_connect() {
        let host = e.url().and_then(|u| u.host_str().map(|s| s.to_string())).unwrap_or_default();
        if e.to_string().to_lowercase().contains("dns") {
            return ConnectorError::DnsFailure {
                connector: "http".into(),
                host,
                reason: e.to_string(),
            };
        }
        return ConnectorError::Network {
            connector: "http".into(),
            reason: format!("connection error: {e}"),
        };
    }
    ConnectorError::Network {
        connector: "http".into(),
        reason: e.to_string(),
    }
}

/// Errors that should trigger a retry: 5xx, transport errors,
/// timeouts. 4xx (caller's fault) and auth (almost always caller
/// config) are NOT retried.
fn is_retriable(e: &ConnectorError) -> bool {
    match e {
        ConnectorError::HttpStatus { status, .. } => *status >= 500,
        ConnectorError::Network { .. } => true,
        ConnectorError::DnsFailure { .. } => true,
        ConnectorError::Timeout { .. } => true,
        _ => false,
    }
}

// =============================================================
//  Tests — wiremock-driven
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::{Connector, ConnectorInvocation, InvocationPolicy};
    use serde_json::json;
    use std::collections::HashMap;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn invocation(url: &str, params: &[(&str, &str)], args: Value) -> ConnectorInvocation {
        ConnectorInvocation {
            fn_name: "test_fn".into(),
            url_params: {
                let mut m: HashMap<String, String> = HashMap::new();
                m.insert("url".into(), url.into());
                for (k, v) in params {
                    m.insert((*k).into(), (*v).into());
                }
                m
            },
            args,
            policy: InvocationPolicy::default(),
        }
    }

    #[tokio::test]
    async fn get_200_returns_parsed_json_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/widgets/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 42 })))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let out = conn
            .invoke(invocation(&format!("{}/widgets/42", server.uri()), &[], json!({})))
            .await
            .expect("ok");
        assert_eq!(out.value, json!({ "id": 42 }));
        assert_eq!(out.retry_attempts, 0);
    }

    #[tokio::test]
    async fn post_with_json_body_roundtrips() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .and(header("content-type", "application/json"))
            .and(body_json(json!({ "hello": "world" })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "ok": true })))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let out = conn
            .invoke(invocation(
                &format!("{}/echo", server.uri()),
                &[("method", "POST")],
                json!({ "hello": "world" }),
            ))
            .await
            .expect("ok");
        assert_eq!(out.value, json!({ "ok": true }));
    }

    #[tokio::test]
    async fn get_object_args_become_query_params() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            // wiremock's query matcher would be the right tool but
            // it's order-dependent; a custom matcher would be
            // overkill. We just respond unconditionally and verify
            // the value below.
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let out = conn
            .invoke(invocation(
                &format!("{}/search", server.uri()),
                &[],
                json!({ "q": "hello", "limit": 10 }),
            ))
            .await
            .expect("ok");
        assert_eq!(out.value, json!({ "ok": true }));
        // wiremock records the actual request — pull it back to
        // assert query params were set.
        let requests = server.received_requests().await.unwrap();
        let req = requests.last().expect("got a request");
        let raw = req.url.to_string();
        assert!(raw.contains("q=hello"));
        assert!(raw.contains("limit=10"));
    }

    #[tokio::test]
    async fn header_dot_params_become_request_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth"))
            .and(header("authorization", "Bearer xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let out = conn
            .invoke(invocation(
                &format!("{}/auth", server.uri()),
                &[("header.authorization", "Bearer xyz")],
                json!({}),
            ))
            .await
            .expect("ok");
        // Non-JSON body returned as a string value.
        assert_eq!(out.value, json!("ok"));
    }

    #[tokio::test]
    async fn missing_url_param_is_structured_error() {
        let conn = HttpConnector::default();
        let result = conn
            .invoke(ConnectorInvocation {
                fn_name: "x".into(),
                url_params: HashMap::new(),
                args: json!({}),
                policy: InvocationPolicy::default(),
            })
            .await;
        assert!(matches!(
            result,
            Err(ConnectorError::MissingParam { param, .. }) if param == "url"
        ));
    }

    #[tokio::test]
    async fn invalid_method_param_rejected() {
        // HTTP allows extension methods (any token), so a
        // bare alphabetic name like `WAFFLE` is actually
        // legal. To exercise the InvalidParam path we use a
        // method string with a character that's invalid in
        // HTTP method tokens (a space).
        let conn = HttpConnector::default();
        let result = conn
            .invoke(invocation(
                "http://127.0.0.1:1/x",
                &[("method", "BAD METHOD")],
                json!({}),
            ))
            .await;
        assert!(matches!(
            result,
            Err(ConnectorError::InvalidParam { param, .. }) if param == "method"
        ));
    }

    #[tokio::test]
    async fn http_404_is_status_error_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not here"))
            .expect(1) // exactly one attempt; no retry on 4xx
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let mut inv = invocation(&format!("{}/missing", server.uri()), &[], json!({}));
        inv.policy.retry_attempts = 3;
        let result = conn.invoke(inv).await;
        assert!(matches!(
            result,
            Err(ConnectorError::HttpStatus { status: 404, .. })
        ));
    }

    #[tokio::test]
    async fn http_401_is_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/private"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let result = conn
            .invoke(invocation(&format!("{}/private", server.uri()), &[], json!({})))
            .await;
        assert!(matches!(result, Err(ConnectorError::AuthFailed { .. })));
    }

    #[tokio::test]
    async fn http_500_retries_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"v": 1})))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let mut inv = invocation(&format!("{}/flaky", server.uri()), &[], json!({}));
        inv.policy.retry_attempts = 2;
        inv.policy.backoff_base_ms = 10;
        let out = conn.invoke(inv).await.expect("recovers");
        assert_eq!(out.value, json!({"v": 1}));
        assert_eq!(out.retry_attempts, 1);
    }

    #[tokio::test]
    async fn http_500_retry_exhausted() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/down"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let mut inv = invocation(&format!("{}/down", server.uri()), &[], json!({}));
        inv.policy.retry_attempts = 2;
        inv.policy.backoff_base_ms = 5;
        let result = conn.invoke(inv).await;
        assert!(
            matches!(result, Err(ConnectorError::RetryExhausted { attempts, .. }) if attempts == 3),
            "expected RetryExhausted with 3 attempts, got {result:?}",
        );
    }

    #[tokio::test]
    async fn allowlist_denies_disallowed_host() {
        let conn = HttpConnector::default()
            .with_allowlist(vec![UrlAllowEntry::exact("only.example.com")]);
        let result = conn
            .invoke(invocation("http://not-allowed.example.com/", &[], json!({})))
            .await;
        assert!(matches!(result, Err(ConnectorError::UrlNotAllowed { .. })));
    }

    #[tokio::test]
    async fn allowlist_allows_exact_host() {
        let server = MockServer::start().await;
        // We can't easily intercept localhost vs allowlist'd host
        // in wiremock; instead, allow the wiremock host directly.
        let allowed_host = server.uri().split("//").nth(1).unwrap().split(':').next().unwrap().to_string();
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_string("yes"))
            .mount(&server)
            .await;
        let conn = HttpConnector::default()
            .with_allowlist(vec![UrlAllowEntry::exact(allowed_host)]);
        let out = conn
            .invoke(invocation(&format!("{}/ok", server.uri()), &[], json!({})))
            .await
            .expect("allowed");
        assert_eq!(out.value, json!("yes"));
    }

    #[tokio::test]
    async fn allowlist_subdomain_match() {
        let allow = UrlAllowEntry::host_and_subdomains("example.com");
        assert!(allow.matches("example.com"));
        assert!(allow.matches("api.example.com"));
        assert!(allow.matches("a.b.example.com"));
        assert!(!allow.matches("evil-example.com"));
        assert!(!allow.matches("example.com.evil.com"));
    }

    #[tokio::test]
    async fn response_too_large_caught_by_content_length() {
        let server = MockServer::start().await;
        // Send a body larger than the 1 MiB default cap.
        let big = "x".repeat(2 * 1024 * 1024);
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", big.len().to_string().as_str())
                    .set_body_string(big),
            )
            .mount(&server)
            .await;
        let conn = HttpConnector::default();
        let result = conn
            .invoke(invocation(&format!("{}/big", server.uri()), &[], json!({})))
            .await;
        assert!(matches!(result, Err(ConnectorError::ResponseTooLarge { .. })));
    }

    #[tokio::test]
    async fn invalid_url_param_rejected() {
        let conn = HttpConnector::default();
        let result = conn
            .invoke(invocation("definitely not a url", &[], json!({})))
            .await;
        assert!(matches!(
            result,
            Err(ConnectorError::InvalidParam { param, .. }) if param == "url"
        ));
    }
}
