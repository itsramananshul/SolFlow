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
//!   RUST_LOG                    tracing filter (default info)
//!
//! Graceful shutdown on Ctrl+C (SIGINT) — in-flight requests
//! drain before exit.

use solflow_controller::executor::RunPolicy;
use solflow_controller::{server, LocalController, SqlitePersistence};
use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

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

    tracing::info!(%bind, %db_path, "starting solflow-controller");
    tracing::info!(
        step_limit = policy.step_limit,
        wall_clock_secs = policy.wall_clock_timeout.as_secs(),
        "run policy",
    );

    let persistence = SqlitePersistence::open(&db_path).await?;
    let controller = LocalController::new(persistence).with_policy(policy);
    // Start the scheduler tick loop AFTER policy is wired so the
    // loop reads the right step-limit / wall-clock cap. Without
    // this call the controller still works for Manual runs;
    // Timer triggers wouldn't fire.
    let _scheduler_handle = controller.start_scheduler();
    let app = server::router(controller);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("solflow-controller stopped cleanly");
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
