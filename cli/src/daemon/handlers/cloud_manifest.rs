use gsd_browser_common::cloud::{
    CloudIdentityCapabilities, CloudInputCapabilities, CloudToolManifest, CloudToolManifestMethod,
    CLOUD_INPUT_COORDINATE_SPACE, CLOUD_INPUT_KINDS, CLOUD_POINTER_PHASES,
    CLOUD_TOOL_MANIFEST_VERSION, CLOUD_TOOL_RUNTIME_MIN_VERSION,
};
use serde_json::{to_value, Value};

use super::cloud_methods::CLOUD_TOOL_METHODS;

pub fn build_cloud_methods_manifest() -> CloudToolManifest {
    CloudToolManifest {
        manifest_version: CLOUD_TOOL_MANIFEST_VERSION,
        runtime_min_version: CLOUD_TOOL_RUNTIME_MIN_VERSION.to_string(),
        input: CloudInputCapabilities {
            coordinate_space: CLOUD_INPUT_COORDINATE_SPACE.to_string(),
            kinds: CLOUD_INPUT_KINDS
                .iter()
                .map(|kind| (*kind).to_string())
                .collect(),
            pointer_phases: CLOUD_POINTER_PHASES
                .iter()
                .map(|phase| (*phase).to_string())
                .collect(),
        },
        identity: CloudIdentityCapabilities {
            scopes: vec![
                "session".to_string(),
                "project".to_string(),
                "global".to_string(),
            ],
            local_first: true,
        },
        methods: CLOUD_TOOL_METHODS
            .iter()
            .map(|method| CloudToolManifestMethod {
                name: method.name.to_string(),
                category: method.category.as_str().to_string(),
            })
            .collect(),
    }
}

pub fn handle_cloud_methods() -> Result<Value, String> {
    to_value(build_cloud_methods_manifest()).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_registered_cloud_methods() {
        let manifest = build_cloud_methods_manifest();
        let expected = CLOUD_TOOL_METHODS
            .iter()
            .map(|method| (method.name, method.category.as_str()))
            .collect::<Vec<_>>();
        let actual = manifest
            .methods
            .iter()
            .map(|method| (method.name.as_str(), method.category.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(manifest.manifest_version, CLOUD_TOOL_MANIFEST_VERSION);
        assert_eq!(manifest.runtime_min_version, CLOUD_TOOL_RUNTIME_MIN_VERSION);
        assert_eq!(manifest.input.coordinate_space, "viewport_css");
        assert!(manifest.identity.local_first);
        assert_eq!(actual, expected);
    }
}
