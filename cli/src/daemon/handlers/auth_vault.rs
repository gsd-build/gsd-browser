use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use chromiumoxide::Page;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tracing::info;

/// Salt for Argon2 KDF — fixed per installation, stored alongside vault.
const ARGON2_SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

fn vault_dir() -> PathBuf {
    gsd_browser_common::state_dir().join("vault")
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultEntry {
    url: String,
    username: String,
    password: String,
    #[serde(default)]
    extra_fields: Value,
}

/// On-disk format: [16-byte salt][12-byte nonce][ciphertext...]
#[derive(Debug)]
struct EncryptedBlob {
    salt: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl EncryptedBlob {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ARGON2_SALT_LEN + NONCE_LEN + self.ciphertext.len());
        buf.extend_from_slice(&self.salt);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.ciphertext);
        buf
    }

    fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < ARGON2_SALT_LEN + NONCE_LEN + 1 {
            return Err("encrypted blob too short".to_string());
        }
        let salt = data[..ARGON2_SALT_LEN].to_vec();
        let nonce = data[ARGON2_SALT_LEN..ARGON2_SALT_LEN + NONCE_LEN].to_vec();
        let ciphertext = data[ARGON2_SALT_LEN + NONCE_LEN..].to_vec();
        Ok(Self {
            salt,
            nonce,
            ciphertext,
        })
    }
}

fn get_vault_key() -> Result<String, String> {
    std::env::var("GSD_BROWSER_VAULT_KEY").map_err(|_| {
        "GSD_BROWSER_VAULT_KEY environment variable not set. \
         Set it to a passphrase to encrypt/decrypt the auth vault."
            .to_string()
    })
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| format!("argon2 KDF failed: {e}"))?;
    Ok(key)
}

fn encrypt(plaintext: &[u8], passphrase: &str) -> Result<EncryptedBlob, String> {
    let mut salt = vec![0u8; ARGON2_SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("create cipher: {e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("encrypt failed: {e}"))?;

    Ok(EncryptedBlob {
        salt,
        nonce: nonce_bytes.to_vec(),
        ciphertext,
    })
}

fn decrypt(blob: &EncryptedBlob, passphrase: &str) -> Result<Vec<u8>, String> {
    let key = derive_key(passphrase, &blob.salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("create cipher: {e}"))?;

    let nonce = Nonce::from_slice(&blob.nonce);
    cipher
        .decrypt(nonce, blob.ciphertext.as_ref())
        .map_err(|e| format!("decrypt failed (wrong key?): {e}"))
}

pub async fn handle_vault_save(_page: &Page, params: &Value) -> Result<Value, String> {
    let passphrase = get_vault_key()?;

    let profile = params
        .get("profile")
        .and_then(|v| v.as_str())
        .ok_or("missing 'profile' parameter")?;
    let profile = gsd_browser_common::sanitize_filename(profile)?;
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("missing 'url' parameter")?;
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or("missing 'username' parameter")?;
    let password = params
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or("missing 'password' parameter")?;
    let extra_fields = params.get("extra_fields").cloned().unwrap_or(json!({}));

    let entry = VaultEntry {
        url: url.to_string(),
        username: username.to_string(),
        password: password.to_string(),
        extra_fields,
    };

    let plaintext =
        serde_json::to_vec(&entry).map_err(|e| format!("serialize vault entry: {e}"))?;
    let blob = encrypt(&plaintext, &passphrase)?;

    let dir = vault_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create vault dir: {e}"))?;

    let file_path = dir.join(format!("{profile}.enc"));
    fs::write(&file_path, blob.to_bytes()).map_err(|e| format!("write vault file: {e}"))?;

    info!(
        "[auth_vault] saved profile '{}' for {} (url: {})",
        profile, username, url
    );

    // NEVER return password in response
    Ok(json!({
        "profile": profile,
        "url": url,
        "username": username,
        "saved": true,
    }))
}

pub async fn handle_vault_login(
    page: &Page,
    params: &Value,
    state: &super::super::state::DaemonState,
) -> Result<Value, String> {
    let passphrase = get_vault_key()?;

    let profile = params
        .get("profile")
        .and_then(|v| v.as_str())
        .ok_or("missing 'profile' parameter")?;
    let profile = gsd_browser_common::sanitize_filename(profile)?;

    let file_path = vault_dir().join(format!("{profile}.enc"));
    if !file_path.exists() {
        return Err(format!(
            "vault profile not found: {profile} (looked in {:?})",
            vault_dir()
        ));
    }

    let data = fs::read(&file_path).map_err(|e| format!("read vault file: {e}"))?;
    let blob = EncryptedBlob::from_bytes(&data)?;
    let plaintext = decrypt(&blob, &passphrase)?;
    let entry: VaultEntry =
        serde_json::from_slice(&plaintext).map_err(|e| format!("parse vault entry: {e}"))?;

    // 1. Navigate to the URL
    let nav_params = json!({"url": entry.url});
    super::navigate::handle_navigate(page, &nav_params, state).await?;

    // 2. Fill form with credentials
    // If extra_fields has field_mappings, use those to map form fields to credentials.
    // Otherwise, try common field names (username, email, password).
    let fill_values = if let Some(mappings) = entry.extra_fields.get("field_mappings") {
        if let Value::Object(map) = mappings {
            let mut vals = serde_json::Map::new();
            for (field_id, field_key) in map {
                let value = match field_key.as_str() {
                    Some("username") => entry.username.as_str(),
                    Some("password") => entry.password.as_str(),
                    _ => continue,
                };
                vals.insert(field_id.clone(), json!(value));
            }
            Value::Object(vals)
        } else {
            json!({
                "username": entry.username,
                "email": entry.username,
                "password": entry.password,
            })
        }
    } else {
        json!({
            "username": entry.username,
            "email": entry.username,
            "password": entry.password,
        })
    };

    let fill_params = json!({
        "values": fill_values,
        "submit": true,
    });

    match super::forms::handle_fill_form(page, &fill_params).await {
        Ok(_) => {
            info!("[auth_vault] logged in with profile '{}'", profile);
        }
        Err(e) => {
            return Err(format!(
                "vault login: navigated to {} but fill_form failed: {e}. \
                 Use extra_fields.field_mappings to map field identifiers.",
                entry.url
            ));
        }
    }

    // NEVER expose password in response
    Ok(json!({
        "profile": profile,
        "url": entry.url,
        "username": entry.username,
        "logged_in": true,
    }))
}

pub async fn handle_vault_list(_page: &Page, _params: &Value) -> Result<Value, String> {
    let dir = vault_dir();
    if !dir.exists() {
        return Ok(json!({
            "profiles": [],
            "count": 0,
        }));
    }

    let mut profiles = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("read vault dir: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".enc") {
            let profile_name = name_str.trim_end_matches(".enc").to_string();
            profiles.push(json!(profile_name));
        }
    }

    let count = profiles.len();
    info!("[auth_vault] listed {} vault profile(s)", count);

    // NEVER return credentials, only profile names
    Ok(json!({
        "profiles": profiles,
        "count": count,
    }))
}
