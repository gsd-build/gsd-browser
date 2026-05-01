use crate::daemon::narration::events::{now_ms, NarrationEvent};
use crate::daemon::state::DaemonState;
use chromiumoxide::cdp::browser_protocol::target::EventTargetCreated;
use chromiumoxide::Browser;
use gsd_browser_common::session::SessionHealthStatus;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};

pub async fn run_target_follow(
    browser: Arc<Mutex<Browser>>,
    state: Arc<DaemonState>,
    active_page_tx: watch::Sender<Arc<chromiumoxide::Page>>,
) {
    let mut events = match browser
        .lock()
        .await
        .event_listener::<EventTargetCreated>()
        .await
    {
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
        if ti.url.starts_with("chrome://") {
            continue;
        }

        let target_id = ti.target_id.as_ref().to_string();
        if state
            .pages
            .lock()
            .unwrap()
            .find_by_target_id(&target_id)
            .is_some()
        {
            continue;
        }

        let mut page = None;
        for _ in 0..10 {
            match browser.lock().await.get_page(ti.target_id.clone()).await {
                Ok(p) => {
                    page = Some(p);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        let Some(page) = page else {
            tracing::warn!("[view] failed to attach new page target: {target_id}");
            continue;
        };

        crate::daemon::set_default_viewport(&page).await;
        crate::daemon::helpers::inject_helpers(&page).await;
        crate::daemon::settle::ensure_mutation_counter(&page).await;

        let url = page.url().await.ok().flatten().unwrap_or_default();
        let title = page
            .evaluate("document.title")
            .await
            .ok()
            .and_then(|value| value.into_value::<String>().ok())
            .unwrap_or_default();
        let page = Arc::new(page);
        let page_id = {
            let mut pages = state.pages.lock().unwrap();
            match pages.find_by_target_id(&target_id) {
                Some(id) => {
                    pages.set_active(id);
                    pages.update_metadata(id, title.clone(), url.clone());
                    id
                }
                None => {
                    let id = pages.register(page.clone(), title.clone(), url.clone());
                    pages.set_active(id);
                    id
                }
            }
        };

        *state.selected_frame.lock().unwrap() = None;
        let _ = active_page_tx.send(page.clone());
        let _ = state.narrator.bus.send(NarrationEvent::TabChanged {
            url: url.clone(),
            target_id: target_id.clone(),
            timestamp_ms: now_ms(),
        });
        let _ = crate::daemon::handlers::session::sync_session_manifest(
            page.as_ref(),
            &state,
            Some(SessionHealthStatus::Healthy),
            None,
        )
        .await;

        tracing::info!("[view] following page {page_id}: {url} ({target_id})");
    }
}
