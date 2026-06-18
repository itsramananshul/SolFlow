//! `solflow-controller` — HTTP server binary (Phase C C.2).
//!
//! Boots an axum server with the routes from
//! `solflow_controller::server::router`, backed by
//! `LocalController` + `SqlitePersistence`.
//!
//! Configuration via env vars (no CLI flags yet — that's a
//! deferred polish item):
//!
//!   SOLFLOW_CONTROLLER_BIND     bind address (default 127.0.0.1:3939)
//!   SOLFLOW_CONTROLLER_DB       SQLite path (default ./solflow.db)
//!   SOLFLOW_CONTROLLER_STEP_LIMIT     per-run step cap (default 10_000_000)
//!   SOLFLOW_CONTROLLER_TIMEOUT_SECS   per-run wall-clock cap (default 600)
//!   SOLFLOW_CONTROLLER_AUTH_TOKEN     bearer token (default unset → no auth)
//!   SOLFLOW_CONTROLLER_TLS_CERT       PEM cert path (default unset → HTTP)
//!   SOLFLOW_CONTROLLER_TLS_KEY        PEM key path (must accompany TLS_CERT)
//!   RUST_LOG                    tracing filter (default info)
//!
//! Graceful shutdown on Ctrl+C (SIGINT) — in-flight requests
//! drain before exit.

use solflow_controller::executor::RunPolicy;
use solflow_controller::tls::{self, TransportConfig};
use solflow_controller::{server, AuthConfig, LocalController, SqlitePersistence};
use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    // Phase C C.7 c100 — rustls 0.23 requires a process-level
    // crypto provider. Install the ring-backed default before any
    // TLS handshake; idempotent (`.ok()` swallows the duplicate-
    // install error if main is invoked twice in tests).
    let _ = rustls::crypto::ring::default_provider().install_default();

    let bind: SocketAddr = std::env::var("SOLFLOW_CONTROLLER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3939".to_string())
        .parse()?;
    let db_path = std::env::var("SOLFLOW_CONTROLLER_DB")
        .unwrap_or_else(|_| "./solflow.db".to_string());
    let policy = RunPolicy {
        step_limit: env_usize("SOLFLOW_CONTROLLER_STEP_LIMIT", 10_000_000),
        wall_clock_timeout: Duration::from_secs(env_u64("SOLFLOW_CONTROLLER_TIMEOUT_SECS", 600)),
        max_output_lines: env_u64("SOLFLOW_CONTROLLER_MAX_OUTPUT_LINES", 100_000),
        max_events_per_run: env_u64("SOLFLOW_CONTROLLER_MAX_EVENTS_PER_RUN", 1_000_000),
    };

    // Phase C C.7 c98 — read auth token from env. Empty / unset =
    // disabled (default). Non-empty = required on every protected
    // endpoint.
    let auth = AuthConfig::from_env_token(
        std::env::var("SOLFLOW_CONTROLLER_AUTH_TOKEN").ok(),
    );

    // Phase C C.7 c100 — read TLS config from env. Both vars set →
    // HTTPS. Neither set → HTTP (default). Exactly-one-set →
    // refuse to start with a clear message.
    let transport = match tls::from_env(
        std::env::var("SOLFLOW_CONTROLLER_TLS_CERT").ok(),
        std::env::var("SOLFLOW_CONTROLLER_TLS_KEY").ok(),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("{e}");
            let boxed: Box<dyn std::error::Error> = Box::new(e);
            return Err(boxed);
        }
    };

    tracing::info!(%bind, %db_path, "starting solflow-controller");
    match std::env::var("SOLFLOW_CONNECTORS") {
        Ok(v) if !v.trim().is_empty() => match serde_json::from_str::<
            std::collections::HashMap<String, String>,
        >(&v)
        {
            Ok(map) if !map.is_empty() => {
                let mut names: Vec<&String> = map.keys().collect();
                names.sort();
                let list = names
                    .iter()
                    .map(|n| format!("{} -> {}", n, map[*n]))
                    .collect::<Vec<_>>()
                    .join(", ");
                tracing::info!("connectors: {list}");
            }
            Ok(_) => tracing::info!("connectors: none registered (empty map)"),
            Err(e) => tracing::warn!("connectors: SOLFLOW_CONNECTORS is not valid JSON: {e}"),
        },
        _ => tracing::info!(
            "connectors: none registered (set SOLFLOW_CONNECTORS to enable external Actions)"
        ),
    }
    tracing::info!(
        step_limit = policy.step_limit,
        wall_clock_secs = policy.wall_clock_timeout.as_secs(),
        "run policy",
    );
    match &auth {
        AuthConfig::Bearer { .. } => tracing::info!(
            "auth: bearer-token required on protected endpoints"
        ),
        AuthConfig::Disabled => tracing::info!(
            "auth: disabled (set SOLFLOW_CONTROLLER_AUTH_TOKEN to enable)"
        ),
    }
    match &transport {
        TransportConfig::Http => tracing::info!(
            "transport: HTTP (set SOLFLOW_CONTROLLER_TLS_CERT + _TLS_KEY for HTTPS)"
        ),
        TransportConfig::Https(_) => {
            tracing::info!("transport: HTTPS (rustls)");
            if matches!(&auth, AuthConfig::Disabled) {
                tracing::warn!(
                    "HTTPS is enabled but bearer-token auth is NOT — anyone \
                     who reaches this endpoint can submit + execute workflows. \
                     Set SOLFLOW_CONTROLLER_AUTH_TOKEN before exposing to a \
                     network you don't fully control."
                );
            }
        }
    }

    let persistence = SqlitePersistence::open(&db_path).await?;
    let controller = LocalController::new(persistence)
        .with_policy(policy)
        .with_auth(auth);
    // Phase C C.6 c91 — boot recovery: re-enqueue any run row
    // the previous controller process left in a non-terminal
    // status (Queued / Starting / Running / Cancelling). At
    // least once: workflow side-effects may execute twice on
    // recovery; idempotency is the workflow author's concern.
    match controller.recover_runs().await {
        Ok(n) if n > 0 => tracing::info!("boot recovery re-enqueued {n} runs"),
        Ok(_) => tracing::debug!("boot recovery found nothing to do"),
        Err(e) => tracing::error!("boot recovery failed: {e}"),
    }
    // Start the scheduler tick loop AFTER recovery so any
    // re-enqueued schedules get fair access.
    let _scheduler_handle = controller.start_scheduler();
    let app = server::router(controller);

    match transport {
        TransportConfig::Http => serve_http(bind, app).await?,
        TransportConfig::Https(paths) => {
            tracing::info!(
                cert = %paths.cert.display(),
                key = %paths.key.display(),
                "TLS enabled — loading cert + key",
            );
            let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
                &paths.cert, &paths.key,
            )
            .await
            .map_err(|e| -> Box<dyn std::error::Error> {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "TLS cert/key load failed (cert={}, key={}): {e}",
                        paths.cert.display(),
                        paths.key.display(),
                    ),
                ))
            })?;
            serve_https(bind, app, config).await?;
        }
    }
    tracing::info!("solflow-controller stopped cleanly");
    Ok(())
}

async fn serve_http(
    bind: SocketAddr,
    app: axum::Router,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn serve_https(
    bind: SocketAddr,
    app: axum::Router,
    config: axum_server::tls_rustls::RustlsConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // axum-server uses a Handle for graceful shutdown rather than
    // a future-based combinator. Wire ctrl-c → handle.graceful_shutdown.
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(30)));
    });
    tracing::info!("listening on https://{bind}");
    axum_server::bind_rustls(bind, config)
        .handle(handle)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("ctrl-c handler installs");
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate())
            .expect("SIGTERM handler installs")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received, shutting down"),
        _ = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
