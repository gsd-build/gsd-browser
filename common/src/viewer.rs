use serde::{Deserialize, Deserializer, Serialize};

pub const VIEWER_COMMAND_SCHEMA: &str = "ViewerCommandV1";
pub const USER_INPUT_SCHEMA: &str = "UserInputEventV1";
pub const ANNOTATION_SCHEMA: &str = "AnnotationV1";
pub const RECORDING_EVENT_SCHEMA: &str = "RecordingEventV1";
pub const BROWSER_ARTIFACT_BUNDLE_SCHEMA: &str = "BrowserArtifactBundleV1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlOwner {
    Agent,
    User,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlMode {
    AgentRunning,
    UserTakeover,
    Paused,
    Step,
    ApprovalRequired,
    Annotating,
    Sensitive,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedControlStateV1 {
    pub owner: ControlOwner,
    pub mode: ControlMode,
    pub control_version: u64,
    pub frame_seq: u64,
    pub requested_by: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub sensitive: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerCommandType {
    Input,
    Control,
    Annotation,
    Recording,
    Sensitive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerCommandV1 {
    pub schema: String,
    pub command_id: String,
    pub session_id: String,
    pub viewer_id: String,
    pub owner: ControlOwner,
    pub control_version: u64,
    pub frame_seq: u64,
    #[serde(rename = "type")]
    pub command_type: ViewerCommandType,
    pub payload: ViewerCommandPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ViewerCommandPayload {
    Input(UserInputEventV1),
    Control(ControlCommandV1),
    Annotation(AnnotationCommandV1),
    Recording(RecordingCommandV1),
    Sensitive(SensitiveCommandV1),
}

impl<'de> Deserialize<'de> for ViewerCommandPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("payload schema is required"))?;
        match schema {
            USER_INPUT_SCHEMA => serde_json::from_value(value)
                .map(ViewerCommandPayload::Input)
                .map_err(serde::de::Error::custom),
            "ControlCommandV1" => serde_json::from_value(value)
                .map(ViewerCommandPayload::Control)
                .map_err(serde::de::Error::custom),
            ANNOTATION_SCHEMA | "AnnotationCommandV1" => serde_json::from_value(value)
                .map(ViewerCommandPayload::Annotation)
                .map_err(serde::de::Error::custom),
            "RecordingCommandV1" => serde_json::from_value(value)
                .map(ViewerCommandPayload::Recording)
                .map_err(serde::de::Error::custom),
            "SensitiveCommandV1" => serde_json::from_value(value)
                .map(ViewerCommandPayload::Sensitive)
                .map_err(serde::de::Error::custom),
            other => Err(serde::de::Error::custom(format!(
                "unsupported viewer command payload schema: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSource {
    Viewer,
    Cloud,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    ViewportCss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInputKind {
    Pointer,
    Wheel,
    Key,
    Text,
    Paste,
    Composition,
    Navigation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputEventV1 {
    pub schema: String,
    pub input_id: Option<String>,
    pub source: UserInputSource,
    pub owner: ControlOwner,
    pub control_version: u64,
    pub frame_seq: u64,
    pub page_id: Option<u64>,
    pub target_id: Option<String>,
    pub frame_id: Option<String>,
    pub coordinate_space: CoordinateSpace,
    pub kind: UserInputKind,
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
    pub delta_x: Option<f64>,
    pub delta_y: Option<f64>,
    pub url: Option<String>,
    pub action: Option<String>,
}

impl UserInputEventV1 {
    pub fn validate(&self) -> Result<(), String> {
        match self.kind {
            UserInputKind::Pointer => {
                let phase = self
                    .phase
                    .as_deref()
                    .ok_or("pointer input requires phase")?;
                if !matches!(
                    phase,
                    "move" | "down" | "up" | "click" | "double_click" | "context_click"
                ) {
                    return Err(format!("invalid pointer phase: {phase}"));
                }
                if self.x.is_none() || self.y.is_none() {
                    return Err("pointer input requires x and y".to_string());
                }
            }
            UserInputKind::Wheel => {}
            UserInputKind::Key => {
                let phase = self.phase.as_deref().ok_or("key input requires phase")?;
                if !matches!(phase, "down" | "up" | "press") {
                    return Err(format!("invalid key phase: {phase}"));
                }
                if self.key.is_none() {
                    return Err("key input requires key".to_string());
                }
            }
            UserInputKind::Text | UserInputKind::Paste | UserInputKind::Composition => {
                if self.text.is_none() {
                    return Err(format!("{:?} input requires text", self.kind));
                }
            }
            UserInputKind::Navigation => {
                let url = self.url.as_deref().ok_or("navigation input requires url")?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err("navigation url must be http or https".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlCommandV1 {
    pub schema: String,
    pub action: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveCommandV1 {
    pub schema: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationCommandV1 {
    pub schema: String,
    pub action: String,
    pub annotation: Option<AnnotationV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingCommandV1 {
    pub schema: String,
    pub action: String,
    pub name: Option<String>,
    pub recording_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerRejectionReason {
    ViewerNotAuthenticated,
    WrongOrigin,
    ExpiredViewerToken,
    StaleControlVersion,
    StaleFrameSeq,
    NonOwnerInput,
    AgentNotAllowedWhilePaused,
    ApprovalRequired,
    ApprovalDenied,
    ApprovalTimeout,
    SensitivePrivacyMode,
    AnnotationModeBlocksPageInput,
    UnsupportedNativeFlow,
    RiskGateRequired,
    MalformedCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationStatus {
    Open,
    Resolved,
    Ambiguous,
    Stale,
    Missing,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportSnapshotV1 {
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationSelectionV1 {
    pub coordinate_space: CoordinateSpace,
    pub box_value: serde_json::Value,
    pub crop_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationV1 {
    pub schema: String,
    pub annotation_id: String,
    pub session_id: String,
    pub viewer_id: String,
    pub page_id: Option<u64>,
    pub target_id: Option<String>,
    pub frame_id: Option<String>,
    pub frame_seq: u64,
    pub kind: String,
    pub status: AnnotationStatus,
    pub note: String,
    pub url: String,
    pub title: String,
    pub origin: String,
    pub created_by: String,
    pub created_at_ms: u64,
    pub viewport: serde_json::Value,
    pub selection: serde_json::Value,
    pub target: Option<serde_json::Value>,
    pub artifact_refs: serde_json::Value,
    pub partial_reasons: Vec<String>,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageStateV1 {
    pub schema: String,
    pub page_id: u64,
    pub target_id: Option<String>,
    pub frame_id: Option<String>,
    pub frame_seq: u64,
    pub url: String,
    pub title: String,
    pub origin: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingEventV1 {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub schema: String,
    pub recording_id: String,
    pub source: String,
    pub owner: String,
    pub control_version: u64,
    pub frame_seq: u64,
    pub kind: String,
    pub url: String,
    pub title: String,
    pub origin: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
    pub redaction: serde_json::Value,
    pub artifact_refs: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserArtifactManifestV1 {
    pub schema: String,
    pub recording_id: String,
    pub session_id: String,
    pub name: String,
    pub started_at_ms: u64,
    pub stopped_at_ms: Option<u64>,
    pub start_seq: u64,
    pub stop_seq: Option<u64>,
    pub event_count: u64,
    pub frame_count: u64,
    pub annotation_count: u64,
    pub console_error_count: u64,
    pub failed_request_count: u64,
    pub origin_scopes: Vec<String>,
    pub excluded_boundary_events: Vec<serde_json::Value>,
    pub redaction: serde_json::Value,
    pub artifacts: serde_json::Value,
    pub hashes: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestV1 {
    pub approval_id: String,
    pub command_hash: String,
    pub summary: String,
    pub origin: String,
    pub expires_at_ms: u64,
    pub risk: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn viewer_command_deserializes_pointer_input() {
        let cmd: ViewerCommandV1 = serde_json::from_value(json!({
            "schema": "ViewerCommandV1",
            "commandId": "cmd_1",
            "sessionId": "sess_1",
            "viewerId": "view_1",
            "owner": "user",
            "controlVersion": 7,
            "frameSeq": 42,
            "type": "input",
            "payload": {
                "schema": "UserInputEventV1",
                "inputId": "inp_1",
                "source": "viewer",
                "owner": "user",
                "controlVersion": 7,
                "frameSeq": 42,
                "coordinateSpace": "viewport_css",
                "kind": "pointer",
                "phase": "click",
                "x": 12.5,
                "y": 44.0,
                "button": "left"
            }
        }))
        .expect("valid command");

        assert_eq!(cmd.command_id, "cmd_1");
        assert_eq!(cmd.command_type, ViewerCommandType::Input);
        match cmd.payload {
            ViewerCommandPayload::Input(input) => {
                assert_eq!(input.kind, UserInputKind::Pointer);
                assert_eq!(input.phase.as_deref(), Some("click"));
                assert_eq!(input.coordinate_space, CoordinateSpace::ViewportCss);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn user_input_rejects_missing_pointer_coordinates() {
        let input = serde_json::from_value::<UserInputEventV1>(json!({
            "schema": "UserInputEventV1",
            "inputId": "inp_2",
            "source": "viewer",
            "owner": "user",
            "controlVersion": 1,
            "frameSeq": 1,
            "coordinateSpace": "viewport_css",
            "kind": "pointer",
            "phase": "click"
        }))
        .expect("deserialize shape");

        let err = input.validate().expect_err("pointer x/y required");
        assert!(err.contains("pointer input requires x and y"));
    }

    #[test]
    fn cloud_user_input_converts_to_canonical_event() {
        let cloud = crate::cloud::CloudUserInput {
            input_id: Some("inp_cloud".to_string()),
            kind: "wheel".to_string(),
            owner: Some("user".to_string()),
            control_version: Some(3),
            frame_seq: Some(9),
            coordinate_space: Some("viewport_css".to_string()),
            phase: None,
            x: Some(1.0),
            y: Some(2.0),
            text: None,
            key: None,
            code: None,
            location: None,
            repeat: None,
            button: None,
            buttons: None,
            click_count: None,
            pointer_type: None,
            modifiers: Some(vec!["shift".to_string()]),
            commit_mode: None,
            mime_types: None,
            delta_x: Some(0.0),
            delta_y: Some(120.0),
            delta_mode: None,
            url: None,
            action: None,
        };

        let event = cloud
            .into_user_input_event("cloud")
            .expect("canonical input");
        assert_eq!(event.input_id.as_deref(), Some("inp_cloud"));
        assert_eq!(event.source, UserInputSource::Cloud);
        assert_eq!(event.kind, UserInputKind::Wheel);
        assert_eq!(event.delta_y, Some(120.0));
    }
}
