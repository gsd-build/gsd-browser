use crate::daemon::narration::events::{now_ms, ControlState, NarrationEvent};
use crate::daemon::view::http::ViewState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
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
    history: Vec<NarrationEvent>,
    timestamp: u64,
}

pub async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<ViewState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: ViewState) {
    let goal = state.narrator.current_goal().await;
    let control = state.narrator.control.get().await;
    let history = state.narrator.history.lock().await.recent(32);
    let snapshot = SnapshotMsg {
        ty: "snapshot",
        goal,
        control,
        history,
        timestamp: now_ms(),
    };
    if let Ok(json) = serde_json::to_string(&snapshot) {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    let mut rx = state.narrator.subscribe();

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
            msg = socket.recv() => {
                match msg {
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
