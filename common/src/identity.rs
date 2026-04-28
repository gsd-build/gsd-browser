use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{sanitize_filename, state_dir};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityScope {
    Session,
    Project,
    Global,
}

impl IdentityScope {
    pub fn as_dir(self) -> &'static str {
        match self {
            Self::Session => "sessions",
            Self::Project => "projects",
            Self::Global => "global",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "session" => Ok(Self::Session),
            "project" => Ok(Self::Project),
            "global" => Ok(Self::Global),
            _ => Err(format!("invalid identity scope: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentity {
    pub scope: IdentityScope,
    pub project_id: Option<String>,
    pub key: String,
    pub display_name: String,
}

pub fn identity_profile_dir(
    scope: IdentityScope,
    project_id: Option<&str>,
    key: &str,
) -> Result<PathBuf, String> {
    let safe_key = sanitize_filename(key)?;
    let root = state_dir().join("identities").join(scope.as_dir());
    match scope {
        IdentityScope::Project => {
            let project =
                sanitize_filename(project_id.ok_or("project identity requires project id")?)?;
            Ok(root.join(project).join(safe_key).join("browser-profile"))
        }
        IdentityScope::Global => Ok(root.join(safe_key).join("browser-profile")),
        IdentityScope::Session => Ok(root.join(safe_key).join("browser-profile")),
    }
}

pub fn identity_metadata_path(
    scope: IdentityScope,
    project_id: Option<&str>,
    key: &str,
) -> Result<PathBuf, String> {
    let profile_dir = identity_profile_dir(scope, project_id, key)?;
    Ok(profile_dir
        .parent()
        .ok_or("identity profile path has no parent")?
        .join("identity.json"))
}
