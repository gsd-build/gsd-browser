# Interactive Browser Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `gsd-browser view` into a secure, bidirectional browser workbench with shared human/agent control, annotation mode, bounded flow recording, and sensitive capture controls.

**Architecture:** The viewer remains a local authenticated surface that streams frames from the daemon and sends typed commands back to the daemon. All browser effects pass through daemon-owned authorization, canonical input dispatch, annotation capture, recording capture, and privacy guards. Cloud input and viewer input share one protocol model and one CDP dispatch path.

**Tech Stack:** Rust 2021, Tokio, Axum WebSocket/HTTP, Chromiumoxide CDP, Serde JSON, embedded HTML/CSS/JS viewer asset.

---

## Source Spec

Primary spec: `docs/superpowers/specs/2026-05-02-interactive-browser-workbench-design.md`

Spike evidence:

- `spikes/001-viewer-input-fidelity`
- `spikes/002-viewer-security-token`
- `spikes/003-shared-control-state-machine`
- `spikes/004-annotation-mode-prototype`
- `spikes/005-flow-recording-artifact`

## File Structure

- Create `common/src/viewer.rs`: shared protocol types for viewer commands, canonical user input, shared control state, annotations, recording manifests, page state, and rejection reasons.
- Modify `common/src/lib.rs`: export `viewer`.
- Modify `common/src/cloud.rs`: keep cloud-facing compatibility and convert `CloudUserInput` into `UserInputEventV1`.
- Modify `common/Cargo.toml`: add `uuid` only if typed ids use UUID generation in common tests; otherwise keep id generation in CLI.
- Create `cli/src/daemon/view/auth.rs`: token issue/verify, loopback host checks, origin checks, capability checks, request rejection reasons.
- Create `cli/src/daemon/view/control.rs`: shared control state machine and authorization logic.
- Create `cli/src/daemon/view/input.rs`: viewer command parsing and mapping into canonical input events.
- Create `cli/src/daemon/input_dispatch.rs`: canonical CDP input dispatcher used by cloud and viewer paths.
- Create `cli/src/daemon/view/page_state.rs`: page state broadcaster and frame sequence state.
- Create `cli/src/daemon/view/privacy.rs`: sensitive mode state and capture suppression policy.
- Create `cli/src/daemon/view/annotations.rs`: annotation store, resolution model, serialization.
- Create `cli/src/daemon/view/recording.rs`: recording store, event writer, artifact layout, bundle validation.
- Modify `cli/src/daemon/view/mod.rs`: construct the expanded `ViewState`, tokenized viewer URL, shared broadcasters, and stores.
- Modify `cli/src/daemon/view/http.rs`: protect viewer routes, add `/input`, `/annotation`, `/recording`, `/state`, and authenticated `/ws`.
- Modify `cli/src/daemon/view/ws.rs`: accept inbound `ViewerCommandV1`, return structured accepts/rejections, broadcast state.
- Modify `cli/src/daemon/view/capture.rs`: enrich frame metadata and route frames through privacy policy.
- Modify `cli/src/daemon/view/target_follow.rs`: emit page state and frame sequence increments.
- Modify `cli/src/daemon/state.rs`: add shared viewer, annotation, recording, and privacy stores.
- Modify `cli/src/daemon/handlers/cloud.rs`: use the canonical dispatcher and privacy guard.
- Modify `cli/src/daemon/handlers/narration_cmds.rs`: return tokenized viewer URLs and route control commands through shared control.
- Modify `cli/src/daemon/handlers/session.rs`: include annotations, recordings, and redaction status in debug bundles.
- Modify `cli/src/main.rs`: add control, annotation, recording, and sensitive commands.
- Modify `cli/assets/viewer.html`: add control mode, annotation mode, recording controls, sensitive indicator, coordinate mapping, input forwarding, and WebSocket command handling.
- Modify `cli/Cargo.toml`: add `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"`, and `uuid = { version = "1", features = ["v4", "serde"] }`.

## Audit-Hardened Execution Requirements

These requirements are authoritative for every task:

- Test-first tasks export the module path before the fail-run command: `common/src/lib.rs` exports `viewer`; `cli/src/daemon/view/mod.rs` exports `auth`, `control`, `input`, `privacy`, `annotations`, `recording`, and `risk`; `cli/src/daemon/mod.rs` exports `input_dispatch`.
- `ViewState` carries `daemon_state: Arc<DaemonState>` and `active_page_rx: tokio::sync::watch::Receiver<Arc<Page>>`. WebSocket and HTTP input handlers use the active page receiver and daemon state for authorization, privacy, recording, and dispatch.
- All page-effect paths use one shared daemon authorization function. Viewer WebSocket, HTTP `/input`, cloud input, CLI navigation/actions, file/download actions, recording/export actions, and annotation export call the same control, privacy, and risk gates.
- `dispatch_user_input` has the contract `dispatch_user_input(page: &Page, state: &DaemonState, input: &UserInputEventV1)`. Navigation dispatch calls `handlers::navigate::handle_navigate(page, &json!({"url": url}), state).await`.
- Viewer token checks include required capabilities. Route and command mappings are: `view`, `state`, `input`, `control`, `annotation`, `recording`, `export`, and `sensitive`.
- Viewer HTML responses include `Referrer-Policy: no-referrer`, `Cache-Control: no-store`, and CSP `default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; frame-ancestors 'none'; base-uri 'none'`. The viewer reads the token into memory and calls `history.replaceState` to remove it from the address bar.
- Viewer auth includes refresh over the authenticated WebSocket, an expiry countdown, read-only expiry behavior, and a CLI path to mint a fresh viewer URL.
- Frame messages use `frameSeq` and `dataBase64` as the serialized fields consumed by the viewer.
- Recording is an overlay state, not a `ControlMode`.
- Browser Use verification is required for viewer UI behavior. The fallback path is a failure report, not a passing final gate.
- Recording implementation includes `gsd-browser recording-validate <id|path> --json` and corruption tests for missing manifest, bad hash, missing start/stop, event sequence gap, unredacted token, missing referenced frame, and malformed JSONL.
- Annotation and recording writes pass through `PrivacyGuard`. Sensitive mode stores annotation geometry with `partialReasons: ["sensitive_redacted"]` and omits crops, full frames, DOM ancestry, cookies, storage, auth headers, request bodies, and raw payloads.
- Risk approval stores an exact pending command hash. Approval dispatches only that pending command. Pointer risk uses target metadata resolved from current refs/accessibility/DOM at the viewport coordinate; URL/navigation/text risk runs without target metadata.
- Final release work bumps Rust crate versions, the npm wrapper version, and `CLOUD_TOOL_RUNTIME_MIN_VERSION` for cloud-visible contract changes.

## Task 1: Shared Protocol Types

**Files:**
- Create: `common/src/viewer.rs`
- Modify: `common/src/lib.rs`
- Modify: `common/src/cloud.rs`
- Test: `common/src/viewer.rs`

- [ ] **Step 0: Export module for test discovery**

Add to `common/src/lib.rs`:

```rust
pub mod viewer;
```

- [ ] **Step 1: Write shared protocol tests**

Add this test module to the bottom of `common/src/viewer.rs` in the same step that creates the file:

```rust
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
        })).expect("valid command");

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
        })).expect("deserialize shape");

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

        let event = cloud.into_user_input_event("cloud").expect("canonical input");
        assert_eq!(event.input_id.as_deref(), Some("inp_cloud"));
        assert_eq!(event.source, UserInputSource::Cloud);
        assert_eq!(event.kind, UserInputKind::Wheel);
        assert_eq!(event.delta_y, Some(120.0));
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test -p gsd-browser-common viewer_command_deserializes_pointer_input -- --nocapture
```

Expected: FAIL with unresolved `ViewerCommandV1`, `UserInputEventV1`, or `into_user_input_event`.

- [ ] **Step 3: Add protocol types**

Create `common/src/viewer.rs` with these public shapes:

```rust
use serde::{Deserialize, Serialize};

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
```

Add the annotation, recording, and page state structs in the same file with only `Serialize`, `Deserialize`, `Debug`, and `Clone` fields from the spec. Keep fields typed as primitives, `Vec<T>`, and `serde_json::Value` where DOM-specific payloads vary.

Implement custom `Deserialize` for `ViewerCommandPayload`. The deserializer reads the full payload as `serde_json::Value`, inspects `value["schema"]`, and deserializes the original value into the matching struct. This preserves the nested `schema` field for `UserInputEventV1` and all command payloads.

- [ ] **Step 4: Add validation helpers**

Add this implementation to `common/src/viewer.rs`:

```rust
impl UserInputEventV1 {
    pub fn validate(&self) -> Result<(), String> {
        match self.kind {
            UserInputKind::Pointer => {
                let phase = self.phase.as_deref().ok_or("pointer input requires phase")?;
                if !matches!(phase, "move" | "down" | "up" | "click" | "double_click" | "context_click") {
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
```

- [ ] **Step 5: Export module and cloud conversion**

Add to `common/src/lib.rs`:

```rust
pub mod viewer;
```

Add to `common/src/cloud.rs`:

```rust
impl CloudUserInput {
    pub fn into_user_input_event(
        self,
        default_source: &str,
    ) -> Result<crate::viewer::UserInputEventV1, String> {
        use crate::viewer::{ControlOwner, CoordinateSpace, UserInputEventV1, UserInputKind, UserInputSource};

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
}
```

- [ ] **Step 6: Run shared protocol tests**

Run:

```bash
cargo test -p gsd-browser-common viewer -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add common/src/lib.rs common/src/viewer.rs common/src/cloud.rs
git commit -m "feat: add viewer protocol types"
```

## Task 2: Viewer Authentication

**Files:**
- Modify: `cli/Cargo.toml`
- Create: `cli/src/daemon/view/auth.rs`
- Modify: `cli/src/daemon/view/mod.rs`
- Modify: `cli/src/daemon/view/http.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Test: `cli/src/daemon/view/auth.rs`

- [ ] **Step 1: Add auth tests**

Create `cli/src/daemon/view/auth.rs` with this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn issuer() -> ViewerTokenIssuer {
        ViewerTokenIssuer::new_for_tests([7; 32])
    }

    #[test]
    fn token_round_trip_binds_session_viewer_origin() {
        let issuer = issuer();
        let token = issuer.issue(ViewerTokenClaims {
            audience: VIEWER_AUDIENCE.to_string(),
            session_id: "sess_1".to_string(),
            viewer_id: "view_1".to_string(),
            origin: "http://127.0.0.1:7777".to_string(),
            issued_at_ms: 1000,
            expires_at_ms: 2000,
            capabilities: vec!["view".to_string(), "input".to_string()],
        }).expect("token");

        let claims = issuer.verify(&token, "sess_1", "view_1", "http://127.0.0.1:7777", 1500, Some("input"))
            .expect("valid claims");
        assert_eq!(claims.viewer_id, "view_1");
        assert!(claims.capabilities.iter().any(|cap| cap == "input"));
    }

    #[test]
    fn token_rejects_wrong_origin() {
        let issuer = issuer();
        let token = issuer.issue(ViewerTokenClaims {
            audience: VIEWER_AUDIENCE.to_string(),
            session_id: "sess_1".to_string(),
            viewer_id: "view_1".to_string(),
            origin: "http://127.0.0.1:7777".to_string(),
            issued_at_ms: 1000,
            expires_at_ms: 2000,
            capabilities: vec!["view".to_string()],
        }).expect("token");

        let err = issuer.verify(&token, "sess_1", "view_1", "http://localhost:7777", 1500, Some("view"))
            .expect_err("origin rejected");
        assert_eq!(err.reason, AuthRejectReason::WrongOrigin);
    }

    #[test]
    fn token_rejects_expired() {
        let issuer = issuer();
        let token = issuer.issue(ViewerTokenClaims {
            audience: VIEWER_AUDIENCE.to_string(),
            session_id: "sess_1".to_string(),
            viewer_id: "view_1".to_string(),
            origin: "http://127.0.0.1:7777".to_string(),
            issued_at_ms: 1000,
            expires_at_ms: 2000,
            capabilities: vec!["view".to_string()],
        }).expect("token");

        let err = issuer.verify(&token, "sess_1", "view_1", "http://127.0.0.1:7777", 2001, Some("view"))
            .expect_err("expired rejected");
        assert_eq!(err.reason, AuthRejectReason::ExpiredToken);
    }

    #[test]
    fn token_rejects_missing_required_capability() {
        let issuer = issuer();
        let token = issuer.issue(ViewerTokenClaims {
            audience: VIEWER_AUDIENCE.to_string(),
            session_id: "sess_1".to_string(),
            viewer_id: "view_1".to_string(),
            origin: "http://127.0.0.1:7777".to_string(),
            issued_at_ms: 1000,
            expires_at_ms: 2000,
            capabilities: vec!["view".to_string()],
        }).expect("token");

        let err = issuer.verify(&token, "sess_1", "view_1", "http://127.0.0.1:7777", 1500, Some("input"))
            .expect_err("capability rejected");
        assert_eq!(err.reason, AuthRejectReason::CapabilityDenied);
    }

    #[test]
    fn loopback_host_check_allows_local_addresses() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("example.com"));
    }

    #[test]
    fn default_ttl_is_short() {
        assert_eq!(ViewerTokenIssuer::default_ttl(), Duration::from_secs(60 * 30));
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser auth::tests -- --nocapture
```

Expected: FAIL with unresolved auth types.

- [ ] **Step 3: Add dependencies**

Add to `cli/Cargo.toml`:

```toml
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
uuid = { version = "1", features = ["v4", "serde"] }
```

- [ ] **Step 4: Implement token issuer**

Use HMAC-SHA256 over canonical JSON claims:

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;
pub const VIEWER_AUDIENCE: &str = "gsd-browser-viewer";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerTokenClaims {
    #[serde(rename = "aud")]
    pub audience: String,
    pub session_id: String,
    pub viewer_id: String,
    pub origin: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRejectReason {
    MissingToken,
    MalformedToken,
    BadSignature,
    WrongSession,
    WrongViewer,
    WrongOrigin,
    ExpiredToken,
    NonLoopbackHost,
    CapabilityDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthReject {
    pub reason: AuthRejectReason,
}

#[derive(Clone)]
pub struct ViewerTokenIssuer {
    secret: [u8; 32],
}

impl ViewerTokenIssuer {
    pub fn new() -> Self {
        let mut secret = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        Self { secret }
    }

    pub fn new_for_tests(secret: [u8; 32]) -> Self {
        Self { secret }
    }

    pub fn default_ttl() -> Duration {
        Duration::from_secs(60 * 30)
    }

    pub fn issue(&self, claims: ViewerTokenClaims) -> Result<String, String> {
        let claims_json = serde_json::to_vec(&claims).map_err(|err| err.to_string())?;
        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|err| err.to_string())?;
        mac.update(&claims_json);
        let signature = mac.finalize().into_bytes();
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(claims_json),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    pub fn verify(
        &self,
        token: &str,
        session_id: &str,
        viewer_id: &str,
        origin: &str,
        now_ms: u64,
        required_capability: Option<&str>,
    ) -> Result<ViewerTokenClaims, AuthReject> {
        let (claims_b64, sig_b64) = token
            .split_once('.')
            .ok_or(AuthReject { reason: AuthRejectReason::MalformedToken })?;
        let claims_json = URL_SAFE_NO_PAD
            .decode(claims_b64)
            .map_err(|_| AuthReject { reason: AuthRejectReason::MalformedToken })?;
        let signature = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| AuthReject { reason: AuthRejectReason::MalformedToken })?;

        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| AuthReject { reason: AuthRejectReason::BadSignature })?;
        mac.update(&claims_json);
        mac.verify_slice(&signature)
            .map_err(|_| AuthReject { reason: AuthRejectReason::BadSignature })?;

        let claims: ViewerTokenClaims = serde_json::from_slice(&claims_json)
            .map_err(|_| AuthReject { reason: AuthRejectReason::MalformedToken })?;
        if claims.audience != VIEWER_AUDIENCE {
            return Err(AuthReject { reason: AuthRejectReason::MalformedToken });
        }
        if claims.session_id != session_id {
            return Err(AuthReject { reason: AuthRejectReason::WrongSession });
        }
        if claims.viewer_id != viewer_id {
            return Err(AuthReject { reason: AuthRejectReason::WrongViewer });
        }
        if claims.origin != origin {
            return Err(AuthReject { reason: AuthRejectReason::WrongOrigin });
        }
        if claims.expires_at_ms < now_ms {
            return Err(AuthReject { reason: AuthRejectReason::ExpiredToken });
        }
        if let Some(required) = required_capability {
            if !claims.capabilities.iter().any(|cap| cap == required) {
                return Err(AuthReject { reason: AuthRejectReason::CapabilityDenied });
            }
        }
        Ok(claims)
    }
}

pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}
```

- [ ] **Step 5: Wire auth into view server state**

Add `auth` to `cli/src/daemon/view/mod.rs`:

```rust
pub mod auth;
```

Add fields to `ViewState` in `cli/src/daemon/view/http.rs`:

```rust
pub token_issuer: crate::daemon::view::auth::ViewerTokenIssuer,
pub session_id: String,
pub viewer_id: String,
pub origin: String,
pub daemon_state: Arc<crate::daemon::state::DaemonState>,
pub active_page_rx: tokio::sync::watch::Receiver<Arc<chromiumoxide::Page>>,
```

In `start_for_session`, create `viewer_id = uuid::Uuid::new_v4().to_string()`, `origin = format!("http://127.0.0.1:{port}")`, issue a token, and return `url = format!("{origin}/?session={session_id}&viewer={viewer_id}&token={token}")`.

- [ ] **Step 6: Protect HTTP and WebSocket routes**

Add helper extraction in `http.rs`:

```rust
fn query_param(uri: &axum::http::Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let (left, right) = pair.split_once('=')?;
        (left == key).then(|| right.to_string())
    })
}
```

Use it in root, `/control`, `/input`, `/annotation`, `/recording`, and `/ws` handlers. For state mutation endpoints, compare `Origin` header with `state.origin`. Return `401` for token failures, `403` for wrong origin, and `403` for `CapabilityDenied`. Root and frame stream require `view`; `/state` requires `state`; `/input` requires `input`; `/control` requires `control`; `/annotation` requires `annotation`; `/recording` requires `recording`; export endpoints require `export`; sensitive commands require `sensitive`.

Set these headers on viewer HTML responses:

```text
Referrer-Policy: no-referrer
Cache-Control: no-store
Content-Security-Policy: default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; frame-ancestors 'none'; base-uri 'none'
```

Add tests for capability denial and security headers.

- [ ] **Step 7: Run auth tests**

```bash
cargo test -p gsd-browser auth::tests -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add cli/Cargo.toml cli/src/daemon/view/auth.rs cli/src/daemon/view/mod.rs cli/src/daemon/view/http.rs cli/src/daemon/view/ws.rs
git commit -m "feat: secure viewer routes"
```

## Task 3: Shared Control State

**Files:**
- Create: `cli/src/daemon/view/control.rs`
- Modify: `cli/src/daemon/view/mod.rs`
- Modify: `cli/src/daemon/view/http.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Modify: `cli/src/daemon/handlers/narration_cmds.rs`
- Test: `cli/src/daemon/view/control.rs`

- [ ] **Step 1: Write state machine tests**

Create `cli/src/daemon/view/control.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gsd_browser_common::viewer::{ControlMode, ControlOwner};

    #[test]
    fn stale_control_version_rejects_ahead_of_owner_check() {
        let mut store = SharedControlStore::new_for_tests(10, 4);
        let req = AuthorizationRequest {
            owner: ControlOwner::User,
            control_version: 9,
            frame_seq: 4,
            effect: PageEffect::Input,
        };
        let err = store.authorize(req).expect_err("stale command");
        assert_eq!(err.reason, ControlRejectReason::StaleControlVersion);
    }

    #[test]
    fn stale_frame_rejects_ahead_of_owner_check() {
        let mut store = SharedControlStore::new_for_tests(10, 4);
        let req = AuthorizationRequest {
            owner: ControlOwner::User,
            control_version: 10,
            frame_seq: 3,
            effect: PageEffect::Input,
        };
        let err = store.authorize(req).expect_err("stale frame");
        assert_eq!(err.reason, ControlRejectReason::StaleFrameSeq);
    }

    #[test]
    fn takeover_preempts_agent_and_increments_version() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        let state = store.takeover("manual test").expect("takeover");
        assert_eq!(state.owner, ControlOwner::User);
        assert_eq!(state.mode, ControlMode::UserTakeover);
        assert_eq!(state.control_version, 2);
    }

    #[test]
    fn step_accepts_one_agent_effect_then_pauses() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        store.pause("inspect").expect("pause");
        store.step("allow one").expect("step");
        let accepted = store.authorize(AuthorizationRequest {
            owner: ControlOwner::Agent,
            control_version: 3,
            frame_seq: 1,
            effect: PageEffect::Input,
        }).expect("one action");
        assert_eq!(accepted.mode, ControlMode::Paused);

        let err = store.authorize(AuthorizationRequest {
            owner: ControlOwner::Agent,
            control_version: 4,
            frame_seq: 1,
            effect: PageEffect::Input,
        }).expect_err("paused blocks agent");
        assert_eq!(err.reason, ControlRejectReason::AgentNotAllowedWhilePaused);
    }

    #[test]
    fn annotating_blocks_page_input() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        store.annotate("select UI").expect("annotating");
        let err = store.authorize(AuthorizationRequest {
            owner: ControlOwner::User,
            control_version: 2,
            frame_seq: 1,
            effect: PageEffect::Input,
        }).expect_err("input blocked");
        assert_eq!(err.reason, ControlRejectReason::AnnotationModeBlocksPageInput);
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser control::tests -- --nocapture
```

Expected: FAIL with unresolved control types.

- [ ] **Step 3: Implement store**

Implement `SharedControlStore` as a synchronous struct protected by `tokio::sync::watch` or `std::sync::Mutex` at call sites:

```rust
use gsd_browser_common::viewer::{ControlMode, ControlOwner, SharedControlStateV1};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEffect {
    Input,
    Observe,
    Export,
    Annotation,
    Recording,
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub owner: ControlOwner,
    pub control_version: u64,
    pub frame_seq: u64,
    pub effect: PageEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlRejectReason {
    StaleControlVersion,
    StaleFrameSeq,
    NonOwnerInput,
    AgentNotAllowedWhilePaused,
    AnnotationModeBlocksPageInput,
    SensitivePrivacyMode,
}

#[derive(Debug, Clone)]
pub struct ControlReject {
    pub reason: ControlRejectReason,
}

pub struct SharedControlStore {
    state: SharedControlStateV1,
}

impl SharedControlStore {
    pub fn new() -> Self {
        Self::new_for_tests(1, 1)
    }

    pub fn new_for_tests(control_version: u64, frame_seq: u64) -> Self {
        Self {
            state: SharedControlStateV1 {
                owner: ControlOwner::Agent,
                mode: ControlMode::AgentRunning,
                control_version,
                frame_seq,
                requested_by: None,
                expires_at_ms: None,
                sensitive: false,
                reason: String::new(),
            },
        }
    }

    pub fn snapshot(&self) -> SharedControlStateV1 {
        self.state.clone()
    }

    fn bump(&mut self) {
        self.state.control_version += 1;
    }

    pub fn authorize(&mut self, req: AuthorizationRequest) -> Result<SharedControlStateV1, ControlReject> {
        if req.control_version != self.state.control_version {
            return Err(ControlReject { reason: ControlRejectReason::StaleControlVersion });
        }
        if req.frame_seq < self.state.frame_seq {
            return Err(ControlReject { reason: ControlRejectReason::StaleFrameSeq });
        }
        if self.state.mode == ControlMode::Annotating && req.effect == PageEffect::Input {
            return Err(ControlReject { reason: ControlRejectReason::AnnotationModeBlocksPageInput });
        }
        if self.state.sensitive && req.owner == ControlOwner::Agent && req.effect == PageEffect::Input {
            return Err(ControlReject { reason: ControlRejectReason::SensitivePrivacyMode });
        }
        if self.state.mode == ControlMode::Paused && req.owner == ControlOwner::Agent && req.effect == PageEffect::Input {
            return Err(ControlReject { reason: ControlRejectReason::AgentNotAllowedWhilePaused });
        }
        if self.state.mode == ControlMode::Step && req.owner == ControlOwner::Agent && req.effect == PageEffect::Input {
            self.state.mode = ControlMode::Paused;
            self.bump();
            return Ok(self.snapshot());
        }
        if req.effect == PageEffect::Input && req.owner != self.state.owner {
            return Err(ControlReject { reason: ControlRejectReason::NonOwnerInput });
        }
        Ok(self.snapshot())
    }

    pub fn takeover(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::UserTakeover;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn release(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::Agent;
        self.state.mode = ControlMode::AgentRunning;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn pause(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::Paused;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn step(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::Step;
        self.state.owner = ControlOwner::Agent;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn annotate(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::Annotating;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }
}
```

- [ ] **Step 4: Wire store into daemon state**

Add to `DaemonState`:

```rust
pub view_control: tokio::sync::Mutex<crate::daemon::view::control::SharedControlStore>,
```

Initialize it with `SharedControlStore::new()`.

- [ ] **Step 5: Route CLI control commands through store**

Modify `handle_pause`, `handle_resume`, and `handle_view_status` in `cli/src/daemon/handlers/narration_cmds.rs` so they return `SharedControlStateV1` JSON. Add daemon methods for `takeover`, `release_control`, `sensitive_on`, and `sensitive_off` in the dispatcher in `cli/src/daemon/mod.rs`.

Add `cli/src/main.rs` `Commands` variants and match arms for:

- `ControlState`
- `Takeover`
- `ReleaseControl`
- `SensitiveOn`
- `SensitiveOff`

Extend `View` with `interactive: bool`. Interactive mode is the default; `--interactive` is accepted as an explicit no-op for script readability.

- [ ] **Step 6: Run state machine tests**

```bash
cargo test -p gsd-browser control::tests -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add cli/src/daemon/view/control.rs cli/src/daemon/view/mod.rs cli/src/daemon/view/http.rs cli/src/daemon/view/ws.rs cli/src/daemon/handlers/narration_cmds.rs cli/src/daemon/state.rs cli/src/daemon/mod.rs
git commit -m "feat: add shared viewer control"
```

## Task 4: Canonical Input Dispatcher

**Files:**
- Create: `cli/src/daemon/input_dispatch.rs`
- Modify: `cli/src/daemon/mod.rs`
- Modify: `cli/src/daemon/handlers/cloud.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Test: `cli/src/daemon/input_dispatch.rs`

- [ ] **Step 1: Extract pure input helpers tests**

Add tests that cover modifier masks, mouse button masks, and key validation in `cli/src/daemon/input_dispatch.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_mask_accepts_browser_names() {
        assert_eq!(modifier_mask(Some(&["shift".into(), "meta".into()])).unwrap(), 12);
        assert_eq!(modifier_mask(Some(&["ctrl".into(), "alt".into()])).unwrap(), 3);
    }

    #[test]
    fn modifier_mask_rejects_unknown() {
        let err = modifier_mask(Some(&["hyper".into()])).expect_err("unknown modifier");
        assert!(err.contains("unsupported modifier"));
    }

    #[test]
    fn mouse_button_mask_matches_cdp_bits() {
        assert_eq!(mouse_buttons_mask("left").unwrap(), 1);
        assert_eq!(mouse_buttons_mask("right").unwrap(), 2);
        assert_eq!(mouse_buttons_mask("middle").unwrap(), 4);
    }
}
```

Implement `mouse_buttons_mask(button: &str) -> Result<i64, String>` as the public pure helper for these tests. CDP-specific conversion from string to `MouseButton` stays inside dispatch helpers.

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser input_dispatch::tests -- --nocapture
```

Expected: FAIL with unresolved module.

- [ ] **Step 3: Move CDP dispatch helpers**

Move these functions from `cli/src/daemon/handlers/cloud.rs` into `cli/src/daemon/input_dispatch.rs` and adapt them to canonical `UserInputEventV1`:

- `modifier_mask`
- `mouse_button`
- `mouse_buttons_mask`
- `dispatch_mouse`
- `key_event_params`
- `dispatch_key`
- `dispatch_text`
- `viewport_center`
- `dispatch_pointer`
- `dispatch_wheel`
- `dispatch_key_input`
- `dispatch_text_input`
- `dispatch_navigation`

Expose one async entrypoint:

```rust
pub async fn dispatch_user_input(
    page: &chromiumoxide::Page,
    state: &crate::daemon::state::DaemonState,
    input: &gsd_browser_common::viewer::UserInputEventV1,
) -> Result<serde_json::Value, String> {
    input.validate()?;
    let modifiers = modifier_mask(input.modifiers.as_deref())?;
    match input.kind {
        gsd_browser_common::viewer::UserInputKind::Pointer => dispatch_pointer(page, input, modifiers).await,
        gsd_browser_common::viewer::UserInputKind::Wheel => dispatch_wheel(page, input, modifiers).await,
        gsd_browser_common::viewer::UserInputKind::Key => dispatch_key_input(page, input, modifiers).await,
        gsd_browser_common::viewer::UserInputKind::Text
        | gsd_browser_common::viewer::UserInputKind::Paste
        | gsd_browser_common::viewer::UserInputKind::Composition => dispatch_text_input(page, input).await,
        gsd_browser_common::viewer::UserInputKind::Navigation => dispatch_navigation(page, state, input).await,
    }
}
```

- [ ] **Step 4: Use dispatcher from cloud handler**

Replace the body of `handle_cloud_user_input` with:

```rust
pub async fn handle_cloud_user_input(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let cloud: CloudUserInput = serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    let input = cloud.into_user_input_event("cloud")?;
    crate::daemon::view::control::authorize_page_effect(
        state,
        crate::daemon::view::control::PageEffectSource::Cloud,
        &input,
    ).await?;
    crate::daemon::input_dispatch::dispatch_user_input(page, state, &input).await
}
```

Add tests for cloud input rejection while paused, during user takeover, during sensitive mode, and while approval is required.

- [ ] **Step 5: Export module**

Add to `cli/src/daemon/mod.rs`:

```rust
pub mod input_dispatch;
```

- [ ] **Step 6: Run cloud input tests**

```bash
cargo test -p gsd-browser input_dispatch::tests -- --nocapture
cargo test -p gsd-browser-common cloud_user_input_converts_to_canonical_event -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add cli/src/daemon/input_dispatch.rs cli/src/daemon/mod.rs cli/src/daemon/handlers/cloud.rs common/src/cloud.rs
git commit -m "feat: share user input dispatch"
```

## Task 5: Interactive Viewer WebSocket Input

**Files:**
- Create: `cli/src/daemon/view/input.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Modify: `cli/src/daemon/view/http.rs`
- Modify: `cli/src/daemon/view/mod.rs`
- Modify: `cli/src/daemon/state.rs`
- Test: `cli/src/daemon/view/input.rs`

- [ ] **Step 1: Write command parsing tests**

Create `cli/src/daemon/view/input.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_viewer_input_command() {
        let cmd = parse_viewer_command(json!({
            "schema": "ViewerCommandV1",
            "commandId": "cmd_1",
            "sessionId": "sess",
            "viewerId": "view",
            "owner": "user",
            "controlVersion": 2,
            "frameSeq": 3,
            "type": "input",
            "payload": {
                "schema": "UserInputEventV1",
                "inputId": "inp",
                "source": "viewer",
                "owner": "user",
                "controlVersion": 2,
                "frameSeq": 3,
                "coordinateSpace": "viewport_css",
                "kind": "pointer",
                "phase": "click",
                "x": 20,
                "y": 30
            }
        })).expect("parsed");
        assert_eq!(cmd.command_id, "cmd_1");
    }

    #[test]
    fn malformed_command_returns_reason() {
        let err = parse_viewer_command(json!({"type": "input"})).expect_err("malformed");
        assert_eq!(err.reason, gsd_browser_common::viewer::ViewerRejectionReason::MalformedCommand);
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser view::input::tests -- --nocapture
```

Expected: FAIL with unresolved parser.

- [ ] **Step 3: Implement parser and response messages**

```rust
use gsd_browser_common::viewer::{ViewerCommandV1, ViewerRejectionReason};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerCommandAccepted {
    #[serde(rename = "type")]
    pub message_type: String,
    pub command_id: String,
    pub control_version: u64,
    pub frame_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerCommandRejected {
    #[serde(rename = "type")]
    pub message_type: String,
    pub command_id: Option<String>,
    pub reason: ViewerRejectionReason,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ViewerInputError {
    pub reason: ViewerRejectionReason,
    pub message: String,
}

pub fn parse_viewer_command(value: Value) -> Result<ViewerCommandV1, ViewerInputError> {
    let command_id = value.get("commandId").and_then(Value::as_str).map(str::to_string);
    let cmd: ViewerCommandV1 = serde_json::from_value(value).map_err(|err| ViewerInputError {
        reason: ViewerRejectionReason::MalformedCommand,
        message: err.to_string(),
    })?;
    if cmd.schema != gsd_browser_common::viewer::VIEWER_COMMAND_SCHEMA {
        return Err(ViewerInputError {
            reason: ViewerRejectionReason::MalformedCommand,
            message: "unsupported viewer command schema".to_string(),
        });
    }
    if cmd.command_id.is_empty() {
        return Err(ViewerInputError {
            reason: ViewerRejectionReason::MalformedCommand,
            message: "commandId is required".to_string(),
        });
    }
    match &cmd.payload {
        gsd_browser_common::viewer::ViewerCommandPayload::Input(input) => input.validate().map_err(|err| ViewerInputError {
            reason: ViewerRejectionReason::MalformedCommand,
            message: err,
        })?,
        _ => {}
    }
    let _ = command_id;
    Ok(cmd)
}
```

- [ ] **Step 4: Handle inbound WebSocket messages**

In `cli/src/daemon/view/ws.rs`, replace `Some(Ok(_)) => {}` with logic that:

1. Parses text into JSON.
2. Calls `parse_viewer_command`.
3. Validates command token/session/viewer through the authenticated WebSocket context.
4. Authorizes input through `SharedControlStore`.
5. Calls shared page-effect authorization, privacy, risk, and recording hooks.
6. Dispatches `ViewerCommandPayload::Input` through `input_dispatch::dispatch_user_input`.
7. Sends `ViewerCommandAccepted` or `ViewerCommandRejected`.

Keep binary messages rejected with `MalformedCommand`.

- [ ] **Step 5: Add HTTP `/input` endpoint**

Add route:

```rust
.route("/input", post(post_input))
```

`post_input` uses the same parser and handler as WebSocket, then returns JSON accepted/rejected response. This endpoint exists for deterministic tests and simple clients. It uses `ViewState.active_page_rx.borrow().clone()` for the active page and `ViewState.daemon_state` for authorization and dispatch.

- [ ] **Step 6: Run parser tests and compile**

```bash
cargo test -p gsd-browser view::input::tests -- --nocapture
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add cli/src/daemon/view/input.rs cli/src/daemon/view/ws.rs cli/src/daemon/view/http.rs cli/src/daemon/view/mod.rs cli/src/daemon/state.rs
git commit -m "feat: accept viewer input commands"
```

## Task 6: Frame Metadata, Page State, and Privacy Guard

**Files:**
- Create: `cli/src/daemon/view/page_state.rs`
- Create: `cli/src/daemon/view/privacy.rs`
- Modify: `cli/src/daemon/view/capture.rs`
- Modify: `cli/src/daemon/view/target_follow.rs`
- Modify: `cli/src/daemon/handlers/cloud.rs`
- Modify: `cli/src/daemon/handlers/session.rs`
- Test: `cli/src/daemon/view/privacy.rs`

- [ ] **Step 1: Write privacy tests**

Create `cli/src/daemon/view/privacy.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_policy_blocks_agent_capture() {
        let policy = PrivacyPolicy::sensitive(3);
        assert_eq!(policy.capture_decision(CaptureConsumer::CloudFrame), CaptureDecision::RedactedFrameCard);
        assert_eq!(policy.capture_decision(CaptureConsumer::DebugBundle), CaptureDecision::OmitPayload);
        assert_eq!(policy.input_decision(InputActor::Agent), InputDecision::Blocked);
        assert_eq!(policy.input_decision(InputActor::User), InputDecision::Allowed);
    }

    #[test]
    fn normal_policy_allows_local_capture() {
        let policy = PrivacyPolicy::normal(1);
        assert_eq!(policy.capture_decision(CaptureConsumer::LocalViewer), CaptureDecision::Allowed);
        assert_eq!(policy.input_decision(InputActor::Agent), InputDecision::Allowed);
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser privacy::tests -- --nocapture
```

Expected: FAIL with unresolved privacy types.

- [ ] **Step 3: Implement privacy policy**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureConsumer {
    LocalViewer,
    CloudFrame,
    Screenshot,
    Refs,
    Dom,
    Accessibility,
    Logs,
    Recording,
    DebugBundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureDecision {
    Allowed,
    RedactedFrameCard,
    OmitPayload,
    PartitionLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputActor {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDecision {
    Allowed,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct PrivacyPolicy {
    pub sensitive: bool,
    pub epoch: u64,
}

impl PrivacyPolicy {
    pub fn normal(epoch: u64) -> Self {
        Self { sensitive: false, epoch }
    }

    pub fn sensitive(epoch: u64) -> Self {
        Self { sensitive: true, epoch }
    }

    pub fn capture_decision(&self, consumer: CaptureConsumer) -> CaptureDecision {
        if !self.sensitive {
            return CaptureDecision::Allowed;
        }
        match consumer {
            CaptureConsumer::LocalViewer => CaptureDecision::Allowed,
            CaptureConsumer::CloudFrame => CaptureDecision::RedactedFrameCard,
            CaptureConsumer::Logs => CaptureDecision::PartitionLogs,
            CaptureConsumer::Screenshot
            | CaptureConsumer::Refs
            | CaptureConsumer::Dom
            | CaptureConsumer::Accessibility
            | CaptureConsumer::Recording
            | CaptureConsumer::DebugBundle => CaptureDecision::OmitPayload,
        }
    }

    pub fn input_decision(&self, actor: InputActor) -> InputDecision {
        if self.sensitive && actor == InputActor::Agent {
            InputDecision::Blocked
        } else {
            InputDecision::Allowed
        }
    }
}
```

- [ ] **Step 4: Add frame metadata**

Expand `FrameMessage` in `cli/src/daemon/view/capture.rs`:

```rust
pub struct FrameMessage {
    #[serde(rename = "type")]
    ty: &'static str,
    #[serde(rename = "frameSeq")]
    pub frame_seq: u64,
    pub content_type: &'static str,
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
    pub viewport: ViewportInfo,
    pub capture_pixel_width: u32,
    pub capture_pixel_height: u32,
    pub device_pixel_ratio: f64,
    pub capture_scale_x: f64,
    pub capture_scale_y: f64,
    pub url: String,
    pub title: String,
    pub timestamp: u64,
}
```

Use viewport CSS size from page evaluation, capture pixel size from screenshot/screencast metadata, and monotonically increasing `frameSeq` from shared page state.

- [ ] **Step 5: Add page state broadcaster**

Create `PageStateStore` in `page_state.rs` with:

```rust
pub struct PageStateStore {
    frame_seq: std::sync::atomic::AtomicU64,
    sender: tokio::sync::watch::Sender<gsd_browser_common::viewer::PageStateV1>,
}
```

Expose:

- `snapshot()`
- `increment_frame_seq()`
- `update_url_title_origin(...)`
- `subscribe()`

- [ ] **Step 6: Apply privacy guard to cloud frame and debug bundle**

In `handle_cloud_frame`, return a redacted frame card JSON when privacy policy says `RedactedFrameCard`:

```json
{
  "sequence": 0,
  "contentType": "application/vnd.gsd.redacted-frame+json",
  "dataBase64": "",
  "redacted": true,
  "reason": "sensitive_privacy_mode"
}
```

In `debug_bundle`, add `redactionStatus` and omit screenshots, DOM, a11y, and logs according to `PrivacyPolicy`.

- [ ] **Step 7: Run tests and compile**

```bash
cargo test -p gsd-browser privacy::tests -- --nocapture
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add cli/src/daemon/view/page_state.rs cli/src/daemon/view/privacy.rs cli/src/daemon/view/capture.rs cli/src/daemon/view/target_follow.rs cli/src/daemon/handlers/cloud.rs cli/src/daemon/handlers/session.rs
git commit -m "feat: add page state and privacy guard"
```

## Task 7: Viewer UI Input and Mode Controls

**Files:**
- Modify: `cli/assets/viewer.html`
- Create: `cli/assets/viewer-coordinate.test.mjs`
- Modify: `cli/src/daemon/view/viewer_html.rs`
- Test: Browser Use pass, coordinate harness, plus embedded asset compile through `cargo build --workspace`

- [ ] **Step 1: Add pure coordinate mapping function**

In `cli/assets/viewer.html`, add this JavaScript near the top of the script block:

```javascript
function mapViewerPoint(event, frameMeta, wrapEl) {
  const rect = wrapEl.getBoundingClientRect();
  const viewportWidth = Number(frameMeta.viewportCssWidth || frameMeta.viewport?.width || 0);
  const viewportHeight = Number(frameMeta.viewportCssHeight || frameMeta.viewport?.height || 0);
  const scale = Math.min(rect.width / viewportWidth, rect.height / viewportHeight);
  const renderedWidth = viewportWidth * scale;
  const renderedHeight = viewportHeight * scale;
  const letterboxX = (rect.width - renderedWidth) / 2;
  const letterboxY = (rect.height - renderedHeight) / 2;
  return {
    x: (event.clientX - rect.left - letterboxX) / scale,
    y: (event.clientY - rect.top - letterboxY) / scale,
    scale,
    letterboxX,
    letterboxY
  };
}
```

- [ ] **Step 1a: Add coordinate harness**

Create `cli/assets/viewer-coordinate.test.mjs` with table-driven cases for DPR 1, DPR 2, wide letterbox, tall letterbox, resized wrapper, scrolled page, and capture-size mismatch. The test imports or duplicates only the pure `mapViewerPoint` math and asserts viewport CSS coordinates, not screenshot pixels.

Run:

```bash
node cli/assets/viewer-coordinate.test.mjs
```

Expected: PASS with all coordinate scenarios.

- [ ] **Step 2: Add control mode state**

Add viewer-local state:

```javascript
const viewerState = {
  sessionId: new URLSearchParams(location.search).get("session") || "",
  viewerId: new URLSearchParams(location.search).get("viewer") || "",
  token: new URLSearchParams(location.search).get("token") || "",
  owner: "agent",
  controlVersion: 0,
  frameSeq: 0,
  mode: "observe",
  recording: false,
  sensitive: false,
  latestFrame: null,
  commandSeq: 1
};
history.replaceState(null, "", location.pathname);
```

Initialize `owner`, `mode`, `controlVersion`, and `frameSeq` from the server `SharedControlState`. In `agent-running`, render observe-only UI and disable input forwarding. The Control button requests takeover; input forwarding starts only after takeover succeeds. Rejections render inline in the status area.

- [ ] **Step 3: Add command sender**

```javascript
function nextCommandId() {
  return `cmd_${Date.now()}_${viewerState.commandSeq++}`;
}

function sendViewerCommand(type, payload) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({
    schema: "ViewerCommandV1",
    commandId: nextCommandId(),
    sessionId: viewerState.sessionId,
    viewerId: viewerState.viewerId,
    owner: viewerState.owner,
    controlVersion: viewerState.controlVersion,
    frameSeq: viewerState.frameSeq,
    type,
    payload
  }));
}
```

- [ ] **Step 4: Forward pointer, wheel, key, and text input in Control mode**

Attach listeners to `#wrap`:

```javascript
wrap.addEventListener("click", (event) => {
  if (viewerState.mode !== "control" || !viewerState.latestFrame) return;
  const point = mapViewerPoint(event, viewerState.latestFrame, wrap);
  sendViewerCommand("input", {
    schema: "UserInputEventV1",
    inputId: `inp_${Date.now()}`,
    source: "viewer",
    owner: "user",
    controlVersion: viewerState.controlVersion,
    frameSeq: viewerState.frameSeq,
    coordinateSpace: "viewport_css",
    kind: "pointer",
    phase: "click",
    x: point.x,
    y: point.y,
    button: "left"
  });
});
```

Add `wheel`, `keydown`, and a focused hidden text input for printable text. Clipboard paste reads `event.clipboardData.getData("text/plain")` from the explicit paste event only.

- [ ] **Step 5: Add mode controls**

Add toolbar buttons for:

- Control
- Annotate
- Record
- Sensitive
- Pause
- Step
- Resume

Each button sends a `ControlCommandV1`, `AnnotationCommandV1`, `RecordingCommandV1`, or `SensitiveCommandV1` payload.

- [ ] **Step 6: Update inbound WebSocket handling**

When messages include `controlVersion`, `frameSeq`, `control`, `pageState`, or `recording`, update `viewerState` and the HUD. Accepted/rejected command messages render in the status area.

- [ ] **Step 7: Build**

```bash
node cli/assets/viewer-coordinate.test.mjs
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 8: Browser verification**

Run:

```bash
cargo run -p gsd-browser -- daemon health
cargo run -p gsd-browser -- navigate https://example.com
cargo run -p gsd-browser -- view --print-only
```

Use Browser Use to open the printed URL against a local fixture page with click, type, wheel, drag, URL, console, and state counters. Verify click, type, wheel, drag, focus/text handling, WebSocket command acceptance, desktop layout, and mobile layout through the viewer UI. Browser Use is a required gate for this task.

- [ ] **Step 9: Commit**

```bash
git add cli/assets/viewer.html cli/assets/viewer-coordinate.test.mjs cli/src/daemon/view/viewer_html.rs
git commit -m "feat: add interactive viewer controls"
```

## Task 8: Annotation Mode

**Files:**
- Create: `cli/src/daemon/view/annotations.rs`
- Modify: `cli/src/daemon/state.rs`
- Modify: `cli/src/daemon/view/http.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Modify: `cli/src/main.rs`
- Modify: `cli/assets/viewer.html`
- Test: `cli/src/daemon/view/annotations.rs`

- [ ] **Step 1: Write annotation store tests**

Create `cli/src/daemon/view/annotations.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gsd_browser_common::viewer::{AnnotationStatus, AnnotationV1};

    #[test]
    fn store_creates_and_lists_annotation() {
        let mut store = AnnotationStore::new();
        let annotation = minimal_annotation_for_tests("ann_1", "Make this primary");
        let saved = store.create(annotation).expect("saved");
        assert_eq!(saved.annotation_id, "ann_1");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn resolve_missing_id_returns_error() {
        let mut store = AnnotationStore::new();
        let err = store.set_status("missing", AnnotationStatus::Resolved).expect_err("missing");
        assert!(err.contains("annotation not found"));
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser annotations::tests -- --nocapture
```

Expected: FAIL with unresolved store or test constructor.

- [ ] **Step 3: Add annotation types in common**

Add production `AnnotationStatus` to `common/src/viewer.rs` and keep `AnnotationV1` as a production protocol type:

```rust
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

```

Define `minimal_annotation_for_tests` inside `cli/src/daemon/view/annotations.rs` test module. Do not rely on `#[cfg(test)]` helpers from `gsd-browser-common`; downstream crates compile dependencies without that cfg.

- [ ] **Step 4: Implement store**

```rust
use gsd_browser_common::viewer::{AnnotationStatus, AnnotationV1};

pub struct AnnotationStore {
    entries: Vec<AnnotationV1>,
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn create(&mut self, annotation: AnnotationV1) -> Result<AnnotationV1, String> {
        let annotation = crate::daemon::view::privacy::redact_annotation(annotation);
        if self.entries.iter().any(|entry| entry.annotation_id == annotation.annotation_id) {
            return Err(format!("annotation already exists: {}", annotation.annotation_id));
        }
        self.entries.push(annotation.clone());
        Ok(annotation)
    }

    pub fn list(&self) -> Vec<AnnotationV1> {
        self.entries.clone()
    }

    pub fn get(&self, id: &str) -> Option<AnnotationV1> {
        self.entries.iter().find(|entry| entry.annotation_id == id).cloned()
    }

    pub fn clear(&mut self, id: &str) -> bool {
        let len = self.entries.len();
        self.entries.retain(|entry| entry.annotation_id != id);
        self.entries.len() != len
    }

    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    pub fn set_status(&mut self, id: &str, status: AnnotationStatus) -> Result<AnnotationV1, String> {
        let entry = self.entries.iter_mut().find(|entry| entry.annotation_id == id)
            .ok_or_else(|| format!("annotation not found: {id}"))?;
        entry.status = status;
        Ok(entry.clone())
    }
}
```

- [ ] **Step 5: Add daemon endpoints and CLI commands**

Add daemon methods:

- `annotations`
- `annotation_get`
- `annotation_clear`
- `annotation_resolve`
- `annotation_export`
- `annotation_request`

Add `Commands` variants in `cli/src/main.rs` and `cmd_annotation_*` functions that send JSON-RPC to the daemon.

- [ ] **Step 6: Add viewer annotation overlay**

In `cli/assets/viewer.html`, add an annotation overlay div over the frame. In annotate mode:

- click creates point annotation
- drag creates region annotation
- note text is required for save
- pointer events do not call `sendViewerCommand("input", ...)`
- `Escape` cancels draft annotation
- sensitive mode saves only geometry, adds `partialReasons: ["sensitive_redacted"]`, and omits crop/full-frame artifacts

- [ ] **Step 7: Run tests and build**

```bash
cargo test -p gsd-browser annotations::tests -- --nocapture
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 8: Verify zero forwarded actions**

Open the viewer with Browser Use, enter annotate mode, click and drag over a fixture button with a DOM click counter, then run:

```bash
cargo run -p gsd-browser -- timeline
```

Expected: annotation events exist, the fixture click counter stays `0`, and no `input.pointer` event is recorded for annotate gestures.

- [ ] **Step 9: Commit**

```bash
git add common/src/viewer.rs cli/src/daemon/view/annotations.rs cli/src/daemon/state.rs cli/src/daemon/view/http.rs cli/src/daemon/view/ws.rs cli/src/main.rs cli/assets/viewer.html
git commit -m "feat: add viewer annotations"
```

## Task 9: Flow Recording Artifacts

**Files:**
- Create: `cli/src/daemon/view/recording.rs`
- Modify: `cli/src/daemon/state.rs`
- Modify: `cli/src/daemon/view/http.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Modify: `cli/src/daemon/handlers/session.rs`
- Modify: `cli/src/main.rs`
- Modify: `cli/assets/viewer.html`
- Test: `cli/src/daemon/view/recording.rs`

- [ ] **Step 1: Write recording bundle tests**

Create `cli/src/daemon/view/recording.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn recording_writes_manifest_and_events() {
        let dir = tempdir().expect("tempdir");
        let mut store = RecordingStore::new(dir.path().to_path_buf());
        let rec = store.start("checkout-bug").expect("started");
        store.record_event(RecordingEventInput {
            source: "viewer".to_string(),
            owner: "user".to_string(),
            kind: "recording.start".to_string(),
            url: "http://127.0.0.1".to_string(),
            title: "Fixture".to_string(),
            redacted: false,
        }).expect("event");
        let manifest = store.stop(&rec.recording_id).expect("stopped");
        assert_eq!(manifest.event_count, 1);
        assert!(dir.path().join(&rec.recording_id).join("manifest.json").exists());
        assert!(dir.path().join(&rec.recording_id).join("events.jsonl").exists());
    }

    #[test]
    fn redaction_scrubs_tokens_from_event_text() {
        let scrubbed = redact_text("email lex@example.com bearer abc.def token=secret data-token=\"abc\"");
        assert!(!scrubbed.contains("lex@example.com"));
        assert!(!scrubbed.contains("secret"));
        assert!(scrubbed.contains("[redacted:email]"));
        assert!(scrubbed.contains("[redacted:token]"));
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser recording::tests -- --nocapture
```

Expected: FAIL with unresolved recording types.

- [ ] **Step 3: Add dev dependency**

Add to `cli/Cargo.toml` dev-dependencies:

```toml
tempfile = "3"
```

- [ ] **Step 4: Implement recording store**

Implement:

- `RecordingStore::new(root: PathBuf)`
- `start(name: &str) -> RecordingSession`
- `pause(recording_id)`
- `resume(recording_id)`
- `record_event(input)`
- `stop(recording_id) -> BrowserArtifactManifestV1`
- `list()`
- `get(recording_id)`
- `export(recording_id, output)`
- `discard(recording_id)`

Directory layout:

```text
recordings/<recordingId>/
  manifest.json
  events.jsonl
  frames/
  snapshots/
  annotations/
  logs/
    console.jsonl
    network.jsonl
    dialog.jsonl
  deltas.json
```

- [ ] **Step 5: Add redaction**

Add `redact_text` rules for:

- email addresses
- bearer tokens
- query parameters containing `token`, `secret`, `key`, `code`, `otp`
- `data-token`
- high entropy strings of 32 or more URL-safe characters

Use `regex-lite` already present in `cli/Cargo.toml`.

- [ ] **Step 6: Wire event capture**

Record events for:

- `recording.start`
- `recording.stop`
- viewer accepted command
- viewer rejected command
- cloud input
- navigation
- console error
- failed request
- dialog
- annotation create/update/resolve
- sensitive interval start/end

- [ ] **Step 7: Add CLI and viewer controls**

Add CLI commands:

- `record-start --name <name>`
- `record-stop`
- `record-pause`
- `record-resume`
- `recordings`
- `recording-get <id>`
- `recording-export <id> --output <path>`
- `recording-discard <id>`
- `recording-validate <id|path> --json`

Add viewer HUD controls for Record, Pause, Resume, Stop, Discard, and Export.

- [ ] **Step 8: Add recording privacy matrix and validator**

Default recording capture policy:

- cookies: excluded
- localStorage/sessionStorage: excluded
- request bodies: excluded
- response bodies: excluded
- authorization headers: excluded
- set-cookie headers: excluded
- raw request/response payloads: excluded
- full URLs: query params redacted before write
- DOM snapshots: redacted through `PrivacyGuard`
- screenshots and crops: omitted during sensitive mode
- annotation exports: redacted through `PrivacyGuard`

Implement `recording-validate <id|path> --json`. The validator checks schema, hashes, manifest presence, start/stop events, monotonic event sequence, referenced files, JSONL validity, redaction metadata, and unredacted token patterns.

- [ ] **Step 9: Run tests and build**

```bash
cargo test -p gsd-browser recording::tests -- --nocapture
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 10: Verify artifact analyzer behavior**

Run:

```bash
cargo run -p gsd-browser -- record-start --name checkout-bug
cargo run -p gsd-browser -- navigate https://example.com
cargo run -p gsd-browser -- record-stop
cargo run -p gsd-browser -- recordings --json
cargo run -p gsd-browser -- recording-validate <recording-id> --json
```

Expected: a recording id exists, manifest contains start/stop sequence, `events.jsonl` exists, redaction count exists, and `recording-validate` passes.

- [ ] **Step 11: Verify automatic capture**

Run a local fixture that triggers navigation, console error, failed request, dialog, annotation, and sensitive interval while recording. Assert `events.jsonl`, `frames/`, `snapshots/`, logs, manifest counts, redaction counts, and boundary hashes all reflect those actions.

- [ ] **Step 12: Commit**

```bash
git add cli/Cargo.toml cli/src/daemon/view/recording.rs cli/src/daemon/state.rs cli/src/daemon/view/http.rs cli/src/daemon/view/ws.rs cli/src/daemon/handlers/session.rs cli/src/main.rs cli/assets/viewer.html
git commit -m "feat: add flow recording bundles"
```

## Task 10: Risk Gate and Approval UI

**Files:**
- Create: `cli/src/daemon/view/risk.rs`
- Modify: `cli/src/daemon/view/control.rs`
- Modify: `cli/src/daemon/view/ws.rs`
- Modify: `cli/assets/viewer.html`
- Test: `cli/src/daemon/view/risk.rs`

- [ ] **Step 1: Write risk gate tests**

Create `cli/src/daemon/view/risk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_button_requires_approval() {
        let risk = evaluate_risk(RiskInput {
            origin: "https://app.example.com".to_string(),
            role: Some("button".to_string()),
            name: Some("Delete project".to_string()),
            text: Some("Delete project".to_string()),
            input_kind: "pointer".to_string(),
            url: "https://app.example.com/projects/acme".to_string(),
        });
        assert_eq!(risk.category, Some(RiskCategory::DeleteDestructive));
        assert!(risk.requires_approval);
    }

    #[test]
    fn plain_local_link_does_not_require_approval() {
        let risk = evaluate_risk(RiskInput {
            origin: "http://localhost:3000".to_string(),
            role: Some("link".to_string()),
            name: Some("Settings".to_string()),
            text: Some("Settings".to_string()),
            input_kind: "pointer".to_string(),
            url: "http://localhost:3000/home".to_string(),
        });
        assert_eq!(risk.category, None);
        assert!(!risk.requires_approval);
    }
}
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p gsd-browser risk::tests -- --nocapture
```

Expected: FAIL with unresolved risk types.

- [ ] **Step 3: Implement risk evaluator**

Implement categories:

- purchase/payment
- delete/destructive
- send/invite/share
- OAuth grant
- credential or token entry
- file upload/download
- production/admin origin
- cross-origin navigation
- sensitive form fields

Use string matching on role/name/text/url/origin with lowercased inputs. Return `ApprovalRequestV1` when `requires_approval` is true.

- [ ] **Step 4: Resolve target metadata**

Before risk evaluation, resolve pointer target metadata from current refs, accessibility, or DOM at the viewport coordinate. Populate `RiskInput` with role, name, text, form context, origin, URL, and input kind. URL/navigation/text risk runs even when target metadata is unavailable.

- [ ] **Step 5: Wire approval state**

Extend `SharedControlStore` with:

- `request_approval`
- `approve`
- `deny`
- `expire_approval`

Move risk evaluation into the shared page-effect authorization function used by viewer WebSocket, HTTP `/input`, cloud input, CLI page actions, navigation, file/download commands, and recording/export actions. If approval is required, store the exact pending command hash, send an approval request to the viewer, and do not dispatch input. Approval dispatches only the command whose hash matches the pending approval.

- [ ] **Step 6: Add approval UI**

In `cli/assets/viewer.html`, render approval banner with action summary, origin, element label, Approve, Deny, and countdown. Approval and denial send `ControlCommandV1`.

- [ ] **Step 7: Run tests and build**

```bash
cargo test -p gsd-browser risk::tests -- --nocapture
cargo test -p gsd-browser control::tests -- --nocapture
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add cli/src/daemon/view/risk.rs cli/src/daemon/view/control.rs cli/src/daemon/view/ws.rs cli/assets/viewer.html
git commit -m "feat: add viewer risk approvals"
```

## Task 11: Final Verification and Documentation

**Files:**
- Modify: `README.md`
- Modify: `SKILL.md`
- Modify: `docs/superpowers/specs/2026-05-02-interactive-browser-workbench-design.md`

- [ ] **Step 1: Run unit and build gates**

```bash
node cli/assets/viewer-coordinate.test.mjs
cargo test --workspace
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 2: Run daemon smoke**

```bash
cargo run -p gsd-browser -- daemon health
cargo run -p gsd-browser -- navigate https://example.com
cargo run -p gsd-browser -- snapshot
cargo run -p gsd-browser -- view --print-only
```

Expected: daemon healthy, navigation succeeds, snapshot returns refs, printed viewer URL contains `session`, `viewer`, and `token`.

- [ ] **Step 3: Run interactive local verification**

Use Browser Use to open the printed URL against the local fixture and verify:

- viewer displays the real Chrome page stream
- clicking in Control mode changes page state
- typing in Control mode updates an input
- wheel scrolling moves the page
- Annotate mode creates point and region annotations
- Annotate mode sends zero page input commands
- Record start/stop writes an artifact bundle
- Sensitive mode blocks cloud frame capture and keeps local viewer usable
- approval banner appears for destructive labels
- agent-owned observe-only, user-takeover, paused, and stale-control states render and reject correctly
- desktop and mobile viewport layouts keep controls usable

- [ ] **Step 4: Verify artifact bundle**

```bash
cargo run -p gsd-browser -- record-start --name final-smoke
cargo run -p gsd-browser -- navigate https://example.com
cargo run -p gsd-browser -- record-stop
cargo run -p gsd-browser -- recordings --json
cargo run -p gsd-browser -- recording-validate <recording-id> --json
```

Expected: exported manifest uses `BrowserArtifactBundleV1`, start/stop events exist inside `events.jsonl`, sensitive intervals contain redaction status.

- [ ] **Step 5: Update docs**

Update `README.md` and `SKILL.md` with:

- `view` opens an authenticated local workbench URL
- `view --print-only` prints the tokenized URL
- control commands: pause, resume, step, takeover, release-control, sensitive-on, sensitive-off
- annotation commands
- recording commands
- privacy behavior and local-only artifact storage

- [ ] **Step 6: Update release/version surface**

Update:

- `cli/Cargo.toml` package version
- `common/Cargo.toml` package version
- `npm/package.json` package version
- `common/src/cloud.rs` `CLOUD_TOOL_RUNTIME_MIN_VERSION`

State release sequencing for GitHub release assets and the npm wrapper in the PR body.

- [ ] **Step 7: Commit**

```bash
git add README.md SKILL.md cli/Cargo.toml common/Cargo.toml npm/package.json common/src/cloud.rs docs/superpowers/specs/2026-05-02-interactive-browser-workbench-design.md
git commit -m "docs: document interactive browser workbench"
```

## Spec Coverage Review

- Secure authenticated viewer routes: Tasks 2 and 5.
- Bidirectional viewer input: Tasks 4, 5, and 7.
- Shared human/agent control: Task 3.
- Deterministic coordinate mapping: Task 7.
- Page state stream and frame sequence: Task 6.
- Annotation mode and annotation storage: Task 8.
- Flow recording and proof bundle: Task 9.
- Sensitive capture suppression: Task 6 and Task 9.
- Risk approvals: Task 10.
- CLI surface and docs: Tasks 8, 9, 10, and 11.

## Execution Notes

- Commit after each task.
- Keep protocol changes in `common` compatible with cloud input names.
- Keep viewer HTML target-origin-free: no iframe, no proxy, no target script execution in the viewer.
- Use Browser Use for final viewer UI verification.
- Use `cargo test --workspace` and `cargo build --workspace` at the final gate.
