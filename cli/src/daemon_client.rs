use gsd_browser_common::{
    ipc, pid_path_for, socket_path_for, state_dir, DaemonRequest, DaemonResponse,
};
use std::fs;
use std::process::{Child, Stdio};
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
pub async fn start_daemon(
    browser_path: Option<&str>,
    session: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
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

    // Spawn the daemon as a hidden subcommand of the current binary.
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot determine current executable: {e}"))?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("_serve");
    if let Some(path) = browser_path {
        cmd.arg("--browser-path").arg(path);
    }
    if let Some(name) = session {
        cmd.arg("--session").arg(name);
    }

    // In debug mode, inherit daemon logs so startup failures are visible.
    cmd.stdin(Stdio::null());
    if std::env::var_os("GSD_BROWSER_DEBUG").is_some() {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start daemon ({:?}): {}", exe, e))?;

    // Wait for socket to appear and fail fast if the daemon exits during startup.
    let result = wait_for_spawned_daemon(session, &mut child, Duration::from_secs(10)).await;
    if result.is_err() {
        let _ = fs::remove_file(socket_path_for(session));
        let _ = fs::remove_file(pid_path_for(session));
    }

    // Release lock
    let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };

    result
}

async fn wait_for_socket(
    session: Option<&str>,
    max_wait: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
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
        "daemon did not start within {}s — re-run with GSD_BROWSER_DEBUG=1 for startup logs",
        max_wait.as_secs()
    )
    .into())
}

async fn wait_for_spawned_daemon(
    session: Option<&str>,
    child: &mut Child,
    max_wait: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let sock = socket_path_for(session);
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < max_wait {
        if sock.exists() && UnixStream::connect(&sock).await.is_ok() {
            return Ok(());
        }

        if let Some(status) = child.try_wait()? {
            return Err(format!(
                "daemon exited during startup with status {status} — re-run with GSD_BROWSER_DEBUG=1 for startup logs"
            )
            .into());
        }

        sleep(poll_interval).await;
    }

    Err(format!(
        "daemon did not start within {}s — re-run with GSD_BROWSER_DEBUG=1 for startup logs",
        max_wait.as_secs()
    )
    .into())
}

/// Stop the daemon by sending SIGTERM to the PID in the pidfile.
/// Treats an already-dead process as success and always cleans up stale files.
pub fn stop_daemon(session: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let pid_file = pid_path_for(session);
    if !pid_file.exists() {
        // No PID file — clean up socket if leftover and treat as success
        let _ = fs::remove_file(socket_path_for(session));
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_file)?;
    let pid: i32 = pid_str.trim().parse().map_err(|_| "invalid PID file")?;

    match nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGTERM,
    ) {
        Ok(()) => {
            // Signal sent — wait briefly for process to exit
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(nix::errno::Errno::ESRCH) => {
            // Process already dead — that's fine, we just need to clean up
        }
        Err(e) => {
            return Err(format!("failed to stop daemon (PID {pid}): {e}").into());
        }
    }

    // Always clean up stale files
    let _ = fs::remove_file(pid_path_for(session));
    let _ = fs::remove_file(socket_path_for(session));

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

    timeout(
        Duration::from_secs(30),
        ipc::write_message(&mut stream, &payload),
    )
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
