use serde::{Deserialize, Deserializer, Serialize};
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
    pub viewport_css_width: u32,
    pub viewport_css_height: u32,
    pub capture_pixel_width: u32,
    pub capture_pixel_height: u32,
    pub device_pixel_ratio: f64,
    pub capture_scale_x: f64,
    pub capture_scale_y: f64,
    pub captured_at_ms: u64,
    pub encoded_bytes: usize,
    pub quality: u32,
    pub capture_pixel_ratio: f64,
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
pub struct CloudToolManifest {
    pub manifest_version: u32,
    pub runtime_min_version: String,
    pub input: CloudInputCapabilities,
    pub identity: CloudIdentityCapabilities,
    pub methods: Vec<CloudToolManifestMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudInputCapabilities {
    pub coordinate_space: String,
    pub kinds: Vec<String>,
    pub pointer_phases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudIdentityCapabilities {
    pub scopes: Vec<String>,
    pub local_first: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudToolManifestMethod {
    pub name: String,
    pub category: String,
}

pub const CLOUD_TOOL_MANIFEST_VERSION: u32 = 1;
pub const CLOUD_TOOL_RUNTIME_MIN_VERSION: &str = "0.1.20";
pub const CLOUD_INPUT_COORDINATE_SPACE: &str = "viewport_css";
pub const CLOUD_INPUT_KINDS: &[&str] = &[
    "pointer",
    "wheel",
    "key",
    "text",
    "paste",
    "composition",
    "navigation",
];
pub const CLOUD_POINTER_PHASES: &[&str] = &[
    "move",
    "down",
    "up",
    "click",
    "double_click",
    "context_click",
];
pub const CLOUD_KEY_PHASES: &[&str] = &["down", "up", "press"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRefs {
    pub version: u64,
    pub refs: Vec<CloudRef>,
    pub truncated: bool,
    pub limit: Option<u64>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRef {
    #[serde(rename = "ref")]
    pub ref_id: String,
    pub key: String,
    pub role: String,
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudUserInput {
    pub input_id: Option<String>,
    pub kind: String,
    pub owner: Option<String>,
    pub control_version: Option<u64>,
    pub frame_seq: Option<u64>,
    pub coordinate_space: Option<String>,
    pub phase: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub text: Option<String>,
    pub key: Option<String>,
    pub code: Option<String>,
    pub location: Option<i64>,
    pub repeat: Option<bool>,
    pub button: Option<String>,
    pub buttons: Option<i64>,
    pub click_count: Option<i64>,
    pub pointer_type: Option<String>,
    pub modifiers: Option<Vec<String>>,
    pub commit_mode: Option<String>,
    pub mime_types: Option<Vec<String>>,
    pub delta_x: Option<f64>,
    pub delta_y: Option<f64>,
    pub delta_mode: Option<String>,
    pub url: Option<String>,
    pub action: Option<String>,
}

impl<'de> Deserialize<'de> for CloudUserInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawCloudUserInput {
            input_id: Option<String>,
            kind: Option<String>,
            owner: Option<String>,
            control_version: Option<u64>,
            #[serde(alias = "frameSequence")]
            frame_seq: Option<u64>,
            coordinate_space: Option<String>,
            phase: Option<String>,
            x: Option<f64>,
            y: Option<f64>,
            text: Option<String>,
            key: Option<String>,
            code: Option<String>,
            location: Option<i64>,
            repeat: Option<bool>,
            button: Option<String>,
            buttons: Option<i64>,
            click_count: Option<i64>,
            pointer_type: Option<String>,
            modifiers: Option<Vec<String>>,
            commit_mode: Option<String>,
            mime_types: Option<Vec<String>>,
            delta_x: Option<f64>,
            delta_y: Option<f64>,
            delta_mode: Option<String>,
            url: Option<String>,
            action: Option<String>,
        }

        let raw = RawCloudUserInput::deserialize(deserializer)?;
        let kind = raw
            .kind
            .ok_or_else(|| serde::de::Error::custom("kind is required"))?;
        let input = CloudUserInput {
            input_id: raw.input_id,
            kind,
            owner: raw.owner,
            control_version: raw.control_version,
            frame_seq: raw.frame_seq,
            coordinate_space: raw.coordinate_space,
            phase: raw.phase,
            x: raw.x,
            y: raw.y,
            text: raw.text,
            key: raw.key,
            code: raw.code,
            location: raw.location,
            repeat: raw.repeat,
            button: raw.button,
            buttons: raw.buttons,
            click_count: raw.click_count,
            pointer_type: raw.pointer_type,
            modifiers: raw.modifiers,
            commit_mode: raw.commit_mode,
            mime_types: raw.mime_types,
            delta_x: raw.delta_x,
            delta_y: raw.delta_y,
            delta_mode: raw.delta_mode,
            url: raw.url,
            action: raw.action,
        };
        input.validate().map_err(serde::de::Error::custom)?;
        Ok(input)
    }
}

impl CloudUserInput {
    pub fn into_user_input_event(
        self,
        default_source: &str,
    ) -> Result<crate::viewer::UserInputEventV1, String> {
        use crate::viewer::{
            ControlOwner, CoordinateSpace, UserInputEventV1, UserInputKind, UserInputSource,
        };

        let source = match default_source {
            "cloud" => UserInputSource::Cloud,
            "viewer" => UserInputSource::Viewer,
            other => return Err(format!("unsupported input source: {other}")),
        };
        let owner = match self.owner.as_deref().unwrap_or("agent") {
            "agent" => ControlOwner::Agent,
            "user" => ControlOwner::User,
            "system" => ControlOwner::System,
            other => return Err(format!("unsupported owner: {other}")),
        };
        let kind = match self.kind.as_str() {
            "pointer" => UserInputKind::Pointer,
            "wheel" => UserInputKind::Wheel,
            "key" => UserInputKind::Key,
            "text" => UserInputKind::Text,
            "paste" => UserInputKind::Paste,
            "composition" => UserInputKind::Composition,
            "navigation" => UserInputKind::Navigation,
            other => return Err(format!("unsupported user input kind: {other}")),
        };

        let event = UserInputEventV1 {
            schema: crate::viewer::USER_INPUT_SCHEMA.to_string(),
            input_id: self.input_id,
            source,
            owner,
            control_version: self.control_version.unwrap_or_default(),
            frame_seq: self.frame_seq.unwrap_or_default(),
            page_id: None,
            target_id: None,
            frame_id: None,
            coordinate_space: CoordinateSpace::ViewportCss,
            kind,
            phase: self.phase,
            x: self.x,
            y: self.y,
            text: self.text,
            key: self.key,
            code: self.code,
            location: self.location,
            repeat: self.repeat,
            button: self.button,
            buttons: self.buttons,
            click_count: self.click_count,
            pointer_type: self.pointer_type,
            modifiers: self.modifiers,
            delta_x: self.delta_x,
            delta_y: self.delta_y,
            url: self.url,
            action: self.action,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(coordinate_space) = self.coordinate_space.as_deref() {
            if coordinate_space != CLOUD_INPUT_COORDINATE_SPACE {
                return Err(format!(
                    "coordinateSpace must be {CLOUD_INPUT_COORDINATE_SPACE}, got {coordinate_space}"
                ));
            }
        }
        if self.click_count.is_some_and(|value| value < 0) {
            return Err("clickCount must be non-negative".to_string());
        }
        match self.kind.as_str() {
            "pointer" => {
                match self.phase.as_deref() {
                    Some(phase) if CLOUD_POINTER_PHASES.contains(&phase) => {}
                    Some(other) => return Err(format!("invalid pointer phase: {other}")),
                    None => return Err("pointer input requires phase".to_string()),
                }
                if self.x.is_none() || self.y.is_none() {
                    return Err("pointer input requires x and y".to_string());
                }
            }
            "wheel" => {}
            "key" => {
                match self.phase.as_deref() {
                    Some(phase) if CLOUD_KEY_PHASES.contains(&phase) => {}
                    Some(other) => return Err(format!("invalid key phase: {other}")),
                    None => return Err("key input requires phase".to_string()),
                }
                if self.key.is_none() {
                    return Err("key input requires key".to_string());
                }
            }
            "text" | "paste" | "composition" => {
                if self.text.is_none() {
                    return Err(format!("{} input requires text", self.kind));
                }
            }
            "navigation" => {
                let url = self
                    .url
                    .as_deref()
                    .ok_or_else(|| "navigation input requires url".to_string())?;
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err("navigation url must be http or https".to_string());
                }
            }
            "ref_action" | "viewport" => {
                return Err(format!("unsupported user input kind: {}", self.kind));
            }
            other => return Err(format!("unsupported user input kind: {other}")),
        }
        Ok(())
    }
}
