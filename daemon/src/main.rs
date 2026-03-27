mod capture;
mod handlers;
mod helpers;
mod logs;
mod settle;
mod state;

use browser_tools_common::{
    ipc, pid_path, socket_path, state_dir, DaemonRequest, DaemonResponse, ERR_INTERNAL,
    ERR_METHOD_NOT_FOUND,
};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::EnableParams as NetworkEnableParams;
use chromiumoxide::cdp::browser_protocol::page::EnableParams as PageEnableParams;
use chromiumoxide::cdp::js_protocol::runtime::EnableParams as RuntimeEnableParams;
use chromiumoxide::Page;
use futures::StreamExt;
use logs::DaemonLogs;
use serde_json::json;
use state::DaemonState;
use std::fs;
use std::process;
use std::sync::Arc;
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() {
    // Initialize tracing — respect BROWSER_TOOLS_DEBUG for verbose output
    let filter = if std::env::var("BROWSER_TOOLS_DEBUG").is_ok() {
        "debug"
    } else {
        "info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    if let Err(e) = run_daemon().await {
        error!("[browser-tools-daemon] fatal: {e}");
        process::exit(1);
    }
}

async fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let browser_path_arg = std::env::args()
        .position(|a| a == "--browser-path")
        .and_then(|i| std::env::args().nth(i + 1));

    // Ensure state directory exists
    let state = state_dir();
    fs::create_dir_all(&state)?;

    // Clean up stale socket if exists
    let sock_path = socket_path();
    if sock_path.exists() {
        // Check if old PID is alive
        let pid_file = pid_path();
        let stale = if pid_file.exists() {
            let old_pid = fs::read_to_string(&pid_file)?.trim().parse::<i32>().ok();
            match old_pid {
                Some(pid) => {
                    // Check if process is alive via kill(pid, 0)
                    nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid),
                        None, // signal 0: check if process exists
                    )
                    .is_err()
                }
                None => true,
            }
        } else {
            true
        };

        if stale {
            warn!("[browser-tools-daemon] removing stale socket");
            let _ = fs::remove_file(&sock_path);
            let _ = fs::remove_file(pid_path());
        } else {
            return Err("daemon already running (socket exists and PID is alive)".into());
        }
    }

    // Write PID file
    fs::write(pid_path(), process::id().to_string())?;
    info!(
        "[browser-tools-daemon] PID {} written to {:?}",
        process::id(),
        pid_path()
    );

    // Discover and launch Chrome
    let chrome_path = browser_tools_common::chrome::find_chrome(browser_path_arg.as_deref())
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    info!(
        "[browser-tools-daemon] launching Chrome from {:?}",
        chrome_path
    );

    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .build()
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    let (mut browser, mut handler) = Browser::launch(config).await?;
    info!("[browser-tools-daemon] Chrome launched successfully");

    // Handler must be polled continuously — spawn it
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    // Create initial page
    let page = browser.new_page("about:blank").await?;
    info!("[browser-tools-daemon] initial page created");

    // Inject browser-side helpers and install mutation counter
    helpers::inject_helpers(&page).await;
    settle::ensure_mutation_counter(&page).await;
    info!("[browser-tools-daemon] browser helpers injected, mutation counter installed");

    // Enable CDP domains for event listening
    if let Err(e) = page.execute(RuntimeEnableParams::default()).await {
        warn!("[browser-tools-daemon] Runtime.enable failed (non-fatal): {e}");
    } else {
        debug!("[browser-tools-daemon] Runtime domain enabled");
    }
    if let Err(e) = page.execute(NetworkEnableParams::default()).await {
        warn!("[browser-tools-daemon] Network.enable failed (non-fatal): {e}");
    } else {
        debug!("[browser-tools-daemon] Network domain enabled");
    }
    if let Err(e) = page.execute(PageEnableParams::default()).await {
        warn!("[browser-tools-daemon] Page.enable failed (non-fatal): {e}");
    } else {
        debug!("[browser-tools-daemon] Page domain enabled");
    }
    info!("[browser-tools-daemon] CDP domains enabled");

    // Create log buffers and spawn event listeners
    let daemon_logs = Arc::new(DaemonLogs::new());
    let daemon_state = Arc::new(DaemonState::new());
    logs::spawn_console_listener(&page, daemon_logs.console.clone()).await;
    logs::spawn_exception_listener(&page, daemon_logs.console.clone()).await;
    logs::spawn_network_listener(&page, daemon_logs.network.clone()).await;
    logs::spawn_dialog_listener(&page, daemon_logs.dialog.clone()).await;
    info!("[browser-tools-daemon] event listeners spawned");

    // Wrap page in Arc for sharing across connection tasks
    let page = Arc::new(page);

    // Bind Unix socket
    let listener = UnixListener::bind(&sock_path)?;
    info!(
        "[browser-tools-daemon] listening on {:?}",
        sock_path
    );

    // Set up ctrl-c handler for clean shutdown
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        info!("[browser-tools-daemon] connection accepted");
                        let page = Arc::clone(&page);
                        let logs = Arc::clone(&daemon_logs);
                        let state = Arc::clone(&daemon_state);
                        tokio::spawn(handle_connection(stream, page, logs, state));
                    }
                    Err(e) => {
                        error!("[browser-tools-daemon] accept error: {e}");
                    }
                }
            }
            _ = &mut shutdown => {
                info!("[browser-tools-daemon] shutting down...");
                break;
            }
        }
    }

    // Clean shutdown
    drop(listener);
    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();
    let _ = fs::remove_file(socket_path());
    let _ = fs::remove_file(pid_path());
    info!("[browser-tools-daemon] shutdown complete");

    Ok(())
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    page: Arc<Page>,
    logs: Arc<DaemonLogs>,
    state: Arc<DaemonState>,
) {
    let raw = match ipc::read_message(&mut stream).await {
        Ok(data) if data.is_empty() => return,
        Ok(data) => data,
        Err(e) => {
            error!("[browser-tools-daemon] read error: {e}");
            return;
        }
    };

    let request: DaemonRequest = match serde_json::from_slice(&raw) {
        Ok(r) => r,
        Err(e) => {
            let resp = DaemonResponse::error(0, ERR_INTERNAL, format!("invalid request: {e}"));
            let payload = serde_json::to_vec(&resp).unwrap();
            let _ = ipc::write_message(&mut stream, &payload).await;
            return;
        }
    };

    info!(
        "[browser-tools-daemon] request: method={} id={}",
        request.method, request.id
    );

    let response = dispatch(&request, &page, &logs, &state).await;

    let payload = serde_json::to_vec(&response).unwrap();
    if let Err(e) = ipc::write_message(&mut stream, &payload).await {
        error!("[browser-tools-daemon] write error: {e}");
    }
}

async fn dispatch(req: &DaemonRequest, page: &Page, logs: &DaemonLogs, state: &DaemonState) -> DaemonResponse {
    // Determine if this method should be timeline-recorded
    let record_timeline = matches!(
        req.method.as_str(),
        "navigate" | "back" | "forward" | "reload" | "click" | "type" | "press"
            | "hover" | "scroll" | "select_option" | "set_checked" | "drag"
            | "snapshot" | "click_ref" | "hover_ref" | "fill_ref"
            | "assert" | "diff" | "wait_for"
    );

    // Params summary for timeline (truncated to 80 chars)
    let params_summary = if record_timeline {
        let s = req.params.to_string();
        if s.len() > 80 {
            format!("{}…", &s[..79])
        } else {
            s
        }
    } else {
        String::new()
    };

    // Record before-URL and begin action
    let action_id = if record_timeline {
        let before_url = match page.url().await {
            Ok(Some(u)) => u,
            _ => String::new(),
        };
        let mut timeline = state.timeline.lock().unwrap();
        Some(timeline.begin_action(&req.method, &params_summary, &before_url))
    } else {
        None
    };

    // Also store before-state in DiffState for navigate/click/etc.
    if matches!(
        req.method.as_str(),
        "navigate" | "back" | "forward" | "reload" | "click" | "type" | "press"
            | "hover" | "click_ref" | "hover_ref" | "fill_ref"
    ) {
        let before_state = crate::capture::capture_compact_page_state(page, false).await;
        let mut diff = state.diff.lock().unwrap();
        diff.before = Some(before_state);
    }

    let response = dispatch_inner(req, page, logs, state).await;

    // Finish action in timeline
    if let Some(id) = action_id {
        let after_url = match page.url().await {
            Ok(Some(u)) => u,
            _ => String::new(),
        };
        let (status, error) = if response.error.is_some() {
            ("error", response.error.as_ref().map(|e| e.message.as_str()).unwrap_or(""))
        } else {
            ("ok", "")
        };
        let mut timeline = state.timeline.lock().unwrap();
        timeline.finish_action(id, &after_url, status, error);
    }

    // Store after-state in DiffState for state-mutating methods
    if matches!(
        req.method.as_str(),
        "navigate" | "back" | "forward" | "reload" | "click" | "type" | "press"
            | "hover" | "click_ref" | "hover_ref" | "fill_ref"
    ) {
        let after_state = crate::capture::capture_compact_page_state(page, false).await;
        let mut diff = state.diff.lock().unwrap();
        diff.after = Some(after_state);
    }

    response
}

async fn dispatch_inner(req: &DaemonRequest, page: &Page, logs: &DaemonLogs, state: &DaemonState) -> DaemonResponse {
    match req.method.as_str() {
        "ping" => DaemonResponse::success(req.id, json!({"pong": true})),
        "health" => DaemonResponse::success(
            req.id,
            json!({
                "status": "ok",
                "pid": process::id(),
            }),
        ),
        "navigate" => match handlers::navigate::handle_navigate(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check URL is valid and reachable"}),
            ),
        },
        "back" => match handlers::navigate::handle_back(page).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "forward" => match handlers::navigate::handle_forward(page).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "reload" => match handlers::navigate::handle_reload(page).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "console" => match handlers::inspect::handle_console(logs, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "network" => match handlers::inspect::handle_network(logs, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "dialog" => match handlers::inspect::handle_dialog(logs, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "eval" => match handlers::inspect::handle_eval(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "click" => match handlers::interaction::handle_click(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector is valid and element exists"}),
            ),
        },
        "type" => match handlers::interaction::handle_type_text(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector targets an input/textarea element"}),
            ),
        },
        "press" => match handlers::interaction::handle_press(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "hover" => match handlers::interaction::handle_hover(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector is valid and element exists"}),
            ),
        },
        "scroll" => match handlers::interaction::handle_scroll(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "select_option" => {
            match handlers::interaction::handle_select_option(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "set_checked" => {
            match handlers::interaction::handle_set_checked(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "drag" => match handlers::interaction::handle_drag(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "set_viewport" => {
            match handlers::interaction::handle_set_viewport(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "upload_file" => {
            match handlers::interaction::handle_upload_file(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "screenshot" => match handlers::screenshot::handle_screenshot(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector is valid or try without --selector"}),
            ),
        }
        "accessibility_tree" => {
            match handlers::inspect::handle_accessibility_tree(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "find" => match handlers::inspect::handle_find(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "page_source" => match handlers::inspect::handle_page_source(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "wait_for" => match handlers::wait::handle_wait_for(page, logs, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "timeline" => match handlers::timeline::handle_timeline(state, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "snapshot" => match handlers::refs::handle_snapshot(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "get_ref" => match handlers::refs::handle_get_ref(state, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "click_ref" => match handlers::refs::handle_click_ref(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check ref is valid and element still exists on page"}),
            ),
        },
        "hover_ref" => match handlers::refs::handle_hover_ref(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check ref is valid and element still exists on page"}),
            ),
        },
        "fill_ref" => match handlers::refs::handle_fill_ref(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check ref targets an input/textarea element"}),
            ),
        },
        "assert" => match handlers::assert_cmd::handle_assert(page, logs, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "diff" => match handlers::assert_cmd::handle_diff(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        _ => DaemonResponse::error(
            req.id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {}", req.method),
        ),
    }
}
