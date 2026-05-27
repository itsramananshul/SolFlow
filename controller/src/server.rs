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
    routing::{delete, get, patch, post},
    Router,
};
use serde::Deserialize;
use crate::connector::ConnectorMeta;
use solflow_host_spec::{
    Health, RunCreated, RunRecord, RunRequest, RunStatus,
    ScheduleCreate, ScheduleRecord,
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
        // Phase C C.3 — scheduling
        .route("/workflows/:id/schedules", post(post_schedule))
        .route("/workflows/:id/schedules", get(get_workflow_schedules))
        .route("/schedules/:id", get(get_schedule_route))
        .route("/schedules/:id", delete(delete_schedule_route))
        .route("/schedules/:id", patch(patch_schedule))
        // Wildcard `*path` captures anything after /events/ (incl.
        // slashes), e.g. POST /events/github/webhook → path =
        // "github/webhook".
        .route("/events/*path", post(post_event))
        // Phase C C.4 — connectors
        .route("/connectors", get(get_connectors))
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

// ============================================================
//  Phase C C.3 — schedules + event ingress
// ============================================================

async fn post_schedule(
    State(s): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(create): Json<ScheduleCreate>,
) -> Result<(StatusCode, Json<ScheduleRecord>), ApiError> {
    let rec = s.controller.create_schedule(&workflow_id, create).await?;
    Ok((StatusCode::CREATED, Json(rec)))
}

async fn get_workflow_schedules(
    State(s): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<Vec<ScheduleRecord>>, ApiError> {
    Ok(Json(s.controller.list_schedules_for_workflow(&workflow_id).await?))
}

async fn get_schedule_route(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ScheduleRecord>, ApiError> {
    Ok(Json(s.controller.get_schedule(&id).await?))
}

async fn delete_schedule_route(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    s.controller.cancel_schedule(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct PatchScheduleBody {
    enabled: bool,
}

async fn patch_schedule(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchScheduleBody>,
) -> Result<Json<ScheduleRecord>, ApiError> {
    Ok(Json(s.controller.set_schedule_enabled(&id, body.enabled).await?))
}

/// Webhook ingress. The body (any JSON) becomes `inputs` on the
/// created run. If no enabled Event schedule matches the path,
/// returns 404.
async fn post_event(
    State(s): State<AppState>,
    Path(path): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> Result<(StatusCode, Json<solflow_host_spec::RunRecord>), ApiError> {
    let body_val = body.map(|Json(v)| v).unwrap_or(serde_json::Value::Null);
    let rec = s.controller.ingress_event(&path, body_val).await?;
    Ok((StatusCode::ACCEPTED, Json(rec)))
}

/// `GET /connectors` — list registered connector metadata.
/// Editor's connector help/UX reads this to render available
/// connectors + their default policies.
async fn get_connectors(
    State(s): State<AppState>,
) -> Result<Json<Vec<ConnectorMeta>>, ApiError> {
    Ok(Json(s.controller.list_connectors()))
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

    // =============================================================
    //  Phase C C.3 — schedule + event-ingress route tests
    // =============================================================

    use solflow_compiler::compile_source;
    use solflow_host_spec::{encode_bytecode, WorkflowSubmission};

    async fn app_with_workflow() -> (Router, String) {
        let p = SqlitePersistence::open_in_memory().await.unwrap();
        let c = LocalController::new(p);
        // Submit a clean workflow so schedules can reference it.
        let cp = compile_source(
            "function start() -> int { print(\"sched\"); return 0; }",
        )
        .value
        .expect("clean");
        let bc = encode_bytecode(&cp.bytecode).unwrap();
        let resp = c
            .submit_workflow(WorkflowSubmission {
                name: "sched-test".into(),
                description: None,
                bytecode: bc,
                instruction_spans: serde_json::to_vec::<Vec<()>>(&vec![]).unwrap(),
                source: None,
            })
            .await
            .unwrap();
        let wf_id = resp.workflow_id;
        (router(c), wf_id)
    }

    fn body_from_json(v: serde_json::Value) -> Body {
        Body::from(serde_json::to_vec(&v).unwrap())
    }

    #[tokio::test]
    async fn post_schedule_creates_timer_schedule() {
        let (app, wf) = app_with_workflow().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workflows/{wf}/schedules"))
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({
                        "trigger": { "kind": "Timer", "schedule_id": "", "cron": "*/5 * * * *" },
                        "enabled": true
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn post_schedule_with_invalid_cron_rejected() {
        let (app, wf) = app_with_workflow().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workflows/{wf}/schedules"))
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({
                        "trigger": { "kind": "Timer", "schedule_id": "", "cron": "not-cron" },
                        "enabled": true
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn schedule_lifecycle_create_list_patch_delete() {
        let (app, wf) = app_with_workflow().await;

        // Create.
        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workflows/{wf}/schedules"))
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({
                        "trigger": { "kind": "Event", "source": "deploy" },
                        "enabled": true
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let body_bytes = axum::body::to_bytes(create_resp.into_body(), 4096)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let id = created["id"].as_str().unwrap().to_string();

        // List for the workflow.
        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/workflows/{wf}/schedules"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let lb = axum::body::to_bytes(list.into_body(), 4096).await.unwrap();
        let list_val: serde_json::Value = serde_json::from_slice(&lb).unwrap();
        assert_eq!(list_val.as_array().unwrap().len(), 1);

        // Patch (disable).
        let patch = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/schedules/{id}"))
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({ "enabled": false })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(patch.status(), StatusCode::OK);
        let pb = axum::body::to_bytes(patch.into_body(), 4096).await.unwrap();
        let patched: serde_json::Value = serde_json::from_slice(&pb).unwrap();
        assert_eq!(patched["enabled"], false);

        // Delete.
        let del = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/schedules/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del.status(), StatusCode::NO_CONTENT);

        // Get-after-delete → 404.
        let g = app
            .oneshot(
                Request::builder()
                    .uri(format!("/schedules/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(g.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn post_event_unmatched_returns_404() {
        let (app, _) = app_with_workflow().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events/unknown")
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_connectors_lists_http_by_default() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/connectors")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let rb = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let list: serde_json::Value = serde_json::from_slice(&rb).unwrap();
        let arr = list.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "http");
        // default_policy serializes the conservative defaults.
        assert_eq!(arr[0]["default_policy"]["timeout_ms"], 10_000);
    }

    #[tokio::test]
    async fn post_event_matched_creates_run_with_body_as_inputs() {
        let (app, wf) = app_with_workflow().await;

        // Register an Event schedule for path "ci/build".
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workflows/{wf}/schedules"))
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({
                        "trigger": { "kind": "Event", "source": "ci/build" },
                        "enabled": true
                    })))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events/ci/build")
                    .header("content-type", "application/json")
                    .body(body_from_json(serde_json::json!({ "ref": "main" })))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let rb = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let rec: serde_json::Value = serde_json::from_slice(&rb).unwrap();
        assert_eq!(rec["inputs"]["ref"], "main");
        assert_eq!(rec["trigger"]["kind"], "Event");
        assert_eq!(rec["trigger"]["source"], "ci/build");
    }
}
