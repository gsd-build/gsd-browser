use gsd_browser_common::viewer::{AnnotationStatus, AnnotationV1};

pub struct AnnotationStore {
    entries: Vec<AnnotationV1>,
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn create(&mut self, annotation: AnnotationV1) -> Result<AnnotationV1, String> {
        let annotation = crate::daemon::view::privacy::redact_annotation(annotation);
        if self
            .entries
            .iter()
            .any(|entry| entry.annotation_id == annotation.annotation_id)
        {
            return Err(format!(
                "annotation already exists: {}",
                annotation.annotation_id
            ));
        }
        self.entries.push(annotation.clone());
        Ok(annotation)
    }

    pub fn list(&self) -> Vec<AnnotationV1> {
        self.entries.clone()
    }

    pub fn get(&self, id: &str) -> Option<AnnotationV1> {
        self.entries
            .iter()
            .find(|entry| entry.annotation_id == id)
            .cloned()
    }

    pub fn clear(&mut self, id: &str) -> bool {
        let len = self.entries.len();
        self.entries.retain(|entry| entry.annotation_id != id);
        self.entries.len() != len
    }

    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    pub fn set_status(
        &mut self,
        id: &str,
        status: AnnotationStatus,
    ) -> Result<AnnotationV1, String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.annotation_id == id)
            .ok_or_else(|| format!("annotation not found: {id}"))?;
        entry.status = status;
        Ok(entry.clone())
    }
}

impl Default for AnnotationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gsd_browser_common::viewer::{AnnotationStatus, AnnotationV1};

    fn minimal_annotation_for_tests(id: &str, note: &str) -> AnnotationV1 {
        AnnotationV1 {
            schema: gsd_browser_common::viewer::ANNOTATION_SCHEMA.to_string(),
            annotation_id: id.to_string(),
            session_id: "sess".to_string(),
            viewer_id: "view".to_string(),
            page_id: Some(1),
            target_id: None,
            frame_id: Some("main".to_string()),
            frame_seq: 1,
            kind: "point".to_string(),
            status: AnnotationStatus::Open,
            note: note.to_string(),
            url: "http://127.0.0.1".to_string(),
            title: "Fixture".to_string(),
            origin: "http://127.0.0.1".to_string(),
            created_by: "user".to_string(),
            created_at_ms: 1,
            viewport: serde_json::json!({"width": 800, "height": 600}),
            selection: serde_json::json!({"coordinateSpace": "viewport_css", "box": {"x": 1, "y": 2, "w": 3, "h": 4}}),
            target: None,
            artifact_refs: serde_json::json!({}),
            partial_reasons: Vec::new(),
            redactions: Vec::new(),
        }
    }

    #[test]
    fn store_creates_and_lists_annotation() {
        let mut store = AnnotationStore::new();
        let annotation = minimal_annotation_for_tests("ann_1", "Make this primary");
        let saved = store.create(annotation).expect("saved");
        assert_eq!(saved.annotation_id, "ann_1");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn resolve_missing_id_returns_error() {
        let mut store = AnnotationStore::new();
        let err = store
            .set_status("missing", AnnotationStatus::Resolved)
            .expect_err("missing");
        assert!(err.contains("annotation not found"));
    }
}
