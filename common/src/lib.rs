pub mod chrome;
pub mod cloud;
pub mod config;
pub mod identity;
pub mod ipc;
pub mod session;
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

// ── Input Sanitization ──

/// Validates that a user-supplied name is safe for use as a filename component.
/// Rejects path traversal attempts and OS-unsafe characters.
pub fn sanitize_filename(name: &str) -> Result<&str, String> {
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
        || name.contains('\0')
        || name.to_ascii_lowercase().contains("%2f")
        || name.to_ascii_lowercase().contains("%5c")
        || name == "."
    {
        return Err(format!(
            "invalid name: must not contain path separators, '..', ':', encoded separators, or null bytes — got '{name}'"
        ));
    }
    Ok(name)
}

pub fn validate_session_name(session: Option<&str>) -> Result<Option<&str>, String> {
    match session {
        Some(name) => Ok(Some(sanitize_filename(name)?)),
        None => Ok(None),
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

const MAX_UNIX_SOCKET_PATH_LEN: usize = 100;

fn stable_socket_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn shortened_socket_path_for(session: Option<&str>) -> std::path::PathBuf {
    let session_key = session.unwrap_or("default");
    let identity = format!("{}::{session_key}", state_dir().display());
    std::env::temp_dir()
        .join("gsd-browser-sockets")
        .join(format!("{}.sock", stable_socket_hash(&identity)))
}

/// Session-aware socket path. When session is Some, uses
/// `~/.gsd-browser/sessions/<name>/daemon.sock` when it fits within the
/// platform socket-path limit, otherwise a stable shortened temp path.
pub fn socket_path_for(session: Option<&str>) -> std::path::PathBuf {
    let candidate = match session {
        Some(name) => state_dir().join("sessions").join(name).join("daemon.sock"),
        None => socket_path(),
    };

    if candidate.display().to_string().len() >= MAX_UNIX_SOCKET_PATH_LEN {
        shortened_socket_path_for(session)
    } else {
        candidate
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
