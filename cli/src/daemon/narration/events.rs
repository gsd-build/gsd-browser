#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    Hover,
    Type,
    Press,
    Scroll,
    Navigate,
    SelectOption,
    SetChecked,
    Drag,
    UploadFile,
    Act,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlState {
    Running,
    Paused,
    Step,
    Aborted,
}

impl Default for ControlState {
    fn default() -> Self {
        ControlState::Running
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetInfo {
    pub selector: Option<String>,
    pub ref_id: Option<String>,
    pub bbox: Option<BoundingBox>,
    pub aim: Option<BoundingBox>,
    pub scrolled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NarrationEvent {
    Intent {
        action: ActionKind,
        label: String,
        target: Option<TargetInfo>,
        lead_ms: u32,
        timestamp_ms: u64,
    },
    Complete {
        action: ActionKind,
        label: String,
        target: Option<TargetInfo>,
        success: bool,
        error: Option<String>,
        timestamp_ms: u64,
    },
    Goal {
        text: Option<String>,
        timestamp_ms: u64,
    },
    Control {
        state: ControlState,
        timestamp_ms: u64,
    },
    TabChanged {
        url: String,
        target_id: String,
        timestamp_ms: u64,
    },
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
