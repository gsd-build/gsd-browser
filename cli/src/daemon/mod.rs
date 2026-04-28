pub mod capture;
pub mod handlers;
pub mod helpers;
pub mod inspection;
pub mod logs;
pub mod settle;
pub mod state;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::EnableParams as NetworkEnableParams;
use chromiumoxide::cdp::browser_protocol::page::EnableParams as PageEnableParams;
use chromiumoxide::cdp::js_protocol::runtime::EnableParams as RuntimeEnableParams;
use chromiumoxide::Page;
use futures::StreamExt;
use gsd_browser_common::session::{
    now_epoch_secs, save_session_manifest, session_dir_for, SessionHealthStatus, SessionManifest,
};
use gsd_browser_common::{
    config::Config,
    identity::{identity_profile_dir, IdentityScope},
    ipc, pid_path_for, socket_path_for, state_dir, validate_session_name, DaemonRequest,
    DaemonResponse, ERR_INTERNAL, ERR_METHOD_NOT_FOUND,
};
use logs::DaemonLogs;
use serde_json::json;
use state::{DaemonState, SessionRuntime};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

/// Entry point for the daemon server. Called when the binary is invoked
/// with the hidden `_daemon` subcommand.
pub async fn run(
    browser_path: Option<String>,
    session: Option<String>,
    cdp_url: Option<String>,
    identity_scope: Option<String>,
    identity_key: Option<String>,
    identity_project_id: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing — respect GSD_BROWSER_DEBUG for verbose output
    let filter = if std::env::var("GSD_BROWSER_DEBUG").is_ok() {
        "debug"
    } else {
        "info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    run_daemon(
        browser_path,
        session,
        cdp_url,
        identity_scope,
        identity_key,
        identity_project_id,
    )
    .await
}

async fn shutdown_signal() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => {}
            _ = sigterm.recv() => {}
        }

        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

fn browser_profile_dir(
    session: Option<&str>,
    identity_scope: Option<IdentityScope>,
    identity_key: Option<&str>,
    identity_project_id: Option<&str>,
) -> Result<PathBuf, String> {
    match (identity_scope, identity_key) {
        (Some(scope), Some(key)) => identity_profile_dir(scope, identity_project_id, key),
        (None, None) => Ok(session_dir_for(session).join("browser-profile")),
        _ => Err("identity profile requires both identity scope and key".to_string()),
    }
}

fn cleanup_browser_profile_singletons(profile_dir: &Path) {
    for artifact in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let path = profile_dir.join(artifact);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };

        let result = if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };

        if let Err(err) = result {
            warn!(
                "[gsd-browser-daemon] failed to remove browser profile artifact {:?}: {}",
                path, err
            );
        }
    }
}

async fn run_daemon(
    browser_path_arg: Option<String>,
    session_arg: Option<String>,
    cdp_url_arg: Option<String>,
    identity_scope_arg: Option<String>,
    identity_key_arg: Option<String>,
    identity_project_id_arg: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load config (layers 1-4: defaults → user → project → env vars)
    let config = Config::load();
    info!(
        "[gsd-browser-daemon] config loaded (settle timeout={}ms, screenshot quality={})",
        config.settle.timeout_ms, config.screenshot.quality
    );

    // CLI flags override config
    let effective_browser_path = browser_path_arg.or_else(|| config.browser.path.clone());
    let effective_cdp_url = cdp_url_arg.or_else(|| config.browser.cdp_url.clone());

    let session = validate_session_name(session_arg.as_deref())
        .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
    let identity_scope = identity_scope_arg
        .as_deref()
        .map(IdentityScope::parse)
        .transpose()
        .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
    if matches!(identity_scope, Some(IdentityScope::Project)) && identity_project_id_arg.is_none() {
        return Err("project identity requires --identity-project".into());
    }
    if identity_scope.is_some() && identity_key_arg.is_none() {
        return Err("identity profile requires --identity-key".into());
    }

    // Ensure state directory exists
    let state = state_dir();
    fs::create_dir_all(&state)?;

    // For session mode, ensure session subdir exists
    let sock_path = socket_path_for(session);
    let pid_file_path = pid_path_for(session);
    if let Some(parent) = sock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Clean up stale socket if exists
    if sock_path.exists() {
        // Check if old PID is alive
        let stale = if pid_file_path.exists() {
            let old_pid = fs::read_to_string(&pid_file_path)?
                .trim()
                .parse::<i32>()
                .ok();
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
            warn!("[gsd-browser-daemon] removing stale socket");
            let _ = fs::remove_file(&sock_path);
            let _ = fs::remove_file(&pid_file_path);
        } else {
            return Err("daemon already running (socket exists and PID is alive)".into());
        }
    }

    // Write PID file
    fs::write(&pid_file_path, process::id().to_string())?;
    info!(
        "[gsd-browser-daemon] PID {} written to {:?}",
        process::id(),
        pid_file_path
    );

    let launch_mode = if effective_cdp_url.is_some() {
        "attached".to_string()
    } else {
        "launched".to_string()
    };
    let start_ts = now_epoch_secs();
    let starting_manifest = SessionManifest {
        manifest_version: 1,
        session_name: session.map(str::to_string),
        daemon_pid: Some(process::id() as i32),
        socket_path: sock_path.to_string_lossy().to_string(),
        daemon_started_at: Some(start_ts),
        browser_started_at: Some(start_ts),
        daemon_version: env!("CARGO_PKG_VERSION").to_string(),
        launch_mode: launch_mode.clone(),
        cdp_url: effective_cdp_url.clone(),
        health: SessionHealthStatus::Starting,
        health_reason: "daemon starting".to_string(),
        last_updated_at: Some(start_ts),
        identity_scope,
        identity_project_id: identity_project_id_arg.clone(),
        identity_key: identity_key_arg.clone(),
        ..SessionManifest::default()
    };
    save_session_manifest(session, &starting_manifest)
        .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;

    let (mut browser, mut handler) = if let Some(ref cdp_url) = effective_cdp_url {
        // Connect to an already-running Chrome instance via CDP
        info!(
            "[gsd-browser-daemon] connecting to existing Chrome at {}",
            cdp_url
        );

        // chromiumoxide needs the WebSocket debugger URL. If the user passed
        // an HTTP endpoint (e.g. http://localhost:9222), fetch /json/version
        // to discover the ws URL automatically.
        let ws_url = if cdp_url.starts_with("ws://") || cdp_url.starts_with("wss://") {
            cdp_url.clone()
        } else {
            let version_url = format!("{}/json/version", cdp_url.trim_end_matches('/'));
            let body: serde_json::Value = reqwest::get(&version_url)
                .await
                .map_err(|e| {
                    format!("failed to reach Chrome debug endpoint at {version_url}: {e}")
                })?
                .json()
                .await
                .map_err(|e| format!("invalid JSON from {version_url}: {e}"))?;
            body["webSocketDebuggerUrl"]
                .as_str()
                .ok_or_else(|| format!("Chrome at {cdp_url} did not return webSocketDebuggerUrl — is --remote-debugging-port enabled?"))?
                .to_string()
        };

        info!("[gsd-browser-daemon] resolved WebSocket URL: {}", ws_url);
        let result =
            Browser::connect(&ws_url)
                .await
                .map_err(|e| -> Box<dyn std::error::Error> {
                    format!("failed to connect to Chrome CDP at {ws_url}: {e}").into()
                })?;
        info!("[gsd-browser-daemon] connected to existing Chrome successfully");
        result
    } else {
        // Launch a new Chrome instance
        let chrome_path =
            gsd_browser_common::chrome::find_chrome(effective_browser_path.as_deref())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        info!(
            "[gsd-browser-daemon] launching Chrome from {:?}",
            chrome_path
        );

        let profile_dir = browser_profile_dir(
            session,
            identity_scope,
            identity_key_arg.as_deref(),
            identity_project_id_arg.as_deref(),
        )
        .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
        fs::create_dir_all(&profile_dir)?;
        cleanup_browser_profile_singletons(&profile_dir);

        let mut builder = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .user_data_dir(&profile_dir)
            .window_size(1920, 1080)
            .arg("--window-size=1920,1080");
        if !config.browser.headless {
            builder = builder.with_head();
        }
        let browser_config = builder
            .build()
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        let result = Browser::launch(browser_config).await?;
        info!("[gsd-browser-daemon] Chrome launched successfully");
        result
    };

    // Handler must be polled continuously — spawn it
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(err) = event {
                error!("[gsd-browser-daemon] browser handler error: {err}");
            }
        }
    });

    // Create initial page
    let page = browser.new_page("about:blank").await?;
    info!("[gsd-browser-daemon] initial page created");

    // Inject browser-side helpers and install mutation counter
    helpers::inject_helpers(&page).await;
    settle::ensure_mutation_counter(&page).await;
    info!("[gsd-browser-daemon] browser helpers injected, mutation counter installed");

    // Enable CDP domains for event listening
    if let Err(e) = page.execute(RuntimeEnableParams::default()).await {
        warn!("[gsd-browser-daemon] Runtime.enable failed (non-fatal): {e}");
    } else {
        debug!("[gsd-browser-daemon] Runtime domain enabled");
    }
    if let Err(e) = page.execute(NetworkEnableParams::default()).await {
        warn!("[gsd-browser-daemon] Network.enable failed (non-fatal): {e}");
    } else {
        debug!("[gsd-browser-daemon] Network domain enabled");
    }
    if let Err(e) = page.execute(PageEnableParams::default()).await {
        warn!("[gsd-browser-daemon] Page.enable failed (non-fatal): {e}");
    } else {
        debug!("[gsd-browser-daemon] Page domain enabled");
    }
    info!("[gsd-browser-daemon] CDP domains enabled");

    // Create log buffers and spawn event listeners
    let daemon_logs = Arc::new(DaemonLogs::new());
    let browser_pid = browser
        .get_mut_child()
        .and_then(|child| child.as_mut_inner().id());
    let browser_user_data_dir = browser
        .config()
        .and_then(|cfg| cfg.user_data_dir.as_ref())
        .map(|path| path.display().to_string());
    let daemon_state = Arc::new(DaemonState::new_with_session(SessionRuntime {
        session_name: session.map(str::to_string),
        launch_mode: launch_mode.clone(),
        cdp_url: effective_cdp_url.clone(),
        websocket_url: Some(browser.websocket_address().clone()),
        browser_pid,
        browser_user_data_dir,
        identity_scope,
        identity_project_id: identity_project_id_arg.clone(),
        identity_key: identity_key_arg.clone(),
        socket_path: sock_path.to_string_lossy().to_string(),
    }));
    logs::spawn_console_listener(&page, daemon_logs.console.clone()).await;
    logs::spawn_exception_listener(&page, daemon_logs.console.clone()).await;
    logs::spawn_network_listener(&page, daemon_logs.network.clone()).await;
    logs::spawn_dialog_listener(&page, daemon_logs.dialog.clone()).await;
    info!("[gsd-browser-daemon] event listeners spawned");

    // Register initial page in the PageRegistry
    {
        let page_arc = Arc::new(page);
        let mut pages = daemon_state.pages.lock().unwrap();
        pages.register(page_arc, String::new(), "about:blank".to_string());
    }

    // Bind Unix socket
    let listener = UnixListener::bind(&sock_path)?;
    info!("[gsd-browser-daemon] listening on {:?}", sock_path);

    if let Some(page) = daemon_state.pages.lock().unwrap().active_page() {
        let state = Arc::clone(&daemon_state);
        tokio::spawn(async move {
            let _ = handlers::session::sync_session_manifest(
                page.as_ref(),
                &state,
                Some(SessionHealthStatus::Healthy),
                None,
            )
            .await;
        });
    }

    // Trap termination signals so `daemon stop` can shut Chrome down cleanly.
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        info!("[gsd-browser-daemon] connection accepted");
                        let logs = Arc::clone(&daemon_logs);
                        let state = Arc::clone(&daemon_state);
                        tokio::spawn(handle_connection(stream, logs, state));
                    }
                    Err(e) => {
                        error!("[gsd-browser-daemon] accept error: {e}");
                    }
                }
            }
            _ = &mut shutdown => {
                info!("[gsd-browser-daemon] shutting down...");
                break;
            }
        }
    }

    // Clean shutdown
    if let Some(page) = daemon_state.pages.lock().unwrap().active_page() {
        let _ = handlers::session::sync_session_manifest(
            page.as_ref(),
            &daemon_state,
            Some(SessionHealthStatus::Stopped),
            Some("daemon stopped".to_string()),
        )
        .await;
    } else {
        let _ = handlers::session::mark_session_stopped(&daemon_state, "daemon stopped").await;
    }
    drop(listener);
    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();
    let _ = fs::remove_file(&sock_path);
    let _ = fs::remove_file(&pid_file_path);
    info!("[gsd-browser-daemon] shutdown complete");

    Ok(())
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    logs: Arc<DaemonLogs>,
    state: Arc<DaemonState>,
) {
    let raw = match ipc::read_message(&mut stream).await {
        Ok(data) if data.is_empty() => return,
        Ok(data) => data,
        Err(e) => {
            error!("[gsd-browser-daemon] read error: {e}");
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
        "[gsd-browser-daemon] request: method={} id={}",
        request.method, request.id
    );

    // Resolve the active page from the registry
    let page = {
        let pages = state.pages.lock().unwrap();
        pages.active_page()
    };

    let response = match page {
        Some(page) => dispatch(&request, &page, &logs, &state).await,
        None => DaemonResponse::error(
            request.id,
            ERR_INTERNAL,
            "no active page in registry".to_string(),
        ),
    };

    let payload = serde_json::to_vec(&response).unwrap();
    if let Err(e) = ipc::write_message(&mut stream, &payload).await {
        error!("[gsd-browser-daemon] write error: {e}");
    }
}

async fn dispatch(
    req: &DaemonRequest,
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
) -> DaemonResponse {
    // Determine if this method should be timeline-recorded
    let record_timeline = matches!(
        req.method.as_str(),
        "navigate"
            | "back"
            | "forward"
            | "reload"
            | "click"
            | "type"
            | "press"
            | "hover"
            | "scroll"
            | "select_option"
            | "set_checked"
            | "drag"
            | "snapshot"
            | "click_ref"
            | "hover_ref"
            | "fill_ref"
            | "assert"
            | "diff"
            | "wait_for"
            | "batch"
            | "fill_form"
            | "act"
    );

    // Params summary for timeline (truncated to 80 chars)
    let params_summary = if record_timeline {
        let s = req.params.to_string();
        if s.len() > 80 {
            format!("{}…", s.chars().take(79).collect::<String>())
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
        "navigate"
            | "back"
            | "forward"
            | "reload"
            | "click"
            | "type"
            | "press"
            | "hover"
            | "click_ref"
            | "hover_ref"
            | "fill_ref"
            | "fill_form"
            | "act"
    ) {
        let before_state = capture::capture_compact_page_state(page, false).await;
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
            (
                "error",
                response
                    .error
                    .as_ref()
                    .map(|e| e.message.as_str())
                    .unwrap_or(""),
            )
        } else {
            ("ok", "")
        };
        let mut timeline = state.timeline.lock().unwrap();
        timeline.finish_action(id, &after_url, status, error);
    }

    // Store after-state in DiffState for state-mutating methods
    if matches!(
        req.method.as_str(),
        "navigate"
            | "back"
            | "forward"
            | "reload"
            | "click"
            | "type"
            | "press"
            | "hover"
            | "click_ref"
            | "hover_ref"
            | "fill_ref"
            | "fill_form"
            | "act"
    ) {
        let after_state = capture::capture_compact_page_state(page, false).await;
        let mut diff = state.diff.lock().unwrap();
        diff.after = Some(after_state);
    }

    if response.error.is_none()
        && !matches!(
            req.method.as_str(),
            "health" | "session_summary" | "debug_bundle"
        )
    {
        let _ = handlers::session::sync_session_manifest(page, state, None, None).await;
    }

    response
}

pub(crate) async fn dispatch_inner(
    req: &DaemonRequest,
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
) -> DaemonResponse {
    match req.method.as_str() {
        "ping" => DaemonResponse::success(req.id, json!({"pong": true})),
        "cloud_session_status" => {
            match handlers::cloud::handle_cloud_session_status(page, state).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "cloud_frame" => match handlers::cloud::handle_cloud_frame(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "cloud_tool" => {
            match handlers::cloud::handle_cloud_tool(page, logs, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "cloud_user_input" => {
            match handlers::cloud::handle_cloud_user_input(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "cloud_identity_list" => match handlers::cloud::handle_cloud_identity_list(&req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "cloud_identity_save" => match handlers::cloud::handle_cloud_identity_save(&req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "cloud_identity_revoke" => {
            match handlers::cloud::handle_cloud_identity_revoke(&req.params) {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "health" => match handlers::session::handle_health(page, state).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "navigate" => match handlers::navigate::handle_navigate(page, &req.params, state).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check URL is valid and reachable"}),
            ),
        },
        "back" => match handlers::navigate::handle_back(page, state).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "forward" => match handlers::navigate::handle_forward(page, state).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "reload" => match handlers::navigate::handle_reload(page, state).await {
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
        "eval" => match handlers::inspect::handle_eval(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "click" => match handlers::interaction::handle_click(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector is valid and element exists"}),
            ),
        },
        "type" => match handlers::interaction::handle_type_text(page, state, &req.params).await {
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
        "hover" => match handlers::interaction::handle_hover(page, state, &req.params).await {
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
            match handlers::interaction::handle_select_option(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "set_checked" => {
            match handlers::interaction::handle_set_checked(page, state, &req.params).await {
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
        "upload_file" => match handlers::interaction::handle_upload_file(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "screenshot" => match handlers::screenshot::handle_screenshot(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check selector is valid or try without --selector"}),
            ),
        },
        "accessibility_tree" => {
            match handlers::inspect::handle_accessibility_tree(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "find" => match handlers::inspect::handle_find(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "page_source" => {
            match handlers::inspect::handle_page_source(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "wait_for" => match handlers::wait::handle_wait_for(page, logs, state, &req.params).await {
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
        "assert" => match handlers::assert_cmd::handle_assert(page, logs, state, &req.params).await
        {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "diff" => match handlers::assert_cmd::handle_diff(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "batch" => match handlers::batch::handle_batch(page, logs, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "list_pages" => match handlers::pages::handle_list_pages(state) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "switch_page" => match handlers::pages::handle_switch_page(state, &req.params).await {
            Ok((result, _new_page)) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "close_page" => match handlers::pages::handle_close_page(state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "list_frames" => match handlers::pages::handle_list_frames(page).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "select_frame" => match handlers::pages::handle_select_frame(state, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "analyze_form" => match handlers::forms::handle_analyze_form(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "fill_form" => match handlers::forms::handle_fill_form(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check field identifiers match form labels/names/placeholders"}),
            ),
        },
        "find_best" => match handlers::intent::handle_find_best(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "act" => match handlers::intent::handle_act(page, state, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error_with_data(
                req.id,
                ERR_INTERNAL,
                &msg,
                json!({"retryHint": "Check intent is valid and matching elements exist on page"}),
            ),
        },
        "session_summary" => {
            match handlers::session::handle_session_summary(page, logs, state).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "debug_bundle" => {
            match handlers::session::handle_debug_bundle(page, logs, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "visual_diff" => match handlers::visual_diff::handle_visual_diff(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "zoom_region" => match handlers::visual_diff::handle_zoom_region(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "save_pdf" => match handlers::pdf::handle_save_pdf(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "extract" => match handlers::extract::handle_extract(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "mock_route" => {
            match handlers::network_mock::handle_mock_route(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "block_urls" => {
            match handlers::network_mock::handle_block_urls(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "clear_routes" => {
            match handlers::network_mock::handle_clear_routes(page, state, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "emulate_device" => {
            match handlers::device::handle_emulate_device(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "save_state" => match handlers::state_persist::handle_save_state(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "restore_state" => {
            match handlers::state_persist::handle_restore_state(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "vault_save" => match handlers::auth_vault::handle_vault_save(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "vault_login" => {
            match handlers::auth_vault::handle_vault_login(page, &req.params, state).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "vault_list" => match handlers::auth_vault::handle_vault_list(page, &req.params).await {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "action_cache" => match handlers::advanced::handle_action_cache(state, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "check_injection" => {
            match handlers::advanced::handle_check_injection(page, &req.params).await {
                Ok(result) => DaemonResponse::success(req.id, result),
                Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
            }
        }
        "generate_test" => match handlers::codegen::handle_generate_test(state, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "har_export" => match handlers::har::handle_har_export(logs, &req.params) {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "trace_start" => match handlers::traces::handle_trace_start(page, state, &req.params).await
        {
            Ok(result) => DaemonResponse::success(req.id, result),
            Err(msg) => DaemonResponse::error(req.id, ERR_INTERNAL, msg),
        },
        "trace_stop" => match handlers::traces::handle_trace_stop(page, state, &req.params).await {
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
