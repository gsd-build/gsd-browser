use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::identity::BrowserIdentity;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSessionStatus {
    pub session_name: Option<String>,
    pub active_url: String,
    pub active_title: String,
    pub identity: Option<BrowserIdentity>,
    pub control_owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFrame {
    pub sequence: u64,
    pub content_type: String,
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub device_pixel_ratio: f64,
    pub captured_at_ms: u64,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudToolRequest {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudUserInput {
    pub kind: String,
    pub owner: Option<String>,
    pub control_version: Option<u64>,
    pub frame_sequence: Option<u64>,
    pub coordinate_space: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub text: Option<String>,
    pub key: Option<String>,
    pub button: Option<String>,
    pub modifiers: Option<Vec<String>>,
    pub delta_x: Option<f64>,
    pub delta_y: Option<f64>,
}
