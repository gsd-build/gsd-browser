use crate::daemon::narration::events::ControlState;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Debug)]
pub struct AbortedError;

impl std::fmt::Display for AbortedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "action aborted by user")
    }
}

impl std::error::Error for AbortedError {}

#[derive(Debug)]
pub struct Control {
    state: Mutex<ControlState>,
    notify: Notify,
}

impl Control {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ControlState::Running),
            notify: Notify::new(),
        })
    }

    pub async fn get(&self) -> ControlState {
        *self.state.lock().await
    }

    pub async fn set(&self, new: ControlState) {
        *self.state.lock().await = new;
        self.notify.notify_waiters();
    }

    /// Block until the gate opens. Returns Ok(()) on Running or Step, consuming Step.
    /// Returns Err(AbortedError) on Aborted and resets control to Running.
    pub async fn wait_go(&self) -> Result<(), AbortedError> {
        loop {
            let current = {
                let mut guard = self.state.lock().await;
                let s = *guard;
                if s == ControlState::Step {
                    *guard = ControlState::Paused;
                }
                if s == ControlState::Aborted {
                    *guard = ControlState::Running;
                }
                s
            };
            match current {
                ControlState::Running | ControlState::Step => return Ok(()),
                ControlState::Aborted => return Err(AbortedError),
                ControlState::Paused => {
                    self.notify.notified().await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn running_passes_immediately() {
        let c = Control::new();
        c.wait_go().await.unwrap();
    }

    #[tokio::test]
    async fn step_passes_once_then_pauses() {
        let c = Control::new();
        c.set(ControlState::Step).await;
        c.wait_go().await.unwrap();
        assert_eq!(c.get().await, ControlState::Paused);
    }

    #[tokio::test]
    async fn aborted_returns_err_and_auto_resets() {
        let c = Control::new();
        c.set(ControlState::Aborted).await;
        let r = c.wait_go().await;
        assert!(r.is_err());
        assert_eq!(c.get().await, ControlState::Running);
    }

    #[tokio::test]
    async fn paused_blocks_until_resumed() {
        let c = Control::new();
        c.set(ControlState::Paused).await;
        let c2 = c.clone();
        let task = tokio::spawn(async move { c2.wait_go().await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished(), "should still be waiting");
        c.set(ControlState::Running).await;
        let r = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(r.is_ok());
    }
}
