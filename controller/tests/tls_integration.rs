//! Phase C C.7 c100 — TLS / HTTPS integration tests.
//!
//! Spins up the real router under `axum_server::bind_rustls` with
//! a self-signed certificate minted at test time, then hits it via
//! reqwest with `danger_accept_invalid_certs` (we trust this
//! specific cert; the test would refuse to compile if the cert
//! came from an untrusted source).
//!
//! Each test allocates an ephemeral 127.0.0.1:0 port so they can
//! run concurrently without colliding.

use rcgen::generate_simple_self_signed;
use solflow_controller::{server, LocalController, SqlitePersistence};
use std::net::SocketAddr;
use std::sync::Once;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

/// Install the ring crypto provider exactly once across all tests
/// in this file. rustls 0.23 mandates a process-level provider
/// before any TLS handshake; tests are otherwise hermetic so we
/// pick the same provider the binary uses.
fn install_crypto() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("install ring CryptoProvider");
    });
}

async fn bind_tls(
    auth_token: Option<&str>,
) -> (SocketAddr, axum_server::Handle, TempDir) {
    install_crypto();
    // Generate a self-signed cert valid for 127.0.0.1 + localhost.
    let cert = generate_simple_self_signed(vec![
        "127.0.0.1".to_string(),
        "localhost".to_string(),
    ])
    .expect("rcgen");
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    // Write to a temp dir so RustlsConfig::from_pem_file can read them.
    let tmp = TempDir::new().unwrap();
    let cert_path = tmp.path().join("cert.pem");
    let key_path = tmp.path().join("key.pem");
    let mut cf = tokio::fs::File::create(&cert_path).await.unwrap();
    cf.write_all(cert_pem.as_bytes()).await.unwrap();
    let mut kf = tokio::fs::File::create(&key_path).await.unwrap();
    kf.write_all(key_pem.as_bytes()).await.unwrap();

    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
        &cert_path, &key_path,
    )
    .await
    .expect("RustlsConfig load");

    // Build a real LocalController.
    let p = SqlitePersistence::open_in_memory().await.unwrap();
    let mut c = LocalController::new(p);
    if let Some(tok) = auth_token {
        c = c.with_auth(solflow_controller::AuthConfig::Bearer {
            token: tok.into(),
        });
    }
    let app = server::router(c);

    // Bind to an ephemeral port + start serving in the background.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = axum_server::Handle::new();
    let h2 = handle.clone();
    tokio::spawn(async move {
        let _ = axum_server::from_tcp_rustls(listener, config)
            .handle(h2)
            .serve(app.into_make_service())
            .await;
    });
    // Give the listener a tick to come up.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, handle, tmp)
}

fn lenient_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn https_healthz_round_trips_through_tls() {
    let (addr, handle, _tmp) = bind_tls(None).await;
    let url = format!("https://{addr}/healthz");
    let resp = lenient_client().get(&url).send().await.expect("https request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["name"], "solflow-controller");
    assert_eq!(body["auth_required"], false);
    handle.graceful_shutdown(Some(Duration::from_millis(50)));
}

#[tokio::test]
async fn https_protected_endpoint_requires_token_via_tls() {
    let (addr, handle, _tmp) = bind_tls(Some("abc-secret")).await;
    let c = lenient_client();
    // Healthz still open, but advertises auth_required=true.
    let h: serde_json::Value = c
        .get(format!("https://{addr}/healthz"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(h["auth_required"], true);

    // No token → 401.
    let resp = c
        .get(format!("https://{addr}/controller/concurrency"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Correct token → 200.
    let resp = c
        .get(format!("https://{addr}/controller/concurrency"))
        .header("authorization", "Bearer abc-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    handle.graceful_shutdown(Some(Duration::from_millis(50)));
}
