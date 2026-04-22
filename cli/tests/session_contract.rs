#![cfg(unix)]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

fn browser_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct BrowserTestEnv {
    home: PathBuf,
    session: String,
}

impl BrowserTestEnv {
    fn new(name: &str) -> Self {
        Self {
            home: unique_temp_home(name),
            session: format!("{name}-{}", std::process::id()),
        }
    }

    fn output(&self, args: &[String]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
            .env("HOME", &self.home)
            .args(["--session", &self.session])
            .args(args)
            .output()
            .expect("run gsd-browser")
    }

    fn json(&self, args: &[String]) -> Value {
        let mut full_args = vec!["--json".to_string()];
        full_args.extend(args.iter().cloned());
        let output = self.output(&full_args);
        assert!(
            output.status.success(),
            "command failed: args={full_args:?}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("parse JSON output")
    }

    fn stop(&self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
            .env("HOME", &self.home)
            .args(["--session", &self.session, "daemon", "stop"])
            .output();
    }
}

impl Drop for BrowserTestEnv {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[derive(Clone)]
struct ResponseSpec {
    status: u16,
    content_type: &'static str,
    body: String,
}

struct TestServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(routes: HashMap<String, ResponseSpec>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("set listener nonblocking");
        let addr = listener.local_addr().expect("listener addr");
        let routes = Arc::new(routes);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => handle_connection(stream, &routes),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            stop,
            handle: Some(handle),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(mut stream: TcpStream, routes: &HashMap<String, ResponseSpec>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let raw_path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header == "\r\n" => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let response = routes.get(path).cloned().unwrap_or(ResponseSpec {
        status: 404,
        content_type: "text/plain; charset=utf-8",
        body: "not found".to_string(),
    });
    let reason = match response.status {
        200 => "OK",
        404 => "Not Found",
        _ => "OK",
    };
    let bytes = response.body.as_bytes();
    let _ = write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        bytes.len()
    );
    let _ = stream.write_all(bytes);
    let _ = stream.flush();
}

fn main_page_html(cross_origin_base: &str) -> String {
    format!(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Session Contract</title>
    <style>
      body {{ font-family: sans-serif; }}
      #overlay {{ border: 1px solid #999; padding: 12px; max-width: 280px; }}
      #far-target {{ display: inline-block; margin-top: 2400px; }}
    </style>
  </head>
  <body>
    <button id="main-target">Main CTA</button>
    <div id="shadow-host"></div>
    <div id="shadow-announcement"></div>
    <div id="shadow-select-announcement"></div>
    <iframe id="same-origin-frame" name="same-origin-frame" src="/frame.html"></iframe>
    <iframe id="cross-origin-frame" name="cross-origin-frame" src="{cross_origin_base}/cross.html"></iframe>
    <div id="overlay" role="dialog" aria-modal="true">
      <button id="overlay-close">Dismiss</button>
      <p>Overlay prompt</p>
    </div>
    <button id="far-target">Far CTA</button>
    <div id="dynamic-status"></div>
    <script>
      const host = document.getElementById('shadow-host');
      const shadow = host.attachShadow({{ mode: 'open' }});
      shadow.innerHTML = `
        <button id="shadow-target">Shadow CTA</button>
        <input id="shadow-input" aria-label="Shadow Input" />
        <select id="shadow-select" aria-label="Shadow Select">
          <option value="alpha">Alpha</option>
          <option value="beta">Beta</option>
        </select>
        <div id="shadow-status"></div>
      `;
      shadow.getElementById('shadow-target').addEventListener('click', () => {{
        shadow.getElementById('shadow-status').textContent = 'Shadow clicked';
        document.getElementById('shadow-announcement').textContent = 'Shadow clicked';
      }});
      shadow.getElementById('shadow-select').addEventListener('change', (event) => {{
        const label = event.target.options[event.target.selectedIndex].textContent.trim();
        document.getElementById('shadow-select-announcement').textContent = 'Shadow select ' + label;
      }});

      setTimeout(() => {{
        document.getElementById('dynamic-status').textContent = 'Dynamic Ready';
      }}, 300);
    </script>
  </body>
</html>"#
    )
}

fn frame_page_html() -> String {
    r#"<!doctype html>
<html>
  <body>
    <button id="frame-target">Frame CTA</button>
    <input id="frame-input" aria-label="Frame Input" />
    <button id="frame-hover">Frame Hover</button>
    <label><input id="frame-check" type="checkbox" /> Frame Check</label>
    <div id="frame-status"></div>
    <script>
      document.getElementById('frame-target').addEventListener('click', () => {
        document.getElementById('frame-status').textContent = 'Frame clicked';
      });
      document.getElementById('frame-input').addEventListener('input', (event) => {
        document.getElementById('frame-status').textContent = event.target.value;
      });
      document.getElementById('frame-hover').addEventListener('mouseover', () => {
        document.getElementById('frame-status').textContent = 'Frame hover';
      });
      document.getElementById('frame-check').addEventListener('change', (event) => {
        document.getElementById('frame-status').textContent = 'Frame checked ' + event.target.checked;
      });
    </script>
  </body>
</html>"#
        .to_string()
}

fn cross_origin_html() -> String {
    r#"<!doctype html><html><body><button id="cross-target">Cross CTA</button></body></html>"#
        .to_string()
}

fn test_servers() -> (TestServer, TestServer) {
    let cross_server = TestServer::start(HashMap::from([(
        "/cross.html".to_string(),
        ResponseSpec {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: cross_origin_html(),
        },
    )]));

    let main_server = TestServer::start(HashMap::from([
        (
            "/".to_string(),
            ResponseSpec {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: main_page_html(&cross_server.base_url()),
            },
        ),
        (
            "/frame.html".to_string(),
            ResponseSpec {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: frame_page_html(),
            },
        ),
    ]));

    (main_server, cross_server)
}

fn wait_until_ready(env: &BrowserTestEnv, url: &str) {
    let _ = env.json(&["navigate".to_string(), url.to_string()]);
    let _ = env.json(&[
        "set-viewport".to_string(),
        "--width".to_string(),
        "1920".to_string(),
        "--height".to_string(),
        "1080".to_string(),
    ]);
    let _ = env.json(&[
        "wait-for".to_string(),
        "--condition".to_string(),
        "text_visible".to_string(),
        "--value".to_string(),
        "Dynamic Ready".to_string(),
        "--timeout".to_string(),
        "5000".to_string(),
    ]);
}

fn ref_id_by_name(snapshot: &Value, name: &str) -> String {
    snapshot["refs"]
        .as_object()
        .expect("snapshot refs object")
        .iter()
        .find_map(|(key, value)| {
            (value.get("name").and_then(|value| value.as_str()) == Some(name))
                .then(|| format!("@v{}:{key}", snapshot["version"].as_u64().unwrap_or(0)))
        })
        .unwrap_or_else(|| panic!("missing ref for {name}"))
}

#[test]
fn navigate_then_session_summary_and_health_preserve_browser_state() {
    let _guard = browser_test_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let env = BrowserTestEnv::new("session-summary");
    let (main_server, _cross_server) = test_servers();
    let url = main_server.base_url();

    wait_until_ready(&env, &url);

    let summary = env.json(&["session-summary".to_string()]);
    assert_eq!(summary["session"]["status"], "healthy");
    assert_eq!(summary["activePage"]["title"], "Session Contract");
    assert!(
        summary["activePage"]["url"]
            .as_str()
            .unwrap_or_default()
            .starts_with(&url),
        "unexpected active URL: {}",
        summary["activePage"]["url"]
    );

    let health = env.json(&["daemon".to_string(), "health".to_string()]);
    assert_eq!(health["session"]["status"], "healthy");
    assert_eq!(health["activePage"]["title"], "Session Contract");
    assert!(
        health["activePage"]["url"]
            .as_str()
            .unwrap_or_default()
            .starts_with(&url),
        "unexpected health URL: {}",
        health["activePage"]["url"]
    );

    let eval = env.json(&[
        "eval".to_string(),
        "JSON.stringify({shadow: document.querySelector('#shadow-host').shadowRoot.querySelector('#shadow-target').textContent.trim(), frame: document.querySelector('#same-origin-frame').contentWindow.document.querySelector('#frame-target').textContent.trim(), dynamic: document.getElementById('dynamic-status').textContent.trim()})".to_string(),
    ]);
    let eval_result: Value =
        serde_json::from_str(eval["result"].as_str().unwrap_or("{}")).expect("parse eval result");
    assert_eq!(eval_result["shadow"], "Shadow CTA");
    assert_eq!(eval_result["frame"], "Frame CTA");
    assert_eq!(eval_result["dynamic"], "Dynamic Ready");

    env.stop();
}

#[test]
fn unified_inspection_and_ref_actions_cover_shadow_iframe_dynamic_long_page_and_boundaries() {
    let _guard = browser_test_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let env = BrowserTestEnv::new("inspection-contract");
    let (main_server, _cross_server) = test_servers();
    let url = main_server.base_url();

    wait_until_ready(&env, &url);

    let snapshot = env.json(&[
        "snapshot".to_string(),
        "--limit".to_string(),
        "40".to_string(),
    ]);
    let ref_names: Vec<String> = snapshot["refs"]
        .as_object()
        .expect("snapshot refs object")
        .values()
        .filter_map(|value| value.get("name").and_then(|value| value.as_str()))
        .map(String::from)
        .collect();

    assert!(ref_names.iter().any(|value| value == "Shadow CTA"));
    assert!(ref_names.iter().any(|value| value == "Frame CTA"));
    assert!(ref_names.iter().any(|value| value == "Far CTA"));
    assert!(ref_names.iter().any(|value| value == "Dismiss"));

    let boundaries = snapshot["boundaries"].as_array().cloned().unwrap_or_default();
    assert!(
        boundaries.iter().any(|boundary| {
            boundary["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("cross-origin frame")
        }),
        "snapshot did not expose cross-origin frame boundary: {boundaries:?}"
    );

    let find_shadow = env.json(&[
        "find".to_string(),
        "--role".to_string(),
        "button".to_string(),
        "--text".to_string(),
        "Shadow CTA".to_string(),
    ]);
    assert_eq!(find_shadow["count"], 1);
    assert_eq!(find_shadow["elements"][0]["name"], "Shadow CTA");

    let find_frame = env.json(&[
        "find".to_string(),
        "--role".to_string(),
        "button".to_string(),
        "--text".to_string(),
        "Frame CTA".to_string(),
    ]);
    assert_eq!(find_frame["count"], 1);
    assert_eq!(find_frame["elements"][0]["name"], "Frame CTA");

    let find_far = env.json(&[
        "find".to_string(),
        "--role".to_string(),
        "button".to_string(),
        "--text".to_string(),
        "Far CTA".to_string(),
    ]);
    assert_eq!(find_far["count"], 1);
    assert_eq!(find_far["elements"][0]["name"], "Far CTA");

    let checks = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Dynamic Ready" },
        { "kind": "text_visible", "text": "Overlay prompt" },
        { "kind": "selector_visible", "selector": "#shadow-target" },
        { "kind": "selector_visible", "selector": "#frame-target" },
        { "kind": "selector_visible", "selector": "#far-target" }
    ]))
    .expect("serialize checks");
    let asserted = env.json(&["assert".to_string(), "--checks".to_string(), checks]);
    assert_eq!(asserted["verified"], true);

    let direct_click = env.json(&["click".to_string(), "#shadow-target".to_string()]);
    assert_eq!(direct_click["clicked"]["frameLabel"], "main");

    let shadow_click_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Shadow clicked" }
    ]))
    .expect("serialize direct shadow click assert");
    let shadow_click_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        shadow_click_assert,
    ]);
    assert_eq!(shadow_click_result["verified"], true);

    let direct_type = env.json(&[
        "type".to_string(),
        "#frame-input".to_string(),
        "Direct typed".to_string(),
        "--clear-first".to_string(),
    ]);
    assert_eq!(direct_type["typed"]["frameLabel"], "same-origin-frame");

    let direct_type_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Direct typed" }
    ]))
    .expect("serialize direct type assert");
    let direct_type_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        direct_type_assert,
    ]);
    assert_eq!(direct_type_result["verified"], true);

    let direct_hover = env.json(&["hover".to_string(), "#frame-hover".to_string()]);
    assert_eq!(direct_hover["hovered"]["frameLabel"], "same-origin-frame");

    let hover_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Frame hover" }
    ]))
    .expect("serialize hover assert");
    let hover_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        hover_assert,
    ]);
    assert_eq!(hover_result["verified"], true);

    let direct_select = env.json(&[
        "select-option".to_string(),
        "#shadow-select".to_string(),
        "Beta".to_string(),
    ]);
    assert_eq!(direct_select["selected"]["frameLabel"], "main");

    let select_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Shadow select Beta" }
    ]))
    .expect("serialize select assert");
    let select_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        select_assert,
    ]);
    assert_eq!(select_result["verified"], true);

    let direct_check = env.json(&[
        "set-checked".to_string(),
        "#frame-check".to_string(),
        "--checked".to_string(),
    ]);
    assert_eq!(direct_check["checked"]["frameLabel"], "same-origin-frame");

    let checked_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Frame checked true" }
    ]))
    .expect("serialize checked assert");
    let checked_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        checked_assert,
    ]);
    assert_eq!(checked_result["verified"], true);

    let shadow_ref = ref_id_by_name(&snapshot, "Shadow CTA");
    let frame_input_ref = ref_id_by_name(&snapshot, "Frame Input");

    let clicked = env.json(&["click-ref".to_string(), shadow_ref.clone()]);
    assert_eq!(clicked["ref_resolution"]["frameLabel"], "main");

    let shadow_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Shadow clicked" }
    ]))
    .expect("serialize shadow assert");
    let shadow_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        shadow_assert,
    ]);
    assert_eq!(shadow_result["verified"], true);

    let filled = env.json(&[
        "fill-ref".to_string(),
        frame_input_ref.clone(),
        "Frame typed".to_string(),
    ]);
    assert_eq!(
        filled["ref_resolution"]["frameLabel"],
        "same-origin-frame"
    );

    let frame_assert = serde_json::to_string(&json!([
        { "kind": "text_visible", "text": "Frame typed" }
    ]))
    .expect("serialize frame assert");
    let frame_result = env.json(&[
        "assert".to_string(),
        "--checks".to_string(),
        frame_assert,
    ]);
    assert_eq!(frame_result["verified"], true);

    env.stop();
}
