//! Axum HTTP server exposing the C.2 endpoints from architecture §5.1.
//!
//! Routes:
//!
//!   GET    /healthz
//!   POST   /workflows
//!   POST   /runs
//!   GET    /runs/:id
//!   GET    /workflows/:id/runs       (?status=Failed&limit=20)
//!   DELETE /runs/:id                 (C.6 — returns 501 today)
//!
//! Permissive CORS so the browser editor can talk to a controller
//! served from a different origin. In production we'd lock this
//! down per environment; this is a developer-experience MVP.

use crate::{Controller, ControllerError, LocalController};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use solflow_host_spec::{
    Health, RunCreated, RunRecord, RunRequest, RunStatus,
    WorkflowSubmission, WorkflowSubmissionResponse,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub controller: Arc<LocalController>,
}

/// Build the axum router with all C.2 endpoints wired up.
pub fn router(controller: LocalController) -> Router {
    let state = AppState {
        controller: Arc::new(controller),
    };
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    Router::new()
        .route("/healthz", get(get_healthz))
        .route("/workflows", post(post_workflows))
        .route("/workflows/:id/runs", get(get_workflow_runs))
        .route("/runs", post(post_runs))
        .route("/runs/:id", get(get_run))
        .route("/runs/:id", delete(delete_run))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

async fn get_healthz(State(s): State<AppState>) -> Result<Json<Health>, ApiError> {
    Ok(Json(s.controller.health().await?))
}

async fn post_workflows(
    State(s): State<AppState>,
    Json(submission): Json<WorkflowSubmission>,
) -> Result<Json<WorkflowSubmissionResponse>, ApiError> {
    Ok(Json(s.controller.submit_workflow(submission).await?))
}

async fn post_runs(
    State(s): State<AppState>,
    Json(request): Json<RunRequest>,
) -> Result<(StatusCode, Json<RunCreated>), ApiError> {
    let created = s.controller.create_run(request).await?;
    Ok((StatusCode::ACCEPTED, Json(created)))
}

async fn get_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunRecord>, ApiError> {
    Ok(Json(s.controller.get_run(&id).await?))
}

#[derive(Debug, Deserialize)]
struct ListRunsQuery {
    status: Option<String>,
    limit: Option<usize>,
}

async fn get_workflow_runs(
    State(s): State<AppState>,
    Path(workflow_id): Path<String>,
    Query(q): Query<ListRunsQuery>,
) -> Result<Json<Vec<RunRecord>>, ApiError> {
    let status = q.status.as_deref().and_then(parse_status);
    Ok(Json(s.controller.list_runs(&workflow_id, status, q.limit).await?))
}

async fn delete_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    s.controller.cancel_run(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_status(s: &str) -> Option<RunStatus> {
    match s {
        "Queued" => Some(RunStatus::Queued),
        "Running" => Some(RunStatus::Running),
        "Succeeded" => Some(RunStatus::Succeeded),
        "Failed" => Some(RunStatus::Failed),
        "Cancelled" => Some(RunStatus::Cancelled),
        _ => None,
    }
}

// =============================================================
//  Error handling — uniform JSON shape on all 4xx/5xx
// =============================================================

#[derive(Debug)]
pub struct ApiError(pub ControllerError);

impl From<ControllerError> for ApiError {
    fn from(e: ControllerError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use ControllerError::*;
        let (status, code) = match &self.0 {
            WorkflowNotFound { .. } => (StatusCode::NOT_FOUND, "workflow_not_found"),
            RunNotFound { .. } => (StatusCode::NOT_FOUND, "run_not_found"),
            ScheduleNotFound { .. } => (StatusCode::NOT_FOUND, "schedule_not_found"),
            BytecodeInvalid { .. } => (StatusCode::BAD_REQUEST, "bytecode_invalid"),
            NotImplemented { .. } => (StatusCode::NOT_IMPLEMENTED, "not_implemented"),
            Persistence { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "persistence"),
            Connector { .. } => (StatusCode::BAD_GATEWAY, "connector"),
        };
        let body = serde_json::json!({
            "error": {
                "code": code,
                "message": self.0.to_string(),
            }
        });
        (status, Json(body)).into_response()
    }
}

// =============================================================
//  Tests — exercise the router with axum's TestServer pattern
// =============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqlitePersistence;
    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    async fn test_app() -> Router {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let c = LocalController::new(p);
        router(c)
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cancel_run_returns_501_in_c2() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/runs/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn get_unknown_run_returns_404() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/runs/run_does_not_exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
