use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn click_produces_intent_then_complete() {
    use futures_util::StreamExt;

    let view_url = std::env::var("GSD_BROWSER_VIEW_URL")
        .expect("GSD_BROWSER_VIEW_URL must point at a running `gsd-browser view` server");
    let ws_url = view_url.replacen("http://", "ws://", 1) + "/ws";
    let session = std::env::var("GSD_BROWSER_TEST_SESSION").ok();

    let resp = reqwest::get(format!("{view_url}/health")).await.unwrap();
    assert!(resp.status().is_success());

    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();
    let _snapshot = ws.next().await.unwrap().unwrap();

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_gsd-browser"));
    if let Some(session) = session {
        command.args(["--session", &session]);
    }
    let output = command
        .args(["--no-narration-delay", "click", "h1", "--json"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "click command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut got_intent = false;
    let mut got_complete = false;
    let mut seen_types = Vec::new();
    let collect = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg) = ws.next().await {
            let msg = msg.unwrap();
            if let tokio_tungstenite::tungstenite::Message::Text(s) = msg {
                let v: serde_json::Value = serde_json::from_str(&s).unwrap();
                let ty = v
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("<missing>")
                    .to_string();
                seen_types.push(ty.clone());
                match ty.as_str() {
                    "intent" => got_intent = true,
                    "complete" => {
                        got_complete = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await;

    assert!(
        collect.is_ok(),
        "timed out collecting narration events; saw {seen_types:?}"
    );
    assert!(
        got_intent,
        "did not receive intent event; saw {seen_types:?}"
    );
    assert!(
        got_complete,
        "did not receive complete event; saw {seen_types:?}"
    );
}
