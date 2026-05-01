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
use crate::daemon::narration::policy::lead_for;
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

#[derive(Debug, Clone)]
pub struct ActionProbe {
    pub action: ActionKind,
    pub target: Option<TargetInfo>,
    pub label: String,
    pub lead_ms: u32,
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

    /// Emit pre-action narration and gate on control state.
    pub async fn emit_pre(&self, probe: &ActionProbe) -> Result<(), AbortedError> {
        if !self.is_active() {
            return Ok(());
        }
        self.control.wait_go().await?;
        let evt = NarrationEvent::Intent {
            action: probe.action,
            label: probe.label.clone(),
            target: probe.target.clone(),
            lead_ms: probe.lead_ms,
            timestamp_ms: now_ms(),
        };
        self.history.lock().await.push(evt.clone());
        let _ = self.bus.send(evt);
        Ok(())
    }

    /// Sleep the adaptive lead time.
    pub async fn sleep_lead(&self, probe: &ActionProbe) {
        if !self.is_active() || self.no_delay {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(probe.lead_ms as u64)).await;
    }

    /// Emit post-action narration with success/failure metadata.
    pub async fn emit_post<T, E: std::fmt::Display>(
        &self,
        probe: &ActionProbe,
        result: &Result<T, E>,
    ) {
        if !self.is_active() {
            return;
        }
        let (success, error) = match result {
            Ok(_) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };
        let evt = NarrationEvent::Complete {
            action: probe.action,
            label: probe.label.clone(),
            target: probe.target.clone(),
            success,
            error,
            timestamp_ms: now_ms(),
        };
        self.history.lock().await.push(evt.clone());
        let _ = self.bus.send(evt);
    }

    /// Build an ActionProbe by running selector geometry probing.
    pub async fn probe_action(
        &self,
        page: &chromiumoxide::Page,
        action: ActionKind,
        selector: Option<&str>,
        hint: Option<&str>,
    ) -> ActionProbe {
        let target = match selector {
            Some(sel) => probe::run_probe(page, sel, true).await,
            None => None,
        };
        let label = probe::label_for(action, target.as_ref(), hint);
        let lead_ms = lead_for(target.as_ref());
        ActionProbe {
            action,
            target,
            label,
            lead_ms,
        }
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

    #[tokio::test]
    async fn emit_pre_skipped_when_inactive() {
        let n = Narrator::new(false);
        let mut rx = n.subscribe();
        let probe = ActionProbe {
            action: ActionKind::Click,
            target: None,
            label: "test".into(),
            lead_ms: 80,
        };
        n.emit_pre(&probe).await.unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn emit_pre_broadcasts_when_active() {
        let n = Narrator::new(false);
        n.activate();
        let mut rx = n.subscribe();
        let probe = ActionProbe {
            action: ActionKind::Click,
            target: None,
            label: "test".into(),
            lead_ms: 80,
        };
        n.emit_pre(&probe).await.unwrap();
        let evt = rx.recv().await.unwrap();
        match evt {
            NarrationEvent::Intent { label, .. } => assert_eq!(label, "test"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn emit_post_includes_error_message() {
        let n = Narrator::new(false);
        n.activate();
        let mut rx = n.subscribe();
        let probe = ActionProbe {
            action: ActionKind::Click,
            target: None,
            label: "x".into(),
            lead_ms: 80,
        };
        let result: Result<(), String> = Err("not found".into());
        n.emit_post(&probe, &result).await;
        let evt = rx.recv().await.unwrap();
        match evt {
            NarrationEvent::Complete { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some("not found".into()));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn sleep_lead_skipped_when_no_delay() {
        let n = Narrator::new(true);
        n.activate();
        let probe = ActionProbe {
            action: ActionKind::Click,
            target: None,
            label: "x".into(),
            lead_ms: 1000,
        };
        let start = std::time::Instant::now();
        n.sleep_lead(&probe).await;
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
    }
}
