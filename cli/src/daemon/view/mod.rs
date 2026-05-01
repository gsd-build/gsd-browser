pub mod capture;
pub mod http;
pub mod refs_poller;
pub mod target_follow;
pub mod viewer_html;
pub mod ws;

use crate::daemon::narration::Narrator;
use chromiumoxide::Page;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
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

pub async fn start_for_session(
    narrator: Arc<Narrator>,
    page: Arc<Page>,
    browser: Arc<Mutex<chromiumoxide::Browser>>,
) -> Result<ViewServerHandle, String> {
    use crate::daemon::view::http::{router, ViewState};

    let (frames_tx, _) = broadcast::channel(64);
    let (refs_tx, _) = broadcast::channel(16);

    let state = ViewState {
        narrator: narrator.clone(),
        frames: frames_tx.clone(),
        refs: refs_tx.clone(),
    };
    let app = router(state);

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

    let http_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let cap_page = page.clone();
    let cap_tx = frames_tx.clone();
    let capture_task = tokio::spawn(async move {
        crate::daemon::view::capture::run_capture_loop(cap_page, cap_tx).await;
    });

    let refs_page = page.clone();
    let refs_tx2 = refs_tx.clone();
    let refs_task = tokio::spawn(async move {
        crate::daemon::view::refs_poller::run_refs_loop(refs_page, refs_tx2).await;
    });

    let target_follow_task = tokio::spawn(async move {
        crate::daemon::view::target_follow::run_target_follow(browser).await;
    });

    narrator.activate();

    let url = format!("http://127.0.0.1:{port}");
    Ok(ViewServerHandle {
        url,
        port,
        _http_task: http_task,
        _capture_task: capture_task,
        _refs_task: refs_task,
        _target_follow_task: target_follow_task,
    })
}
