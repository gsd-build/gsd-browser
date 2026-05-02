use crate::daemon::narration::events::ControlState;
use crate::daemon::narration::Narrator;
use crate::daemon::view::viewer_html::viewer_html;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::{Html, IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct ViewState {
    pub narrator: Arc<Narrator>,
    pub frames: tokio::sync::broadcast::Sender<crate::daemon::view::capture::FrameMessage>,
    pub refs: tokio::sync::broadcast::Sender<crate::daemon::view::refs_poller::RefsMessage>,
    pub token_issuer: crate::daemon::view::auth::ViewerTokenIssuer,
    pub session_id: String,
    pub viewer_id: String,
    pub origin: String,
    pub daemon_state: Arc<crate::daemon::state::DaemonState>,
    pub active_page_rx: tokio::sync::watch::Receiver<Arc<chromiumoxide::Page>>,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn query_param(uri: &Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let (left, right) = pair.split_once('=')?;
        (left == key).then(|| right.to_string())
    })
}

pub fn verify_viewer_token(
    state: &ViewState,
    uri: &Uri,
    required_capability: &'static str,
) -> Result<crate::daemon::view::auth::ViewerTokenClaims, (StatusCode, String)> {
    let token = query_param(uri, "token").ok_or((
        StatusCode::UNAUTHORIZED,
        "viewer token is required".to_string(),
    ))?;
    let session_id = query_param(uri, "session").unwrap_or_else(|| state.session_id.clone());
    let viewer_id = query_param(uri, "viewer").unwrap_or_else(|| state.viewer_id.clone());
    state
        .token_issuer
        .verify(
            &token,
            &session_id,
            &viewer_id,
            &state.origin,
            now_ms(),
            Some(required_capability),
        )
        .map_err(|err| {
            let status = match err.reason {
                crate::daemon::view::auth::AuthRejectReason::CapabilityDenied
                | crate::daemon::view::auth::AuthRejectReason::WrongOrigin => StatusCode::FORBIDDEN,
                _ => StatusCode::UNAUTHORIZED,
            };
            (status, format!("viewer auth rejected: {:?}", err.reason))
        })
}

pub fn verify_origin(state: &ViewState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };
    if origin == state.origin {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "origin rejected".to_string()))
    }
}

fn viewer_html_response() -> Response {
    let mut response = Html(viewer_html()).into_response();
    let headers = response.headers_mut();
    headers.insert("Referrer-Policy", "no-referrer".parse().expect("header"));
    headers.insert("Cache-Control", "no-store".parse().expect("header"));
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; frame-ancestors 'none'; base-uri 'none'"
            .parse()
            .expect("header"),
    );
    response
}

async fn root(State(state): State<ViewState>, uri: Uri) -> Result<Response, (StatusCode, String)> {
    verify_viewer_token(&state, &uri, "view")?;
    Ok(viewer_html_response())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn get_control(
    State(s): State<ViewState>,
    uri: Uri,
) -> Result<Json<ControlResponse>, (StatusCode, String)> {
    verify_viewer_token(&s, &uri, "state")?;
    Ok(Json(ControlResponse {
        state: s.narrator.control.get().await,
    }))
}

async fn post_control(
    State(s): State<ViewState>,
    uri: Uri,
    headers: HeaderMap,
    Json(req): Json<ControlRequest>,
) -> Result<Json<ControlResponse>, (StatusCode, String)> {
    verify_viewer_token(&s, &uri, "control")?;
    verify_origin(&s, &headers)?;
    s.narrator.set_control(req.state).await;
    Ok(Json(ControlResponse { state: req.state }))
}

pub fn router(state: ViewState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/control", get(get_control).post(post_control))
        .route("/ws", get(crate::daemon::view::ws::ws_upgrade))
        .with_state(state)
}
