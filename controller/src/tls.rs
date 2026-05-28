//! TLS configuration helpers (Phase C C.7 c100).
//!
//! Centralizes how the binary reads cert + key paths from env vars
//! and turns them into a `RustlsConfig`. The actual `bind_rustls`
//! call stays in the binary so this module is testable without
//! opening sockets.
//!
//! Two states only:
//!
//!   - Both `SOLFLOW_CONTROLLER_TLS_CERT` + `SOLFLOW_CONTROLLER_TLS_KEY`
//!     are set → HTTPS, refusing to start if the files don't load.
//!   - Both are unset / empty → HTTP, identical to pre-C.7.
//!
//! Setting one without the other is rejected at startup with a
//! clear error so operators don't think they've enabled TLS when
//! they've only half-configured it.

use std::path::PathBuf;

/// Resolved TLS file paths. Constructed via `TlsPaths::from_env`,
/// consumed by `RustlsConfig::from_pem_file` in the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
}

/// What the binary should do for the transport layer.
///
/// `Http` is the default and matches pre-C.7 behavior. `Https`
/// carries the resolved cert + key paths; the binary loads them
/// via `axum_server::tls_rustls::RustlsConfig::from_pem_file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportConfig {
    Http,
    Https(TlsPaths),
}

/// Possible env-var configurations that don't resolve to a valid
/// transport. The binary translates these into a clean stderr
/// message + non-zero exit, instead of panicking deep inside
/// `axum_server`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TlsConfigError {
    #[error(
        "TLS misconfigured: SOLFLOW_CONTROLLER_TLS_CERT is set but \
         SOLFLOW_CONTROLLER_TLS_KEY is not. Set both, or neither."
    )]
    KeyMissing,
    #[error(
        "TLS misconfigured: SOLFLOW_CONTROLLER_TLS_KEY is set but \
         SOLFLOW_CONTROLLER_TLS_CERT is not. Set both, or neither."
    )]
    CertMissing,
}

/// Read the two TLS env vars and decide whether the controller
/// runs as HTTP or HTTPS. Returns `Ok(TransportConfig::Http)` when
/// both are empty / unset (the default). Returns
/// `Ok(TransportConfig::Https)` when both are set. Returns an
/// error when exactly one is set — half-configured TLS is more
/// dangerous than no TLS, so we refuse rather than silently
/// fall back to HTTP.
///
/// `cert_var` / `key_var` parameters exist for tests to inject
/// values directly; the binary calls
/// `from_env(std::env::var("SOLFLOW_CONTROLLER_TLS_CERT").ok(),
///           std::env::var("SOLFLOW_CONTROLLER_TLS_KEY").ok())`.
pub fn from_env(
    cert_var: Option<String>,
    key_var: Option<String>,
) -> Result<TransportConfig, TlsConfigError> {
    let cert = cert_var.filter(|s| !s.is_empty());
    let key = key_var.filter(|s| !s.is_empty());
    match (cert, key) {
        (None, None) => Ok(TransportConfig::Http),
        (Some(cert), Some(key)) => Ok(TransportConfig::Https(TlsPaths {
            cert: PathBuf::from(cert),
            key: PathBuf::from(key),
        })),
        (Some(_), None) => Err(TlsConfigError::KeyMissing),
        (None, Some(_)) => Err(TlsConfigError::CertMissing),
    }
}

// =============================================================
//  Tests
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_env_vars_means_http() {
        assert_eq!(from_env(None, None).unwrap(), TransportConfig::Http);
        // Empty strings also count as unset — matches how shells
        // typically pass cleared env vars.
        assert_eq!(
            from_env(Some(String::new()), Some(String::new())).unwrap(),
            TransportConfig::Http,
        );
    }

    #[test]
    fn both_set_means_https_with_those_paths() {
        let cfg = from_env(
            Some("/etc/tls/cert.pem".into()),
            Some("/etc/tls/key.pem".into()),
        )
        .unwrap();
        match cfg {
            TransportConfig::Https(p) => {
                assert_eq!(p.cert, PathBuf::from("/etc/tls/cert.pem"));
                assert_eq!(p.key, PathBuf::from("/etc/tls/key.pem"));
            }
            TransportConfig::Http => panic!("expected Https"),
        }
    }

    #[test]
    fn half_configured_cert_only_is_rejected() {
        assert_eq!(
            from_env(Some("c.pem".into()), None),
            Err(TlsConfigError::KeyMissing),
        );
    }

    #[test]
    fn half_configured_key_only_is_rejected() {
        assert_eq!(
            from_env(None, Some("k.pem".into())),
            Err(TlsConfigError::CertMissing),
        );
    }

    #[test]
    fn half_configured_with_empty_other_is_treated_as_missing() {
        // An operator who clears one var to empty (e.g. via
        // `SOLFLOW_CONTROLLER_TLS_KEY=`) should get the same
        // error as if they unset it — we don't want a silent HTTP
        // fallback hiding the misconfig.
        assert_eq!(
            from_env(Some("c.pem".into()), Some(String::new())),
            Err(TlsConfigError::KeyMissing),
        );
    }
}
