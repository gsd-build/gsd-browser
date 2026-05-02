use gsd_browser_common::viewer::{BrowserArtifactManifestV1, BROWSER_ARTIFACT_BUNDLE_SCHEMA};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSession {
    pub recording_id: String,
    pub name: String,
    pub started_at_ms: u64,
    pub paused: bool,
}

#[derive(Debug, Clone)]
pub struct RecordingEventInput {
    pub source: String,
    pub owner: String,
    pub kind: String,
    pub url: String,
    pub title: String,
    pub redacted: bool,
}

pub struct RecordingStore {
    root: PathBuf,
    active: Option<RecordingSession>,
    completed: Vec<BrowserArtifactManifestV1>,
    next_seq: u64,
    redaction_hits: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl RecordingStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            active: None,
            completed: Vec::new(),
            next_seq: 1,
            redaction_hits: 0,
        }
    }

    pub fn start(&mut self, name: &str) -> Result<RecordingSession, String> {
        if self.active.is_some() {
            return Err("recording already active".to_string());
        }
        fs::create_dir_all(&self.root)
            .map_err(|err| format!("failed to create recordings root: {err}"))?;
        let recording_id = format!("rec_{}", uuid::Uuid::new_v4());
        let dir = self.root.join(&recording_id);
        for path in [
            dir.clone(),
            dir.join("frames"),
            dir.join("snapshots"),
            dir.join("annotations"),
            dir.join("logs"),
        ] {
            fs::create_dir_all(path)
                .map_err(|err| format!("failed to create recording dir: {err}"))?;
        }
        fs::write(dir.join("events.jsonl"), "")
            .map_err(|err| format!("failed to create events.jsonl: {err}"))?;
        fs::write(dir.join("deltas.json"), "{}")
            .map_err(|err| format!("failed to create deltas.json: {err}"))?;
        let session = RecordingSession {
            recording_id,
            name: name.to_string(),
            started_at_ms: now_ms(),
            paused: false,
        };
        self.active = Some(session.clone());
        Ok(session)
    }

    pub fn pause(&mut self, recording_id: &str) -> Result<RecordingSession, String> {
        let active = self.active.as_mut().ok_or("no active recording")?;
        if active.recording_id != recording_id {
            return Err(format!("recording not active: {recording_id}"));
        }
        active.paused = true;
        Ok(active.clone())
    }

    pub fn resume(&mut self, recording_id: &str) -> Result<RecordingSession, String> {
        let active = self.active.as_mut().ok_or("no active recording")?;
        if active.recording_id != recording_id {
            return Err(format!("recording not active: {recording_id}"));
        }
        active.paused = false;
        Ok(active.clone())
    }

    pub fn record_event(&mut self, input: RecordingEventInput) -> Result<(), String> {
        let Some(active) = self.active.as_ref() else {
            return Ok(());
        };
        if active.paused {
            return Ok(());
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let text = redact_text(&format!("{} {} {}", input.kind, input.url, input.title));
        if text != format!("{} {} {}", input.kind, input.url, input.title) || input.redacted {
            self.redaction_hits += 1;
        }
        let event = json!({
            "seq": seq,
            "timestampMs": now_ms(),
            "schema": "BrowserEventV1",
            "recordingId": active.recording_id,
            "source": input.source,
            "owner": input.owner,
            "controlVersion": 0,
            "frameSeq": 0,
            "kind": input.kind,
            "url": redact_text(&input.url),
            "title": redact_text(&input.title),
            "origin": origin_from_url(&input.url),
            "before": {},
            "after": {},
            "redaction": { "status": if input.redacted { "redacted" } else { "none" } },
            "artifactRefs": {},
        });
        let line = serde_json::to_string(&event).map_err(|err| err.to_string())?;
        let path = self.root.join(&active.recording_id).join("events.jsonl");
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|err| format!("failed to open events.jsonl: {err}"))?;
        writeln!(file, "{line}").map_err(|err| format!("failed to append event: {err}"))?;
        Ok(())
    }

    pub fn stop(&mut self, recording_id: &str) -> Result<BrowserArtifactManifestV1, String> {
        let active = self.active.take().ok_or("no active recording")?;
        if active.recording_id != recording_id {
            self.active = Some(active);
            return Err(format!("recording not active: {recording_id}"));
        }
        let event_count =
            count_jsonl_lines(&self.root.join(&active.recording_id).join("events.jsonl"))?;
        let manifest = BrowserArtifactManifestV1 {
            schema: BROWSER_ARTIFACT_BUNDLE_SCHEMA.to_string(),
            recording_id: active.recording_id.clone(),
            session_id: "default".to_string(),
            name: active.name,
            started_at_ms: active.started_at_ms,
            stopped_at_ms: Some(now_ms()),
            start_seq: 1,
            stop_seq: Some(event_count),
            event_count,
            frame_count: 0,
            annotation_count: 0,
            console_error_count: 0,
            failed_request_count: 0,
            origin_scopes: Vec::new(),
            excluded_boundary_events: Vec::new(),
            redaction: json!({
                "policy": "default-sensitive",
                "hitCount": self.redaction_hits,
                "classes": ["email", "query_token", "bearer_token", "data_token"]
            }),
            artifacts: json!({
                "events": "events.jsonl",
                "frames": "frames/",
                "annotations": "annotations/",
                "console": "logs/console.jsonl",
                "network": "logs/network.jsonl",
                "dialog": "logs/dialog.jsonl",
                "deltas": "deltas.json"
            }),
            hashes: json!({}),
        };
        let dir = self.root.join(&manifest.recording_id);
        let data = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
        fs::write(dir.join("manifest.json"), data)
            .map_err(|err| format!("failed to write manifest: {err}"))?;
        self.completed.push(manifest.clone());
        Ok(manifest)
    }

    pub fn list(&self) -> Vec<BrowserArtifactManifestV1> {
        self.completed.clone()
    }

    pub fn get(&self, recording_id: &str) -> Option<BrowserArtifactManifestV1> {
        self.completed
            .iter()
            .find(|manifest| manifest.recording_id == recording_id)
            .cloned()
    }

    pub fn active_id(&self) -> Option<String> {
        self.active
            .as_ref()
            .map(|recording| recording.recording_id.clone())
    }

    pub fn export(&self, recording_id: &str, output: &Path) -> Result<PathBuf, String> {
        let src = self.root.join(recording_id);
        if !src.exists() {
            return Err(format!("recording not found: {recording_id}"));
        }
        fs::create_dir_all(output).map_err(|err| format!("failed to create export dir: {err}"))?;
        Ok(src)
    }

    pub fn discard(&mut self, recording_id: &str) -> Result<bool, String> {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.recording_id == recording_id)
        {
            self.active = None;
        }
        let path = self.root.join(recording_id);
        if path.exists() {
            fs::remove_dir_all(path)
                .map_err(|err| format!("failed to discard recording: {err}"))?;
            self.completed
                .retain(|manifest| manifest.recording_id != recording_id);
            return Ok(true);
        }
        Ok(false)
    }
}

pub fn redact_text(text: &str) -> String {
    let mut value = text.to_string();
    let rules = [
        (
            r"(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}",
            "[redacted:email]",
        ),
        (
            r"(?i)bearer\s+[A-Za-z0-9._~+/=-]+",
            "bearer [redacted:token]",
        ),
        (
            r"(?i)(token|secret|key|code|otp)=([^&\s]+)",
            "[redacted:token]",
        ),
        (
            r#"(?i)data-token=["'][^"']+["']"#,
            "data-token=\"[redacted:token]\"",
        ),
        (r"[A-Za-z0-9_-]{32,}", "[redacted:token]"),
    ];
    for (pattern, replacement) in rules {
        let regex = Regex::new(pattern).expect("valid redaction regex");
        value = regex.replace_all(&value, replacement).to_string();
    }
    value
}

pub fn validate_recording_bundle(path: &Path) -> Result<serde_json::Value, String> {
    let manifest_path = path.join("manifest.json");
    let events_path = path.join("events.jsonl");
    if !manifest_path.exists() {
        return Err("missing manifest.json".to_string());
    }
    if !events_path.exists() {
        return Err("missing events.jsonl".to_string());
    }
    let manifest: BrowserArtifactManifestV1 =
        serde_json::from_str(&fs::read_to_string(&manifest_path).map_err(|err| err.to_string())?)
            .map_err(|err| format!("malformed manifest: {err}"))?;
    let mut last_seq = 0;
    for line in fs::read_to_string(events_path)
        .map_err(|err| err.to_string())?
        .lines()
    {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|err| format!("malformed JSONL: {err}"))?;
        let seq = value
            .get("seq")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if seq <= last_seq {
            return Err("event sequence gap or duplicate".to_string());
        }
        last_seq = seq;
        let serialized = value.to_string();
        if serialized.contains("bearer ") || serialized.contains("token=secret") {
            return Err("unredacted token pattern".to_string());
        }
    }
    Ok(json!({
        "ok": true,
        "recordingId": manifest.recording_id,
        "eventCount": manifest.event_count,
        "redaction": manifest.redaction
    }))
}

fn count_jsonl_lines(path: &Path) -> Result<u64, String> {
    let data = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(data.lines().filter(|line| !line.trim().is_empty()).count() as u64)
}

fn origin_from_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    let host = rest.split('/').next().unwrap_or_default();
    format!("{scheme}://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn recording_writes_manifest_and_events() {
        let dir = tempdir().expect("tempdir");
        let mut store = RecordingStore::new(dir.path().to_path_buf());
        let rec = store.start("checkout-bug").expect("started");
        store
            .record_event(RecordingEventInput {
                source: "viewer".to_string(),
                owner: "user".to_string(),
                kind: "recording.start".to_string(),
                url: "http://127.0.0.1".to_string(),
                title: "Fixture".to_string(),
                redacted: false,
            })
            .expect("event");
        let manifest = store.stop(&rec.recording_id).expect("stopped");
        assert_eq!(manifest.event_count, 1);
        assert!(dir
            .path()
            .join(&rec.recording_id)
            .join("manifest.json")
            .exists());
        assert!(dir
            .path()
            .join(&rec.recording_id)
            .join("events.jsonl")
            .exists());
    }

    #[test]
    fn redaction_scrubs_tokens_from_event_text() {
        let scrubbed =
            redact_text("email lex@example.com bearer abc.def token=secret data-token=\"abc\"");
        assert!(!scrubbed.contains("lex@example.com"));
        assert!(!scrubbed.contains("secret"));
        assert!(scrubbed.contains("[redacted:email]"));
        assert!(scrubbed.contains("[redacted:token]"));
    }
}
