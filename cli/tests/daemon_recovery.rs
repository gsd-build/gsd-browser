#![cfg(unix)]

use std::fs;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_home(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    PathBuf::from(format!(
        "/tmp/gb-{test_name}-{}-{nanos}",
        std::process::id()
    ))
}

fn session_dir(home: &Path, session: &str) -> PathBuf {
    home.join(".gsd-browser").join("sessions").join(session)
}

fn spawn_live_pid_process() -> Child {
    Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep process")
}

#[test]
fn request_failure_does_not_replace_live_session_bookkeeping() {
    let home = unique_temp_home("daemon-recovery");
    let session = "live-daemon";
    let session_dir = session_dir(&home, session);
    fs::create_dir_all(&session_dir).expect("create session dir");

    let socket_path = session_dir.join("daemon.sock");
    let pid_path = session_dir.join("daemon.pid");

    let mut live_pid = spawn_live_pid_process();
    fs::write(&pid_path, live_pid.id().to_string()).expect("write pid file");

    let listener = UnixListener::bind(&socket_path).expect("bind dummy daemon socket");
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept client");
        drop(stream);
    });

    let output = Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
        .env("HOME", &home)
        .args([
            "--session",
            session,
            "--browser-path",
            "/definitely/missing/chrome",
            "back",
        ])
        .output()
        .expect("run gsd-browser");

    let _ = server.join();
    let _ = live_pid.kill();
    let _ = live_pid.wait();

    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("daemon connection failed, restarting..."),
        "client should surface the live-daemon failure instead of replacing the session:\n{stderr}"
    );
    assert!(
        socket_path.exists(),
        "live daemon socket bookkeeping should remain intact after request failure"
    );
    assert!(
        pid_path.exists(),
        "live daemon pid bookkeeping should remain intact after request failure"
    );

    let _ = fs::remove_dir_all(&home);
}
