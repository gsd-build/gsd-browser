use crate::daemon::narration::events::NarrationEvent;
use std::collections::VecDeque;

pub const HISTORY_CAPACITY: usize = 256;

#[derive(Debug, Default)]
pub struct History {
    events: VecDeque<NarrationEvent>,
}

impl History {
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    pub fn push(&mut self, event: NarrationEvent) {
        if self.events.len() == HISTORY_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn recent(&self, n: usize) -> Vec<NarrationEvent> {
        let take = n.min(self.events.len());
        self.events.iter().rev().take(take).rev().cloned().collect()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::narration::events::{now_ms, ActionKind, NarrationEvent};

    fn make_intent(label: &str) -> NarrationEvent {
        NarrationEvent::Intent {
            action: ActionKind::Click,
            label: label.into(),
            target: None,
            lead_ms: 120,
            timestamp_ms: now_ms(),
        }
    }

    #[test]
    fn push_increases_len() {
        let mut h = History::new();
        h.push(make_intent("a"));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn evicts_oldest_at_capacity() {
        let mut h = History::new();
        for i in 0..(HISTORY_CAPACITY + 5) {
            h.push(make_intent(&i.to_string()));
        }
        assert_eq!(h.len(), HISTORY_CAPACITY);
        let first = h.recent(HISTORY_CAPACITY).into_iter().next().unwrap();
        if let NarrationEvent::Intent { label, .. } = first {
            assert_eq!(label, "5");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn recent_returns_in_chronological_order() {
        let mut h = History::new();
        h.push(make_intent("a"));
        h.push(make_intent("b"));
        h.push(make_intent("c"));
        let r = h.recent(3);
        let labels: Vec<_> = r
            .iter()
            .filter_map(|e| match e {
                NarrationEvent::Intent { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, vec!["a", "b", "c"]);
    }
}
