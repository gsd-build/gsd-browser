use gsd_browser_common::viewer::{
    ControlCommandV1, SensitiveCommandV1, ViewerCommandPayload, ViewerCommandV1,
    ViewerRejectionReason,
};
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
        ViewerCommandPayload::Input(input) => input.validate().map_err(|err| ViewerInputError {
            reason: ViewerRejectionReason::MalformedCommand,
            message: err,
        })?,
        _ => {}
    }
    Ok(cmd)
}

pub fn rejected(
    command_id: Option<String>,
    reason: ViewerRejectionReason,
    message: impl Into<String>,
) -> ViewerCommandRejected {
    ViewerCommandRejected {
        message_type: "commandRejected".to_string(),
        command_id,
        reason,
        message: message.into(),
    }
}

fn accepted(command_id: String, control_version: u64, frame_seq: u64) -> ViewerCommandAccepted {
    ViewerCommandAccepted {
        message_type: "commandAccepted".to_string(),
        command_id,
        control_version,
        frame_seq,
    }
}

fn map_control_rejection(message: String) -> ViewerRejectionReason {
    if message.contains("StaleControlVersion") {
        ViewerRejectionReason::StaleControlVersion
    } else if message.contains("StaleFrameSeq") {
        ViewerRejectionReason::StaleFrameSeq
    } else if message.contains("NonOwnerInput") {
        ViewerRejectionReason::NonOwnerInput
    } else if message.contains("AgentNotAllowedWhilePaused") {
        ViewerRejectionReason::AgentNotAllowedWhilePaused
    } else if message.contains("AnnotationModeBlocksPageInput") {
        ViewerRejectionReason::AnnotationModeBlocksPageInput
    } else if message.contains("SensitivePrivacyMode") {
        ViewerRejectionReason::SensitivePrivacyMode
    } else {
        ViewerRejectionReason::MalformedCommand
    }
}

async fn apply_control_command(
    store: &mut crate::daemon::view::control::SharedControlStore,
    command: &ControlCommandV1,
) -> Result<gsd_browser_common::viewer::SharedControlStateV1, String> {
    let reason = command
        .reason
        .clone()
        .unwrap_or_else(|| command.action.clone());
    match command.action.as_str() {
        "takeover" => store.takeover(reason),
        "release" | "release_control" => store.release(reason),
        "pause" => store.pause(reason),
        "step" => store.step(reason),
        "annotate" | "annotating" => store.annotate(reason),
        "abort" => store.abort(reason),
        other => Err(format!("unsupported control action: {other}")),
    }
}

async fn apply_sensitive_command(
    store: &mut crate::daemon::view::control::SharedControlStore,
    command: &SensitiveCommandV1,
) -> Result<gsd_browser_common::viewer::SharedControlStateV1, String> {
    let reason = command
        .reason
        .clone()
        .unwrap_or_else(|| "sensitive command".to_string());
    if command.enabled {
        store.sensitive_on(reason)
    } else {
        store.sensitive_off(reason)
    }
}

pub async fn handle_viewer_command(
    cmd: ViewerCommandV1,
    state: &crate::daemon::view::http::ViewState,
) -> Result<ViewerCommandAccepted, ViewerCommandRejected> {
    if cmd.session_id != state.session_id || cmd.viewer_id != state.viewer_id {
        return Err(rejected(
            Some(cmd.command_id),
            ViewerRejectionReason::ViewerNotAuthenticated,
            "viewer command identity does not match this viewer",
        ));
    }

    match cmd.payload {
        ViewerCommandPayload::Input(input) => {
            let page = state.active_page_rx.borrow().clone();
            let control = crate::daemon::view::control::authorize_page_effect(
                &state.daemon_state,
                crate::daemon::view::control::PageEffectSource::Viewer,
                &input,
            )
            .await
            .map_err(|message| {
                rejected(
                    Some(cmd.command_id.clone()),
                    map_control_rejection(message.clone()),
                    message,
                )
            })?;
            crate::daemon::input_dispatch::dispatch_user_input(&page, &state.daemon_state, &input)
                .await
                .map_err(|message| {
                    rejected(
                        Some(cmd.command_id.clone()),
                        ViewerRejectionReason::MalformedCommand,
                        message,
                    )
                })?;
            Ok(accepted(
                cmd.command_id,
                control.control_version,
                control.frame_seq,
            ))
        }
        ViewerCommandPayload::Control(command) => {
            let mut store = state.daemon_state.view_control.lock().await;
            let control = apply_control_command(&mut store, &command)
                .await
                .map_err(|message| {
                    rejected(
                        Some(cmd.command_id.clone()),
                        ViewerRejectionReason::MalformedCommand,
                        message,
                    )
                })?;
            Ok(accepted(
                cmd.command_id,
                control.control_version,
                control.frame_seq,
            ))
        }
        ViewerCommandPayload::Sensitive(command) => {
            let mut store = state.daemon_state.view_control.lock().await;
            let control = apply_sensitive_command(&mut store, &command)
                .await
                .map_err(|message| {
                    rejected(
                        Some(cmd.command_id.clone()),
                        ViewerRejectionReason::MalformedCommand,
                        message,
                    )
                })?;
            Ok(accepted(
                cmd.command_id,
                control.control_version,
                control.frame_seq,
            ))
        }
        ViewerCommandPayload::Annotation(command) => {
            if command.action == "create" {
                if let Some(annotation) = command.annotation {
                    state
                        .daemon_state
                        .annotations
                        .lock()
                        .await
                        .create(annotation)
                        .map_err(|message| {
                            rejected(
                                Some(cmd.command_id.clone()),
                                ViewerRejectionReason::MalformedCommand,
                                message,
                            )
                        })?;
                }
            }
            let control = state.daemon_state.view_control.lock().await.snapshot();
            Ok(accepted(
                cmd.command_id,
                control.control_version,
                control.frame_seq,
            ))
        }
        ViewerCommandPayload::Recording(_) => {
            let control = state.daemon_state.view_control.lock().await.snapshot();
            Ok(accepted(
                cmd.command_id,
                control.control_version,
                control.frame_seq,
            ))
        }
    }
}

pub fn command_id_from_value(value: &Value) -> Option<String> {
    value
        .get("commandId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

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
        }))
        .expect("parsed");
        assert_eq!(cmd.command_id, "cmd_1");
    }

    #[test]
    fn malformed_command_returns_reason() {
        let err = parse_viewer_command(json!({"type": "input"})).expect_err("malformed");
        assert_eq!(
            err.reason,
            gsd_browser_common::viewer::ViewerRejectionReason::MalformedCommand
        );
    }
}
