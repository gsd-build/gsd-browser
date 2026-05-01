use chromiumoxide::cdp::browser_protocol::target::EventTargetCreated;
use chromiumoxide::Browser;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn run_target_follow(browser: Arc<Mutex<Browser>>) {
    let mut events = match browser.lock().await.event_listener::<EventTargetCreated>().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("[view] target-follow subscribe failed: {e}");
            return;
        }
    };

    while let Some(evt) = futures::StreamExt::next(&mut events).await {
        let ti = &evt.target_info;
        if ti.r#type != "page" {
            continue;
        }
        if ti.url.starts_with("chrome://") || ti.url.starts_with("about:") {
            continue;
        }
        tracing::info!(
            "[view] new page target: {} ({})",
            ti.url,
            ti.target_id.as_ref()
        );
    }
}
