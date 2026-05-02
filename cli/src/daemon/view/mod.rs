pub mod annotations;
pub mod auth;
pub mod capture;
pub mod control;
pub mod http;
pub mod input;
pub mod page_state;
pub mod privacy;
pub mod refs_poller;
pub mod target_follow;
pub mod viewer_html;
pub mod ws;

use crate::daemon::narration::Narrator;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, watch, Mutex};
use tokio::task::JoinHandle;

pub struct ViewServerHandle {
    pub url: String,
    pub port: u16,
    pub _http_task: JoinHandle<()>,
    pub _capture_task: JoinHandle<()>,
    pub _refs_task: JoinHandle<()>,
    pub _target_follow_task: JoinHandle<()>,
}

const PORT_RANGE: std::ops::RangeInclusive<u16> = 7777..=7876;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub async fn start_for_session(
    state: Arc<DaemonState>,
    narrator: Arc<Narrator>,
    page: Arc<Page>,
    browser: Arc<Mutex<chromiumoxide::Browser>>,
) -> Result<ViewServerHandle, String> {
    use crate::daemon::view::http::{router, ViewState};

    let (frames_tx, _) = broadcast::channel(64);
    let (refs_tx, _) = broadcast::channel(16);
    let (active_page_tx, active_page_rx) = watch::channel(page.clone());

    let mut listener: Option<tokio::net::TcpListener> = None;
    let mut chosen: Option<u16> = None;
    for port in PORT_RANGE {
        match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => {
                listener = Some(l);
                chosen = Some(port);
                break;
            }
            Err(_) => continue,
        }
    }
    let listener = listener.ok_or_else(|| "no free port in 7777..=7876".to_string())?;
    let port = chosen.ok_or_else(|| "no free port in 7777..=7876".to_string())?;
    let origin = format!("http://127.0.0.1:{port}");
    let session_id = state
        .session
        .session_name
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let viewer_id = uuid::Uuid::new_v4().to_string();
    let token_issuer = crate::daemon::view::auth::ViewerTokenIssuer::new();
    let issued_at_ms = now_ms();
    let expires_at_ms = issued_at_ms
        + crate::daemon::view::auth::ViewerTokenIssuer::default_ttl().as_millis() as u64;
    let token = token_issuer.issue(crate::daemon::view::auth::ViewerTokenClaims {
        audience: crate::daemon::view::auth::VIEWER_AUDIENCE.to_string(),
        session_id: session_id.clone(),
        viewer_id: viewer_id.clone(),
        origin: origin.clone(),
        issued_at_ms,
        expires_at_ms,
        capabilities: vec![
            "view".to_string(),
            "state".to_string(),
            "input".to_string(),
            "control".to_string(),
            "annotation".to_string(),
            "recording".to_string(),
            "export".to_string(),
            "sensitive".to_string(),
        ],
    })?;
    let view_state = ViewState {
        narrator: narrator.clone(),
        frames: frames_tx.clone(),
        refs: refs_tx.clone(),
        token_issuer,
        session_id: session_id.clone(),
        viewer_id: viewer_id.clone(),
        origin: origin.clone(),
        daemon_state: state.clone(),
        active_page_rx: active_page_rx.clone(),
    };
    let app = router(view_state);

    let http_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let cap_page_rx = active_page_rx.clone();
    let cap_tx = frames_tx.clone();
    let capture_task = tokio::spawn(async move {
        crate::daemon::view::capture::run_capture_manager(cap_page_rx, cap_tx).await;
    });

    let refs_page_rx = active_page_rx.clone();
    let refs_tx2 = refs_tx.clone();
    let refs_task = tokio::spawn(async move {
        crate::daemon::view::refs_poller::run_refs_loop(refs_page_rx, refs_tx2).await;
    });

    let follow_state = state.clone();
    let target_follow_task = tokio::spawn(async move {
        crate::daemon::view::target_follow::run_target_follow(
            browser,
            follow_state,
            active_page_tx,
        )
        .await;
    });

    narrator.activate();

    let url = format!("{origin}/?session={session_id}&viewer={viewer_id}&token={token}");
    Ok(ViewServerHandle {
        url,
        port,
        _http_task: http_task,
        _capture_task: capture_task,
        _refs_task: refs_task,
        _target_follow_task: target_follow_task,
    })
}
