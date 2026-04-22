use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionHealthStatus {
    Starting,
    Healthy,
    Degraded,
    Recovering,
    Stopped,
    Unhealthy,
}

impl SessionHealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Recovering => "recovering",
            Self::Stopped => "stopped",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl Default for SessionHealthStatus {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub daemon_pid: Option<i32>,
    #[serde(default)]
    pub browser_pid: Option<u32>,
    #[serde(default)]
    pub socket_path: String,
    #[serde(default)]
    pub daemon_started_at: Option<f64>,
    #[serde(default)]
    pub browser_started_at: Option<f64>,
    #[serde(default)]
    pub daemon_version: String,
    #[serde(default)]
    pub launch_mode: String,
    #[serde(default)]
    pub cdp_url: Option<String>,
    #[serde(default)]
    pub websocket_url: Option<String>,
    #[serde(default)]
    pub browser_user_data_dir: Option<String>,
    #[serde(default)]
    pub health: SessionHealthStatus,
    #[serde(default)]
    pub health_reason: String,
    #[serde(default)]
    pub last_heartbeat_at: Option<f64>,
    #[serde(default)]
    pub last_updated_at: Option<f64>,
    #[serde(default)]
    pub active_page_id: Option<u64>,
    #[serde(default)]
    pub active_page_url: String,
    #[serde(default)]
    pub active_page_title: String,
}

fn default_manifest_version() -> u32 {
    1
}

impl Default for SessionManifest {
    fn default() -> Self {
        Self {
            manifest_version: default_manifest_version(),
            session_name: None,
            daemon_pid: None,
            browser_pid: None,
            socket_path: String::new(),
            daemon_started_at: None,
            browser_started_at: None,
            daemon_version: String::new(),
            launch_mode: String::new(),
            cdp_url: None,
            websocket_url: None,
            browser_user_data_dir: None,
            health: SessionHealthStatus::Starting,
            health_reason: String::new(),
            last_heartbeat_at: None,
            last_updated_at: None,
            active_page_id: None,
            active_page_url: String::new(),
            active_page_title: String::new(),
        }
    }
}

pub fn now_epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub fn session_dir_for(session: Option<&str>) -> PathBuf {
    match session {
        Some(name) => crate::state_dir().join("sessions").join(name),
        None => crate::state_dir(),
    }
}

pub fn manifest_path_for(session: Option<&str>) -> PathBuf {
    session_dir_for(session).join("session.json")
}

pub fn load_session_manifest(session: Option<&str>) -> Result<Option<SessionManifest>, String> {
    let path = manifest_path_for(session);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("failed to read session manifest {}: {err}", path.display())),
    };

    let manifest = serde_json::from_str(&contents)
        .map_err(|err| format!("failed to parse session manifest {}: {err}", path.display()))?;
    Ok(Some(manifest))
}

pub fn save_session_manifest(
    session: Option<&str>,
    manifest: &SessionManifest,
) -> Result<(), String> {
    let path = manifest_path_for(session);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create session manifest dir {}: {err}", parent.display()))?;
    }
    let data = serde_json::to_string_pretty(manifest)
        .map_err(|err| format!("failed to serialize session manifest: {err}"))?;
    fs::write(&path, data)
        .map_err(|err| format!("failed to write session manifest {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_strings_are_stable() {
        assert_eq!(SessionHealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(SessionHealthStatus::Unhealthy.as_str(), "unhealthy");
    }

    #[test]
    fn manifest_defaults_are_valid() {
        let manifest = SessionManifest::default();
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.health, SessionHealthStatus::Starting);
    }
}
