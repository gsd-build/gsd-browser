const EMBEDDED: &str = include_str!("../../../assets/viewer.html");

/// Returns the viewer HTML. `GSD_VIEWER_PATH` reads from disk for dev iteration.
pub fn viewer_html() -> String {
    if let Ok(path) = std::env::var("GSD_VIEWER_PATH") {
        if let Ok(bytes) = std::fs::read_to_string(&path) {
            return bytes;
        }
        eprintln!("[view] GSD_VIEWER_PATH set to {path} but could not read; using embedded HTML");
    }
    EMBEDDED.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_returns_nonempty() {
        let html = viewer_html();
        assert!(!html.is_empty());
        assert!(html.contains("<html") || html.contains("<!doctype"));
    }
}
