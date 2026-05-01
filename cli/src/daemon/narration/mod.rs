pub mod control;
pub mod events;
pub mod history;
pub mod policy;
pub mod probe;

#[allow(unused_imports)]
pub use events::{now_ms, ActionKind, BoundingBox, ControlState, NarrationEvent, TargetInfo};
#[allow(unused_imports)]
pub use control::{AbortedError, Control};

use crate::daemon::narration::history::History;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

const BUS_CAPACITY: usize = 256;

pub struct Narrator {
    pub control: Arc<Control>,
    pub history: Mutex<History>,
    pub bus: broadcast::Sender<NarrationEvent>,
    /// When false, emit and delay paths stay inert until a viewer is started.
    pub active: std::sync::atomic::AtomicBool,
    pub goal: Mutex<Option<String>>,
    pub no_delay: bool,
}

impl Narrator {
    pub fn new(no_delay: bool) -> Arc<Self> {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Arc::new(Self {
            control: Control::new(),
            history: Mutex::new(History::new()),
            bus: tx,
            active: std::sync::atomic::AtomicBool::new(false),
            goal: Mutex::new(None),
            no_delay,
        })
    }

    pub fn activate(&self) {
        self.active
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NarrationEvent> {
        self.bus.subscribe()
    }

    pub async fn set_goal(&self, text: Option<String>) {
        *self.goal.lock().await = text.clone();
        let _ = self.bus.send(NarrationEvent::Goal {
            text,
            timestamp_ms: now_ms(),
        });
    }

    pub async fn current_goal(&self) -> Option<String> {
        self.goal.lock().await.clone()
    }

    pub async fn set_control(&self, state: ControlState) {
        self.control.set(state).await;
        let _ = self.bus.send(NarrationEvent::Control {
            state,
            timestamp_ms: now_ms(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn narrator_inactive_by_default() {
        let n = Narrator::new(false);
        assert!(!n.is_active());
    }

    #[tokio::test]
    async fn set_goal_broadcasts() {
        let n = Narrator::new(false);
        let mut rx = n.subscribe();
        n.set_goal(Some("test".into())).await;
        let evt = rx.recv().await.unwrap();
        match evt {
            NarrationEvent::Goal { text, .. } => assert_eq!(text, Some("test".into())),
            _ => panic!("wrong event"),
        }
    }

    #[tokio::test]
    async fn current_goal_reads_back() {
        let n = Narrator::new(false);
        n.set_goal(Some("x".into())).await;
        assert_eq!(n.current_goal().await, Some("x".into()));
    }
}
