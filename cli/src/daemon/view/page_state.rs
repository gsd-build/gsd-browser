use gsd_browser_common::viewer::PageStateV1;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

pub struct PageStateStore {
    frame_seq: AtomicU64,
    sender: watch::Sender<PageStateV1>,
}

impl PageStateStore {
    pub fn new() -> Self {
        let initial = PageStateV1 {
            schema: "PageStateV1".to_string(),
            page_id: 0,
            target_id: None,
            frame_id: None,
            frame_seq: 1,
            url: String::new(),
            title: String::new(),
            origin: String::new(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        };
        let (sender, _) = watch::channel(initial);
        Self {
            frame_seq: AtomicU64::new(1),
            sender,
        }
    }

    pub fn snapshot(&self) -> PageStateV1 {
        self.sender.borrow().clone()
    }

    pub fn increment_frame_seq(&self) -> u64 {
        let seq = self.frame_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let mut state = self.snapshot();
        state.frame_seq = seq;
        let _ = self.sender.send(state);
        seq
    }

    pub fn update_url_title_origin(&self, url: String, title: String, origin: String) {
        let mut state = self.snapshot();
        state.url = url;
        state.title = title;
        state.origin = origin;
        let _ = self.sender.send(state);
    }

    pub fn subscribe(&self) -> watch::Receiver<PageStateV1> {
        self.sender.subscribe()
    }
}

impl Default for PageStateStore {
    fn default() -> Self {
        Self::new()
    }
}
