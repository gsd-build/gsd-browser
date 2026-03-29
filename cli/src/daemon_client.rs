use gsd_browser_common::{ipc, pid_path_for, socket_path_for, state_dir, DaemonRequest, DaemonResponse};
use std::fs;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};

/// Check if daemon is alive: PID file exists, process alive, socket connectable.
pub fn is_daemon_alive(session: Option<&str>) -> bool {
    let pid_file = pid_path_for(session);
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
pub async fn start_daemon(browser_path: Option<&str>, session: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure state dir exists (and session subdir if needed)
    let sock = socket_path_for(session);
    if let Some(parent) = sock.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(state_dir())?;

    // Advisory lock to prevent race conditions
    let lock_file = gsd_browser_common::lock_path_for(session);
    if let Some(parent) = lock_file.parent() {
        fs::create_dir_all(parent)?;
    }
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
        eprintln!("[gsd-browser] waiting for daemon start by another process...");
        return wait_for_socket(session, Duration::from_secs(10)).await;
    }

    // We hold the lock — check if daemon is already alive
    if is_daemon_alive(session) && sock.exists() {
        // Already running — release lock and return
        let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };
        return Ok(());
    }

    // Clean up stale files
    let _ = fs::remove_file(socket_path_for(session));
    let _ = fs::remove_file(pid_path_for(session));

    // Find the daemon binary — it's the same binary we're running, as a sibling crate.
    // In dev: target/debug/gsd-browser-daemon
    // We locate it relative to the current executable.
    let daemon_bin = find_daemon_binary()?;

    let mut cmd = std::process::Command::new(&daemon_bin);
    if let Some(path) = browser_path {
        cmd.arg("--browser-path").arg(path);
    }
    if let Some(name) = session {
        cmd.arg("--session").arg(name);
    }

    // Detach: redirect stdio so parent doesn't hang
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    cmd.spawn()
        .map_err(|e| format!("failed to start daemon ({:?}): {}", daemon_bin, e))?;

    // Wait for socket to appear
    let result = wait_for_socket(session, Duration::from_secs(10)).await;

    // Release lock
    let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };

    result
}

fn find_daemon_binary() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    // Try relative to current exe first (works in cargo build layout)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("gsd-browser-daemon");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // Try PATH
    if let Ok(path) = which::which("gsd-browser-daemon") {
        return Ok(path);
    }

    Err("cannot find gsd-browser-daemon binary. Run `cargo build --workspace` first.".into())
}

async fn wait_for_socket(session: Option<&str>, max_wait: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let sock = socket_path_for(session);
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
        "daemon did not start within {}s — check logs with GSD_BROWSER_DEBUG=1",
        max_wait.as_secs()
    )
    .into())
}

/// Stop the daemon by sending SIGTERM to the PID in the pidfile.
pub fn stop_daemon(session: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let pid_file = pid_path_for(session);
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
        let _ = fs::remove_file(pid_path_for(session));
        let _ = fs::remove_file(socket_path_for(session));
    }

    Ok(())
}

/// Send a JSON-RPC request to the daemon. Auto-starts daemon if not running.
pub async fn send_request(
    method: &str,
    params: serde_json::Value,
    browser_path: Option<&str>,
    session: Option<&str>,
) -> Result<DaemonResponse, Box<dyn std::error::Error>> {
    // Ensure daemon is running
    if !is_daemon_alive(session) || !socket_path_for(session).exists() {
        start_daemon(browser_path, session).await?;
    }

    // Connect and send
    let result = send_once(method, params.clone(), session).await;

    match result {
        Ok(resp) => Ok(resp),
        Err(_) => {
            // Connection failed — daemon might have died. Restart and retry once.
            eprintln!("[gsd-browser] daemon connection failed, restarting...");
            let _ = fs::remove_file(socket_path_for(session));
            let _ = fs::remove_file(pid_path_for(session));
            start_daemon(browser_path, session).await?;
            send_once(method, params, session).await
        }
    }
}

async fn send_once(
    method: &str,
    params: serde_json::Value,
    session: Option<&str>,
) -> Result<DaemonResponse, Box<dyn std::error::Error>> {
    let sock = socket_path_for(session);
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
