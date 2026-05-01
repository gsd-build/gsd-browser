pub mod advanced;
pub mod assert_cmd;
pub mod auth_vault;
pub mod batch;
pub mod cloud;
pub mod cloud_manifest;
pub mod cloud_methods;
pub mod codegen;
pub mod device;
pub mod extract;
pub mod forms;
pub mod har;
pub mod inspect;
pub mod intent;
pub mod interaction;
pub mod narration_cmds;
pub mod navigate;
pub mod network_mock;
pub mod pages;
pub mod pdf;
pub mod refs;
pub mod screenshot;
pub mod session;
pub mod state_persist;
pub mod timeline;
pub mod traces;
pub mod visual_diff;
pub mod wait;

/// Extract a clean error message from a chromiumoxide CDP error.
///
/// CDP errors often contain the full `ExceptionDetails` debug struct.
/// This extracts the JS exception description (e.g. "TypeError: Cannot read properties of null")
/// from the raw error string, falling back to the full string if parsing fails.
pub fn clean_cdp_error(err: &impl std::fmt::Display) -> String {
    let raw = err.to_string();

    // Try to extract the description field from ExceptionDetails debug output
    // Pattern: description: Some("Error: ...")
    if let Some(start) = raw.find("description: Some(\"") {
        let after = &raw[start + 19..]; // skip 'description: Some("'
                                        // Find the closing '")' — account for escaped quotes
        let mut end = 0;
        let chars: Vec<char> = after.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' {
                i += 2; // skip escaped char
                continue;
            }
            if chars[i] == '"' {
                end = i;
                break;
            }
            i += 1;
        }
        if end > 0 {
            let desc: String = after[..end].to_string();
            // Trim stack traces — split on literal "\n" (escaped in Debug output) or real newlines
            let first_line = desc
                .split("\\n")
                .next()
                .unwrap_or(&desc)
                .lines()
                .next()
                .unwrap_or(&desc);
            return first_line.to_string();
        }
    }

    // Fallback: if the raw string is short enough, return as-is
    if raw.len() <= 200 {
        return raw;
    }

    // Last resort: truncate the raw error
    format!("{}...", &raw[..200])
}
