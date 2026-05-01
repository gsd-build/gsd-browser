use crate::daemon::narration::events::ControlState;
use crate::daemon::narration::Narrator;
use crate::daemon::view::viewer_html::viewer_html;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ViewState {
    pub narrator: Arc<Narrator>,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}

#[derive(Serialize)]
struct ControlResponse {
    state: ControlState,
}

#[derive(Deserialize)]
struct ControlRequest {
    state: ControlState,
}

async fn root(State(_): State<ViewState>) -> Html<String> {
    Html(viewer_html())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn get_control(State(s): State<ViewState>) -> Json<ControlResponse> {
    Json(ControlResponse {
        state: s.narrator.control.get().await,
    })
}

async fn post_control(
    State(s): State<ViewState>,
    Json(req): Json<ControlRequest>,
) -> Result<Json<ControlResponse>, (StatusCode, String)> {
    s.narrator.set_control(req.state).await;
    Ok(Json(ControlResponse { state: req.state }))
}

pub fn router(state: ViewState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/control", get(get_control).post(post_control))
        .with_state(state)
}
