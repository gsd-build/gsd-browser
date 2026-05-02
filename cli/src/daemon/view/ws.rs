use crate::daemon::narration::events::{now_ms, ControlState, NarrationEvent};
use crate::daemon::view::http::ViewState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{StatusCode, Uri},
    response::IntoResponse,
};
use serde::Serialize;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

#[derive(Serialize)]
struct SnapshotMsg<'a> {
    #[serde(rename = "type")]
    ty: &'a str,
    goal: Option<String>,
    control: ControlState,
    shared_control: gsd_browser_common::viewer::SharedControlStateV1,
    approval: Option<gsd_browser_common::viewer::ApprovalRequestV1>,
    history: Vec<NarrationEvent>,
    timestamp: u64,
}

pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<ViewState>,
    uri: Uri,
) -> impl IntoResponse {
    if let Err((status, message)) =
        crate::daemon::view::http::verify_viewer_token(&state, &uri, "view")
    {
        return (status, message).into_response();
    }
    if let Some(host) = uri.host() {
        if !crate::daemon::view::auth::is_loopback_host(host) {
            return (
                StatusCode::FORBIDDEN,
                "viewer websocket requires loopback host".to_string(),
            )
                .into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: ViewState) {
    let goal = state.narrator.current_goal().await;
    let control = state.narrator.control.get().await;
    let (shared_control, approval) = {
        let store = state.daemon_state.view_control.lock().await;
        (store.snapshot(), store.pending_approval())
    };
    let history = state.narrator.history.lock().await.recent(32);
    let snapshot = SnapshotMsg {
        ty: "snapshot",
        goal,
        control,
        shared_control,
        approval,
        history,
        timestamp: now_ms(),
    };
    if let Ok(json) = serde_json::to_string(&snapshot) {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    let mut rx = state.narrator.subscribe();
    let mut frames_rx = state.frames.subscribe();
    let mut refs_rx = state.refs.subscribe();

    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
            ev = frames_rx.recv() => {
                match ev {
                    Ok(frame) => {
                        if let Ok(json) = serde_json::to_string(&frame) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
            ev = refs_rx.recv() => {
                match ev {
                    Ok(refs) => {
                        if let Ok(json) = serde_json::to_string(&refs) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let value = match serde_json::from_str::<serde_json::Value>(text.as_str()) {
                            Ok(value) => value,
                            Err(err) => {
                                let rejected = crate::daemon::view::input::rejected(
                                    None,
                                    gsd_browser_common::viewer::ViewerRejectionReason::MalformedCommand,
                                    err.to_string(),
                                );
                                if let Ok(json) = serde_json::to_string(&rejected) {
                                    let _ = socket.send(Message::Text(json.into())).await;
                                }
                                continue;
                            }
                        };
                        let command_id = crate::daemon::view::input::command_id_from_value(&value);
                        match crate::daemon::view::input::parse_viewer_command(value) {
                            Ok(cmd) => {
                                let response = match crate::daemon::view::input::handle_viewer_command(cmd, &state).await {
                                    Ok(accepted) => serde_json::to_value(accepted),
                                    Err(rejected) => serde_json::to_value(rejected),
                                };
                                if let Ok(value) = response {
                                    if socket.send(Message::Text(value.to_string().into())).await.is_err() {
                                        break;
                                    }
                                }
                                let (shared_control, approval) = {
                                    let store = state.daemon_state.view_control.lock().await;
                                    (store.snapshot(), store.pending_approval())
                                };
                                let control_event = serde_json::json!({
                                    "type": "control",
                                    "state": shared_control,
                                    "approval": approval,
                                });
                                if socket.send(Message::Text(control_event.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                let rejected = crate::daemon::view::input::rejected(command_id, err.reason, err.message);
                                if let Ok(json) = serde_json::to_string(&rejected) {
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        let rejected = crate::daemon::view::input::rejected(
                            None,
                            gsd_browser_common::viewer::ViewerRejectionReason::MalformedCommand,
                            "binary viewer commands are unsupported",
                        );
                        if let Ok(json) = serde_json::to_string(&rejected) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(15)) => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
        }
    }
}
