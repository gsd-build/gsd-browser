use gsd_browser_common::session::{
    load_session_manifest, manifest_path_for, now_epoch_secs, save_session_manifest,
    SessionHealthStatus, SessionManifest,
};
use gsd_browser_common::{
    ipc, pid_path_for, socket_path_for, state_dir, validate_session_name, DaemonRequest,
    DaemonResponse,
};
use serde_json::json;
use std::fs;
use std::io;
use std::process::{Child, Stdio};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};

fn read_daemon_pid(session: Option<&str>) -> Option<i32> {
    let pid_file = pid_path_for(session);
    let pid_str = fs::read_to_string(pid_file).ok()?;
    pid_str.trim().parse().ok()
}

fn cleanup_daemon_artifacts(session: Option<&str>) {
    let _ = fs::remove_file(socket_path_for(session));
    let _ = fs::remove_file(pid_path_for(session));
}

fn replacement_refused_error(
    session: Option<&str>,
    manifest: &SessionManifest,
) -> Box<dyn std::error::Error> {
    let stop_hint = match session {
        Some(name) => format!("gsd-browser --session {name} daemon stop"),
        None => "gsd-browser daemon stop".to_string(),
    };
    let session_label = session.unwrap_or("default");
    let reason = if manifest.health_reason.is_empty() {
        "session replacement requires explicit recovery".to_string()
    } else {
        manifest.health_reason.clone()
    };
    format!(
        "session '{session_label}' is in '{}' state ({reason}). Refusing to replace it automatically; run `{stop_hint}` and retry",
        manifest.health.as_str()
    )
    .into()
}

fn write_stopped_manifest(session: Option<&str>, reason: &str) -> Result<(), String> {
    let mut manifest = load_session_manifest(session)?.unwrap_or_default();
    let now = now_epoch_secs();
    manifest.session_name = session.map(str::to_string);
    manifest.daemon_pid = None;
    manifest.health = SessionHealthStatus::Stopped;
    manifest.health_reason = reason.to_string();
    manifest.last_updated_at = Some(now);
    manifest.last_heartbeat_at = Some(now);
    manifest.socket_path = socket_path_for(session).to_string_lossy().to_string();
    save_session_manifest(session, &manifest)
}

fn refuse_implicit_named_session_replacement(
    session: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(_) = session else {
        return Ok(());
    };
    let manifest = match load_session_manifest(session)? {
        Some(manifest) => manifest,
        None => return Ok(()),
    };
    if manifest.health == SessionHealthStatus::Stopped {
        return Ok(());
    }
    if is_daemon_alive(session) {
        return Ok(());
    }
    Err(replacement_refused_error(session, &manifest))
}

#[cfg(unix)]
fn configure_detached_daemon_process(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // The daemon must survive the lifecycle of the foreground CLI command.
    // Creating a new session keeps it out of the parent's process group.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_detached_daemon_process(_cmd: &mut std::process::Command) {}

fn live_daemon_recovery_error(session: Option<&str>, context: &str) -> Box<dyn std::error::Error> {
    let stop_hint = match session {
        Some(name) => format!("gsd-browser --session {name} daemon stop"),
        None => "gsd-browser daemon stop".to_string(),
    };

    format!(
        "{context}. Refusing to replace a live browser session automatically; stop it with `{stop_hint}` and retry"
    )
    .into()
}

/// Check if daemon is alive: PID file exists, process alive, socket connectable.
pub fn is_daemon_alive(session: Option<&str>) -> bool {
    read_daemon_pid(session)
        .map(|pid| nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok())
        .unwrap_or(false)
}

/// Start the daemon process. Spawns the daemon binary in the background and
/// waits for the socket to appear.
pub async fn start_daemon(
    browser_path: Option<&str>,
    cdp_url: Option<&str>,
    session: Option<&str>,
    identity_scope: Option<&str>,
    identity_key: Option<&str>,
    identity_project_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let session = validate_session_name(session)?;

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
    if is_daemon_alive(session) {
        let result = if sock.exists() {
            Ok(())
        } else {
            match wait_for_socket(session, Duration::from_secs(10)).await {
                Ok(()) => Ok(()),
                Err(_) => Err(live_daemon_recovery_error(
                    session,
                    "daemon PID is alive but its socket is unavailable",
                )),
            }
        };

        let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };
        return result;
    }

    refuse_implicit_named_session_replacement(session)?;

    // Clean up stale files
    cleanup_daemon_artifacts(session);

    // Spawn the daemon as a hidden subcommand of the current binary.
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot determine current executable: {e}"))?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("_serve");
    if let Some(path) = browser_path {
        cmd.arg("--browser-path").arg(path);
    }
    if let Some(url) = cdp_url {
        cmd.arg("--cdp-url").arg(url);
    }
    if let Some(name) = session {
        cmd.arg("--session").arg(name);
    }
    if let Some(scope) = identity_scope {
        cmd.arg("--identity-scope").arg(scope);
    }
    if let Some(key) = identity_key {
        cmd.arg("--identity-key").arg(key);
    }
    if let Some(project_id) = identity_project_id {
        cmd.arg("--identity-project").arg(project_id);
    }

    // In debug mode, inherit daemon logs so startup failures are visible.
    cmd.stdin(Stdio::null());
    if std::env::var_os("GSD_BROWSER_DEBUG").is_some() {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    configure_detached_daemon_process(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start daemon ({:?}): {}", exe, e))?;

    // Wait for socket to appear and fail fast if the daemon exits during startup.
    let result = wait_for_spawned_daemon(session, &mut child, Duration::from_secs(10)).await;
    if result.is_err() {
        cleanup_daemon_artifacts(session);
    }

    // Release lock
    let _ = unsafe { libc::flock(fd, libc::LOCK_UN) };

    result
}

#[cfg(test)]
mod tests {
    use super::configure_detached_daemon_process;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    #[cfg(unix)]
    #[test]
    fn detached_daemon_process_starts_in_its_own_session() {
        let parent_sid = unsafe { libc::getsid(0) };
        assert!(parent_sid > 0, "parent session id should be available");

        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        configure_detached_daemon_process(&mut cmd);

        let mut child = cmd.spawn().expect("spawn detached child");
        thread::sleep(Duration::from_millis(50));

        let child_pid = child.id() as libc::pid_t;
        let child_sid = unsafe { libc::getsid(child_pid) };
        let child_pgid = unsafe { libc::getpgid(child_pid) };

        let _ = child.kill();
        let _ = child.wait();

        assert_eq!(
            child_sid, child_pid,
            "detached child should become a session leader"
        );
        assert_eq!(
            child_pgid, child_pid,
            "detached child should become its own process group leader"
        );
        assert_ne!(
            child_sid, parent_sid,
            "detached child should not remain in the parent's session"
        );
    }
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
    let session = validate_session_name(session)?;
    let pid_file = pid_path_for(session);
    if !pid_file.exists() {
        // No PID file — clean up socket if leftover and treat as success
        let _ = fs::remove_file(socket_path_for(session));
        let _ = write_stopped_manifest(session, "daemon stopped");
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
    let _ = write_stopped_manifest(session, "daemon stopped");

    Ok(())
}

pub async fn collect_health(
    session: Option<&str>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let session = validate_session_name(session)?;
    let manifest = load_session_manifest(session)?;
    let pid = read_daemon_pid(session);
    let daemon_alive = is_daemon_alive(session);
    let socket_path = socket_path_for(session);
    let socket_exists = socket_path.exists();
    let socket_connected = if socket_exists {
        timeout(
            Duration::from_millis(300),
            UnixStream::connect(&socket_path),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .is_some()
    } else {
        false
    };

    if socket_connected {
        if let Ok(resp) = send_once("health", json!({}), session).await {
            if let Some(result) = resp.result {
                return Ok(result);
            }
        }
    }

    let mut manifest = manifest.unwrap_or_default();
    manifest.session_name = session.map(str::to_string);
    manifest.socket_path = socket_path.to_string_lossy().to_string();

    let (status, reason) = if daemon_alive && !socket_connected {
        (
            SessionHealthStatus::Degraded,
            "daemon PID is alive but the socket is unavailable".to_string(),
        )
    } else if !daemon_alive && socket_exists {
        (
            SessionHealthStatus::Unhealthy,
            "daemon socket exists without a live daemon PID".to_string(),
        )
    } else if daemon_alive && socket_connected {
        (SessionHealthStatus::Healthy, String::new())
    } else if manifest.health == SessionHealthStatus::Stopped {
        (SessionHealthStatus::Stopped, manifest.health_reason.clone())
    } else if manifest.session_name.is_some() || manifest.daemon_pid.is_some() {
        (
            SessionHealthStatus::Unhealthy,
            "session metadata exists but no live daemon is running".to_string(),
        )
    } else {
        (
            SessionHealthStatus::Stopped,
            "daemon not running".to_string(),
        )
    };

    manifest.health = status;
    if !reason.is_empty() {
        manifest.health_reason = reason.clone();
    }
    manifest.daemon_pid = pid;
    manifest.last_updated_at = Some(now_epoch_secs());
    if status == SessionHealthStatus::Unhealthy || status == SessionHealthStatus::Stopped {
        let _ = save_session_manifest(session, &manifest);
    }

    Ok(json!({
        "session": {
            "name": manifest.session_name,
            "status": manifest.health.as_str(),
            "reason": manifest.health_reason,
            "daemonPid": manifest.daemon_pid,
            "browserPid": manifest.browser_pid,
            "socketPath": manifest.socket_path,
            "manifestPath": manifest_path_for(session).to_string_lossy().to_string(),
            "launchMode": manifest.launch_mode,
            "cdpUrl": manifest.cdp_url,
            "websocketUrl": manifest.websocket_url,
            "browserUserDataDir": manifest.browser_user_data_dir,
            "identityScope": manifest.identity_scope,
            "identityProjectId": manifest.identity_project_id,
            "identityKey": manifest.identity_key.as_ref().map(|_| "<redacted>"),
            "lastHeartbeatAt": manifest.last_heartbeat_at,
            "lastUpdatedAt": manifest.last_updated_at,
            "daemonAlive": daemon_alive,
            "socketConnected": socket_connected,
            "browserConnected": false,
        },
        "activePage": {
            "id": manifest.active_page_id.unwrap_or(0),
            "url": manifest.active_page_url,
            "title": manifest.active_page_title,
        }
    }))
}

/// Send a JSON-RPC request to the daemon. Auto-starts daemon if not running.
pub async fn send_request(
    method: &str,
    params: serde_json::Value,
    browser_path: Option<&str>,
    cdp_url: Option<&str>,
    session: Option<&str>,
) -> Result<DaemonResponse, Box<dyn std::error::Error>> {
    let session = validate_session_name(session)?;
    let identity_scope = std::env::var("GSD_BROWSER_IDENTITY_SCOPE").ok();
    let identity_key = std::env::var("GSD_BROWSER_IDENTITY_KEY").ok();
    let identity_project_id = std::env::var("GSD_BROWSER_IDENTITY_PROJECT").ok();

    // Ensure daemon is running
    if !is_daemon_alive(session) || !socket_path_for(session).exists() {
        start_daemon(
            browser_path,
            cdp_url,
            session,
            identity_scope.as_deref(),
            identity_key.as_deref(),
            identity_project_id.as_deref(),
        )
        .await?;
    }

    // Connect and send
    let result = send_once(method, params.clone(), session).await;

    match result {
        Ok(resp) => Ok(resp),
        Err(err) => {
            if is_daemon_alive(session) {
                return Err(live_daemon_recovery_error(
                    session,
                    &format!("request failed while the daemon PID was still alive: {err}"),
                ));
            }

            // Connection failed and the daemon is gone — restart and retry once.
            refuse_implicit_named_session_replacement(session)?;
            eprintln!("[gsd-browser] daemon connection failed, restarting...");
            cleanup_daemon_artifacts(session);
            start_daemon(
                browser_path,
                cdp_url,
                session,
                identity_scope.as_deref(),
                identity_key.as_deref(),
                identity_project_id.as_deref(),
            )
            .await?;
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
