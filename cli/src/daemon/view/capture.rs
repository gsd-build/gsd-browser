use chromiumoxide::cdp::browser_protocol::page::{
    EventScreencastFrame, ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
};
use chromiumoxide::Page;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

#[derive(Clone, serde::Serialize)]
pub struct FrameMessage {
    #[serde(rename = "type")]
    ty: &'static str,
    pub data: String,
    pub viewport: ViewportInfo,
    pub timestamp: u64,
}

#[derive(Clone, serde::Serialize)]
pub struct ViewportInfo {
    pub width: u32,
    pub height: u32,
}

pub async fn run_capture_loop(page: Arc<Page>, frames_tx: broadcast::Sender<FrameMessage>) {
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

    let mut events = match page.event_listener::<EventScreencastFrame>().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("[view] failed to subscribe to screencast events: {e}");
            return;
        }
    };

    while let Some(evt) = futures::StreamExt::next(&mut events).await {
        let _ = page
            .execute(ScreencastFrameAckParams::new(evt.session_id))
            .await;
        let data: String = evt.data.clone().into();
        let msg = FrameMessage {
            ty: "frame",
            data,
            viewport: ViewportInfo {
                width: evt.metadata.device_width as u32,
                height: evt.metadata.device_height as u32,
            },
            timestamp: crate::daemon::narration::events::now_ms(),
        };
        let _ = frames_tx.send(msg);
    }
}

pub async fn run_capture_manager(
    mut page_rx: watch::Receiver<Arc<Page>>,
    frames_tx: broadcast::Sender<FrameMessage>,
) {
    let mut task = tokio::spawn(run_capture_loop(
        page_rx.borrow().clone(),
        frames_tx.clone(),
    ));

    loop {
        tokio::select! {
            changed = page_rx.changed() => {
                if changed.is_err() {
                    task.abort();
                    break;
                }
                task.abort();
                task = tokio::spawn(run_capture_loop(page_rx.borrow().clone(), frames_tx.clone()));
            }
            result = &mut task => {
                if let Err(err) = result {
                    if !err.is_cancelled() {
                        tracing::warn!("[view] capture task ended: {err}");
                    }
                }
                task = tokio::spawn(run_capture_loop(page_rx.borrow().clone(), frames_tx.clone()));
            }
        }
    }
}
