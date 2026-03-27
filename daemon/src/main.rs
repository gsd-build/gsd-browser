use browser_tools_common::{
    ipc, pid_path, socket_path, state_dir, DaemonRequest, DaemonResponse, ERR_INTERNAL,
    ERR_METHOD_NOT_FOUND,
};
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use serde_json::json;
use std::fs;
use std::process;
use tokio::net::UnixListener;
use tracing::{error, info, warn};

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
    let _page = browser.new_page("about:blank").await?;
    info!("[browser-tools-daemon] initial page created");

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
                        tokio::spawn(handle_connection(stream));
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

async fn handle_connection(mut stream: tokio::net::UnixStream) {
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

    let response = dispatch(&request).await;

    let payload = serde_json::to_vec(&response).unwrap();
    if let Err(e) = ipc::write_message(&mut stream, &payload).await {
        error!("[browser-tools-daemon] write error: {e}");
    }
}

async fn dispatch(req: &DaemonRequest) -> DaemonResponse {
    match req.method.as_str() {
        "ping" => DaemonResponse::success(req.id, json!({"pong": true})),
        "health" => DaemonResponse::success(
            req.id,
            json!({
                "status": "ok",
                "pid": process::id(),
            }),
        ),
        _ => DaemonResponse::error(
            req.id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {}", req.method),
        ),
    }
}
