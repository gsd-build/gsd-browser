pub mod chrome;
pub mod config;
pub mod ipc;
pub mod types;

use serde::{Deserialize, Serialize};

// ── JSON-RPC 2.0 Types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── Error Codes (JSON-RPC 2.0 spec) ──

pub const ERR_INVALID_REQUEST: i32 = -32600;
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INTERNAL: i32 = -32603;

// ── Helpers ──

impl DaemonRequest {
    pub fn new(id: u64, method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }
}

impl DaemonResponse {
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn error_with_data(
        id: u64,
        code: i32,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: Some(data),
            }),
        }
    }
}

// ── Paths ──

/// Returns the directory for gsd-browser state files (~/.gsd-browser)
pub fn state_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".gsd-browser")
}

pub fn socket_path() -> std::path::PathBuf {
    state_dir().join("daemon.sock")
}

pub fn pid_path() -> std::path::PathBuf {
    state_dir().join("daemon.pid")
}

pub fn lock_path() -> std::path::PathBuf {
    state_dir().join("daemon.lock")
}

/// Session-aware socket path. When session is Some, uses
/// `~/.gsd-browser/sessions/<name>/daemon.sock`.
pub fn socket_path_for(session: Option<&str>) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        match session {
            Some(name) => state_dir().join("sessions").join(name).join("daemon.sock"),
            None => socket_path(),
        }
    }

    #[cfg(windows)]
    {
        let name = session.unwrap_or("default");
        std::path::PathBuf::from(format!(r"\\.\pipe\gsd-browser-{}", name))
    }
}

/// Session-aware PID path. When session is Some, uses
/// `~/.gsd-browser/sessions/<name>/daemon.pid`.
pub fn pid_path_for(session: Option<&str>) -> std::path::PathBuf {
    match session {
        Some(name) => state_dir().join("sessions").join(name).join("daemon.pid"),
        None => pid_path(),
    }
}

/// Session-aware lock path.
pub fn lock_path_for(session: Option<&str>) -> std::path::PathBuf {
    match session {
        Some(name) => state_dir().join("sessions").join(name).join("daemon.lock"),
        None => lock_path(),
    }
}

/// Session-aware Chrome user-data-dir.  Each daemon session gets its own
/// Chrome profile so it never collides with the user's personal browser or
/// other daemon sessions.  Lives under `~/.gsd-browser/chrome-profiles/<name>/`.
pub fn chrome_data_dir_for(session: Option<&str>) -> std::path::PathBuf {
    let name = session.unwrap_or("default");
    state_dir().join("chrome-profiles").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn socket_path_for_none_uses_default_on_windows() {
        let path = socket_path_for(None);
        let s = path.to_string_lossy();
        assert!(
            s.starts_with(r"\\.\pipe\gsd-browser-"),
            "expected named pipe prefix, got: {}",
            s
        );
        assert!(
            s.ends_with("default"),
            "expected 'default' session name, got: {}",
            s
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn socket_path_for_named_session_on_windows() {
        let path = socket_path_for(Some("staging"));
        assert_eq!(
            path.to_string_lossy(),
            r"\\.\pipe\gsd-browser-staging",
            "named session should produce exact named pipe path"
        );
    }
}
