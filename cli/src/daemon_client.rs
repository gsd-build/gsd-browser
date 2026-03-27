use browser_tools_common::{ipc, pid_path, socket_path, state_dir, DaemonRequest, DaemonResponse};
use std::fs;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};

/// Check if daemon is alive: PID file exists, process alive, socket connectable.
pub fn is_daemon_alive() -> bool {
    let pid_file = pid_path();
    if !pid_file.exists() {
        return false;
    }

    let pid_str = match fs::read_to_string(&pid_file) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let pid: i32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Check process alive via kill(pid, 0)
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

/// Start the daemon process. Spawns the daemon binary in the background and
/// waits for the socket to appear.
pub async fn start_daemon(browser_path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure state dir exists
    fs::create_dir_all(state_dir())?;

    // Advisory lock to prevent race conditions
    let lock_file = browser_tools_common::lock_path();
    let lock_fd = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_file)?;

    // Try to acquire exclusive lock (non-blocking first)
    use std::os::unix::io::AsRawFd;
    let fd = lock_fd.as_raw_fd();
    let lock_result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if lock_result != 0 {
        // Another process is starting the daemon — wait for socket
        eprintln!("[browser-tools] waiting for daemon start by another process...");
        return wait_for_socket(Duration::from_secs(10)).await;
    }

    // We hold the lock — check if daemon is already alive
    if is_daemon_alive() && socket_path().exists() {
        // Already running — release lock and return
        let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };
        return Ok(());
    }

    // Clean up stale files
    let _ = fs::remove_file(socket_path());
    let _ = fs::remove_file(pid_path());

    // Find the daemon binary — it's the same binary we're running, as a sibling crate.
    // In dev: target/debug/browser-tools-daemon
    // We locate it relative to the current executable.
    let daemon_bin = find_daemon_binary()?;

    let mut cmd = std::process::Command::new(&daemon_bin);
    if let Some(path) = browser_path {
        cmd.arg("--browser-path").arg(path);
    }

    // Detach: redirect stdio so parent doesn't hang
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    cmd.spawn()
        .map_err(|e| format!("failed to start daemon ({:?}): {}", daemon_bin, e))?;

    // Wait for socket to appear
    let result = wait_for_socket(Duration::from_secs(10)).await;

    // Release lock
    let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };

    result
}

fn find_daemon_binary() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    // Try relative to current exe first (works in cargo build layout)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("browser-tools-daemon");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // Try PATH
    if let Ok(path) = which::which("browser-tools-daemon") {
        return Ok(path);
    }

    Err("cannot find browser-tools-daemon binary. Run `cargo build --workspace` first.".into())
}

async fn wait_for_socket(max_wait: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let sock = socket_path();
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < max_wait {
        if sock.exists() {
            // Try connecting to verify it's actually listening
            if UnixStream::connect(&sock).await.is_ok() {
                return Ok(());
            }
        }
        sleep(poll_interval).await;
    }

    Err(format!(
        "daemon did not start within {}s — check logs with BROWSER_TOOLS_DEBUG=1",
        max_wait.as_secs()
    )
    .into())
}

/// Stop the daemon by sending SIGTERM to the PID in the pidfile.
pub fn stop_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let pid_file = pid_path();
    if !pid_file.exists() {
        return Err("daemon not running (no PID file)".into());
    }

    let pid_str = fs::read_to_string(&pid_file)?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|_| "invalid PID file")?;

    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGTERM,
    )
    .map_err(|e| format!("failed to stop daemon (PID {pid}): {e}"))?;

    // Wait briefly for cleanup
    std::thread::sleep(Duration::from_millis(500));

    // Clean up if process is gone
    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err() {
        let _ = fs::remove_file(pid_path());
        let _ = fs::remove_file(socket_path());
    }

    Ok(())
}

/// Send a JSON-RPC request to the daemon. Auto-starts daemon if not running.
pub async fn send_request(
    method: &str,
    params: serde_json::Value,
    browser_path: Option<&str>,
) -> Result<DaemonResponse, Box<dyn std::error::Error>> {
    // Ensure daemon is running
    if !is_daemon_alive() || !socket_path().exists() {
        start_daemon(browser_path).await?;
    }

    // Connect and send
    let result = send_once(method, params.clone()).await;

    match result {
        Ok(resp) => Ok(resp),
        Err(_) => {
            // Connection failed — daemon might have died. Restart and retry once.
            eprintln!("[browser-tools] daemon connection failed, restarting...");
            let _ = fs::remove_file(socket_path());
            let _ = fs::remove_file(pid_path());
            start_daemon(browser_path).await?;
            send_once(method, params).await
        }
    }
}

async fn send_once(
    method: &str,
    params: serde_json::Value,
) -> Result<DaemonResponse, Box<dyn std::error::Error>> {
    let sock = socket_path();
    let mut stream = timeout(Duration::from_secs(5), UnixStream::connect(&sock))
        .await
        .map_err(|_| "timeout connecting to daemon")?
        .map_err(|e| format!("cannot connect to daemon socket: {e}"))?;

    let req = DaemonRequest::new(1, method, params);
    let payload = serde_json::to_vec(&req)?;

    timeout(Duration::from_secs(30), ipc::write_message(&mut stream, &payload))
        .await
        .map_err(|_| "timeout writing request to daemon")??;

    let raw = timeout(Duration::from_secs(30), ipc::read_message(&mut stream))
        .await
        .map_err(|_| "timeout reading response from daemon")?
        .map_err(|e| format!("error reading response: {e}"))?;

    if raw.is_empty() {
        return Err("daemon closed connection without response".into());
    }

    let resp: DaemonResponse = serde_json::from_slice(&raw)?;
    Ok(resp)
}
