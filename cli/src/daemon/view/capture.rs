use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, EventScreencastFrame,
    ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
};
use chromiumoxide::Page;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

static FRAME_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, serde::Serialize)]
pub struct FrameMessage {
    #[serde(rename = "type")]
    pub(crate) ty: &'static str,
    #[serde(rename = "frameSeq")]
    pub frame_seq: u64,
    pub content_type: &'static str,
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
    pub data: String,
    pub viewport: ViewportInfo,
    pub capture_pixel_width: u32,
    pub capture_pixel_height: u32,
    pub device_pixel_ratio: f64,
    pub capture_scale_x: f64,
    pub capture_scale_y: f64,
    pub url: String,
    pub title: String,
    pub timestamp: u64,
}

#[derive(Clone, serde::Serialize)]
pub struct ViewportInfo {
    pub width: u32,
    pub height: u32,
    #[serde(rename = "devicePixelRatio")]
    pub device_pixel_ratio: f64,
    #[serde(rename = "scrollX")]
    pub scroll_x: f64,
    #[serde(rename = "scrollY")]
    pub scroll_y: f64,
}

async fn viewport_info(page: &Page, fallback_width: u32, fallback_height: u32) -> ViewportInfo {
    let value = page
        .evaluate_expression(
            r#"(() => ({
                width: Math.max(0, Math.round(window.innerWidth || 0)),
                height: Math.max(0, Math.round(window.innerHeight || 0)),
                devicePixelRatio: Number(window.devicePixelRatio || 1),
                scrollX: Number(window.scrollX || 0),
                scrollY: Number(window.scrollY || 0)
            }))()"#,
        )
        .await
        .ok()
        .and_then(|result| result.into_value().ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let width = value
        .get("width")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(fallback_width as u64) as u32;
    let height = value
        .get("height")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(fallback_height as u64) as u32;

    ViewportInfo {
        width: if width == 0 { fallback_width } else { width },
        height: if height == 0 { fallback_height } else { height },
        device_pixel_ratio: value
            .get("devicePixelRatio")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.0),
        scroll_x: value
            .get("scrollX")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default(),
        scroll_y: value
            .get("scrollY")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default(),
    }
}

async fn page_url(page: &Page) -> String {
    page.url().await.unwrap_or_default().unwrap_or_default()
}

async fn page_title(page: &Page) -> String {
    page.get_title()
        .await
        .unwrap_or_default()
        .unwrap_or_default()
}

async fn frame_message(
    page: &Page,
    data: String,
    pixel_width: u32,
    pixel_height: u32,
) -> FrameMessage {
    let viewport = viewport_info(page, pixel_width, pixel_height).await;
    let capture_scale_x = if viewport.width == 0 {
        1.0
    } else {
        pixel_width as f64 / viewport.width as f64
    };
    let capture_scale_y = if viewport.height == 0 {
        1.0
    } else {
        pixel_height as f64 / viewport.height as f64
    };
    FrameMessage {
        ty: "frame",
        frame_seq: FRAME_SEQ.fetch_add(1, Ordering::Relaxed),
        content_type: "image/jpeg",
        data_base64: data.clone(),
        data,
        capture_pixel_width: pixel_width,
        capture_pixel_height: pixel_height,
        device_pixel_ratio: viewport.device_pixel_ratio,
        capture_scale_x,
        capture_scale_y,
        url: page_url(page).await,
        title: page_title(page).await,
        viewport,
        timestamp: crate::daemon::narration::events::now_ms(),
    }
}

pub async fn run_capture_loop(
    page: Arc<Page>,
    frames_tx: broadcast::Sender<FrameMessage>,
    daemon_state: Option<Arc<crate::daemon::state::DaemonState>>,
) {
    let mut events = match page.event_listener::<EventScreencastFrame>().await {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!("[view] failed to subscribe to screencast events: {e}");
            None
        }
    };

    let params = StartScreencastParams::builder()
        .format(StartScreencastFormat::Jpeg)
        .quality(65)
        .max_width(1920)
        .max_height(1080)
        .every_nth_frame(1)
        .build();

    if let Err(e) = page.execute(params).await {
        tracing::warn!("[view] failed to start screencast: {e}");
        return;
    }

    let mut fallback = tokio::time::interval(std::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            maybe_evt = async {
                match events.as_mut() {
                    Some(stream) => futures::StreamExt::next(stream).await,
                    None => std::future::pending().await,
                }
            } => {
                let Some(evt) = maybe_evt else {
                    events = None;
                    continue;
                };
                let _ = page
                    .execute(ScreencastFrameAckParams::new(evt.session_id))
                    .await;
                let data: String = evt.data.clone().into();
                let msg = frame_message(
                    &page,
                    data,
                    evt.metadata.device_width as u32,
                    evt.metadata.device_height as u32,
                ).await;
                record_frame_metadata(&daemon_state, &msg).await;
                let _ = frames_tx.send(msg);
            }
            _ = fallback.tick() => {
                if frames_tx.receiver_count() == 0 {
                    continue;
                }
                let params = CaptureScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Jpeg)
                    .quality(65)
                    .from_surface(true)
                    .optimize_for_speed(true)
                    .build();
                let Ok(resp) = page.execute(params).await else {
                    continue;
                };
                let data: String = resp.result.data.clone().into();
                let msg = frame_message(&page, data, 1920, 1080).await;
                record_frame_metadata(&daemon_state, &msg).await;
                let _ = frames_tx.send(msg);
            }
        }
    }
}

async fn record_frame_metadata(
    daemon_state: &Option<Arc<crate::daemon::state::DaemonState>>,
    frame: &FrameMessage,
) {
    let Some(state) = daemon_state else {
        return;
    };
    {
        let mut control = state.view_control.lock().await;
        control.update_frame_seq(frame.frame_seq);
    }
    let mut recordings = state.recordings.lock().await;
    if let Err(err) = recordings.record_frame(frame) {
        tracing::warn!("[view] failed to record frame: {err}");
    }
}

pub async fn run_capture_manager(
    mut page_rx: watch::Receiver<Arc<Page>>,
    frames_tx: broadcast::Sender<FrameMessage>,
    daemon_state: Option<Arc<crate::daemon::state::DaemonState>>,
) {
    let mut task = tokio::spawn(run_capture_loop(
        page_rx.borrow().clone(),
        frames_tx.clone(),
        daemon_state.clone(),
    ));

    loop {
        tokio::select! {
            changed = page_rx.changed() => {
                if changed.is_err() {
                    task.abort();
                    break;
                }
                task.abort();
                task = tokio::spawn(run_capture_loop(
                    page_rx.borrow().clone(),
                    frames_tx.clone(),
                    daemon_state.clone(),
                ));
            }
            result = &mut task => {
                if let Err(err) = result {
                    if !err.is_cancelled() {
                        tracing::warn!("[view] capture task ended: {err}");
                    }
                }
                task = tokio::spawn(run_capture_loop(
                    page_rx.borrow().clone(),
                    frames_tx.clone(),
                    daemon_state.clone(),
                ));
            }
        }
    }
}
